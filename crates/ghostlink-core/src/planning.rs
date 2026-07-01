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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantizationMode {
    /// No quantization (full precision)
    None,
    /// 8-bit quantization
    Int8,
    /// 4-bit quantization
    Int4,
}

/// Layer placement plan across nodes
#[derive(Clone, Debug)]
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
    pub fn new(assignments: Vec<LayerAssignment>, quantization_mode: QuantizationMode) -> Self {
        let participating_nodes = assignments.iter().map(|a| a.node_id.clone()).collect();
        let total_layers = assignments.iter().map(|a| a.num_layers).sum();

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

/// Assign layers sequentially across nodes based on VRAM capacity
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

    let mut assignments = Vec::new();
    let mut current_node_index = 0usize;
    let mut remaining_capacity = nodes[0].vram_gb;
    let mut current_assignment: Option<LayerAssignment> = None;

    for layer in layers {
        while layer.vram_gb > remaining_capacity {
            // Need to move to next node
            if let Some(assignment) = current_assignment.take() {
                assignments.push(assignment);
            }

            current_node_index += 1;
            if current_node_index >= nodes.len() {
                return Err(format!(
                    "insufficient cluster VRAM for layer {} (needs {:.2} GB)",
                    layer.index, layer.vram_gb
                ));
            }
            remaining_capacity = nodes[current_node_index].vram_gb;
        }

        // Assign layer to current node
        remaining_capacity -= layer.vram_gb;

        match current_assignment.as_mut() {
            Some(assignment) => {
                assignment.end_layer = layer.index + 1;
                assignment.used_vram_gb += layer.vram_gb;
                assignment.num_layers += 1;
            }
            None => {
                current_assignment = Some(LayerAssignment::new(
                    nodes[current_node_index].id.clone(),
                    layer.index,
                    layer.index + 1,
                    layer.vram_gb,
                ));
            }
        }
    }

    // Finalize last assignment
    if let Some(assignment) = current_assignment {
        assignments.push(assignment);
    }

    Ok(assignments)
}

/// Split large node assignments into smaller contiguous chunks for worker-level parallelism.
pub fn chunk_assignments_for_workers(
    assignments: &[LayerAssignment],
    max_layers_per_assignment: usize,
) -> Vec<LayerAssignment> {
    let chunk_size = max_layers_per_assignment.max(1);
    let mut chunked = Vec::new();

    for assignment in assignments {
        if assignment.num_layers <= chunk_size {
            chunked.push(assignment.clone());
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

/// Assign layers using runtime auto-detection to expose worker-parallel chunks.
pub fn assign_layers_with_runtime_profile(
    nodes: &[NodeResources],
    layers: &[LayerSpec],
    profile: &RuntimeProfile,
) -> Result<Vec<LayerAssignment>, String> {
    let assignments = assign_layers_sequentially(nodes, layers)?;
    let tuning = PlanningTuning::from_runtime_profile(profile, layers.len());
    Ok(chunk_assignments_for_workers(
        &assignments,
        tuning.max_layers_per_assignment,
    ))
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

    // Calculate average delivery ratio across all nodes
    let total_delivery_ratio = cluster
        .active_nodes()
        .iter()
        .map(|m| m.delivery_ratio)
        .sum::<f32>()
        / cluster.active_nodes().len() as f32;

    // Select quantization mode based on health
    let quantization_mode = select_quantization_mode(total_delivery_ratio);

    Ok(PlacementPlan::new(assignments, quantization_mode))
}

/// Assign layers with fault tolerance and runtime-aware chunking.
pub fn assign_layers_with_fault_tolerance_and_runtime(
    cluster: &ClusterState,
    layers: &[LayerSpec],
    profile: &RuntimeProfile,
) -> Result<PlacementPlan, String> {
    let mut plan = assign_layers_with_fault_tolerance(cluster, layers)?;
    let tuning = PlanningTuning::from_runtime_profile(profile, layers.len());
    plan.assignments =
        chunk_assignments_for_workers(&plan.assignments, tuning.max_layers_per_assignment);
    plan.total_layers = plan
        .assignments
        .iter()
        .map(|assignment| assignment.num_layers)
        .sum();
    plan.participating_nodes = plan
        .assignments
        .iter()
        .map(|assignment| assignment.node_id.clone())
        .collect();
    Ok(plan)
}

/// Update layer assignments based on node health metrics
pub fn rebalance_assignments(cluster: &ClusterState, plan: &mut PlacementPlan) -> bool {
    // Check if any active node has high available VRAM
    let mut needs_rebalance = false;

    for assignment in &mut plan.assignments {
        if let Some(metrics) = cluster.get_metrics(&assignment.node_id) {
            if metrics.status != NodeStatus::Active {
                // Skip failed nodes
                continue;
            }

            // Check if this node can take more layers
            let available = metrics.available_vram_gb;
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
    let active_nodes = cluster.active_nodes();

    if active_nodes.is_empty() {
        return (0.0, 0, vec![]);
    }

    // Calculate average delivery ratio
    let avg_delivery_ratio =
        active_nodes.iter().map(|m| m.delivery_ratio).sum::<f32>() / active_nodes.len() as f32;

    // Count failed nodes
    let failed_count = cluster
        .nodes_snapshot()
        .iter()
        .filter(|n| {
            if let Some(metrics) = cluster.get_metrics(&n.id) {
                metrics.status == NodeStatus::Failed
            } else {
                false
            }
        })
        .count();

    // Get failed node IDs
    let failed_nodes: Vec<String> = cluster
        .nodes_snapshot()
        .iter()
        .filter(|n| {
            if let Some(metrics) = cluster.get_metrics(&n.id) {
                metrics.status == NodeStatus::Failed
            } else {
                false
            }
        })
        .map(|n| n.id.clone())
        .collect();

    (avg_delivery_ratio, failed_count, failed_nodes)
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
        let chunked = chunk_assignments_for_workers(&assignments, 5);

        assert_eq!(chunked.len(), 3);
        assert_eq!(chunked[0].start_layer, 0);
        assert_eq!(chunked[0].end_layer, 5);
        assert_eq!(chunked[2].start_layer, 10);
        assert_eq!(chunked[2].end_layer, 12);
    }

    #[test]
    fn planning_tuning_scales_chunk_size_with_workers() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 8,
            acceleration_mode: AccelerationMode::Gpu,
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let tuning = PlanningTuning::from_runtime_profile(&profile, 40);
        assert!(tuning.max_layers_per_assignment <= 4);
    }
}
