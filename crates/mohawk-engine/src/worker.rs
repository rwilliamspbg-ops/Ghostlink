//! Worker configuration and management

use serde::{Deserialize, Serialize};

/// Worker configuration for distributed inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub host: String,
    pub port: u16,
    pub gpu_id: Option<usize>,
    pub memory_limit_mb: usize,
    pub layer_range: (usize, usize),
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: format!("worker-{}", rand::random::<u16>()),
            host: "localhost".to_string(),
            port: 8003,
            gpu_id: None,
            memory_limit_mb: 8192,
            layer_range: (0, 0),
        }
    }
}

impl WorkerConfig {
    /// Create a new worker config
    pub fn new(worker_id: &str, host: &str, port: u16) -> Self {
        Self {
            worker_id: worker_id.to_string(),
            host: host.to_string(),
            port,
            ..Default::default()
        }
    }

    /// Set GPU assignment
    pub fn with_gpu(mut self, gpu_id: usize) -> Self {
        self.gpu_id = Some(gpu_id);
        self
    }

    /// Set layer range for this worker
    pub fn with_layers(mut self, start: usize, end: usize) -> Self {
        self.layer_range = (start, end);
        self
    }

    /// Set memory limit
    pub fn with_memory_limit(mut self, limit_mb: usize) -> Self {
        self.memory_limit_mb = limit_mb;
        self
    }
}
