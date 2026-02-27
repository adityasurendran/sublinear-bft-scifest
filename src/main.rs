mod consensus;
mod types;
mod net;
mod dag;
mod crypto;

use crate::consensus::ConsensusState;
use crate::types::{Event, Vertex, CoA, Message, Hash};
use crate::net::{TcpNetwork, NetworkHandle};
use std::time::{Instant, Duration};
use std::env;
use std::collections::HashMap;
use ed25519_dalek::{SigningKey, Signer};
use rand::rngs::OsRng;
use tokio::sync::mpsc;
use std::sync::Arc;
use parking_lot::Mutex;
use rkyv::ser::serializers::AllocSerializer;
use rkyv::ser::Serializer;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mode_a = args.iter().any(|arg| arg == "--mode-a");
    let mode_b = args.iter().any(|arg| arg == "--mode-b");

    // Parse n and port_offset from command line
    let mut args_iter = args.iter().skip(1); // Skip program name
    let mut n: usize = 3; // Default
    let mut port_offset: u16 = 10000; // Default

    if mode_a || mode_b {
        // Consume the mode flag
        args_iter.find(|&arg| arg == "--mode-a" || arg == "--mode-b");

        // Next argument is n
        if let Some(n_str) = args_iter.next() {
            n = n_str.parse().expect("N must be a number");
        }

        // Next argument is port_offset
        if let Some(offset_str) = args_iter.next() {
            port_offset = offset_str.parse().expect("Port offset must be a number");
        }
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async move {
        if mode_a {
            run_surface_test("Mode T", 30000, n, port_offset).await; // Pass n here
        } else if mode_b {
            run_surface_test("Mode L", 1, n, port_offset).await; // Pass n here
        }
    });
}

async fn run_surface_test(label: &str, batch_window: usize, n: usize, port_offset: u16) {
    println!("--- Aether-V2 Phase C: {} ---", label);
    let mut keys = Vec::new();
    let mut csprng = OsRng;
    for _ in 0..n {
        let mut bytes = [0u8; 32];
        use rand::RngCore;
        csprng.fill_bytes(&mut bytes);
        keys.push(SigningKey::from_bytes(&bytes));
    }

    let latencies = Arc::new(Mutex::new(Vec::<u128>::new()));
    let mut futures = Vec::new();

    for i in 0..n {
        let node_id = i as u32;
        let mut peer_addrs = HashMap::new();
        for j in 0..n { if i != j { peer_addrs.insert(j as u32, format!("127.0.0.1:{}", port_offset + j as u16)); } }
        let listen_addr = format!("127.0.0.1:{}", port_offset + i as u16);
        let sk = SigningKey::from_bytes(&keys[i].to_bytes());
        let latencies_clone = latencies.clone();

        futures.push(tokio::spawn(async move {
            let (event_tx, mut event_rx) = mpsc::channel(1_000_000);
            let network = TcpNetwork::new(node_id, listen_addr, peer_addrs);
            let net_handle: Arc<NetworkHandle> = network.start(event_tx).await;
            let mut state = ConsensusState::new(node_id, n);

            tokio::time::sleep(Duration::from_secs(2)).await;

            let start_time = Instant::now();
            let mut last_report = Instant::now();
            let mut total_v = 0u64;
            let mut round_starts = HashMap::<u64, Instant>::new();

            loop {
                if last_report.elapsed() > Duration::from_secs(2) {
                    if node_id == 0 {
                        let m = net_handle.get_metrics();
                        let dur = start_time.elapsed().as_secs_f64();
                        let mut lats = latencies_clone.lock();
                        lats.sort();
                        let p99 = if !lats.is_empty() { lats[(lats.len() * 99) / 100] } else { 0 };
                        
                        // Fix committed count for better metrics
                        let committed_count = state.dag.committed_round as f64 * 6.0; 
                        let bytes_per_tx = if committed_count > 0.0 { m.bytes_sent as f64 / committed_count } else { 0.0 };
                        
                        println!("RESULT: VPS={:.2}, P99={}ms, MBps={:.2}, B/Tx={:.2}, R={}/CR={}", 
                            (state.dag.committed_round * 6) as f64 / dur, p99, (m.bytes_sent as f64 / 1024.0 / 1024.0) / dur, bytes_per_tx,
                            state.round, state.dag.committed_round);
                    }
                    last_report = Instant::now();
                }

                // Propose: Allow round overlap (Phase D)
                const MAX_OVERLAP_ROUNDS: usize = 5;
                while (state.round as usize).saturating_sub(state.dag.committed_round as usize) < MAX_OVERLAP_ROUNDS {
                    // Phase D.1: Increase proposal density (6 vertices per round)
                    for _ in 0..6 {
                        let v = Vertex { 
                            round: state.round, 
                            author: node_id, 
                            batch_hash: [0u8; 32], 
                            parent_indices: if state.round > 1 { (0..(n as u32)).collect() } else { vec![] }
                        };
                        if node_id == 0 { round_starts.insert(v.round, Instant::now()); }
                        
                        let mut serializer = AllocSerializer::<2048>::default();
                        serializer.serialize_value(&Message::Vertex(v.clone())).unwrap();
                        let msg_bytes = serializer.into_serializer().into_inner();
                        let _ = net_handle.broadcast_raw(msg_bytes.to_vec()).await;
                        
                        // Use standard rkyv hashing
                        let mut v_ser = AllocSerializer::<1024>::default();
                        v_ser.serialize_value(&v).unwrap();
                        let v_bytes = v_ser.into_serializer().into_inner();
                        let v_hash = crate::crypto::hash(&v_bytes);
                        
                        state.dag.vertices.insert(v_hash, v.clone());
                        state.dag.round_to_vertices.entry(v.round).or_default().push(v_hash);
                        
                        // Self-sign and add to collector
                        let sig = sk.sign(&v_bytes).to_bytes().to_vec();
                        state.on_event(Event::CoAReceived(CoA { batch_hash: v_hash, signatures: vec![(node_id, sig)] }));
                        
                        total_v += 1;
                    }
                    state.round += 1;
                }

                // Drain events
                loop {
                    match event_rx.try_recv() {
                        Ok(event) => {
                            match event {
                                Event::VertexReceived(v) => {
                                    let mut serializer = AllocSerializer::<1024>::default();
                                    serializer.serialize_value(&v).unwrap();
                                    let v_bytes = serializer.into_serializer().into_inner();
                                    let v_hash = crate::crypto::hash(&v_bytes);
                                    
                                    state.on_event(Event::VertexReceived(v.clone()));
                                    let sig = sk.sign(&v_bytes).to_bytes().to_vec();
                                    let coa = CoA { batch_hash: v_hash, signatures: vec![(node_id, sig)] };
                                    
                                    // Deliver to self to ensure we get the full quorum
                                    state.on_event(Event::CoAReceived(coa.clone()));

                                    // Broadcast to all peers
                                    let coa_msg = Message::CoA(coa);
                                    let mut serializer = AllocSerializer::<2048>::default();
                                    serializer.serialize_value(&coa_msg).unwrap();
                                    let msg_bytes = serializer.into_serializer().into_inner().to_vec();
                                    let _ = net_handle.broadcast_raw(msg_bytes).await;
                                }
                                Event::CoAReceived(coa) => {
                                    let old_committed = state.dag.committed_round;
                                    state.on_event(Event::CoAReceived(coa));
                                    let new_committed = state.dag.committed_round;
                                    if node_id == 0 && new_committed > old_committed {
                                        for r in (old_committed + 1)..=new_committed {
                                            if let Some(start) = round_starts.remove(&r) {
                                                let lat = start.elapsed().as_millis();
                                                // println!("DEBUG: Latency for round {}: {}ms", r, lat);
                                                latencies_clone.lock().push(lat);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        Err(_) => break,
                    }
                }

                if state.dag.committed_round >= 25000 { 
                    if node_id == 0 {
                        let m = net_handle.get_metrics();
                        let dur = start_time.elapsed().as_secs_f64();
                        let mut lats = latencies_clone.lock();
                        lats.sort();
                        let p99 = if !lats.is_empty() { lats[(lats.len() * 99) / 100] } else { 0 };
                        println!("FINAL_RESULT: VPS={:.2}, P99={}ms, MBps={:.2}", 
                            total_v as f64 / dur, p99, (m.bytes_sent as f64 / 1024.0 / 1024.0) / dur);
                    }
                    break; 
                }
            }
        }));
    }
    futures::future::join_all(futures).await;
}
