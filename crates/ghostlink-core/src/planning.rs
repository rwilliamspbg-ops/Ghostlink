//! Greedy Layer Assignment with Fault Tolerance and Adaptive Quantization
//!
//! This module provides:
//! - Sequential greedy layer splitting across nodes based on VRAM capacity
//! - Adaptive quantization trigger (select_quantization_mode)
//! - Load balancing and fault detection integration

use crate::accelerator::ExecutionBackend;
use crate::cluster::ClusterState;
use crate::cluster::NodeStatus;
use crate::host::{AccelerationMode, RuntimeProfile};
use crate::protocol::NodeResources;

/// Delivery ratio thresholds for adaptive quantization
pub const DELIVERY_RATIO_INT8_THRESHOLD: f32 = 0.95;
pub const DELIVERY_RATIO_INT4_THRESHOLD: f32 = 0.80;

/// Layer specification with VRAM requirements
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSpec {
    /// Layer index (0-based)
    pub index: usize,
    /// VRAM required in GB
    pub vram_gb: f32,
    /// Number of weights in the layer
    pub num_weights: u32,
}

impl Default for LayerSpec {
    fn default() -> Self {
        Self {
            index: 0,
            vram_gb: 1.0,
            num_weights: 0,
        }
    }
}

/// Layer assignment to a specific node
#[derive(Clone, Debug, PartialEq)]
pub struct LayerAssignment {
    /// Node ID
    pub node_id: String,
    /// Start layer index (inclusive)
    pub start_layer: usize,
    /// End layer index (exclusive)
    pub end_layer: usize,
    /// VRAM used on this node
    pub used_vram_gb: f32,
    /// Number of layers assigned
    pub num_layers: usize,
}

impl LayerAssignment {
    /// Create new layer assignment
    pub fn new(node_id: String, start_layer: usize, end_layer: usize, vram_gb: f32) -> Self {
        Self {
            node_id,
            start_layer,
            end_layer,
            used_vram_gb: vram_gb,
            num_layers: end_layer - start_layer,
        }
    }

    /// Get average VRAM per layer
    pub fn avg_vram_per_layer(&self) -> f32 {
        if self.num_layers == 0 {
            0.0
        } else {
            self.used_vram_gb / self.num_layers as f32
        }
    }
}

/// Quantization mode enumeration
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuantizationMode {
    /// No quantization (full precision)
    #[default]
    None,
    /// 8-bit quantization
    Int8,
    /// 4-bit quantization
    Int4,
}

/// Layer placement plan across nodes
#[derive(Clone, Debug, Default)]
pub struct PlacementPlan {
    /// Assignments per node
    pub assignments: Vec<LayerAssignment>,
    /// Selected quantization mode
    pub quantization_mode: QuantizationMode,
    /// Total layers assigned
    pub total_layers: usize,
    /// Nodes participating in plan
    pub participating_nodes: Vec<String>,
}

impl PlacementPlan {
    /// Create new placement plan
    ///
    /// OPTIMIZATION: Consolidates total_layers summation and participating_nodes collection
    /// into a single pass over assignments. Deduplicates node IDs in-place without duplicate
    /// string allocations when nodes have multiple chunked assignments.
    pub fn new(assignments: Vec<LayerAssignment>, quantization_mode: QuantizationMode) -> Self {
        let mut participating_nodes = Vec::with_capacity(assignments.len());
        let mut total_layers = 0usize;

        for a in &assignments {
            total_layers += a.num_layers;
            if !participating_nodes.contains(&a.node_id) {
                participating_nodes.push(a.node_id.clone());
            }
        }

        Self {
            assignments,
            quantization_mode,
            total_layers,
            participating_nodes,
        }
    }

    /// Get human-readable plan summary
    pub fn summary(&self) -> String {
        let mode_str = match self.quantization_mode {
            QuantizationMode::None => "Full Precision",
            QuantizationMode::Int8 => "8-bit Quantized",
            QuantizationMode::Int4 => "4-bit Quantized",
        };

        format!(
            "Placement Plan ({})\n\
             =================\n\
             Total layers: {}\n\
             Quantization: {}\n\
             Nodes: {}\n",
            mode_str,
            self.total_layers,
            match self.quantization_mode {
                QuantizationMode::None => "Full Precision".to_string(),
                QuantizationMode::Int8 => "8-bit Quantized".to_string(),
                QuantizationMode::Int4 => "4-bit Quantized".to_string(),
            },
            self.participating_nodes.join(", ")
        )
    }
}

/// Runtime-aware planning hints derived from host auto-detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanningTuning {
    /// Preferred maximum chunk size to expose parallel work to workers.
    pub max_layers_per_assignment: usize,
}

impl PlanningTuning {
    /// Derive planning hints from the detected runtime profile.
    pub fn from_runtime_profile(profile: &RuntimeProfile, total_layers: usize) -> Self {
        let backend = ExecutionBackend::from_runtime_profile(profile);
        let worker_count = backend.worker_count.max(1);
        let accelerator_bias = match profile.acceleration_mode {
            AccelerationMode::Gpu => 2,
            AccelerationMode::Avx512 => 1,
            _ => 0,
        };
        let vector_bias = (backend.vector_width_bits / 256).max(1);
        let target_chunks = (worker_count + accelerator_bias + vector_bias - 1).max(1);
        let chunk_size = if total_layers == 0 {
            1
        } else {
            total_layers.div_ceil(target_chunks).max(1)
        };

        Self {
            max_layers_per_assignment: chunk_size,
        }
    }
}

/// Select quantization mode based on cluster health metrics
pub fn select_quantization_mode(delivery_ratio: f32) -> QuantizationMode {
    if delivery_ratio >= DELIVERY_RATIO_INT8_THRESHOLD {
        QuantizationMode::None
    } else if delivery_ratio >= DELIVERY_RATIO_INT4_THRESHOLD {
        QuantizationMode::Int8
    } else {
        QuantizationMode::Int4
    }
}

/// Assign layers sequentially across nodes based on VRAM capacity.
///
/// OPTIMIZATION: Replaces per-layer `Option<LayerAssignment>` state tracking and repeated `.as_mut()`
/// calls with a single-pass index range and VRAM accumulator. Pre-allocates output capacity for `nodes.len()`
/// and constructs `LayerAssignment` objects only once per node transition or stream completion,
/// preserving exact `LayerSpec.index` range boundaries while reducing allocation churn and instructions.
pub fn assign_layers_sequentially(
    nodes: &[NodeResources],
    layers: &[LayerSpec],
) -> Result<Vec<LayerAssignment>, String> {
    if nodes.is_empty() {
        return Err("at least one node is required".into());
    }
    if layers.is_empty() {
        return Ok(Vec::new());
    }

    let mut assignments = Vec::with_capacity(nodes.len());
    let mut node_idx = 0usize;
    let mut remaining_capacity = nodes[0].vram_gb;
    let mut current_start = 0usize;
    let mut current_vram = 0.0f32;

    for (i, layer) in layers.iter().enumerate() {
        while layer.vram_gb > remaining_capacity {
            // Need to move to next node: flush current accumulated layer assignment
            if i > current_start {
                let start_layer = layers[current_start].index;
                let end_layer = layers[i - 1].index + 1;
                assignments.push(LayerAssignment::new(
                    nodes[node_idx].id.clone(),
                    start_layer,
                    end_layer,
                    current_vram,
                ));
            }

            node_idx += 1;
            if node_idx >= nodes.len() {
                return Err(format!(
                    "insufficient cluster VRAM for layer {} (needs {:.2} GB)",
                    layer.index, layer.vram_gb
                ));
            }
            remaining_capacity = nodes[node_idx].vram_gb;
            current_start = i;
            current_vram = 0.0;
        }

        // Accumulate layer onto current node
        remaining_capacity -= layer.vram_gb;
        current_vram += layer.vram_gb;
    }

    // Finalize last assignment for remaining layers
    if current_start < layers.len() {
        let start_layer = layers[current_start].index;
        let end_layer = layers[layers.len() - 1].index + 1;
        assignments.push(LayerAssignment::new(
            nodes[node_idx].id.clone(),
            start_layer,
            end_layer,
            current_vram,
        ));
    }

    Ok(assignments)
}

/// Split large node assignments into smaller contiguous chunks for worker-level parallelism.
pub fn chunk_assignments_for_workers(
    assignments: Vec<LayerAssignment>,
    max_layers_per_assignment: usize,
) -> Vec<LayerAssignment> {
    let chunk_size = max_layers_per_assignment.max(1);

    let needs_chunking = assignments.iter().any(|a| a.num_layers > chunk_size);
    if !needs_chunking {
        return assignments;
    }

    let mut chunked = Vec::with_capacity(assignments.len() * 2);

    for assignment in assignments {
        if assignment.num_layers <= chunk_size {
            chunked.push(assignment);
            continue;
        }

        let avg_vram = assignment.avg_vram_per_layer();
        let mut start_layer = assignment.start_layer;

        while start_layer < assignment.end_layer {
            let end_layer = (start_layer + chunk_size).min(assignment.end_layer);
            let num_layers = end_layer - start_layer;
            chunked.push(LayerAssignment::new(
                assignment.node_id.clone(),
                start_layer,
                end_layer,
                avg_vram * num_layers as f32,
            ));
            start_layer = end_layer;
        }
    }

    chunked
}

/// Assign layers sequentially across nodes, directly producing chunked assignments
/// sized for worker-level parallelism. Avoids creating intermediate full-size
/// `LayerAssignment` objects and their associated allocation/clone overhead.
///
/// OPTIMIZATION: Computes chunked layer assignments directly in a single pass over
/// the input layer slice, bypassing intermediate `PlacementPlan`/`LayerAssignment` allocations,
/// post-allocation chunking loops (`chunk_assignments_for_workers`), and redundant meta-calculations.
/// Preserves exact `LayerSpec.index` range boundaries (matching `assign_layers_sequentially`).
fn assign_layers_chunked(
    nodes: &[NodeResources],
    layers: &[LayerSpec],
    max_layers_per_assignment: usize,
) -> Result<Vec<LayerAssignment>, String> {
    if nodes.is_empty() {
        return Err("at least one node is required".into());
    }
    if layers.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_size = max_layers_per_assignment.max(1);
    let est_assignments = (layers.len().div_ceil(chunk_size)).max(nodes.len());
    let mut assignments = Vec::with_capacity(est_assignments);
    let mut node_idx = 0usize;
    let mut remaining_vram = nodes[0].vram_gb;
    // Index of the first layer in the current chunk
    let mut chunk_start = 0usize;
    let mut chunk_vram = 0.0f32;

    for (i, layer) in layers.iter().enumerate() {
        // Move to next node if current node is out of capacity
        while layer.vram_gb > remaining_vram {
            if i > chunk_start {
                let start_layer = layers[chunk_start].index;
                let end_layer = layers[i - 1].index + 1;
                assignments.push(LayerAssignment::new(
                    nodes[node_idx].id.clone(),
                    start_layer,
                    end_layer,
                    chunk_vram,
                ));
            }
            node_idx += 1;
            if node_idx >= nodes.len() {
                return Err(format!(
                    "insufficient cluster VRAM for layer {} (needs {:.2} GB)",
                    layer.index, layer.vram_gb
                ));
            }
            remaining_vram = nodes[node_idx].vram_gb;
            chunk_start = i;
            chunk_vram = 0.0;
        }

        remaining_vram -= layer.vram_gb;
        chunk_vram += layer.vram_gb;

        // Flush chunk if it reached the maximum chunk size
        let chunk_len = i + 1 - chunk_start;
        if chunk_len >= chunk_size {
            let start_layer = layers[chunk_start].index;
            let end_layer = layers[i].index + 1;
            assignments.push(LayerAssignment::new(
                nodes[node_idx].id.clone(),
                start_layer,
                end_layer,
                chunk_vram,
            ));
            chunk_start = i + 1;
            chunk_vram = 0.0;
        }
    }

    // Finalize last chunk. Checked by pending-layer range, not accumulated
    // VRAM: a trailing run of zero-VRAM layers (e.g. bias-only layers) would
    // otherwise leave `chunk_vram == 0.0` and be silently dropped from the
    // plan even though `chunk_start..layers.len()` is non-empty.
    if chunk_start < layers.len() {
        let start_layer = layers[chunk_start].index;
        let end_layer = layers[layers.len() - 1].index + 1;
        assignments.push(LayerAssignment::new(
            nodes[node_idx].id.clone(),
            start_layer,
            end_layer,
            chunk_vram,
        ));
    }

    Ok(assignments)
}

/// Assign layers using runtime auto-detection to expose worker-parallel chunks.
pub fn assign_layers_with_runtime_profile(
    nodes: &[NodeResources],
    layers: &[LayerSpec],
    profile: &RuntimeProfile,
) -> Result<Vec<LayerAssignment>, String> {
    let tuning = PlanningTuning::from_runtime_profile(profile, layers.len());
    assign_layers_chunked(nodes, layers, tuning.max_layers_per_assignment)
}

/// Helper to calculate average delivery ratio across all active nodes in a single lock pass.
fn calculate_average_delivery_ratio(cluster: &ClusterState) -> f32 {
    let (sum_delivery, active_count) = {
        let metrics = cluster
            .metrics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for metric in metrics.values() {
            if metric.status == NodeStatus::Active {
                sum += metric.delivery_ratio;
                count += 1;
            }
        }
        (sum, count)
    };

    if active_count > 0 {
        sum_delivery / active_count as f32
    } else {
        0.0
    }
}

/// Assign layers with fault tolerance and load balancing
pub fn assign_layers_with_fault_tolerance(
    cluster: &ClusterState,
    layers: &[LayerSpec],
) -> Result<PlacementPlan, String> {
    let nodes = cluster.nodes_snapshot();

    if nodes.is_empty() {
        return Err("no nodes available".into());
    }

    // First pass: greedy assignment
    let assignments = assign_layers_sequentially(&nodes, layers)?;

    let total_delivery_ratio = calculate_average_delivery_ratio(cluster);

    // Select quantization mode based on health
    let quantization_mode = select_quantization_mode(total_delivery_ratio);

    Ok(PlacementPlan::new(assignments, quantization_mode))
}

/// Assign layers with fault tolerance and runtime-aware chunking.
///
/// Optimized to directly call `assign_layers_chunked`, bypassing intermediate
/// `LayerAssignment` allocations and `chunk_assignments_for_workers` overhead.
pub fn assign_layers_with_fault_tolerance_and_runtime(
    cluster: &ClusterState,
    layers: &[LayerSpec],
    profile: &RuntimeProfile,
) -> Result<PlacementPlan, String> {
    let nodes = cluster.nodes_snapshot();

    if nodes.is_empty() {
        return Err("no nodes available".into());
    }

    let tuning = PlanningTuning::from_runtime_profile(profile, layers.len());
    // Directly run the chunked layout computation
    let assignments = assign_layers_chunked(&nodes, layers, tuning.max_layers_per_assignment)?;

    let total_delivery_ratio = calculate_average_delivery_ratio(cluster);

    // Select quantization mode based on health
    let quantization_mode = select_quantization_mode(total_delivery_ratio);

    Ok(PlacementPlan::new(assignments, quantization_mode))
}

/// Update layer assignments based on node health metrics
pub fn rebalance_assignments(cluster: &ClusterState, plan: &mut PlacementPlan) -> bool {
    // Check if any active node has high available VRAM
    let mut needs_rebalance = false;

    let metrics = cluster
        .metrics
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    for assignment in &mut plan.assignments {
        if let Some(metric) = metrics.get(&assignment.node_id) {
            if metric.status != NodeStatus::Active {
                // Skip failed nodes
                continue;
            }

            // Check if this node can take more layers
            let available = metric.available_vram_gb;
            let avg_layer_size = assignment.avg_vram_per_layer();

            if available > 0.0 && avg_layer_size > 0.0 {
                let potential_layers = (available / avg_layer_size) as usize;

                // If node has significant capacity, mark for rebalancing
                if potential_layers > 2 {
                    needs_rebalance = true;
                    break;
                }
            }
        }
    }

    needs_rebalance
}

/// Simulate layer streaming on a node with metrics updates
#[cfg(test)]
pub fn simulate_layer_streaming(
    node_id: &str,
    cluster: &ClusterState,
    start_layer: usize,
    end_layer: usize,
) -> Option<crate::cluster::NodeMetrics> {
    let mut metrics = cluster.get_metrics(node_id)?;

    // Record VRAM usage
    let num_layers = end_layer - start_layer;
    let avg_vram = (metrics.available_vram_gb + metrics.vram_gb) / 2.0;
    let vram_per_layer = avg_vram / num_layers as f32;
    metrics.record_vram_usage(vram_per_layer * num_layers as f32);

    // Set streaming layers
    metrics.set_streaming_layers(start_layer, end_layer);

    Some(metrics)
}

/// Calculate network health across the cluster
pub fn calculate_cluster_health(cluster: &ClusterState) -> (f32, usize, Vec<String>) {
    // Acquire the lock on metrics map exactly ONCE to avoid redundant mutex lock/unlock and clones
    let metrics = cluster
        .metrics
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    if metrics.is_empty() {
        return (0.0, 0, vec![]);
    }

    let mut sum_delivery_ratio = 0.0f32;
    let mut active_count = 0usize;
    let mut failed_nodes = Vec::new();

    // Iterate once over all live node metrics to aggregate statistics and collect failed nodes
    for (node_id, metric) in metrics.iter() {
        match metric.status {
            NodeStatus::Active => {
                sum_delivery_ratio += metric.delivery_ratio;
                active_count += 1;
            }
            NodeStatus::Failed => {
                failed_nodes.push(node_id.clone());
            }
            _ => {}
        }
    }

    let avg_delivery_ratio = if active_count > 0 {
        sum_delivery_ratio / active_count as f32
    } else {
        0.0
    };

    let failed_count = failed_nodes.len();

    (avg_delivery_ratio, failed_count, failed_nodes)
}

/// Dynamic migration planner for moving layers between nodes under load.
#[derive(Debug, Clone)]
pub struct MigrationPlanner {
    pub source_node: String,
    pub target_node: String,
    pub layers: Vec<usize>,
    pub est_migration_time_ms: f32,
}

impl MigrationPlanner {
    pub fn new(source: String, target: String, layers: Vec<usize>) -> Self {
        let est_time = layers.len() as f32 * 45.0; // Baseline 45ms per layer migration
        Self {
            source_node: source,
            target_node: target,
            layers,
            est_migration_time_ms: est_time,
        }
    }

    /// Generate a safe handoff sequence for a migration.
    pub fn generate_handoff_plan(&self) -> Vec<String> {
        let mut steps = Vec::new();
        let target_node = &self.target_node;
        let source_node = &self.source_node;
        steps.push(format!(
            "PREPARE: Target node {target_node} allocating VRAM"
        ));
        steps.push(format!(
            "STREAM: Moving layers {:?} to {target_node}",
            self.layers
        ));
        steps.push(format!("VERIFY: Integrity check on {target_node}"));
        steps.push(format!("COMMIT: Switch routing to {target_node}"));
        steps.push(format!("CLEANUP: Free VRAM on source node {source_node}"));
        steps
    }
}

/// Trigger mechanism for dynamic rebalancing based on load/health.
pub struct RebalanceTrigger {
    pub min_imbalance_ratio: f32,
    pub max_p95_latency_ms: f32,
}

impl Default for RebalanceTrigger {
    fn default() -> Self {
        Self {
            min_imbalance_ratio: 0.25, // 25% drift
            max_p95_latency_ms: 25.0,
        }
    }
}

impl RebalanceTrigger {
    /// Evaluates if cluster migration is needed due to load or latency imbalance.
    ///
    /// OPTIMIZATION: Consolidates node counting and overtaxed/available node search into a
    /// single pass over node metrics under lock. Borrows string references (&str) during traversal,
    /// allocating owned Strings only when a migration candidate pair is confirmed.
    pub fn evaluate(
        &self,
        cluster: &ClusterState,
        current_plan: &PlacementPlan,
    ) -> Option<MigrationPlanner> {
        let (overtaxed, available) = {
            let metrics = cluster
                .metrics
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());

            let mut active_count = 0usize;
            let mut overtaxed_ref: Option<&str> = None;
            let mut available_ref: Option<&str> = None;

            for m in metrics.values() {
                if m.status == NodeStatus::Active {
                    active_count += 1;
                    let is_overtaxed = m.delivery_ratio < 0.90 || m.avg_latency_us > 15.0;
                    if is_overtaxed {
                        if overtaxed_ref.is_none() {
                            overtaxed_ref = Some(&m.name);
                        }
                    } else if available_ref.is_none() && m.available_vram_gb > 8.0 {
                        available_ref = Some(&m.name);
                    }
                }
            }

            if active_count < 2 {
                return None;
            }

            (
                overtaxed_ref.map(|s| s.to_string()),
                available_ref.map(|s| s.to_string()),
            )
        };

        if let (Some(source), Some(target)) = (overtaxed, available) {
            if source == target {
                return None;
            }

            // Find layers to move from overtaxed node
            let layers_to_move = current_plan
                .assignments
                .iter()
                .find(|a| a.node_id == source)
                .map(|a| (a.start_layer..a.end_layer).take(2).collect::<Vec<_>>())
                .unwrap_or_default();

            if !layers_to_move.is_empty() {
                return Some(MigrationPlanner::new(source, target, layers_to_move));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::AccelerationMode;

    fn sample_layers(count: usize, vram_gb: f32) -> Vec<LayerSpec> {
        (0..count)
            .map(|index| LayerSpec {
                index,
                vram_gb,
                num_weights: 0,
            })
            .collect()
    }

    #[test]
    fn greedily_places_layers_across_nodes() {
        let nodes = vec![
            NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
            NodeResources::new("node-b", 12.0, 32.0, "8.6", None),
        ];

        let assignments = assign_layers_sequentially(&nodes, &sample_layers(33, 1.0)).unwrap();

        assert_eq!(
            assignments,
            vec![
                LayerAssignment {
                    node_id: "node-a".into(),
                    start_layer: 0,
                    end_layer: 24,
                    used_vram_gb: 24.0,
                    num_layers: 24,
                },
                LayerAssignment {
                    node_id: "node-b".into(),
                    start_layer: 24,
                    end_layer: 33,
                    used_vram_gb: 9.0,
                    num_layers: 9,
                }
            ]
        );
    }

    #[test]
    fn reports_insufficient_capacity() {
        let nodes = vec![NodeResources::new("node-a", 2.0, 64.0, "8.9", None)];
        let error = assign_layers_sequentially(&nodes, &sample_layers(3, 1.0)).unwrap_err();

        assert!(error.contains("insufficient cluster VRAM"));
    }

    #[test]
    fn selects_quantization_mode_from_delivery_ratio() {
        assert_eq!(select_quantization_mode(0.98), QuantizationMode::None);
        assert_eq!(select_quantization_mode(0.90), QuantizationMode::Int8);
        assert_eq!(select_quantization_mode(0.75), QuantizationMode::Int4);
    }

    #[test]
    fn placement_plan_summary() {
        let plan = PlacementPlan::new(
            vec![
                LayerAssignment::new("node-a".into(), 0, 24, 24.0),
                LayerAssignment::new("node-b".into(), 24, 33, 9.0),
            ],
            QuantizationMode::None,
        );

        let summary = plan.summary();
        assert!(summary.contains("Total layers: 33"));
        assert!(summary.contains("Full Precision"));
    }

    #[test]
    fn rebalance_assignments_detects_capacity() {
        let mut plan = PlacementPlan::new(
            vec![LayerAssignment::new("node-a".into(), 0, 24, 24.0)],
            QuantizationMode::None,
        );

        // Would need actual cluster state to test rebalancing
        assert!(!rebalance_assignments(&ClusterState::default(), &mut plan));
    }

    #[test]
    fn runtime_profile_chunks_large_assignments() {
        let assignments = vec![LayerAssignment::new("node-a".into(), 0, 12, 12.0)];
        let chunked = chunk_assignments_for_workers(assignments, 5);

        assert_eq!(chunked.len(), 3);
        assert_eq!(chunked[0].start_layer, 0);
        assert_eq!(chunked[0].end_layer, 5);
        assert_eq!(chunked[2].start_layer, 10);
        assert_eq!(chunked[2].end_layer, 12);
    }

    #[test]
    fn runtime_profile_assignment_keeps_trailing_zero_vram_layers() {
        // Regression: assign_layers_chunked finalized its last pending chunk
        // by checking `chunk_vram > 0.0`, so a trailing run of zero-VRAM
        // layers left a non-empty pending chunk with chunk_vram == 0.0 and
        // silently dropped those layers from the plan.
        let nodes = vec![NodeResources::new("node-a", 24.0, 64.0, "8.9", None)];
        let layers = sample_layers(3, 0.0);
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
            logical_cores: 4,
            recommended_workers: 1,
            acceleration_mode: AccelerationMode::Generic,
            gpu_backend: crate::host::GpuBackend::Cpu,
            xdp_supported: false,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let assignments = assign_layers_with_runtime_profile(&nodes, &layers, &profile).unwrap();
        let total: usize = assignments.iter().map(|a| a.num_layers).sum();
        assert_eq!(total, 3, "all zero-VRAM layers must still be assigned");
    }

    #[test]
    fn fault_tolerance_handles_all_failed_nodes_without_panicking() {
        // Regression: dividing active_nodes' summed delivery ratio by
        // active_nodes().len() produced NaN (or a TOCTOU-inconsistent
        // average) when every registered node's metrics were marked Failed.
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.mark_failed("node-a");

        let plan = assign_layers_with_fault_tolerance(&cluster, &sample_layers(4, 1.0)).unwrap();
        assert_eq!(plan.quantization_mode, QuantizationMode::Int4);
    }

    #[test]
    fn planning_tuning_scales_chunk_size_with_workers() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 8,
            acceleration_mode: AccelerationMode::Gpu,
            gpu_backend: crate::host::GpuBackend::Cuda,
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let tuning = PlanningTuning::from_runtime_profile(&profile, 40);
        assert!(tuning.max_layers_per_assignment <= 4);
    }

    #[test]
    fn test_rebalance_trigger_evaluate_never_selects_overtaxed_as_available() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-1", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-2", 24.0, 64.0, "8.9", None));

        // Node 1 is overtaxed (delivery ratio < 0.90)
        cluster.get_metrics_mut("node-1", |m| {
            m.delivery_ratio = 0.50;
            m.available_vram_gb = 16.0;
        });

        // Node 2 is also overtaxed (delivery ratio < 0.90) with high VRAM
        cluster.get_metrics_mut("node-2", |m| {
            m.delivery_ratio = 0.60;
            m.available_vram_gb = 16.0;
        });

        let trigger = RebalanceTrigger::default();
        let plan = PlacementPlan::new(
            vec![LayerAssignment::new("node-1".into(), 0, 10, 10.0)],
            QuantizationMode::None,
        );

        let planner = trigger.evaluate(&cluster, &plan);
        // Since both nodes are overtaxed, neither should be chosen as an "available" target
        assert!(
            planner.is_none(),
            "overtaxed nodes must never be selected as available rebalance targets"
        );
    }
}
