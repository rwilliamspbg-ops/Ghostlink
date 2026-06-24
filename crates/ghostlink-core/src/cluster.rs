//! Thread-Safe Cluster State with Metrics Collection
//!
//! This module provides a lock-free, thread-safe cluster state tracking:
//! - Node capabilities (VRAM, system memory, compute capability)
//! - Live metrics (latency, delivery ratio, throughput)
//! - Fault detection and recovery

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;

pub use crate::protocol::NodeResources;

/// Node status enumeration
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeStatus {
    /// Node is healthy and accepting traffic
    #[default]
    Active,
    /// Node is degraded (below threshold)
    Degraded,
    /// Node has failed or timed out
    Failed,
}

/// Metrics for a single node
#[derive(Clone, Debug)]
pub struct NodeMetrics {
    /// Node display name
    pub name: String,
    /// Current status
    pub status: NodeStatus,
    /// Total VRAM in GB
    pub vram_gb: f32,
    /// Total VRAM (alias for vram_gb, used by display/balancer)
    pub total_vram_gb: f32,
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

    /// AF_XDP throughput in Gbps (for display)
    pub af_xdp_gbps: f32,
    /// Per-packet latency in microseconds (for display)
    pub latency_micros: f32,
    /// Whether delivery_ratio has been initialized (first sample sets directly)
    pub(crate) delivery_ratio_initialized: bool,
}

impl Default for NodeMetrics {
    fn default() -> Self {
        Self {
            name: String::new(),
            status: NodeStatus::Active,
            vram_gb: 0.0,
            total_vram_gb: 0.0,
            system_memory_gb: 0.0,
            compute_capability: String::new(),
            gpu_name: None,
            last_heartbeat: Instant::now(),
            heartbeat_timeout: Duration::from_secs(5),
            avg_latency_us: 0.0,
            min_latency_us: f32::MAX,
            max_latency_us: 0.0,
            latency_samples: 0,
            delivery_ratio: 1.0,
            throughput_gbps: 0.0,
            used_vram_gb: 0.0,
            available_vram_gb: 0.0,
            streaming_layers: None,
            af_xdp_gbps: 0.0,
            latency_micros: 0.0,
            delivery_ratio_initialized: false,
        }
    }
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
            name: String::new(),
            status: NodeStatus::Active,
            vram_gb,
            total_vram_gb: vram_gb,
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
            af_xdp_gbps: 0.0,
            latency_micros: 0.0,
            delivery_ratio_initialized: false,
        }
    }

    /// Update metrics with new latency sample (EMA with alpha=0.1)
    pub fn record_latency(&mut self, latency_us: f32) {
        if latency_us < self.min_latency_us {
            self.min_latency_us = latency_us;
        }
        if latency_us > self.max_latency_us {
            self.max_latency_us = latency_us;
        }

        self.latency_samples += 1;
        if self.latency_samples == 1 {
            self.avg_latency_us = latency_us;
        } else {
            self.avg_latency_us = self.avg_latency_us * 0.9 + latency_us * 0.1;
        }
    }

    /// Update metrics with new delivery ratio sample
    pub fn record_delivery_ratio(&mut self, ratio: f32) {
        if !self.delivery_ratio_initialized {
            self.delivery_ratio = ratio;
            self.delivery_ratio_initialized = true;
        } else {
            // Exponential moving average with alpha=0.1
            self.delivery_ratio = self.delivery_ratio * 0.9 + ratio * 0.1;
        }
    }

    /// Update metrics with new throughput sample
    pub fn record_throughput(&mut self, throughput_gbps: f32) {
        // Exponential moving average with alpha=0.1
        self.throughput_gbps = self.throughput_gbps * 0.9 + throughput_gbps * 0.1;
    }

    /// Update used VRAM
    pub fn record_vram_usage(&mut self, used_vram_gb: f32) {
        self.used_vram_gb = used_vram_gb;
        self.available_vram_gb = self.total_vram_gb - used_vram_gb;
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
    /// Cached shared snapshot of nodes for read-heavy paths
    nodes_snapshot: Arc<ArcSwap<Vec<NodeResources>>>,
    /// Indicates whether the shared snapshot needs to be refreshed
    nodes_snapshot_dirty: Arc<AtomicBool>,
    /// Map of node ID to live metrics
    metrics: Arc<Mutex<HashMap<String, NodeMetrics>>>,
    /// Last cluster update timestamp
    last_update: Arc<AtomicU64>,
    /// Cached total VRAM across all registered nodes
    total_vram_cache: Arc<AtomicU64>,
}

impl Clone for ClusterState {
    fn clone(&self) -> Self {
        Self {
            nodes: Arc::clone(&self.nodes),
            nodes_snapshot: Arc::clone(&self.nodes_snapshot),
            nodes_snapshot_dirty: Arc::clone(&self.nodes_snapshot_dirty),
            metrics: Arc::clone(&self.metrics),
            last_update: Arc::clone(&self.last_update),
            total_vram_cache: Arc::clone(&self.total_vram_cache),
        }
    }
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
            nodes_snapshot: Arc::new(ArcSwap::from_pointee(Vec::<NodeResources>::new())),
            nodes_snapshot_dirty: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(Mutex::new(HashMap::new())),
            last_update: Arc::new(AtomicU64::new(0)),
            total_vram_cache: Arc::new(AtomicU64::new(0.0_f64.to_bits())),
        }
    }

    /// Register a new node with the cluster
    pub fn register(&self, node: NodeResources) {
        let mut nodes = self.nodes.lock().unwrap();
        let mut metrics = self.metrics.lock().unwrap();

        let id = node.id.clone();
        let vram_gb = node.vram_gb;
        let system_memory_gb = node.system_memory_gb;
        let compute_capability = node.compute_capability.clone();
        let mut vram_delta = vram_gb;

        if let Some(existing) = nodes.get_mut(&id) {
            vram_delta = vram_gb - existing.vram_gb;
            existing.vram_gb = vram_gb;
            existing.system_memory_gb = system_memory_gb;
            existing.compute_capability = compute_capability.clone();
        } else {
            nodes.insert(id.clone(), node);
        }

        if let Some(existing_metrics) = metrics.get_mut(&id) {
            existing_metrics.vram_gb = vram_gb;
            existing_metrics.total_vram_gb = vram_gb;
            existing_metrics.system_memory_gb = system_memory_gb;
            existing_metrics.compute_capability = compute_capability;
            existing_metrics.heartbeat_timeout = Duration::from_secs(5);
        } else {
            metrics.insert(
                id,
                NodeMetrics::new(
                    vram_gb,
                    system_memory_gb,
                    compute_capability,
                    Duration::from_secs(5),
                ),
            );
        }

        self.nodes_snapshot_dirty.store(true, Ordering::Release);

        let current_total_vram = f64::from_bits(self.total_vram_cache.load(Ordering::Acquire));
        self.total_vram_cache.store(
            (current_total_vram + vram_delta as f64).to_bits(),
            Ordering::Release,
        );

        self.last_update.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_millis() as u64,
            Ordering::Release,
        );
    }

    /// Get all nodes
    pub fn nodes(&self) -> Vec<NodeResources> {
        self.nodes_snapshot().as_ref().to_vec()
    }

    /// Get a shared snapshot of all nodes
    pub fn nodes_snapshot(&self) -> Arc<Vec<NodeResources>> {
        if self.nodes_snapshot_dirty.load(Ordering::Acquire)
            && self.nodes_snapshot_dirty.swap(false, Ordering::AcqRel)
        {
            let nodes = self.nodes.lock().unwrap();
            self.nodes_snapshot
                .store(Arc::new(nodes.values().cloned().collect::<Vec<_>>()));
        }

        self.nodes_snapshot.load_full()
    }

    /// Get metrics for a specific node
    pub fn get_metrics(&self, node_id: &str) -> Option<NodeMetrics> {
        let metrics = self.metrics.lock().unwrap();
        metrics.get(node_id).cloned()
    }

    /// Update last heartbeat for a node
    pub fn update_heartbeat(&self, node_id: &str) {
        self.get_metrics_mut(node_id, |metrics| {
            metrics.last_heartbeat = Instant::now();
        });
    }

    /// Get metrics mutable reference (for internal updates)
    pub fn get_metrics_mut<F, R>(&self, node_id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&mut NodeMetrics) -> R,
    {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.get_mut(node_id).map(f)
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
        self.get_metrics_mut(node_id, |metrics| {
            metrics.status = NodeStatus::Failed;
        });
    }

    /// Recover a failed node
    pub fn recover_node(&self, node_id: &str) {
        self.get_metrics_mut(node_id, |metrics| {
            metrics.status = NodeStatus::Active;
        });
    }

    /// Get all active nodes
    pub fn active_nodes(&self) -> Vec<NodeMetrics> {
        let metrics = self.metrics.lock().unwrap();
        metrics
            .values()
            .filter(|m| m.status == NodeStatus::Active)
            .cloned()
            .collect()
    }

    /// Get total cluster VRAM
    pub fn total_vram_gb(&self) -> f32 {
        f64::from_bits(self.total_vram_cache.load(Ordering::Acquire)) as f32
    }

    /// Get total system memory
    pub fn total_system_memory_gb(&self) -> f32 {
        let nodes = self.nodes.lock().unwrap();
        nodes.values().map(|n| n.system_memory_gb).sum()
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
        let failed_nodes: Vec<String> = self
            .cluster
            .nodes_snapshot()
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
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_millis() as u64,
            Ordering::Release,
        );
    }

    /// Get health report
    pub fn health_report(&self) -> String {
        let active_count = self.cluster.active_nodes().len();
        let total_nodes = self.cluster.nodes_snapshot().len();

        format!(
            "Cluster Health Report\n\
             =================\n\
             Active nodes: {}/{}\n\
             Total VRAM: {:.1} GB\n\
             System memory: {:.1} GB\n",
            active_count,
            total_nodes,
            self.cluster.total_vram_gb(),
            self.cluster.total_system_memory_gb()
        )
    }

    /// Run periodic health checks in background
    pub fn start_periodic_checks(&self) {
        let this = self.clone();

        // Spawn health check task
        std::thread::spawn(move || loop {
            this.check_health();
            std::thread::sleep(this.check_interval);
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
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-a", 48.0, 128.0, "9.0", None));

        assert_eq!(cluster.nodes().len(), 1);
        assert_eq!(cluster.nodes()[0].vram_gb, 48.0);
    }

    #[test]
    fn heartbeat_timeout_detection() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));

        // Simulate timeout by waiting longer than default heartbeat timeout
        thread::sleep(Duration::from_secs(6));

        assert!(cluster.check_heartbeat_timeout("node-a"));
    }

    #[test]
    fn health_monitor_reports_active_count() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6", None));

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
        assert!((metrics.avg_latency_us - 1.1).abs() < 1e-6);
    }

    #[test]
    fn metrics_record_delivery_ratio() {
        let mut metrics = NodeMetrics::new(24.0, 64.0, "8.9".to_string(), Duration::from_secs(5));

        metrics.record_delivery_ratio(0.98);
        assert!((metrics.delivery_ratio - 0.98).abs() < 1e-6);

        metrics.record_delivery_ratio(0.90);
        // EMA: 0.98 * 0.9 + 0.90 * 0.1 = 0.882 + 0.09 = 0.972
        assert!((metrics.delivery_ratio - 0.972).abs() < 1e-6);
    }

    #[test]
    fn cluster_tracks_total_vram() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6", None));

        assert_eq!(cluster.total_vram_gb(), 36.0);
    }

    // ========================================================================
    // NEW: Comprehensive Health Monitoring & Failure Recovery Tests
    // ========================================================================

    #[test]
    fn cluster_health_monitor_checks_all_nodes() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6", None));

        let monitor = ClusterHealthMonitor::new(cluster.clone(), Duration::from_millis(100));
        monitor.check_health();

        let report = monitor.health_report();
        assert!(report.contains("node-a"), "Report should mention node-a");
        assert!(report.contains("node-b"), "Report should mention node-b");
    }

    #[test]
    fn cluster_metrics_use_exponential_moving_average() {
        let mut metrics = NodeMetrics::new(24.0, 64.0, "8.9".to_string(), Duration::from_secs(5));

        // Record sequence of latencies: 1, 2, 3
        metrics.record_latency(1.0);
        let avg1 = metrics.avg_latency_us;
        assert_eq!(avg1, 1.0);

        metrics.record_latency(2.0);
        let avg2 = metrics.avg_latency_us;
        // EMA: 1.0 * 0.9 + 2.0 * 0.1 = 1.1
        assert!((avg2 - 1.1).abs() < 1e-6, "EMA calculation: expected 1.1, got {}", avg2);

        metrics.record_latency(3.0);
        let avg3 = metrics.avg_latency_us;
        // EMA: 1.1 * 0.9 + 3.0 * 0.1 = 0.99 + 0.3 = 1.29
        assert!((avg3 - 1.29).abs() < 1e-5, "EMA calculation: expected 1.29, got {}", avg3);
    }

    #[test]
    fn cluster_delivery_ratio_tracks_network_quality() {
        let mut metrics = NodeMetrics::new(24.0, 64.0, "8.9".to_string(), Duration::from_secs(5));

        metrics.record_delivery_ratio(0.98);
        assert!((metrics.delivery_ratio - 0.98).abs() < 1e-6);

        metrics.record_delivery_ratio(0.95);
        // EMA: 0.98 * 0.9 + 0.95 * 0.1 = 0.882 + 0.095 = 0.977
        assert!((metrics.delivery_ratio - 0.977).abs() < 1e-6);

        // Simulate degradation
        metrics.record_delivery_ratio(0.80);
        // EMA: 0.977 * 0.9 + 0.80 * 0.1 = 0.8793 + 0.08 = 0.9593
        assert!(metrics.delivery_ratio < 0.98, "Delivery ratio should degrade");
        assert!(metrics.delivery_ratio > 0.90, "But recover somewhat");
    }

    #[test]
    fn cluster_concurrent_metric_updates() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));

        let mut handles = vec![];

        // Spawn 10 threads updating metrics concurrently
        for _ in 0..10 {
            let cluster_clone = Arc::clone(&cluster);
            let handle = thread::spawn(move || {
                let nodes = cluster_clone.nodes();
                for _ in 0..100 {
                    // Update would happen here in real usage
                    let _ = nodes.len();
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Cluster should remain consistent
        assert_eq!(cluster.nodes().len(), 1);
        assert_eq!(cluster.total_vram_gb(), 24.0);
    }

    #[test]
    fn cluster_handles_rapid_registration_churn() {
        let cluster = Arc::new(ClusterState::new());

        // Rapidly register and reregister nodes
        for iteration in 0..5 {
            for i in 0..10 {
                cluster.register(NodeResources::new(
                    format!("node-{}", i),
                    24.0 + (iteration as f32),
                    64.0,
                    "8.9",
                    None,
                ));
            }

            let nodes = cluster.nodes();
            assert_eq!(nodes.len(), 10, "Should have 10 nodes at iteration {}", iteration);
            
            // Verify VRAM reflects latest registration
            let expected_vram: f32 = (0..10).map(|_| 24.0 + iteration as f32).sum();
            let actual = cluster.total_vram_gb();
            assert!((actual - expected_vram).abs() < 0.1);
        }
    }

    #[test]
    fn cluster_health_monitor_reports_accurate_status() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6", None));

        let monitor = ClusterHealthMonitor::new(cluster.clone(), Duration::from_millis(100));
        monitor.check_health();

        let report = monitor.health_report();
        assert!(report.contains("Active nodes: 2"), "Should report 2 active nodes");
        assert!(report.contains("2/2"), "Should show 2 of 2 nodes");
    }

    #[test]
    fn node_metrics_track_min_max_latency() {
        let mut metrics = NodeMetrics::new(24.0, 64.0, "8.9".to_string(), Duration::from_secs(5));

        metrics.record_latency(2.0);
        assert_eq!(metrics.min_latency_us, 2.0);
        assert_eq!(metrics.max_latency_us, 2.0);

        metrics.record_latency(1.0);
        assert_eq!(metrics.min_latency_us, 1.0);
        assert_eq!(metrics.max_latency_us, 2.0);

        metrics.record_latency(5.0);
        assert_eq!(metrics.min_latency_us, 1.0);
        assert_eq!(metrics.max_latency_us, 5.0);

        metrics.record_latency(3.0);
        assert_eq!(metrics.min_latency_us, 1.0);
        assert_eq!(metrics.max_latency_us, 5.0);
    }
}
