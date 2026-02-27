// ML-Based Adaptive Batching Predictor
// Uses historical latency/throughput patterns to optimize batch sizes

use std::collections::VecDeque;

/// Simple EWMA (Exponentially Weighted Moving Average) predictor
/// Can be upgraded to LSTM/CNN later for SciFest demo
pub struct AdaptiveBatchPredictor {
    history_window: VecDeque<(f64, f64)>, // (latency_ms, throughput_tps)
    max_history: usize,
    alpha: f64, // EWMA smoothing factor
    
    // Predicted values
    predicted_latency: f64,
    predicted_throughput: f64,
    
    // Batch size optimization
    current_batch_size: usize,
    min_batch_size: usize,
    max_batch_size: usize,
}

impl AdaptiveBatchPredictor {
    pub fn new(initial_batch_size: usize) -> Self {
        Self {
            history_window: VecDeque::with_capacity(100),
            max_history: 100,
            alpha: 0.3, // 30% weight to new data, 70% to history
            predicted_latency: 50.0, // Initial guess: 50ms
            predicted_throughput: 300.0, // Initial guess: 300 TPS
            current_batch_size: initial_batch_size,
            min_batch_size: 10,
            max_batch_size: 1000,
        }
    }
    
    /// Record a new observation (latency in ms, throughput in TPS)
    pub fn observe(&mut self, latency_ms: f64, throughput_tps: f64) {
        self.history_window.push_back((latency_ms, throughput_tps));
        
        if self.history_window.len() > self.max_history {
            self.history_window.pop_front();
        }
        
        // Update predictions using EWMA
        self.predicted_latency = self.alpha * latency_ms + (1.0 - self.alpha) * self.predicted_latency;
        self.predicted_throughput = self.alpha * throughput_tps + (1.0 - self.alpha) * self.predicted_throughput;
    }
    
    /// Predict the optimal batch size for the next round
    pub fn predict_batch_size(&mut self) -> usize {
        // Simple heuristic: if latency is rising, reduce batch size
        // if throughput is dropping, increase batch size
        
        if self.history_window.len() < 10 {
            return self.current_batch_size; // Not enough data
        }
        
        let recent_latencies: Vec<f64> = self.history_window
            .iter()
            .rev()
            .take(10)
            .map(|(lat, _)| *lat)
            .collect();
        
        let latency_trend = Self::compute_trend(&recent_latencies);
        
        // Adaptive rule:
        // - If latency is increasing rapidly (>10% per round), shrink batch
        // - If latency is stable/decreasing, grow batch
        
        let new_batch_size = if latency_trend > 0.1 {
            // Latency rising -> reduce batch size by 20%
            (self.current_batch_size as f64 * 0.8) as usize
        } else if latency_trend < -0.05 {
            // Latency falling -> increase batch size by 10%
            (self.current_batch_size as f64 * 1.1) as usize
        } else {
            // Stable -> no change
            self.current_batch_size
        };
        
        // Clamp to valid range
        self.current_batch_size = new_batch_size.clamp(self.min_batch_size, self.max_batch_size);
        self.current_batch_size
    }
    
    /// Compute the trend (positive = increasing, negative = decreasing)
    fn compute_trend(values: &[f64]) -> f64 {
        if values.len() < 2 {
            return 0.0;
        }
        
        // Simple linear regression slope
        let n = values.len() as f64;
        let x_mean = (n - 1.0) / 2.0;
        let y_mean: f64 = values.iter().sum::<f64>() / n;
        
        let mut numerator = 0.0;
        let mut denominator = 0.0;
        
        for (i, &y) in values.iter().enumerate() {
            let x = i as f64;
            numerator += (x - x_mean) * (y - y_mean);
            denominator += (x - x_mean).powi(2);
        }
        
        if denominator == 0.0 {
            return 0.0;
        }
        
        numerator / denominator / y_mean // Normalized slope
    }
    
    /// Get current predictions
    pub fn get_predictions(&self) -> (f64, f64) {
        (self.predicted_latency, self.predicted_throughput)
    }
    
    /// Export model state for visualization/logging
    pub fn export_state(&self) -> String {
        format!(
            "AdaptiveBatchPredictor State:\n\
             - Current batch size: {}\n\
             - Predicted latency: {:.2}ms\n\
             - Predicted throughput: {:.2} TPS\n\
             - History samples: {}",
            self.current_batch_size,
            self.predicted_latency,
            self.predicted_throughput,
            self.history_window.len()
        )
    }
}

/// Neural Network-based predictor (placeholder for future LSTM/CNN implementation)
#[allow(dead_code)]
pub struct NeuralBatchPredictor {
    // Placeholder for ONNX runtime or PyTorch bindings
    // model_path: String,
    // Can be upgraded to use `tract` or `tch-rs` for real neural inference
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_trend_computation() {
        let increasing = vec![10.0, 12.0, 14.0, 16.0];
        let trend = AdaptiveBatchPredictor::compute_trend(&increasing);
        assert!(trend > 0.0, "Trend should be positive for increasing values");
        
        let decreasing = vec![20.0, 18.0, 16.0, 14.0];
        let trend = AdaptiveBatchPredictor::compute_trend(&decreasing);
        assert!(trend < 0.0, "Trend should be negative for decreasing values");
    }
    
    #[test]
    fn test_batch_size_adjustment() {
        let mut predictor = AdaptiveBatchPredictor::new(100);
        
        // Simulate rising latency
        for i in 0..20 {
            predictor.observe(50.0 + (i as f64) * 5.0, 300.0); // Latency rising
        }
        
        let new_batch = predictor.predict_batch_size();
        assert!(new_batch < 100, "Batch size should shrink when latency rises");
    }
    
    #[test]
    fn test_batch_size_clamping() {
        let mut predictor = AdaptiveBatchPredictor::new(100);
        predictor.min_batch_size = 50;
        predictor.max_batch_size = 200;
        
        // Try to force batch size below minimum
        for _ in 0..50 {
            predictor.observe(200.0, 50.0); // Very high latency
        }
        
        let new_batch = predictor.predict_batch_size();
        assert!(new_batch >= 50, "Batch size should not go below minimum");
    }
}
