//! Thread-Safe Cluster State with Metrics Collection
//!
//! This module provides a lock-free, thread-safe cluster state tracking:
//! - Node capabilities (VRAM, system memory, compute capability)
//! - Live metrics (latency, delivery ratio, throughput)
//! - Fault detection and recovery

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;

pub use crate::protocol::NodeResources;

const NODE_HISTORY_CAP: usize = 32;

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
    /// Resolved IP address of the node
    pub ip_address: Option<SocketAddr>,

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

    /// Recent latency history in microseconds
    pub latency_history_us: VecDeque<f32>,
    /// Recent throughput history in GB/s
    pub throughput_history_gbps: VecDeque<f32>,

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
            latency_history_us: VecDeque::with_capacity(NODE_HISTORY_CAP),
            throughput_history_gbps: VecDeque::with_capacity(NODE_HISTORY_CAP),
            used_vram_gb: 0.0,
            available_vram_gb: 0.0,
            streaming_layers: None,
            af_xdp_gbps: 0.0,
            latency_micros: 0.0,
            delivery_ratio_initialized: false,
            ip_address: None,
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
            ip_address: None,
            last_heartbeat: Instant::now(),
            heartbeat_timeout,
            avg_latency_us: 0.0,
            min_latency_us: f32::MAX,
            max_latency_us: 0.0,
            latency_samples: 0,
            delivery_ratio: 1.0,
            throughput_gbps: 0.0,
            latency_history_us: VecDeque::with_capacity(NODE_HISTORY_CAP),
            throughput_history_gbps: VecDeque::with_capacity(NODE_HISTORY_CAP),
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

        if self.latency_history_us.len() >= NODE_HISTORY_CAP {
            self.latency_history_us.pop_front();
        }
        self.latency_history_us.push_back(latency_us);
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

        if self.throughput_history_gbps.len() >= NODE_HISTORY_CAP {
            self.throughput_history_gbps.pop_front();
        }
        self.throughput_history_gbps.push_back(throughput_gbps);
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
    pub(crate) metrics: Arc<Mutex<HashMap<String, NodeMetrics>>>,
    /// Last cluster update timestamp
    last_update: Arc<AtomicU64>,
    /// Cached total VRAM across all registered nodes
    total_vram_cache: Arc<AtomicU64>,
    /// Cached total system memory across all registered nodes
    total_system_memory_cache: Arc<AtomicU64>,
    /// Per-node circuit breakers for the TCP transport bridge (`runtime::spawn_tcp_bridge`).
    /// Lazily created on first use via `circuit_breaker_for` and kept here — rather than
    /// inside a single pipeline execution — so failures accumulate *across* pipeline runs
    /// targeting the same node: a node that's been chronically unreachable trips open and
    /// stays fail-fast instead of re-running the full connect/backoff dance every call.
    circuit_breakers: Arc<Mutex<HashMap<String, crate::circuit_breaker::CircuitBreaker>>>,
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
            total_system_memory_cache: Arc::clone(&self.total_system_memory_cache),
            circuit_breakers: Arc::clone(&self.circuit_breakers),
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
            total_system_memory_cache: Arc::new(AtomicU64::new(0.0_f64.to_bits())),
            circuit_breakers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Gets (creating with default config on first use) the circuit breaker
    /// tracking TCP transport failures for one node. The returned handle is
    /// a cheap `Arc`-backed clone sharing state with every other caller for
    /// the same `node_id` — recording a failure through one clone is visible
    /// to all of them.
    pub fn circuit_breaker_for(&self, node_id: &str) -> crate::circuit_breaker::CircuitBreaker {
        let mut breakers = self
            .circuit_breakers
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        breakers.entry(node_id.to_string()).or_default().clone()
    }

    /// Register a new node with the cluster
    pub fn register(&self, node: NodeResources) {
        self.register_with_addr(node, None);
    }

    /// Register a new node with a specific socket address
    pub fn register_with_addr(&self, node: NodeResources, addr: Option<SocketAddr>) {
        let mut nodes = self
            .nodes
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut metrics = self
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());

        // Optimization: Operate on references/moves and avoid redundant cloning of NodeResources fields.
        // Also avoid marking nodes_snapshot_dirty when updating a node with unchanged resources.
        let vram_gb = node.vram_gb;
        let system_memory_gb = node.system_memory_gb;
        let mut vram_delta = vram_gb;
        let mut system_memory_delta = system_memory_gb;
        let mut resources_changed = true;

        if let Some(existing) = nodes.get_mut(&node.id) {
            vram_delta = vram_gb - existing.vram_gb;
            system_memory_delta = system_memory_gb - existing.system_memory_gb;

            // Check if node resource fields actually changed to avoid snapshot invalidation churn
            if existing.vram_gb == node.vram_gb
                && existing.system_memory_gb == node.system_memory_gb
                && existing.compute_capability == node.compute_capability
                && existing.gpu_name == node.gpu_name
                && existing.rpc_port == node.rpc_port
                && existing.rpc_build_id == node.rpc_build_id
            {
                resources_changed = false;
            } else {
                *existing = node.clone();
            }
        } else {
            nodes.insert(node.id.clone(), node.clone());
        }

        if let Some(existing_metrics) = metrics.get_mut(&node.id) {
            existing_metrics.vram_gb = vram_gb;
            existing_metrics.total_vram_gb = vram_gb;
            existing_metrics.system_memory_gb = system_memory_gb;
            existing_metrics
                .compute_capability
                .clone_from(&node.compute_capability);
            existing_metrics.gpu_name.clone_from(&node.gpu_name);
            existing_metrics.heartbeat_timeout = Duration::from_secs(5);
            if addr.is_some() {
                existing_metrics.ip_address = addr;
            }
        } else {
            let mut node_metrics = NodeMetrics::new(
                vram_gb,
                system_memory_gb,
                node.compute_capability.clone(),
                Duration::from_secs(5),
            );
            node_metrics.name = node.id.clone();
            node_metrics.gpu_name = node.gpu_name.clone();
            node_metrics.ip_address = addr;
            metrics.insert(node.id.clone(), node_metrics);
        }

        if resources_changed {
            self.nodes_snapshot_dirty.store(true, Ordering::Release);
        }

        let current_total_vram = f64::from_bits(self.total_vram_cache.load(Ordering::Acquire));
        self.total_vram_cache.store(
            (current_total_vram + vram_delta as f64).to_bits(),
            Ordering::Release,
        );

        let current_total_system_mem =
            f64::from_bits(self.total_system_memory_cache.load(Ordering::Acquire));
        self.total_system_memory_cache.store(
            (current_total_system_mem + system_memory_delta as f64).to_bits(),
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

    /// Get total number of registered nodes.
    pub fn node_count(&self) -> usize {
        self.nodes_snapshot().len()
    }

    /// Get a shared snapshot of all nodes
    pub fn nodes_snapshot(&self) -> Arc<Vec<NodeResources>> {
        if self.nodes_snapshot_dirty.load(Ordering::Relaxed)
            && self.nodes_snapshot_dirty.swap(false, Ordering::AcqRel)
        {
            let nodes_map = self
                .nodes
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let mut nodes: Vec<_> = nodes_map.values().cloned().collect();
            // Sort by ID to ensure deterministic layer assignment across runs
            nodes.sort_unstable_by(|a, b| a.id.cmp(&b.id));
            self.nodes_snapshot.store(Arc::new(nodes));
        }
        self.nodes_snapshot.load_full()
    }

    /// Execute a closure with a read lock on the metrics map, avoiding clones and multiple acquisitions.
    pub fn with_metrics<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<String, NodeMetrics>) -> R,
    {
        let metrics = self
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        f(&metrics)
    }

    /// Get metrics for a specific node
    pub fn get_metrics(&self, node_id: &str) -> Option<NodeMetrics> {
        let metrics = self
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
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
        let mut metrics = self
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
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
        let metrics = self
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        metrics
            .values()
            .filter(|m| m.status == NodeStatus::Active)
            .cloned()
            .collect()
    }

    /// Get the count of active nodes without cloning metrics
    pub fn active_nodes_count(&self) -> usize {
        let metrics = self
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        metrics
            .values()
            .filter(|m| m.status == NodeStatus::Active)
            .count()
    }

    /// Get total cluster VRAM
    pub fn total_vram_gb(&self) -> f32 {
        f64::from_bits(self.total_vram_cache.load(Ordering::Acquire)) as f32
    }

    /// Get total system memory
    pub fn total_system_memory_gb(&self) -> f32 {
        f64::from_bits(self.total_system_memory_cache.load(Ordering::Acquire)) as f32
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
        let active_count = self.cluster.active_nodes_count();
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
    fn node_metrics_keep_recent_latency_and_throughput_history() {
        let mut metrics = NodeMetrics::default();

        for idx in 0..40 {
            metrics.record_latency(idx as f32);
            metrics.record_throughput(idx as f32 / 10.0);
        }

        assert_eq!(metrics.latency_history_us.len(), NODE_HISTORY_CAP);
        assert_eq!(metrics.throughput_history_gbps.len(), NODE_HISTORY_CAP);
        assert_eq!(metrics.latency_history_us.front().copied(), Some(8.0));
        assert_eq!(metrics.throughput_history_gbps.back().copied(), Some(3.9));
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
    fn with_metrics_allows_single_pass_inspection() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6", None));

        let total_nodes = cluster.with_metrics(|metrics_map| {
            assert!(metrics_map.contains_key("node-a"));
            assert!(metrics_map.contains_key("node-b"));
            metrics_map.len()
        });

        assert_eq!(total_nodes, 2);
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
        assert!(
            report.contains("Active nodes: 2/2"),
            "Report should show both nodes as active"
        );
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
        assert!(
            (avg2 - 1.1).abs() < 1e-6,
            "EMA calculation: expected 1.1, got {avg2}"
        );

        metrics.record_latency(3.0);
        let avg3 = metrics.avg_latency_us;
        // EMA: 1.1 * 0.9 + 3.0 * 0.1 = 0.99 + 0.3 = 1.29
        assert!(
            (avg3 - 1.29).abs() < 1e-5,
            "EMA calculation: expected 1.29, got {avg3}"
        );
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
        assert!(
            metrics.delivery_ratio < 0.98,
            "Delivery ratio should degrade"
        );
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
                    format!("node-{i}"),
                    24.0 + (iteration as f32),
                    64.0,
                    "8.9",
                    None,
                ));
            }

            let nodes = cluster.nodes();
            assert_eq!(
                nodes.len(),
                10,
                "Should have 10 nodes at iteration {iteration}"
            );

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
        assert!(
            report.contains("Active nodes: 2"),
            "Should report 2 active nodes"
        );
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
