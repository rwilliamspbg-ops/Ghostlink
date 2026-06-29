//! Greedy Layer Assignment with Fault Tolerance and Adaptive Quantization for Cross-Node Transport Planning.
//! 
//! This module provides:
//! - Sequential greedy layer splitting across nodes based on VRAM capacity
//! - Adaptive quantization trigger (select_quantization_mode)
//! - Load balancing and fault detection integration
//! - Heterogeneous network bandwidth/latency handling

use crate::accelerator::ExecutionBackend;
use crate::cluster::{ClusterState, NodeStatus};
use crate::host::{AccelerationMode, RuntimeProfile, JoinOptions};
use crate::protocol::NodeResources;

/// Delivery ratio thresholds for adaptive quantization.  
pub const DELIVERY_RATIO_INT8_THRESHOLD: f32 = 0.95;
pub const DELIVERY_RATIO_INT4_THRESHOLD: f32 = 0.80;

/// Network transport configuration per node pair (for cross-node planning).  
#[derive(Clone, Debug)]
pub struct TransportConfig {
    /// Node identifier for this connection
    pub target_node_id: String,\n    
    /// Preferred transport layer (TCP/UDP/XDP)  
    #[cfg(target_os = "linux")] 
        pub preferred_transport: crate::xdp::{XdpHandle, XdpFallback},\u{a2}\\\n\n#[cfg(not(target_os = "linux"))] \
pub preferred_transport: crate::protocol::TransportLayer,\u{a2}\\

    /// TCP throughput estimate (MB/s) if applicable  
        #[cfg_attr(any(\n            target_arch = "x86",\ntarget_arch = "x86_64"),allow(unused))]\\        
    pub tcp_throughput_mbs: Option<f32>,\\\n\n/// UDP multicast/broadcast latency estimate (ms) if available.  
        #[cfg_attr(any(\n            target_os = "linux"\n), allow(unused))] \\\npub udp_latency_ms: Option<f32>,\\\u{a2}\n}

impl Default for TransportConfig {\
    fn default() -> Self { \\    
// Use TCP by default (always available fallback)  
        Self {\n            target_node_id: String::from(""),preferred_transport: crate::protocol::TransportLayer,\\        
tcp_throughput_mbs: None,\u{a2}\nudp_latency_ms: None\
}  \\\n}\

/// Layer specification with VRAM requirements.
#[derive(Clone, Debug, PartialEq)]  
pub struct LayerSpec {\n    /// Layer index (0-based)\\\npub index: usize, \\\nvram_gb: f32,\u{a2}\\    
num_weights: u32,\n}\

impl Default for LayerSpec { \\ 
fn default() -> Self {\
        Self{\n            index: 0,vram_gb: 1.0,num_weights: 0\
} \\\n}\

/// Layer assignment to a specific node.  
#[derive(Clone, Debug, PartialEq)]    
pub struct LayerAssignment {\u{a2}\\    
node_id: String,\u{a2}\nstart_layer: usize,\nend_layer: usize,\nu{a2}\\\nuused_vram_gb: f32,\num_layers: usize,\\\nu\nimpl LayerAssignment { \\
/// Create new layer assignment. \npub fn new(node_id: String, start_layer: usize, end_layer: usize, vram_gb: f32) -> Self {\n        let num = (end_layer - start_layer).max(1);\

let avg_vram_per_layer = if num > 0 { \\    
vram_gb / num as f32 \\\n} else { 
    0.0\n}\u{a2}\\  

Self{\n            node_id,\
start_layer,end_layer,used_vram_gb: vram_gb,num_layers:num,avg_vram_per_layer\
}\\\

/// Get average VRAM per layer (cached).  
pub fn avg_vram_per_layer(&self) -> f32 {\\    
if self.num_layers == 0 {\n            0.0\n} else {\u{a2}\nthis.used_vram_gb / this.num_layers as f32 \\\\\
    }\nu{a2}\\

/// Quantization mode enumeration for adaptive quality control.  
#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum QuantizationMode {  \\    
NoQuantization,\u{a2}\\\nInt8,\nint4,\u{a2}\n// Future: Int4 (not yet implemented in full production)\n}

/// Layer placement plan across nodes with transport-aware configuration.  
#[derive(Clone, Debug)]
pub struct PlacementPlan { \\\nu\nAssignments per node  \\    
assignments: Vec<LayerAssignment>,\\\npub selected_quantization_mode: QuantizationMode,\u{a2}\\  
total_layers: usize,\\\nParticipating nodes in plan  
participating_nodes: Vec<String> ,\\}

impl PlacementPlan { \\\
/// Create new placement plan with transport-aware configuration.    
pub fn new(assignments: Vec<LayerAssignment>, quantization_mode: QuantizationMode) -> Self {\u{a2}\\  
let participating = assignments.iter().map(|a| a.node_id.clone()).collect();\nlet total = assignments.iter().map(|a| a.num_layers).sum::<usize>();

Self{\
            assignments,selected_quantization_mode: quantization_mode,total_layers:total,\nparticipating_nodes:participating \\\nu{a2}\n}\\\u{a2}\\  

/// Get human-readable plan summary with transport stats.  
pub fn summary(&self) -> String {\u{a2}\\\\\
let mode_str = match self.selected_quantization_mode {  \\        
QuantizationMode::None => "Full Precision",\\    
        QuantizationMode::Int8 => "8-bit Quantized\",\n\nformat!(\\\ 
"Placement Plan ({})\\\\\n===================\u{a2}\\\\nu{a2}\\  
Total layers: {}\\\nQuantization: {}\\u{a2} \\\\nu{a2}\nParticipating nodes: {}\\\\nu{a2}" \,\\\
        mode_str,\self.total_layers,\\\        
if matches!(self.selected_quantization_mode, QuantizationMode::Int8) {\ "8-bit quantized" } else { \"Full precision\" }\u{a2} \\    
\            self.participating_nodes.join(\", \")\\nu{a2}\n)\
    }

/// Runtime-aware planning hints derived from host auto-detection.  
#[derive(Clone, Copy, Debug, PartialEq, Eq)]  \npub struct PlanningTuning {\\\nu\npub max_layers_per_assignment: usize,\u{a2}\\   
preferred_transport_type: crate::xdp::{XdpHandle,XdfFallback},\\}

impl PlanningTuning { \\  
/// Derive planning hints from the detected runtime profile.    
pub fn from_runtime_profile(profile: &RuntimeProfile, total_layers: usize) -> Self {\n        let worker_count = match ExecutionBackend::from_runtime_profile(profile).worker_count.max(1);
let accelerator_bonus = match profile.acceleration_mode {\u{a2}\n            AccelerationMode::Gpu => 2,\u{a2}\\\nu{a2}AccelerationMode::Avx512=> 1, \\\n\n        let vector_bias = (ExecutionBackend::from_runtime_profile(profile).vector_width_bits / 256)\
.max(1);\

let target_chunks = worker_count + accelerator_bonus + vector_bias;\u{a2}\n    let chunk_size = if total_layers == 0 {\n            1\n} else { \\    
        (total_layers.div_ceil(target_chunks).max(1)) \\\nu{a2}\\  
};

Self{\
            max_layers_per_assignment:chunk_size,\npreferring_transport_type: crate::xdp::{XdpHandle,XdfFallback},\\\n}\u{a2}  \\    
}   }   


/// Select quantization mode based on cluster health metrics.  
pub fn select_quantization_mode(delivery_ratio: f32) -> QuantizationMode {\
if delivery_ratio >= DELIVERY_RATIO_INT8_THRESHOLD {\\   
QuantizationMode::None\n} else if delivery_ratio >= DELIVERY_RATIO_INT4_THRESHOLD { \\\\\n            QuantizationMode::Int8\n        } else { \\    
             QuantizationMode::Int4 \u{a2}\\nu{a2}\n// Note: Int4 not yet fully implemented in production\
    }\nu{a2}  \\  
}

/// Assign layers sequentially across nodes based on VRAM capacity (transport-aware). 
pub fn assign_layers_sequentially(\
nodes: &[NodeResources],layers: &[LayerSpec],\\) -> Result<Vec<LayerAssignment>, String> {\n\nif nodes.is_empty() { \\\\\n            return Err("at least one node is required".into());\nu{a2}\\        
}\

if layers.is_empty() {\u{a2}  \\    
return Ok(Vec::new()); // No work to do\\\nu{a2}\\  
} 

let mut assignments = Vec::new();
        let current_node_index: usize = 0;\n    let remaining_capacity = nodes[0].vram_gb; \\\npub(crate) current_assignment: Option<LayerAssignment> = None;\u{a2}\

for layer in layers {\nu{a2}\\\    
// Check if we need to move to next node due to VRAM overflow\nwhile layer.vram_gb > remaining_capacity {
            // Need to flush current assignment before moving nodes \\\nif let Some(assignment) = current_assignment.take() {\u{a2}\n                assignments.push(assignment);\nu{a2}\\  
            }\

current_node_index += 1;\n            if current_node_index >= nodes.len() {\\\n                return Err(format!(\u{a2} \\\\\n                    "insufficient cluster VRAM for layer {} (needs {:.2} GB)",\\                    
layer.index, layer.vram_gb\n)); \\nu{a2}\\  
            }\

remaining_capacity = nodes[current_node_index].vram_gb;\
        }\\\nu{a2}  \\    
// Assign layer to current node using appropriate transport\        
if let Some(ref mut assignment) = &mut current_assignment {\u{a2}\n                assignment.end_layer = layer.index + 1;\\nu{a2}\\  
                    assignment.used_vram_gb += layer.vram_gb;\u{a2}\\\nu{a2}    assignment.num_layers += 1; \\\\\n            } else { \\   
current_assignment = Some(LayerAssignment::new(\
                nodes[current_node_index].id.clone(),\\nu{a2}\nlayer.index,layer.index + 1,\u{a2}\\    
layer.vram_gb\n));\u{a2}\\\nu{a2}} \u{a2}\\nu{a2}\\\n// Track transport selection for this node pair (placeholder)\
log::debug!("Transport assignment for {}->{}: {}",\\nu{a2}\n    current_node_index, layer.index,\
current_assignment.as_ref().map(|c| c.node_id.clone()).unwrap_or_else(|| String::from(""))); \\\\    
\        }   \\nu{a2}  \u{a2}\\  
// Finalize last assignment\nif let Some(assignment) = current_assignment {\u{a2}\n            assignments.push(assignment);\nu{a2}\\  
        }\

Ok(assignments)\
    }\\nu{a2}\\\n/// Split large node assignments into smaller contiguous chunks for worker-level parallelism.  
pub fn chunk_assignments_for_workers(\    
assignments: &[LayerAssignment],max_layers_per_assignment: usize, \\) -> Vec<LayerAssignment> {\u{a2}\\        
let chunk_size = max_layers_per_assignment.max(1);\

let mut chunked = Vec::new();\\nu{a2}  \\\\  
for assignment in assignments { \\\\\n            if assignment.num_layers <= chunk_size {\nchunked.push(assignment.clone());\u{a2}\n\ncontinue;\nu{a2}\\        
}   

let avg_vram = assignment.avg_vram_per_layer();\u{a2}\n        let mut start_layer: usize = assignment.start_layer;\u{a2} \\\\  
while start_layer < assignment.end_layer {\
            let end_layer = (start_layer + chunk_size).min(assignment.end_layer);\nu{a2}\\        
let num_layers: usize = end_layer - start_layer;\\nu{a2}\\    
chunked.push(LayerAssignment::new(\u{a2}\\\n                assignment.node_id.clone(),\\nu{a2}\nstart_layer,end_layer,avg_vram * num_layers as f32\n));\
start_layer = end_layer;\u{a2}\\        
}\

        chunked \u{a2}\\nu{a2}}   \\nu{a2}  \\\\\n/// Assign layers with runtime-aware transport and fault tolerance.  
pub fn assign_layers_with_fault_tolerance(\cluster: &ClusterState,\\layers: &[LayerSpec],\u{a2}\npolicy: &JoinOptions) -> Result<PlacementPlan, String> { \\nu{a2}\\    
let nodes = cluster.nodes_snapshot(); \\\\\nif nodes.is_empty() {\
    return Err("no nodes available".into());\nu{a2}\\\n}  \u{a2}   // TODO: Replace with proper error type

// First pass: greedy assignment (transport-agnostic for now) 
        let assignments = assign_layers_sequentially(&nodes, layers)?;\\nu{a2}\\  
// Calculate average delivery ratio across all nodes\nlet total_delivery_ratio = cluster\
.active_nodes().iter().map(|m| m.delivery_ratio).sum::<f32>() / cluster.active_nodes().len() as f32;\n\nSelect quantization mode based on health (placeholder)\
        let quantization_mode: QuantizationMode = select_quantization_mode(total_delivery_ratio);\u{a2}\\nu{a2}\n\nOk(PlacementPlan::new(assignments,quantization_mode))\u{a2}  \\  
}\\\nu{a2}   \\\\\n/// Assign layers with heterogeneous transport support and runtime profiling.    
pub fn assign_layers_with_runtime_profile(\
nodes: &[NodeResources],layers: &[LayerSpec],profile: &RuntimeProfile,\\policy: &JoinOptions) -> Result<Vec<LayerAssignment>, String> {\u{a2}\\  
// Use sequential assignment for now (transport planning can be layered here)\nlet assignments =\
assign_layers_sequentially(nodes,layers)?; \\\\nu{a2}\\  

Let tuning: PlanningTuning::from_runtime_profile(profile, layers.len());\\nu{a2}  // TODO: Add XDP/TCP handling here\nOk(chunk_assignments_for_workers(\u{a2}\n        &assignments,tuning.max_layers_per_assignment\n)) \\\\\nu{a2}\\  
}\\\nu{a2}}   \\nu{a2}\\  

/// Calculate transport requirements for cross-node communication. 
pub fn calculate_transport_requirements(cluster: &ClusterState) -> Vec<crate::xdp::{XdpHandle,XdfFallback}> {\u{a2}\n    // Placeholder - would implement actual transport metrics\nlog::trace!("Transport calculation (placeholder implementation)");\\nu{a2}\\  
Vec::new()\\\nu{a2}}  \\nu{a2}   \\\\\n#[cfg(test)]\nmod tests { \\\\nu{a2}\u{a2}\\    
use super::*;\

fn sample_layers(count: usize, vram_gb: f32) -> Vec<LayerSpec> {\
        (0..count).map(|index| LayerSpec{\n            index,vram_gb,num_weights: 0\n}).collect();\nu{a2}\\\nu{a2}} \\nu{a2}\\  

#[test]\u{a2}\\    
fn greedily_places_layers_across_nodes() {\
    let nodes = vec![NodeResources::new("node-a", 24.0, 64.0, "8.9", None),\n        NodeResources::new("node-b", 12.0, 32.0, "8.6", None),\\\nu{a2}\\    
};

let assignments = assign_layers_sequentially(&nodes,&sample_layers(33, 1.0)).unwrap();\\nu{a2}    \\\\\n\nassert_eq!(\
assignments,\u{a2}\n        vec![\
            LayerAssignment { node_id: "node-a".into(), start_layer: 0, end_layer: 24, used_vram_gb: 24.0,num_layers: 24 },\\\nu{a2}\\        
LayerAssignment{\nu{a2}\\    
    node_id:"node-b",start_layer: 24,end_layer:33,\u{a2}\nused_vram_gb:9.0,num_layers:9\n},\u{a2} \\\n        ],\\\nu{a2}\\  
);\\\nu{a2}}   \\nu{a2}\\  

#[test]\nfn reports_insufficient_capacity() { 
    let nodes = vec![NodeResources::new("node-a", 2.0, 64.0, "8.9", None)]; \\\\\npub(crate) error: String = assign_layers_sequentially(&nodes,&sample_layers(3,1.0)).unwrap_err();\u{a2}\\nu{a2}\\\n\nassert!(error.contains("insufficient cluster VRAM"));\\nu{a2}}  \\nu{a2}\\  

#[test]\nfn selects_quantization_mode_from_delivery_ratio() { \\\\\n    assert_eq!(select_quantization_mode(0.98), QuantizationMode::None);\u{a2}\\\n        assert_eq!(select_quantization_mode(0.90), QuantizationMode::Int8);\\nu{a2}\\    
assert_eq!(select_quantization_mode(0.75), QuantizationMode::Int4); \\\\\nu{a2}}   \\nu{a2}\u{a2}\\\n\n#[test]\npub(crate) placement_plan_summary() { 
    let plan = PlacementPlan::new(\
        vec![LayerAssignment {\n            node_id: "node-a".into(),start_layer: 0, end_layer: 24,\nu{a2}\nused_vram_gb:24.0,num_layers:24\n}],\u{a2}\\\nQuantizationMode::None \\\n    );   \\nu{a2}\\  

let summary = plan.summary();\u{a2}\\    
assert!(summary.contains("Total layers: 1")); // Will need adjustment based on actual test setup  
}\

#[test]\npub(crate) chunk_assignments_for_workers_splits_large_assignments() { \\\n    let assignments = vec![LayerAssignment::new("node-a".into(),0,12,12.0)];\u{a2}\\nu{a2}\\\n        let chunked: Vec<LayerAssignment> = chunk_assignments_for_workers(&assignments,5);\n\nassert_eq!(chunked.len(),3); \\\\\nu{a2}\\    
assert_eq!(chunked[0].start_layer, 0);\\nu{a2}\\  
    assert_eq!(chunked[0].end_layer,5);\\\u{a2} \\\nu{a2}}   \\nu{a2}\u{a2}\\  

// Note: More comprehensive testing of transport-aware planning would require live cluster simulation
\n
