// Phase F.2: Robust BLS Batch Verification using ConsensusState
mod consensus;
mod types;
mod net;
mod dag;
mod crypto;
mod bls_crypto;

use crate::consensus::ConsensusState;
use crate::types::{Event, Vertex, CoA, Message, Hash, ValidatorId};
use crate::net::{TcpNetwork, NetworkHandle};
use crate::bls_crypto::{BlsSecretKey, aggregate_signatures_with_metrics, verify_aggregated_batch_with_metrics};
use std::time::{Instant, Duration};
use std::env;
use std::collections::HashMap;
use rand::rngs::OsRng;
use tokio::sync::mpsc;
use std::sync::Arc;
use parking_lot::Mutex;
use rkyv::ser::serializers::AllocSerializer;
use rkyv::ser::Serializer;

const MAX_ROUND_DRIFT: u64 = 50;
const VERIFICATION_WINDOW: usize = 200; // Allow more in-flight to sustain throughput on i9

#[derive(Debug, Clone, Default)]
struct CryptoMetrics {
    bls_sign_count: u64,
    bls_verify_micros: u64,
    cert_count: u64,
    batch_count: u64,
    pairing_count: u64,
}

#[derive(Debug, Clone, Default)]
struct DriftMetrics {
    drift_samples: Vec<u64>,
    max_drift: u64,
}

impl DriftMetrics {
    fn record(&mut self, drift: u64) {
        self.drift_samples.push(drift);
        if drift > self.max_drift {
            self.max_drift = drift;
        }
    }

    fn mean_drift(&self) -> f64 {
        if self.drift_samples.is_empty() {
            0.0
        } else {
            self.drift_samples.iter().sum::<u64>() as f64 / self.drift_samples.len() as f64
        }
    }
}

impl CryptoMetrics {
    fn report(&self, total_dur_micros: u64) -> String {
        let avg_v = if self.cert_count > 0 { self.bls_verify_micros as f64 / self.cert_count as f64 } else { 0.0 };
        let avg_p = if self.cert_count > 0 { self.pairing_count as f64 / self.cert_count as f64 } else { 0.0 };
        let crypto_pct = (self.bls_verify_micros as f64 / total_dur_micros as f64) * 100.0;
        format!("BLS Signs: {} | Verify: {} certs, {} batches | Pairings: {} total ({:.2}/cert) | Pairing_µs/cert: {:.1} | Crypto_CPU: {:.2}%",
            self.bls_sign_count, self.cert_count, self.batch_count, self.pairing_count, avg_p, avg_v, crypto_pct)
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
    let port_offset: u16 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10000);
    
    // Phase G.2: Dedicated worker pools
    // 3 crypto, 3 validator, 2 network = 8 total
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        println!("=== Aether-V2 Phase F.2: BLS Batch Verification (n={}) ===", n);
        let mut bls_keys = Vec::new();
        let mut bls_pks = Vec::new();
        let mut rng = OsRng;
        for _ in 0..n {
            let sk = BlsSecretKey::generate(&mut rng);
            bls_pks.push(sk.public_key());
            bls_keys.push(sk);
        }
        
        // Precompute uncompressed public keys for the entire lifetime
        let pks_shared: Vec<(u32, Vec<u8>)> = (0..n).map(|idx| (idx as u32, bls_pks[idx].clone())).collect();
        let pks_shared = Arc::new(pks_shared);

        let latencies = Arc::new(Mutex::new(Vec::new()));
        let crypto_metrics = Arc::new(Mutex::new(CryptoMetrics::default()));
        let drift_metrics = Arc::new(Mutex::new(DriftMetrics::default()));
        let mut tasks = Vec::new();

        for i in 0..n {
            let node_id = i as u32;
            let bls_sk = bls_keys[i].clone();
            let _all_pks = bls_pks.clone();
            let pks_node = pks_shared.clone();
            let latencies = latencies.clone();
            let metrics = crypto_metrics.clone();
            let drift_m = drift_metrics.clone();
            let mut peer_addrs = HashMap::new();
            for j in 0..n { if i != j { peer_addrs.insert(j as u32, format!("127.0.0.1:{}", port_offset + j as u16)); } }
            let listen_addr = format!("127.0.0.1:{}", port_offset + i as u16);

            tasks.push(tokio::spawn(async move {
                let (tx, mut rx) = mpsc::channel(1_000_000);
                let network = TcpNetwork::new(node_id, listen_addr, peer_addrs);
                let handle = network.start(tx).await;
                let mut state = ConsensusState::new(node_id, n);
                tokio::time::sleep(Duration::from_secs(2)).await;

                let start = Instant::now();
                let mut last_report = Instant::now();
                let mut round_starts = HashMap::new();
                let mut in_flight = std::collections::HashSet::new();

                loop {
                    let drift = state.round.saturating_sub(state.dag.committed_round);
                    drift_m.lock().record(drift);

                    if node_id == 0 && last_report.elapsed() > Duration::from_secs(2) {
                        let dur = start.elapsed().as_secs_f64();
                        let total_micros = start.elapsed().as_micros() as u64;
                        let mut l = latencies.lock();
                        l.sort();
                        let p99 = if !l.is_empty() { l[(l.len() * 99) / 100] } else { 0 };
                        
                        let net_m = handle.get_metrics();
                        let tx_count = state.dag.committed_round * n as u64;
                        let b_tx = if tx_count > 0 { net_m.bytes_sent as f64 / tx_count as f64 } else { 0.0 };

                        let dm = drift_m.lock();
                        println!("RESULT: VPS={:.2}, P99={}ms, B/Tx={:.1}, Drift={:.1}/{}, R={}/CR={}", 
                            tx_count as f64 / dur, p99, b_tx, dm.mean_drift(), dm.max_drift, state.round, state.dag.committed_round);
                        println!("DEBUG_METRICS: {}", metrics.lock().report(total_micros));
                        last_report = Instant::now();
                    }

                    // 1. Propose Vertex (with backpressure)
                    let can_propose = drift < MAX_ROUND_DRIFT && in_flight.len() < VERIFICATION_WINDOW;
                    if can_propose {
                        let v = Vertex { 
                            round: state.round, 
                            author: node_id, 
                            batch_hash: [0u8; 32], 
                            parent_indices: if state.round > 1 { (0..(n as u32)).collect() } else { vec![] } 
                        };
                        if node_id == 0 { round_starts.insert(v.round, Instant::now()); }
                        
                        // Broadcast Vertex
                        let mut ser = AllocSerializer::<1024>::default();
                        ser.serialize_value(&Message::Vertex(v.clone())).unwrap();
                        let v_bytes = ser.into_serializer().into_inner().to_vec();
                        let _ = handle.broadcast_raw(v_bytes).await;
                        
                        // Hash Vertex and Sign
                        let h = crate::crypto::hash_vertex(&v);
                        state.on_event(Event::VertexReceived(v));
                        
                        let sig = bls_sk.sign(&h);
                        metrics.lock().bls_sign_count += 1;
                        
                        let coa = CoA { batch_hash: h, signatures: vec![(node_id, sig)] };
                        state.on_event(Event::CoAReceived(coa.clone()));
                        
                        // Broadcast CoA
                        let mut ser_coa = AllocSerializer::<1024>::default();
                        ser_coa.serialize_value(&Message::CoA(coa)).unwrap();
                        let _ = handle.broadcast_raw(ser_coa.into_serializer().into_inner().to_vec()).await;
                        
                        state.round += 1;
                    }

                    // 2. Process incoming events
                    let mut event_count = 0;
                    while let Ok(event) = rx.try_recv() {
                        match event {
                            Event::VertexReceived(v) => {
                                let h = crate::crypto::hash_vertex(&v);
                                state.on_event(Event::VertexReceived(v));
                                
                                let sig = bls_sk.sign(&h);
                                metrics.lock().bls_sign_count += 1;
                                
                                let coa = CoA { batch_hash: h, signatures: vec![(node_id, sig)] };
                                state.on_event(Event::CoAReceived(coa.clone()));
                                
                                let mut ser_coa = AllocSerializer::<1024>::default();
                                ser_coa.serialize_value(&Message::CoA(coa)).unwrap();
                                let _ = handle.broadcast_raw(ser_coa.into_serializer().into_inner().to_vec()).await;
                            }
                            Event::CoAReceived(coa) => {
                                state.on_event(Event::CoAReceived(coa));
                            }
                            _ => {}
                        }
                        event_count += 1;
                        if event_count > 1000 { break; }
                    }

                    // 3. Batch Verification
                    let pending = state.get_pending_quorums();
                    let mut batch_items = Vec::new();

                    for (h, _vertex, signatures) in pending {
                        if in_flight.contains(&h) { continue; }
                        let q = n - (n - 1) / 3;
                        if let Ok((agg, bitmap, _)) = aggregate_signatures_with_metrics(&signatures, q) {
                            batch_items.push((h, agg, bitmap, q, signatures));
                            in_flight.insert(h);
                        }
                    }

                    // Adaptive batch trigger based on drift
                    let batch_trigger = if drift > 10 { 
                        4  // Aggressive: verify smaller batches to reduce latency
                    } else { 
                        8  // Normal: balance batching efficiency
                    };
                    
                    if !batch_items.is_empty() && (batch_items.len() >= batch_trigger || drift > 15) {
                        let mut batch_input = Vec::new();
                        for (h, agg, bitmap, q, _) in batch_items.iter() {
                            batch_input.push((h.as_slice(), agg, pks_node.as_slice(), *bitmap, *q));
                        }

                        let (valid, v_metrics) = verify_aggregated_batch_with_metrics(batch_input);
                        
                        let mut m = metrics.lock();
                        m.cert_count += batch_items.len() as u64;
                        m.bls_verify_micros += v_metrics.verify_micros;
                        m.batch_count += 1;
                        m.pairing_count += v_metrics.pairing_count;
                        drop(m);

                        if valid {
                            let old_cr = state.dag.committed_round;
                            for (h, _, _, _, sigs) in batch_items {
                                state.certify_vertex(h, sigs);
                                in_flight.remove(&h); // Release the credit
                            }
                            if node_id == 0 && state.dag.committed_round > old_cr {
                                for r in (old_cr + 1)..=state.dag.committed_round {
                                    if let Some(s) = round_starts.remove(&r) {
                                        latencies.lock().push(s.elapsed().as_millis());
                                    }
                                }
                            }
                        } else {
                            for (h, _, _, _, _) in &batch_items {
                                in_flight.remove(h);
                            }
                        }
                    }

                    if state.dag.committed_round >= 25000 { break; }
                    tokio::task::yield_now().await;
                }
            }));
        }
        futures::future::join_all(tasks).await;
    });
}
