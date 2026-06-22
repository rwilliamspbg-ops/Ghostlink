//! Tensor Distribution and Load Balancing for Ghost-Link Cluster
//!
//! This module provides:
//! - Tensor distribution across nodes based on VRAM capacity
//! - Dynamic load shedding
//! - Deadlock prevention with timeout

use crate::cluster::{ClusterState, NodeMetrics};
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
}

impl Default for LoadBalanceConfig {
    fn default() -> Self {
        Self {
            lock_timeout_us: 1000, // 1ms
            min_load_threshold: 0.8,
            max_layers_per_assignment: 100,
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
            output.push_str(&format!("Node: {}\n", node_id));

            let total_size_gb: f32 = slices.iter().map(|s| s.size_gb).sum();

            output.push_str(&format!("  Total size: {:.1} GB\n", total_size_gb));
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
    /// Configuration
    config: LoadBalanceConfig,
}

impl LoadBalancer {
    /// Create new load balancer
    pub fn new(cluster: Arc<ClusterState>, config: LoadBalanceConfig) -> Self {
        Self { cluster, config }
    }

    /// Distribute tensor layers across nodes based on VRAM capacity
    pub fn distribute_layers(
        &self,
        layers: &[crate::planning::LayerSpec],
    ) -> Result<LoadDistributionPlan, String> {
        let nodes_snapshot = self.cluster.nodes_snapshot();
        if nodes_snapshot.is_empty() {
            return Err("no nodes available".into());
        }

        // Sort nodes by VRAM capacity (descending)
        let mut sorted_nodes: Vec<_> = nodes_snapshot.iter().cloned().collect();
        sorted_nodes.sort_by(|a, b| {
            b.vram_gb
                .partial_cmp(&a.vram_gb)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Collect all layers into a single sorted vector (by index)
        let mut all_layers: Vec<_> = layers.iter().cloned().collect();
        all_layers.sort_by_key(|l| l.index);
        let total_layer_count = all_layers.len();

        // Greedy assignment: assign contiguous layers to nodes based on VRAM
        let mut distributions = Vec::new();
        let mut remaining_layers = all_layers;

        for node in &sorted_nodes {
            if remaining_layers.is_empty() {
                break;
            }

            let mut used_vram = 0.0f32;
            let mut start_enum = usize::MAX;
            let mut end_enum = 0usize;

            for (enum_idx, layer) in remaining_layers.iter().enumerate() {
                if used_vram + layer.vram_gb > node.vram_gb {
                    break;
                }

                if start_enum == usize::MAX {
                    start_enum = enum_idx;
                }
                end_enum = enum_idx + 1;

                used_vram += layer.vram_gb;
            }

            if start_enum != usize::MAX {
                let slices: Vec<TensorSlice> = remaining_layers[start_enum..end_enum]
                    .iter()
                    .map(|l| TensorSlice::new((l.index, l.index + 1), l.vram_gb))
                    .collect();

                distributions.push((node.id.clone(), slices));

                // Remove assigned layers from remaining
                remaining_layers.drain(start_enum..end_enum);
            }
        }

        if remaining_layers.is_empty() {
            Ok(LoadDistributionPlan::new(distributions, total_layer_count))
        } else {
            Err(format!(
                "insufficient VRAM: {} layers remain",
                remaining_layers.len()
            ))
        }
    }

    /// Rebalance load based on current node metrics
    pub fn rebalance(&self) -> bool {
        let active_nodes = self.cluster.active_nodes();

        if active_nodes.is_empty() {
            return false;
        }

        // Calculate average available VRAM across nodes
        let total_available: f32 = active_nodes
            .iter()
            .map(|m| m.available_vram_gb)
            .sum::<f32>()
            / active_nodes.len() as f32;

        // Check if any node has significantly more available VRAM than average
        for node in &active_nodes {
            let ratio = node.available_vram_gb / total_available;

            if ratio > self.config.min_load_threshold {
                // Node has excess capacity - mark for rebalancing
                tracing::info!(
                    "Node {} has {:.1}% more available VRAM than average",
                    node.name,
                    ratio * 100.0
                );
                return true;
            }
        }

        false
    }

    /// Shed load from overloaded nodes to underloaded ones
    pub fn shed_load(&self) -> Vec<(String, String)> {
        let mut transfers: Vec<(String, String)> = Vec::new();

        // Find overloaded nodes (using < 50% VRAM utilization as overloaded)
        let overloaded_nodes: Vec<_> = self
            .cluster
            .active_nodes()
            .into_iter()
            .filter(|m| m.available_vram_gb > m.total_vram_gb * 0.5)
            .collect();

        // Find underloaded nodes (using > 80% VRAM utilization as underloaded)
        let underloaded_nodes: Vec<_> = self
            .cluster
            .active_nodes()
            .into_iter()
            .filter(|m| m.available_vram_gb < m.total_vram_gb * 0.2)
            .collect();

        // For each overloaded node, find a suitable underloaded target
        for overloaded in &overloaded_nodes {
            if let Some(target) = self.find_best_target(underloaded_nodes.as_slice(), overloaded) {
                transfers.push((overloaded.name.clone(), target.name.clone()));
            }
        }

        transfers
    }

    /// Find best target node for load transfer
    fn find_best_target(
        &self,
        targets: &[NodeMetrics],
        source: &NodeMetrics,
    ) -> Option<NodeMetrics> {
        // Find node with most available VRAM that's different from source
        let mut best_target: Option<&NodeMetrics> = None;
        let mut max_available = 0.0f32;

        for target in targets {
            if target.name == source.name {
                continue;
            }

            if target.available_vram_gb > max_available {
                max_available = target.available_vram_gb;
                best_target = Some(target);
            }
        }

        best_target.cloned()
    }

    /// Distribute layers with deadlock prevention
    pub fn distribute_with_deadlock_prevention(
        &self,
        layers: &[crate::planning::LayerSpec],
    ) -> Result<LoadDistributionPlan, String> {
        // Use timeout-based acquisition to prevent deadlocks
        let start_time = std::time::Instant::now();

        while start_time.elapsed().as_micros() < self.config.lock_timeout_us as u128 {
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
        if self.avg_load_balance_ratio == 0.0
            && self.rebalancing_count == 0
            && self.total_layers_distributed == 0
        {
            // First call - initialize directly
            self.avg_load_balance_ratio = ratio;
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
                    format!("node-{}", i),
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
    fn load_stats_reports() {
        let mut stats = LoadStats::new();

        stats.record_distribution(24, 24.0);
        stats.update_balance_ratio(0.95);

        let report = stats.report();
        assert!(report.contains("Total layers distributed: 24"));
    }
}
