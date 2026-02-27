pub mod types;
pub mod crypto;
pub mod bls_crypto;  // Phase E.4: BLS12-381 Signature Aggregation
pub mod dag;
pub mod net;
pub mod consensus;

// SciFest Feature Additions
pub mod geo_latency;     // Multi-Region Geo-Latency Simulation
pub mod fault_injector;  // Byzantine Fault Injection & Resilience Testing
pub mod ml_predictor;    // ML-Based Adaptive Batching
