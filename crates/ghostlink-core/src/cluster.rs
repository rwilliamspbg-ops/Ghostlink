//! Thread-Safe Cluster State with Metrics Collection
//! 
//! This module provides a lock-free, thread-safe cluster state tracking:
//! - Node capabilities (VRAM, system memory, compute capability)
//! - Live metrics (latency, delivery ratio, throughput)
//! - Fault detection and recovery

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::protocol::NodeResources;

/// Node status enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeStatus {
    /// Node is healthy and accepting traffic
    Active,
    /// Node is degraded (below threshold)
    Degraded,
    /// Node has failed or timed out
    Failed,
}

impl Default for NodeStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Metrics for a single node
#[derive(Clone, Debug, Default)]
pub struct NodeMetrics {
    /// Current status
    pub status: NodeStatus,
    /// Total VRAM in GB
    pub vram_gb: f32,
    /// System memory in GB
    pub system_memory_gb: f32,
    /// Compute capability
    pub compute_capability: String,
    /// GPU name/model
    pub gpu_name: Option<String>,
    
    /// Last heartbeat time
    pub last_heartbeat: Instant,
    /// Heartbeat interval threshold
    pub heartbeat_timeout: Duration,
    
    /// Average latency in microseconds
    pub avg_latency_us: f32,
    /// Minimum observed latency
    pub min_latency_us: f32,
    /// Maximum observed latency
    pub max_latency_us: f32,
    /// Number of latency samples
    pub latency_samples: u64,
    
    /// Delivery ratio (0.0 to 1.0)
    pub delivery_ratio: f32,
    /// Throughput in GB/s
    pub throughput_gbps: f32,
    
    /// Current used VRAM in GB
    pub used_vram_gb: f32,
    /// Available VRAM in GB
    pub available_vram_gb: f32,
    
    /// Number of layers streaming on this node
    pub streaming_layers: Option<(usize, usize)>,
}

impl NodeMetrics {
    /// Create new metrics for a node
    pub fn new(
        vram_gb: f32,
        system_memory_gb: f32,
        compute_capability: String,
        heartbeat_timeout: Duration,
    ) -> Self {
        Self {
            status: NodeStatus::Active,
            vram_gb,
            system_memory_gb,
            compute_capability,
            gpu_name: None,
            last_heartbeat: Instant::now(),
            heartbeat_timeout,
            avg_latency_us: 0.0,
            min_latency_us: f32::MAX,
            max_latency_us: 0.0,
            latency_samples: 0,
            delivery_ratio: 1.0,
            throughput_gbps: 0.0,
            used_vram_gb: 0.0,
            available_vram_gb: vram_gb,
            streaming_layers: None,
        }
    }
    
    /// Update metrics with new latency sample
    pub fn record_latency(&mut self, latency_us: f32) {
        if latency_us < self.min_latency_us {
            self.min_latency_us = latency_us;
        }
        if latency_us > self.max_latency_us {
            self.max_latency_us = latency_us;
        }
        
        self.latency_samples += 1;
        self.avg_latency_us = (self.avg_latency_us * (self.latency_samples - 1) + latency_us) / self.latency_samples as f32;
    }
    
    /// Update metrics with new delivery ratio sample
    pub fn record_delivery_ratio(&mut self, ratio: f32) {
        // Exponential moving average with alpha=0.1
        self.delivery_ratio = self.delivery_ratio * 0.9 + ratio * 0.1;
    }
    
    /// Update metrics with new throughput sample
    pub fn record_throughput(&mut self, throughput_gbps: f32) {
        // Exponential moving average with alpha=0.1
        self.throughput_gbps = self.throughput_gbps * 0.9 + throughput_gbps * 0.1;
    }
    
    /// Update used VRAM
    pub fn record_vram_usage(&mut self, used_vram_gb: f32) {
        self.used_vram_gb = used_vram_gb;
        self.available_vram_gb = self.vram_gb - used_vram_gb;
    }
    
    /// Set streaming layers
    pub fn set_streaming_layers(&mut self, start: usize, end: usize) {
        self.streaming_layers = Some((start, end));
    }
    
    /// Clear streaming layers
    pub fn clear_streaming_layers(&mut self) {
        self.streaming_layers = None;
    }
}

/// Cluster state with thread-safe metrics collection
#[derive(Debug)]
pub struct ClusterState {
    /// Map of node ID to resources and metrics
    nodes: Arc<Mutex<HashMap<String, NodeResources>>>,
    /// Map of node ID to live metrics
    metrics: Arc<Mutex<HashMap<String, NodeMetrics>>>,
    /// Last cluster update timestamp
    last_update: AtomicU64,
}

impl Default for ClusterState {
    fn default() -> Self {
        Self::new()
    }
}

impl ClusterState {
    /// Create new cluster state
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(Mutex::new(HashMap::new())),
            last_update: AtomicU64::new(0),
        }
    }
    
    /// Register a new node with the cluster
    pub fn register(&self, node: NodeResources) {
        let mut nodes = self.nodes.lock().unwrap();
        let mut metrics = self.metrics.lock().unwrap();
        
        // Check if node already exists
        if nodes.contains_key(&node.id) {
            // Update existing node with new resources
            let existing = nodes.get_mut(&node.id).unwrap();
            existing.vram_gb = node.vram_gb;
            existing.system_memory_gb = node.system_memory_gb;
            existing.compute_capability = node.compute_capability;
            
            // Initialize or update metrics
            if !metrics.contains_key(&node.id) {
                metrics.insert(
                    node.id.clone(),
                    NodeMetrics::new(
                        node.vram_gb,
                        node.system_memory_gb,
                        node.compute_capability,
                        Duration::from_secs(5), // Default heartbeat timeout
                    ),
                );
            } else {
                let existing_metrics = metrics.get_mut(&node.id).unwrap();
                existing_metrics.vram_gb = node.vram_gb;
                existing_metrics.system_memory_gb = node.system_memory_gb;
                existing_metrics.compute_capability = node.compute_capability;
                existing_metrics.heartbeat_timeout = Duration::from_secs(5);
            }
        } else {
            // New node
            nodes.insert(node.id.clone(), node);
            metrics.insert(
                node.id.clone(),
                NodeMetrics::new(
                    node.vram_gb,
                    node.system_memory_gb,
                    node.compute_capability,
                    Duration::from_secs(5),
                ),
            );
        }
        
        self.last_update.store(std::time::SystemTime::now()
            .duration_since(Instant::now())
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64, Ordering::Release);
    }
    
    /// Get all nodes
    pub fn nodes(&self) -> Vec<NodeResources> {
        let nodes = self.nodes.lock().unwrap();
        nodes.values().cloned().collect()
    }
    
    /// Get metrics for a specific node
    pub fn get_metrics(&self, node_id: &str) -> Option<NodeMetrics> {
        let metrics = self.metrics.lock().unwrap();
        metrics.get(node_id).cloned()
    }
    
    /// Update last heartbeat for a node
    pub fn update_heartbeat(&self, node_id: &str) {
        if let Some(mut metrics) = self.get_metrics_mut(node_id) {
            metrics.last_heartbeat = Instant::now();
        }
    }
    
    /// Get metrics mutable reference (for internal updates)
    fn get_metrics_mut(&self, node_id: &str) -> Option<NodeMetrics> {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.get_mut(node_id).cloned()
    }
    
    /// Check if a node has timed out
    pub fn check_heartbeat_timeout(&self, node_id: &str) -> bool {
        if let Some(metrics) = self.get_metrics(node_id) {
            let elapsed = Instant::now().duration_since(metrics.last_heartbeat);
            elapsed >= metrics.heartbeat_timeout
        } else {
            false
        }
    }
    
    /// Mark a node as failed due to timeout
    pub fn mark_failed(&self, node_id: &str) {
        if let Some(mut metrics) = self.get_metrics_mut(node_id) {
            metrics.status = NodeStatus::Failed;
        }
    }
    
    /// Recover a failed node
    pub fn recover_node(&self, node_id: &str) {
        if let Some(mut metrics) = self.get_metrics_mut(node_id) {
            metrics.status = NodeStatus::Active;
        }
    }
    
    /// Get all active nodes
    pub fn active_nodes(&self) -> Vec<NodeMetrics> {
        let metrics = self.metrics.lock().unwrap();
        metrics.values()
            .filter(|m| m.status == NodeStatus::Active)
            .cloned()
            .collect()
    }
    
    /// Get total cluster VRAM
    pub fn total_vram_gb(&self) -> f32 {
        let nodes = self.nodes.lock().unwrap();
        nodes.values()
            .map(|n| n.vram_gb)
            .sum()
    }
    
    /// Get total system memory
    pub fn total_system_memory_gb(&self) -> f32 {
        let nodes = self.nodes.lock().unwrap();
        nodes.values()
            .map(|n| n.system_memory_gb)
            .sum()
    }
}

/// Cluster health monitor with periodic checks
#[derive(Clone, Debug)]
pub struct ClusterHealthMonitor {
    cluster: Arc<ClusterState>,
    /// Health check interval
    check_interval: Duration,
}

impl ClusterHealthMonitor {
    /// Create new health monitor
    pub fn new(cluster: Arc<ClusterState>, check_interval: Duration) -> Self {
        Self {
            cluster,
            check_interval,
        }
    }
    
    /// Run health check on all nodes
    pub fn check_health(&self) {
        let failed_nodes: Vec<String> = self.cluster.nodes()
            .iter()
            .filter(|n| self.cluster.check_heartbeat_timeout(&n.id))
            .map(|n| n.id.clone())
            .collect();
        
        // Mark timed-out nodes as failed
        for node_id in &failed_nodes {
            self.cluster.mark_failed(node_id);
        }
        
        // Update last update timestamp
        self.cluster.last_update.store(
            std::time::SystemTime::now()
                .duration_since(Instant::now())
                .unwrap_or(Duration::ZERO)
                .as_millis() as u64,
            Ordering::Release
        );
    }
    
    /// Get health report
    pub fn health_report(&self) -> String {
        let active_count = self.cluster.active_nodes().len();
        let total_nodes = self.cluster.nodes().len();
        
        format!(
            "Cluster Health Report\n\
             =================\n\
             Active nodes: {}/{}\n\
             Total VRAM: {:.1} GB\n\
             System memory: {:.1} GB\n",
            active_count, total_nodes,
            self.cluster.total_vram_gb(),
            self.cluster.total_system_memory_gb()
        )
    }
    
    /// Run periodic health checks in background
    pub fn start_periodic_checks(&self) {
        let this = self.clone();
        
        // Spawn health check task
        std::thread::spawn(move || {
            loop {
                this.check_health();
                std::thread::sleep(this.check_interval);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn register_replaces_existing_nodes() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9".to_string()));
        cluster.register(NodeResources::new("node-a", 48.0, 128.0, "9.0".to_string()));

        assert_eq!(cluster.nodes().len(), 1);
        assert_eq!(cluster.nodes()[0].vram_gb, 48.0);
    }

    #[test]
    fn heartbeat_timeout_detection() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9".to_string()));
        
        // Simulate timeout by waiting longer than default heartbeat timeout
        thread::sleep(Duration::from_secs(6));
        
        assert!(cluster.check_heartbeat_timeout("node-a"));
    }

    #[test]
    fn health_monitor_reports_active_count() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9".to_string()));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6".to_string()));
        
        let monitor = ClusterHealthMonitor::new(cluster.clone(), Duration::from_secs(1));
        monitor.check_health();
        
        let report = monitor.health_report();
        assert!(report.contains("Active nodes: 2/2"));
    }

    #[test]
    fn metrics_record_latency() {
        let mut metrics = NodeMetrics::new(24.0, 64.0, "8.9".to_string(), Duration::from_secs(5));
        
        metrics.record_latency(1.0);
        assert_eq!(metrics.avg_latency_us, 1.0);
        assert_eq!(metrics.min_latency_us, 1.0);
        assert_eq!(metrics.max_latency_us, 1.0);
        
        metrics.record_latency(2.0);
        // EMA with alpha=0.1: (1.0 * 0.9) + 2.0 * 0.1 = 0.9 + 0.2 = 1.1
        assert_eq!(metrics.avg_latency_us, 1.1);
    }

    #[test]
    fn metrics_record_delivery_ratio() {
        let mut metrics = NodeMetrics::new(24.0, 64.0, "8.9".to_string(), Duration::from_secs(5));
        
        metrics.record_delivery_ratio(0.98);
        assert_eq!(metrics.delivery_ratio, 0.98);
        
        metrics.record_delivery_ratio(0.90);
        // EMA: 0.98 * 0.9 + 0.90 * 0.1 = 0.882 + 0.09 = 0.972
        assert_eq!(metrics.delivery_ratio, 0.972);
    }

    #[test]
    fn cluster_tracks_total_vram() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9".to_string()));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6".to_string()));
        
        assert_eq!(cluster.total_vram_gb(), 36.0);
    }
}
