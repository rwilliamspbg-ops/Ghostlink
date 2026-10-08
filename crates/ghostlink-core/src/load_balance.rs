//! Tensor Distribution and Load Balancing for Ghost-Link Cluster
//!
//! This module provides:
//! - Tensor distribution across nodes based on VRAM capacity
//! - Dynamic load shedding
//! - Deadlock prevention with timeout
//! - Dynamic reconfiguration on hardware changes via `SystemProfileWatcher`.

use crate::accelerator::ExecutionBackend;
use crate::cluster::ClusterState;
use crate::host::{AccelerationMode, RuntimeProfile};
use crate::system_profile::SystemProfile;
use crate::watcher::ProfileChange;
use std::sync::Arc;

/// Load balancing configuration
#[derive(Clone, Copy, Debug)]
pub struct LoadBalanceConfig {
    /// Maximum time to wait for lock acquisition (microseconds)
    pub lock_timeout_us: u64,
    /// Minimum load threshold for rebalancing
    pub min_load_threshold: f32,
    /// Maximum layers per node for single assignment
    pub max_layers_per_assignment: usize,
    /// Maximum concurrent rebalance transfers.
    pub max_concurrent_rebalances: usize,
}

impl Default for LoadBalanceConfig {
    fn default() -> Self {
        Self {
            lock_timeout_us: 1000, // 1ms
            min_load_threshold: 1.25,
            max_layers_per_assignment: 100,
            max_concurrent_rebalances: 1,
        }
    }
}

impl LoadBalanceConfig {
    /// Auto-tune load balancing thresholds from the detected runtime profile.
    pub fn autotuned(profile: &RuntimeProfile) -> Self {
        let base_timeout = match profile.acceleration_mode {
            AccelerationMode::Gpu => 500,
            AccelerationMode::Avx512 => 750,
            _ => 1000,
        };
        // skew_ratio (max_available / min_available) is mathematically always
        // >= 1.0, so a threshold below 1.0 here made rebalance() return true
        // unconditionally for this tier — including the all-zero-VRAM CPU-only
        // case (skew_ratio pinned to exactly 1.0), which a threshold of 1.0
        // itself would still have wrongly matched. Must stay strictly > 1.0.
        let min_load_threshold = match profile.acceleration_mode {
            AccelerationMode::Gpu => 1.15,
            AccelerationMode::Avx512 => 1.05,
            _ => 1.02,
        };
        let max_layers_per_assignment = profile.recommended_workers.max(1) * 2;

        Self {
            lock_timeout_us: base_timeout,
            min_load_threshold,
            max_layers_per_assignment,
            max_concurrent_rebalances: profile.recommended_workers.clamp(1, 8),
        }
    }
}

/// Tensor slice specification
#[derive(Clone, Debug)]
pub struct TensorSlice {
    /// Layer index range for this tensor slice
    pub layer_range: (usize, usize), // (start, end) exclusive
    /// Size in GB
    pub size_gb: f32,
    /// Number of weights
    pub num_weights: u32,
}

impl TensorSlice {
    /// Create new tensor slice
    pub fn new(layer_range: (usize, usize), size_gb: f32) -> Self {
        Self {
            layer_range,
            size_gb,
            num_weights: 0,
        }
    }
}

/// Load distribution plan across nodes
#[derive(Clone, Debug)]
pub struct LoadDistributionPlan {
    /// Distribution of tensor slices per node
    pub distributions: Vec<(String, Vec<TensorSlice>)>,
    /// Total layers in plan
    pub total_layers: usize,
    /// Nodes participating
    pub participating_nodes: Vec<String>,
}

impl LoadDistributionPlan {
    /// Create new distribution plan
    pub fn new(distributions: Vec<(String, Vec<TensorSlice>)>, total_layers: usize) -> Self {
        let participating_nodes = distributions
            .iter()
            .map(|(node_id, _)| node_id.clone())
            .collect();

        Self {
            distributions,
            total_layers,
            participating_nodes,
        }
    }

    /// Generate human-readable plan summary
    pub fn summary(&self) -> String {
        let mut output = String::from("Load Distribution Plan\n");
        output.push_str("======================\n\n");

        for (node_id, slices) in &self.distributions {
            output.push_str(&format!("Node: {node_id}\n"));

            let total_size_gb: f32 = slices.iter().map(|s| s.size_gb).sum();

            output.push_str(&format!("  Total size: {total_size_gb:.1} GB\n"));
            output.push_str(&format!(
                "  Layers: {}-{}\n",
                slices.first().map(|s| s.layer_range.0).unwrap_or(0),
                slices.last().map(|s| s.layer_range.1).unwrap_or(0)
            ));
            output.push('\n');
        }

        output.push_str(&format!("Total layers: {}\n", self.total_layers));
        output.push_str(&format!("Nodes: {}\n", self.participating_nodes.join(", ")));

        output
    }
}

/// Load balancer for tensor distribution
#[derive(Clone, Debug)]
pub struct LoadBalancer {
    /// Cluster state
    cluster: Arc<ClusterState>,
    /// Configuration (behind Mutex for atomic re-tune from watcher)
    config: Arc<std::sync::Mutex<LoadBalanceConfig>>,
}

impl LoadBalancer {
    /// Create new load balancer
    pub fn new(cluster: Arc<ClusterState>, config: LoadBalanceConfig) -> Self {
        Self {
            cluster,
            config: Arc::new(std::sync::Mutex::new(config)),
        }
    }

    /// Create a load balancer with configuration derived from the local runtime profile.
    pub fn with_runtime_profile(cluster: Arc<ClusterState>, profile: &RuntimeProfile) -> Self {
        Self::new(cluster, LoadBalanceConfig::autotuned(profile))
    }

    /// Access the current configuration.
    pub fn config(&self) -> LoadBalanceConfig {
        self.config
            .lock()
            .ok()
            .as_deref()
            .copied()
            .unwrap_or_default()
    }

    /// Distribute tensor layers across nodes based on VRAM capacity.
    ///
    /// Optimized using cursor-based index traversal instead of costly vector draining
    /// and shifting elements, improving complexity from O(N^2) to O(N).
    /// Further optimized by sorting references to NodeResources instead of cloning them,
    /// and dynamically avoiding layer cloning/sorting if they are already in sequential order.
    pub fn distribute_layers(
        &self,
        layers: &[crate::planning::LayerSpec],
    ) -> Result<LoadDistributionPlan, String> {
        self.distribute_layers_internal(layers, None)
    }

    fn distribute_layers_internal(
        &self,
        layers: &[crate::planning::LayerSpec],
        max_layers_per_slice: Option<usize>,
    ) -> Result<LoadDistributionPlan, String> {
        let nodes_snapshot = self.cluster.nodes_snapshot();
        if nodes_snapshot.is_empty() {
            return Err("no nodes available".into());
        }

        // OPTIMIZATION: Use `sort_unstable_by` when sorting node references by VRAM capacity.
        // `sort_unstable_by` avoids allocating scratch memory buffers and executing stability overhead,
        // reducing sorting latency on node resource references during greedy load distribution.
        let mut sorted_nodes: Vec<&crate::protocol::NodeResources> =
            nodes_snapshot.iter().collect();
        sorted_nodes.sort_unstable_by(|a, b| {
            b.vram_gb
                .partial_cmp(&a.vram_gb)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Zero-copy shortcut: check if layers are already sorted (nearly always true).
        // If already sorted, borrow instead of allocating/sorting a new vector.
        let is_sorted = layers.windows(2).all(|w| w[0].index <= w[1].index);
        let sorted_layers: std::borrow::Cow<'_, [crate::planning::LayerSpec]> = if is_sorted {
            std::borrow::Cow::Borrowed(layers)
        } else {
            let mut all_layers = layers.to_vec();
            all_layers.sort_by_key(|l| l.index);
            std::borrow::Cow::Owned(all_layers)
        };
        let total_layer_count = sorted_layers.len();

        // Greedy assignment: assign contiguous layers to nodes based on VRAM using O(1) indices
        let mut distributions = Vec::with_capacity(sorted_nodes.len());
        let mut participating_nodes = Vec::with_capacity(sorted_nodes.len());
        let mut current_layer_idx = 0usize;
        let slice_limit = max_layers_per_slice.unwrap_or(usize::MAX).max(1);

        for node in &sorted_nodes {
            if current_layer_idx >= sorted_layers.len() {
                break;
            }

            let mut used_vram = 0.0f32;
            let start_idx = current_layer_idx;
            let mut end_idx = current_layer_idx;

            for layer in &sorted_layers[current_layer_idx..] {
                if used_vram + layer.vram_gb > node.vram_gb {
                    break;
                }
                end_idx += 1;
                used_vram += layer.vram_gb;
            }

            if end_idx > start_idx {
                let total_node_layers = end_idx - start_idx;
                let node_id = node.id.clone();
                participating_nodes.push(node_id.clone());

                if total_node_layers <= slice_limit {
                    let start_layer = sorted_layers[start_idx].index;
                    let end_layer = sorted_layers[end_idx - 1].index + 1;
                    let slice = TensorSlice::new((start_layer, end_layer), used_vram);
                    distributions.push((node_id, vec![slice]));
                } else {
                    let chunks_count = total_node_layers.div_ceil(slice_limit);
                    let mut slices = Vec::with_capacity(chunks_count);
                    let avg_size = used_vram / total_node_layers as f32;

                    let mut chunk_start_idx = start_idx;
                    while chunk_start_idx < end_idx {
                        let chunk_end_idx = (chunk_start_idx + slice_limit).min(end_idx);
                        let chunk_layers = chunk_end_idx - chunk_start_idx;
                        let start_layer = sorted_layers[chunk_start_idx].index;
                        let end_layer = sorted_layers[chunk_end_idx - 1].index + 1;
                        slices.push(TensorSlice::new(
                            (start_layer, end_layer),
                            avg_size * chunk_layers as f32,
                        ));
                        chunk_start_idx = chunk_end_idx;
                    }
                    distributions.push((node_id, slices));
                }

                current_layer_idx = end_idx;
            }
        }

        if current_layer_idx >= sorted_layers.len() {
            Ok(LoadDistributionPlan {
                distributions,
                total_layers: total_layer_count,
                participating_nodes,
            })
        } else {
            Err(format!(
                "insufficient VRAM: {} layers remain",
                sorted_layers.len() - current_layer_idx
            ))
        }
    }

    /// Distribute layers and directly chunk large node allocations to match worker parallelism.
    ///
    /// OPTIMIZATION: Delegates directly to `distribute_layers_internal` with `Some(max_layers_per_slice)`
    /// to generate chunked tensor slices in a single pass during greedy assignment. This completely
    /// eliminates intermediate single-slice vector allocations (`vec![slice]`), avoids intermediate
    /// `LoadDistributionPlan` construction, and bypasses multi-pass post-processing traversals.
    pub fn distribute_layers_with_runtime_profile(
        &self,
        layers: &[crate::planning::LayerSpec],
        profile: &RuntimeProfile,
    ) -> Result<LoadDistributionPlan, String> {
        let cfg = self.config();
        let backend = ExecutionBackend::from_runtime_profile(profile);
        let vector_bias = (backend.vector_width_bits / 128).max(1);
        let max_layers_per_slice = cfg
            .max_layers_per_assignment
            .min(profile.recommended_workers.max(1).saturating_mul(2))
            .min(backend.preferred_batch_size / vector_bias)
            .max(1);

        self.distribute_layers_internal(layers, Some(max_layers_per_slice))
    }

    /// Rebalance load based on current node metrics
    pub fn rebalance(&self) -> bool {
        let cfg = self.config();

        let (min_available, max_available, has_nodes) = {
            let metrics = self
                .cluster
                .metrics
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());

            let mut min_val = f32::MAX;
            let mut max_val = 0.0_f32;
            let mut count = 0usize;

            for node in metrics.values() {
                if node.status == crate::cluster::NodeStatus::Active {
                    min_val = min_val.min(node.available_vram_gb.max(0.0));
                    max_val = max_val.max(node.available_vram_gb.max(0.0));
                    count += 1;
                }
            }
            (min_val, max_val, count > 0)
        };

        if !has_nodes {
            return false;
        }

        // A large spread in available VRAM indicates load imbalance.
        let skew_ratio = if min_available <= f32::EPSILON {
            if max_available > 0.0 {
                f32::INFINITY
            } else {
                1.0
            }
        } else {
            max_available / min_available
        };

        if skew_ratio >= cfg.min_load_threshold {
            tracing::info!(
                "Detected load skew ratio {:.2} (threshold {:.2})",
                skew_ratio,
                cfg.min_load_threshold
            );
            return true;
        }

        false
    }

    /// Shed load from overloaded nodes to underloaded ones.
    ///
    /// OPTIMIZATION: Collects overloaded and underloaded node candidates in a single-pass loop
    /// over cluster metrics under lock, completely eliminating intermediate `active_nodes` vector
    /// allocations and inlining best target lookup.
    pub fn shed_load(&self) -> Vec<(String, String)> {
        let cfg = self.config();
        let mut transfers: Vec<(String, String)> = Vec::new();

        let metrics = self
            .cluster
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());

        let mut overloaded_nodes: Vec<&String> = Vec::new();
        let mut underloaded_nodes: Vec<(&String, f32)> = Vec::new();

        for m in metrics.values() {
            if m.status == crate::cluster::NodeStatus::Active {
                if m.available_vram_gb < m.total_vram_gb * 0.2 {
                    overloaded_nodes.push(&m.name);
                } else if m.available_vram_gb > m.total_vram_gb * 0.5 {
                    underloaded_nodes.push((&m.name, m.available_vram_gb));
                }
            }
        }

        for &overloaded_name in &overloaded_nodes {
            if transfers.len() >= cfg.max_concurrent_rebalances {
                break;
            }

            let mut best_target: Option<&String> = None;
            let mut max_available = 0.0f32;

            for &(name, available) in &underloaded_nodes {
                if name != overloaded_name && available > max_available {
                    max_available = available;
                    best_target = Some(name);
                }
            }

            if let Some(target_name) = best_target {
                transfers.push((overloaded_name.clone(), target_name.clone()));
            }
        }

        transfers
    }

    // ------------------------------------------------------------------
    // SystemProfileWatcher integration
    // ------------------------------------------------------------------

    /// Re-tune load-balancing thresholds from a system profile detected at runtime.
    pub fn reconfigure_from_system_profile(&self, profile: &SystemProfile) {
        let rp: RuntimeProfile = profile.into();
        if let Ok(mut guard) = self.config.lock() {
            *guard = LoadBalanceConfig::autotuned(&rp);
        }
    }

    /// Subscribe to a `SystemProfileWatcher`'s broadcast channel and
    /// automatically re-tune whenever hardware changes are detected.
    ///
    /// Returns a `JoinHandle` that can be cancelled by dropping it.
    pub fn subscribe_to_watcher(
        self: &Arc<Self>,
        watcher: &crate::watcher::SystemProfileWatcher,
    ) -> tokio::task::JoinHandle<()> {
        let this = self.clone();
        let mut rx = watcher.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let ProfileChange::Updated(profile) = event {
                    this.reconfigure_from_system_profile(&profile);
                    tracing::info!(
                        "load balancer re-tuned from profile: {} workers, {:?} accel",
                        profile.recommended_workers,
                        profile.acceleration_mode,
                    );
                }
            }
        })
    }

    /// Distribute layers with deadlock prevention
    pub fn distribute_with_deadlock_prevention(
        &self,
        layers: &[crate::planning::LayerSpec],
    ) -> Result<LoadDistributionPlan, String> {
        let cfg = self.config();
        // Use timeout-based acquisition to prevent deadlocks
        let start_time = std::time::Instant::now();

        while start_time.elapsed().as_micros() < cfg.lock_timeout_us as u128 {
            match self.distribute_layers(layers) {
                Ok(plan) => return Ok(plan),
                Err(_) => {
                    // Retry with backoff
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }

        Err("deadlock prevention timeout".into())
    }
}

/// Split large tensor slices in an existing distribution plan into smaller contiguous chunks.
pub fn chunk_distribution_plan(
    plan: LoadDistributionPlan,
    max_layers_per_slice: usize,
) -> LoadDistributionPlan {
    let slice_limit = max_layers_per_slice.max(1);

    // Fast path: Avoid rebuilding distribution tuples and vector allocations if no slice exceeds the limit.
    let needs_chunking = plan.distributions.iter().any(|(_, slices)| {
        slices
            .iter()
            .any(|s| s.layer_range.1.saturating_sub(s.layer_range.0) > slice_limit)
    });
    if !needs_chunking {
        return plan;
    }

    let distributions = plan
        .distributions
        .into_iter()
        .map(|(node_id, slices)| {
            let estimated_chunks = slices
                .iter()
                .map(|slice| {
                    let total_layers = slice.layer_range.1.saturating_sub(slice.layer_range.0);
                    total_layers.div_ceil(slice_limit).max(1)
                })
                .sum();
            let mut chunked = Vec::with_capacity(estimated_chunks);
            for slice in slices {
                let total_layers = slice.layer_range.1.saturating_sub(slice.layer_range.0);
                if total_layers <= slice_limit {
                    chunked.push(slice);
                    continue;
                }

                let avg_size = if total_layers == 0 {
                    0.0
                } else {
                    slice.size_gb / total_layers as f32
                };
                let mut start = slice.layer_range.0;
                while start < slice.layer_range.1 {
                    let end = (start + slice_limit).min(slice.layer_range.1);
                    let layers_in_chunk = end - start;
                    let mut chunk =
                        TensorSlice::new((start, end), avg_size * layers_in_chunk as f32);
                    chunk.num_weights = slice.num_weights;
                    chunked.push(chunk);
                    start = end;
                }
            }
            (node_id, chunked)
        })
        .collect();

    // Optimize: Reuse existing owned participating_nodes vector from input plan rather than re-cloning string IDs from distributions.
    LoadDistributionPlan {
        distributions,
        total_layers: plan.total_layers,
        participating_nodes: plan.participating_nodes,
    }
}

/// Load statistics collector
#[derive(Clone, Debug, Default)]
pub struct LoadStats {
    /// Total layers distributed
    pub total_layers_distributed: usize,
    /// Total VRAM used across cluster
    pub total_vram_used_gb: f32,
    /// Average load balance ratio (0.0 to 1.0)
    pub avg_load_balance_ratio: f32,
    /// Number of rebalancing operations
    pub rebalancing_count: usize,
    /// Whether avg_load_balance_ratio has received its first sample yet.
    balance_ratio_initialized: bool,
}

impl LoadStats {
    /// Create new statistics collector
    pub fn new() -> Self {
        Self::default()
    }

    /// Record layer distribution
    pub fn record_distribution(&mut self, num_layers: usize, vram_gb: f32) {
        self.total_layers_distributed += num_layers;
        self.total_vram_used_gb += vram_gb;
    }

    /// Update load balance ratio
    pub fn update_balance_ratio(&mut self, ratio: f32) {
        if !self.balance_ratio_initialized {
            // First call - initialize directly
            self.avg_load_balance_ratio = ratio;
            self.balance_ratio_initialized = true;
        } else {
            // EMA with alpha=0.1
            self.avg_load_balance_ratio = self.avg_load_balance_ratio * 0.9 + ratio * 0.1;
        }
    }

    /// Record rebalancing operation
    pub fn record_rebalancing(&mut self) {
        self.rebalancing_count += 1;
    }

    /// Get load report
    pub fn report(&self) -> String {
        format!(
            "Load Statistics\n\
             ==========\n\
             Total layers distributed: {}\n\
             Total VRAM used: {:.1} GB\n\
             Avg load balance ratio: {:.2}\n\
             Rebalancing operations: {}",
            self.total_layers_distributed,
            self.total_vram_used_gb,
            self.avg_load_balance_ratio,
            self.rebalancing_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::ClusterState;
    use crate::host::AccelerationMode;
    use crate::protocol::NodeResources;
    use std::sync::Arc;

    fn sample_layers(count: usize, vram_gb: f32) -> Vec<crate::planning::LayerSpec> {
        (0..count)
            .map(|index| crate::planning::LayerSpec {
                index,
                vram_gb,
                num_weights: 0,
            })
            .collect()
    }

    fn sample_nodes(count: usize, base_vram: f32) -> Vec<NodeResources> {
        (0..count)
            .map(|i| {
                NodeResources::new(
                    format!("node-{i}"),
                    base_vram + (i as f32 * 6.0),
                    64.0,
                    "8.9".to_string(),
                    None,
                )
            })
            .collect()
    }

    #[test]
    fn load_balancer_distributes_layers() {
        let cluster = ClusterState::new();
        for node in sample_nodes(2, 24.0) {
            cluster.register(node);
        }

        let layers = sample_layers(33, 1.0);
        let balancer = LoadBalancer::new(Arc::new(cluster), LoadBalanceConfig::default());

        let plan = balancer.distribute_layers(&layers).unwrap();
        assert_eq!(plan.total_layers, 33);
    }

    #[test]
    fn load_balancer_generates_summary() {
        let cluster = ClusterState::new();
        for node in sample_nodes(2, 24.0) {
            cluster.register(node);
        }

        let layers = sample_layers(33, 1.0);
        let balancer = LoadBalancer::new(Arc::new(cluster), LoadBalanceConfig::default());

        let plan = balancer.distribute_layers(&layers).unwrap();
        let summary = plan.summary();

        assert!(summary.contains("Total layers: 33"));
    }

    #[test]
    fn load_stats_records_distribution() {
        let mut stats = LoadStats::new();

        stats.record_distribution(24, 24.0);
        stats.record_distribution(9, 9.0);

        assert_eq!(stats.total_layers_distributed, 33);
        assert!((stats.total_vram_used_gb - 33.0).abs() < 0.01);
    }

    #[test]
    fn load_stats_updates_balance_ratio() {
        let mut stats = LoadStats::new();

        stats.update_balance_ratio(0.95);
        assert!((stats.avg_load_balance_ratio - 0.95).abs() < 1e-6);

        stats.update_balance_ratio(0.85);
        // EMA: 0.95 * 0.9 + 0.85 * 0.1 = 0.855 + 0.085 = 0.94
        assert!((stats.avg_load_balance_ratio - 0.94).abs() < 1e-6);
    }

    #[test]
    fn load_stats_first_balance_ratio_not_diluted_by_prior_distribution() {
        // Regression: the "first call" check for update_balance_ratio used
        // total_layers_distributed == 0 as a proxy for "never updated
        // before". If record_distribution() ran first (the normal order:
        // record layer placements, then periodically report a balance
        // ratio), the very first update_balance_ratio() call fell through
        // to the EMA branch and diluted the true first sample by 10x
        // (0.0 * 0.9 + ratio * 0.1) instead of setting it directly.
        let mut stats = LoadStats::new();

        stats.record_distribution(24, 24.0);
        stats.update_balance_ratio(0.95);

        assert!(
            (stats.avg_load_balance_ratio - 0.95).abs() < 1e-6,
            "first balance ratio sample must be set directly, got {}",
            stats.avg_load_balance_ratio
        );
    }

    #[test]
    fn load_stats_reports() {
        let mut stats = LoadStats::new();

        stats.record_distribution(24, 24.0);
        stats.update_balance_ratio(0.95);

        let report = stats.report();
        assert!(report.contains("Total layers distributed: 24"));
    }

    #[test]
    fn autotuned_config_reflects_runtime_profile() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 6,
            acceleration_mode: AccelerationMode::Gpu,
            gpu_backend: crate::host::GpuBackend::Cuda,
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let config = LoadBalanceConfig::autotuned(&profile);
        assert_eq!(config.lock_timeout_us, 500);
        assert_eq!(config.max_layers_per_assignment, 12);
        assert_eq!(config.max_concurrent_rebalances, 6);
        assert!(config.min_load_threshold > 1.0);
    }

    #[test]
    fn autotuned_cpu_threshold_stays_above_one() {
        // Regression: the CPU/generic tier's min_load_threshold was 0.95,
        // below the mathematical minimum of skew_ratio (always >= 1.0), so
        // rebalance() always returned true regardless of actual balance.
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 0.0, 16.0, "generic", None),
            logical_cores: 4,
            recommended_workers: 2,
            acceleration_mode: AccelerationMode::Generic,
            gpu_backend: crate::host::GpuBackend::Cpu,
            xdp_supported: false,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };
        let config = LoadBalanceConfig::autotuned(&profile);
        assert!(
            config.min_load_threshold > 1.0,
            "CPU-tier threshold {} must exceed skew_ratio's mathematical floor of 1.0",
            config.min_load_threshold
        );
    }

    #[test]
    fn rebalance_does_not_always_trigger_on_balanced_cpu_cluster() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 0.0, 16.0, "generic", None));
        cluster.register(NodeResources::new("node-b", 0.0, 16.0, "generic", None));

        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 0.0, 16.0, "generic", None),
            logical_cores: 4,
            recommended_workers: 2,
            acceleration_mode: AccelerationMode::Generic,
            gpu_backend: crate::host::GpuBackend::Cpu,
            xdp_supported: false,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };
        let balancer = LoadBalancer::new(cluster, LoadBalanceConfig::autotuned(&profile));
        assert!(
            !balancer.rebalance(),
            "a perfectly balanced (zero-VRAM) CPU cluster must not be reported as needing rebalance"
        );
    }

    #[test]
    fn runtime_profile_chunks_large_distribution_slices() {
        let plan = LoadDistributionPlan::new(
            vec![("node-a".into(), vec![TensorSlice::new((0, 10), 10.0)])],
            10,
        );

        let chunked = chunk_distribution_plan(plan, 4);
        assert_eq!(chunked.distributions[0].1.len(), 3);
        assert_eq!(chunked.distributions[0].1[0].layer_range, (0, 4));
        assert_eq!(chunked.distributions[0].1[2].layer_range, (8, 10));
    }

    #[test]
    fn shed_load_moves_from_low_headroom_to_high_headroom_nodes() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-hot", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-cool", 24.0, 64.0, "8.9", None));

        // node-hot is overloaded (very low available VRAM)
        cluster.get_metrics_mut("node-hot", |m| {
            m.available_vram_gb = 2.0;
            m.total_vram_gb = 24.0;
        });

        // node-cool has enough headroom to receive work
        cluster.get_metrics_mut("node-cool", |m| {
            m.available_vram_gb = 20.0;
            m.total_vram_gb = 24.0;
        });

        let balancer = LoadBalancer::new(Arc::new(cluster), LoadBalanceConfig::default());
        let transfers = balancer.shed_load();

        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].0, "node-hot");
        assert_eq!(transfers[0].1, "node-cool");
    }
}
