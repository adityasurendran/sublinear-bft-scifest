// SciFest Features Comprehensive Benchmark
// Tests: Geo-Latency, Fault Injection, ML Batching

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use sublinear_bft_scifest::geo_latency::{GeoLatencyMatrix, Region};
use sublinear_bft_scifest::fault_injector::{FaultInjector, ResilienceMetrics};
use sublinear_bft_scifest::ml_predictor::AdaptiveBatchPredictor;

fn bench_geo_latency(c: &mut Criterion) {
    let matrix = GeoLatencyMatrix::new();
    
    c.bench_function("geo_latency_lookup", |b| {
        b.iter(|| {
            black_box(matrix.get_latency_ms(Region::USEast, Region::APNortheast))
        })
    });
    
    let global_validators = vec![
        Region::USEast,
        Region::EUWest,
        Region::APNortheast,
        Region::SAEast,
    ];
    
    c.bench_function("geo_average_latency", |b| {
        b.iter(|| {
            black_box(matrix.average_latency(&global_validators))
        })
    });
}

fn bench_fault_injection(c: &mut Criterion) {
    let injector = FaultInjector::new(vec![0, 1], 0.3);
    
    c.bench_function("fault_intercept_check", |b| {
        b.iter(|| {
            black_box(injector.should_intercept(0))
        })
    });
}

fn bench_ml_predictor(c: &mut Criterion) {
    let mut predictor = AdaptiveBatchPredictor::new(100);
    
    // Populate some history
    for i in 0..50 {
        predictor.observe(50.0 + (i as f64) * 0.5, 300.0 - (i as f64) * 2.0);
    }
    
    c.bench_function("ml_batch_prediction", |b| {
        b.iter(|| {
            black_box(predictor.predict_batch_size())
        })
    });
    
    c.bench_function("ml_observation_update", |b| {
        b.iter(|| {
            black_box(predictor.observe(55.0, 280.0))
        })
    });
}

fn bench_combined_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined_features");
    
    for &n in &[4, 10, 20] {
        group.bench_with_input(BenchmarkId::new("full_stack", n), &n, |b, &n| {
            let matrix = GeoLatencyMatrix::new();
            let injector = FaultInjector::new(vec![0], 0.1);
            let mut predictor = AdaptiveBatchPredictor::new(100);
            
            b.iter(|| {
                // Simulate one consensus round with all features enabled
                let latency = matrix.get_latency_ms(Region::USEast, Region::EUWest);
                let should_drop = injector.should_intercept(0);
                predictor.observe(latency as f64, 300.0);
                let batch_size = predictor.predict_batch_size();
                
                black_box((latency, should_drop, batch_size))
            });
        });
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_geo_latency,
    bench_fault_injection,
    bench_ml_predictor,
    bench_combined_overhead
);
criterion_main!(benches);
