use criterion::{black_box, criterion_group, criterion_main, Criterion};
use aether_rs::crypto::{hash, verify_signature, derive_vrf_seed};
use aether_rs::types::{Vertex, Hash, ValidatorId, CoA};
use aether_rs::dag::Dag;
use ed25519_dalek::{SigningKey, Signer};
use rand::rngs::OsRng;

fn bench_crypto(c: &mut Criterion) {
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let verifying_key = signing_key.verifying_key();
    let message = b"Aether-V2 Benchmark Message for CoA Signatures";
    let signature = signing_key.sign(message).to_bytes().to_vec();

    c.bench_function("ed25519_verify", |b| b.iter(|| {
        verify_signature(black_box(message), black_box(&signature), black_box(&verifying_key))
    }));

    c.bench_function("blake3_hash_512b", |b| b.iter(|| {
        hash(black_box(&[0u8; 512]))
    }));
}

fn bench_dag_sort(c: &mut Criterion) {
    let n = 20;
    let f = 6;
    let mut dag = Dag::new(n, f);
    let seed = [0u8; 32];

    // Simulate a DAG with 1000 vertices across 10 rounds
    for r in 1..=10 {
        for i in 0..n {
            let v = Vertex {
                round: r as u64,
                author: i as u32,
                batch_hash: [i as u8; 32],
                parents: vec![], // simplified
            };
            let coa = CoA {
                batch_hash: [i as u8; 32],
                signatures: vec![],
            };
            dag.insert_certified(aether_rs::types::CertifiedVertex { vertex: v, coa });
        }
    }

    let anchor = dag.round_to_vertices[&10][0];

    c.bench_function("aether_sort_200_nodes", |b| b.iter(|| {
        dag.aether_sort(black_box(&anchor), black_box(&seed))
    }));
}

criterion_group!(benches, bench_crypto, bench_dag_sort);
criterion_main!(benches);
