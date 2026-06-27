//! Engine metrics collection

/// Metrics for the inference engine
#[derive(Debug, Clone, Default)]
pub struct EngineMetrics {
    pub requests_total: u64,
    pub requests_per_second: f32,
    pub avg_latency_ms: f32,
    pub p99_latency_ms: f32,
    pub gpu_memory_used_mb: f32,
    pub gpu_utilization: f32,
}

impl EngineMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a latency measurement
    pub fn record_latency(&mut self, latency_ms: f32) {
        self.requests_total += 1;
        // Simple EMA for average latency
        self.avg_latency_ms = self.avg_latency_ms * 0.9 + latency_ms * 0.1;
    }
}
