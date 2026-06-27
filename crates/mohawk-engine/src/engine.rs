//! Mohawk Inference Engine Core
//!
//! Provides the main inference engine with distributed computation support

use anyhow::Result;
use ghostlink_core::cluster::ClusterState;
use ghostlink_core::health::NetworkHealthMonitor;
use std::sync::Arc;
use tracing::{info, warn};

/// Main inference engine configuration
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub node_id: String,
    pub model_path: String,
    pub worker_threads: usize,
    pub enable_ghostlink: bool,
    pub interface: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            node_id: format!("node-{}", uuid_simple()),
            model_path: String::new(),
            worker_threads: num_cpus::get(),
            enable_ghostlink: true,
            interface: None,
        }
    }
}

fn uuid_simple() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:04x}", rng.gen_range(0..65536))
}

/// Mohawk Inference Engine
pub struct InferenceEngine {
    config: EngineConfig,
    cluster: Arc<ClusterState>,
    health_monitor: Arc<NetworkHealthMonitor>,
    loaded_models: std::collections::HashMap<String, ModelInfo>,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub layers: usize,
    pub size_mb: f32,
    pub loaded_at: std::time::Instant,
}

impl InferenceEngine {
    /// Create a new inference engine
    pub fn new(config: EngineConfig) -> Result<Self> {
        let cluster = Arc::new(ClusterState::new());
        let health_monitor = Arc::new(NetworkHealthMonitor::new(cluster.clone()));

        info!(
            "Creating Mohawk Inference Engine for node: {}",
            config.node_id
        );

        Ok(Self {
            config,
            cluster,
            health_monitor,
            loaded_models: std::collections::HashMap::new(),
        })
    }

    /// Get the cluster state for node discovery
    pub fn cluster(&self) -> Arc<ClusterState> {
        self.cluster.clone()
    }

    /// Get the engine configuration
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Get the health monitor
    pub fn health_monitor(&self) -> Arc<NetworkHealthMonitor> {
        self.health_monitor.clone()
    }

    /// Load a model for inference
    pub fn load_model(&mut self, model_path: &str) -> Result<String> {
        info!("Loading model: {}", model_path);

        // TODO: Integrate with ONNX Runtime
        let model_info = ModelInfo {
            name: model_path.to_string(),
            layers: 0, // Will be populated by ONNX parser
            size_mb: 0.0,
            loaded_at: std::time::Instant::now(),
        };

        self.loaded_models
            .insert(model_path.to_string(), model_info);

        Ok(model_path.to_string())
    }

    /// Run inference on input data
    pub async fn infer(&self, model_id: &str, input: Vec<f32>) -> Result<Vec<f32>> {
        if !self.loaded_models.contains_key(model_id) {
            anyhow::bail!("Model not loaded: {}", model_id);
        }

        // TODO: Implement actual inference
        // For now, return dummy output
        Ok(vec![0.0; input.len()])
    }

    /// Start auto-discovery using Ghost-Link
    pub async fn start_discovery(&self) -> Result<()> {
        if !self.config.enable_ghostlink {
            warn!("Ghost-Link discovery disabled");
            return Ok(());
        }

        let interface = self.config.interface.as_deref().unwrap_or("auto");
        info!(
            "Starting Ghost-Link auto-discovery on interface: {}",
            interface
        );

        // TODO: Integrate with ghost-link crate for actual socket operations
        // This will broadcast join frames and listen for responses

        Ok(())
    }

    /// Get engine metrics
    pub fn get_metrics(&self) -> EngineMetrics {
        EngineMetrics {
            node_id: self.config.node_id.clone(),
            loaded_models: self.loaded_models.len(),
            cluster_nodes: self.cluster.node_count(),
            healthy_nodes: self.health_monitor.healthy_node_count(),
        }
    }
}

/// Engine metrics
#[derive(Debug, Clone)]
pub struct EngineMetrics {
    pub node_id: String,
    pub loaded_models: usize,
    pub cluster_nodes: usize,
    pub healthy_nodes: usize,
}
