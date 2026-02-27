// Byzantine Fault Injector for Resilience Testing
// Simulates malicious validators and network failures

use rand::Rng;
use crate::types::{ValidatorId, Event, Vertex};

#[derive(Debug, Clone)]
pub enum FaultType {
    MessageDrop,       // Validator doesn't send messages
    MessageDelay(u64), // Validator delays messages by X ms
    Equivocation,      // Validator sends conflicting messages
    SlowResponse,      // Validator takes 10x longer to respond
    RandomCrash,       // Validator randomly goes offline
}

pub struct FaultInjector {
    byzantine_nodes: Vec<ValidatorId>,
    fault_probability: f64, // 0.0 to 1.0
    active_faults: Vec<FaultType>,
}

impl FaultInjector {
    pub fn new(byzantine_nodes: Vec<ValidatorId>, fault_probability: f64) -> Self {
        Self {
            byzantine_nodes,
            fault_probability,
            active_faults: vec![
                FaultType::MessageDrop,
                FaultType::MessageDelay(100_000), // 100ms delay
                FaultType::SlowResponse,
            ],
        }
    }
    
    /// Check if a message from this validator should be intercepted
    pub fn should_intercept(&self, validator_id: ValidatorId) -> bool {
        if !self.byzantine_nodes.contains(&validator_id) {
            return false;
        }
        
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.fault_probability
    }
    
    /// Apply a random fault to the message
    pub fn apply_fault(&self, event: Event) -> Option<(Event, FaultType)> {
        let mut rng = rand::thread_rng();
        let fault = &self.active_faults[rng.gen_range(0..self.active_faults.len())];
        
        match fault {
            FaultType::MessageDrop => None, // Drop the message entirely
            FaultType::MessageDelay(delay_us) => {
                // In a real implementation, this would schedule the message for later
                Some((event, FaultType::MessageDelay(*delay_us)))
            }
            FaultType::SlowResponse => {
                Some((event, FaultType::SlowResponse))
            }
            _ => Some((event, FaultType::MessageDrop)),
        }
    }
    
    /// Create an equivocating vertex (two conflicting proposals)
    pub fn create_equivocation(&self, original: &Vertex) -> Vertex {
        let mut conflicting = original.clone();
        // Modify the payload to create a conflicting vertex
        conflicting.payload = format!("BYZANTINE_PAYLOAD_{}", rand::thread_rng().gen::<u64>()).into_bytes();
        conflicting
    }
    
    /// Track recovery time after a fault is resolved
    pub fn measure_recovery_time(&self) -> std::time::Duration {
        // In a real implementation, this would measure the time between
        // fault injection and system returning to normal operation
        std::time::Duration::from_millis(500)
    }
}

/// Metrics collector for Byzantine resilience testing
pub struct ResilienceMetrics {
    pub total_faults_injected: u64,
    pub messages_dropped: u64,
    pub messages_delayed: u64,
    pub equivocations_detected: u64,
    pub recovery_times: Vec<std::time::Duration>,
}

impl ResilienceMetrics {
    pub fn new() -> Self {
        Self {
            total_faults_injected: 0,
            messages_dropped: 0,
            messages_delayed: 0,
            equivocations_detected: 0,
            recovery_times: Vec::new(),
        }
    }
    
    pub fn record_fault(&mut self, fault: &FaultType) {
        self.total_faults_injected += 1;
        match fault {
            FaultType::MessageDrop => self.messages_dropped += 1,
            FaultType::MessageDelay(_) => self.messages_delayed += 1,
            FaultType::Equivocation => self.equivocations_detected += 1,
            _ => {}
        }
    }
    
    pub fn mean_recovery_time(&self) -> Option<std::time::Duration> {
        if self.recovery_times.is_empty() {
            return None;
        }
        
        let total: std::time::Duration = self.recovery_times.iter().sum();
        Some(total / self.recovery_times.len() as u32)
    }
    
    pub fn report(&self) -> String {
        format!(
            "Byzantine Resilience Report:\n\
             - Total faults injected: {}\n\
             - Messages dropped: {}\n\
             - Messages delayed: {}\n\
             - Equivocations detected: {}\n\
             - Mean recovery time: {:?}",
            self.total_faults_injected,
            self.messages_dropped,
            self.messages_delayed,
            self.equivocations_detected,
            self.mean_recovery_time()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fault_injection_probability() {
        let injector = FaultInjector::new(vec![0, 1], 0.5);
        
        // Run 1000 trials, expect ~500 intercepts
        let mut intercept_count = 0;
        for _ in 0..1000 {
            if injector.should_intercept(0) {
                intercept_count += 1;
            }
        }
        
        assert!(intercept_count > 400 && intercept_count < 600, 
                "Expected ~500 intercepts, got {}", intercept_count);
    }
    
    #[test]
    fn test_honest_nodes_not_intercepted() {
        let injector = FaultInjector::new(vec![0, 1], 0.5);
        
        // Honest node (ID 2) should never be intercepted
        for _ in 0..100 {
            assert!(!injector.should_intercept(2));
        }
    }
}
