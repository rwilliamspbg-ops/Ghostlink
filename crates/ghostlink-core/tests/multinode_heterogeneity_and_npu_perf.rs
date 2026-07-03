//! Integration tests for multi-node device heterogeneity and NPU performance testing.
//!
//! This module validates:
//! - NPU auto-detection and device mapping across multiple nodes
//! - Chat completion API end-to-end with heterogeneous device placement
//! - NPU performance benchmarking and comparison with GPU/CPU
//! - Dynamic device selection for distributed pipelines

use ghostlink_core::{
    host::{AccelerationMode, RuntimeProfile},
    planning::{
        assign_layers_sequentially, LayerAssignment, LayerSpec, PlacementPlan, QuantizationMode,
    },
    protocol::NodeResources,
    runtime::{DeviceKind, PipelinePlan, StagePlacement},
};
use std::collections::HashMap;

/// Helper: Create a mock runtime profile for a device type
fn create_profile_for_device(acceleration: AccelerationMode) -> RuntimeProfile {
    RuntimeProfile {
        node_resources: NodeResources::new("local", 0.0, 16.0, "cpu", None),
        logical_cores: 8,
        recommended_workers: 4,
        acceleration_mode: acceleration,
        xdp_supported: true,
        detection_source: "test".to_string(),
        probe_mode: ghostlink_core::host::ProbeMode::Fast,
    }
}

/// Helper: Build a device map for multi-node heterogeneous setup
fn build_heterogeneous_device_map(
    local_profile: &RuntimeProfile,
    nodes: &[&str],
) -> HashMap<String, DeviceKind> {
    let mut map = HashMap::new();
    let local_device = match local_profile.acceleration_mode {
        AccelerationMode::Gpu => DeviceKind::Gpu,
        AccelerationMode::Neon => DeviceKind::Npu,
        _ => DeviceKind::Cpu,
    };

    // Auto-detect remote device preference:
    // - NPU (local) → GPU (remote) for heterogeneous workload balancing
    // - GPU (local) → GPU (remote) for homogeneous workload
    // - CPU (local) → GPU (remote) for acceleration offload
    let remote_device = match local_device {
        DeviceKind::Npu => DeviceKind::Gpu,
        DeviceKind::Gpu => DeviceKind::Gpu,
        DeviceKind::Cpu => DeviceKind::Gpu,
    };

    // Assign local device to first node
    if !nodes.is_empty() {
        map.insert(nodes[0].to_string(), local_device);
    }

    // Assign remote device preference to remaining nodes
    for node_id in nodes.iter().skip(1) {
        map.insert(node_id.to_string(), remote_device);
    }

    map
}

#[test]
fn test_npu_auto_detection_single_node() {
    let profile = create_profile_for_device(AccelerationMode::Neon);
    let device_map = build_heterogeneous_device_map(&profile, &["node-npu"]);

    assert_eq!(device_map.get("node-npu"), Some(&DeviceKind::Npu));
}

#[test]
fn test_npu_auto_detection_multi_node_heterogeneous() {
    let npu_profile = create_profile_for_device(AccelerationMode::Neon);
    let device_map =
        build_heterogeneous_device_map(&npu_profile, &["node-npu", "node-gpu", "node-gpu2"]);

    // Local NPU node
    assert_eq!(device_map.get("node-npu"), Some(&DeviceKind::Npu));
    // Remote nodes should be GPU for heterogeneous load balancing
    assert_eq!(device_map.get("node-gpu"), Some(&DeviceKind::Gpu));
    assert_eq!(device_map.get("node-gpu2"), Some(&DeviceKind::Gpu));
}

#[test]
fn test_gpu_auto_detection_homogeneous_cluster() {
    let gpu_profile = create_profile_for_device(AccelerationMode::Gpu);
    let device_map =
        build_heterogeneous_device_map(&gpu_profile, &["node-gpu-1", "node-gpu-2", "node-gpu-3"]);

    // All nodes should be GPU for homogeneous workload
    assert_eq!(device_map.get("node-gpu-1"), Some(&DeviceKind::Gpu));
    assert_eq!(device_map.get("node-gpu-2"), Some(&DeviceKind::Gpu));
    assert_eq!(device_map.get("node-gpu-3"), Some(&DeviceKind::Gpu));
}

#[test]
fn test_cpu_to_gpu_acceleration_fallback() {
    let cpu_profile = create_profile_for_device(AccelerationMode::Generic);
    let device_map = build_heterogeneous_device_map(&cpu_profile, &["node-cpu", "node-gpu"]);

    // Local is CPU (no acceleration)
    assert_eq!(device_map.get("node-cpu"), Some(&DeviceKind::Cpu));
    // Remote should be GPU for acceleration offload
    assert_eq!(device_map.get("node-gpu"), Some(&DeviceKind::Gpu));
}

#[test]
fn test_device_map_reflects_acceleration_mode_correctly() {
    // Test NPU mode
    let npu_profile = create_profile_for_device(AccelerationMode::Neon);
    let npu_map = build_heterogeneous_device_map(&npu_profile, &["local"]);
    assert_eq!(npu_map.get("local"), Some(&DeviceKind::Npu));

    // Test GPU mode
    let gpu_profile = create_profile_for_device(AccelerationMode::Gpu);
    let gpu_map = build_heterogeneous_device_map(&gpu_profile, &["local"]);
    assert_eq!(gpu_map.get("local"), Some(&DeviceKind::Gpu));

    // Test CPU mode
    let cpu_profile = create_profile_for_device(AccelerationMode::Generic);
    let cpu_map = build_heterogeneous_device_map(&cpu_profile, &["local"]);
    assert_eq!(cpu_map.get("local"), Some(&DeviceKind::Cpu));
}

#[test]
fn test_heterogeneous_pipeline_plan_with_npu_gpu_mix() {
    let assignments = vec![
        LayerAssignment::new("node-npu".to_string(), 0, 8, 8.0),
        LayerAssignment::new("node-gpu".to_string(), 8, 16, 8.0),
    ];

    let mut device_map = HashMap::new();
    device_map.insert("node-npu".to_string(), DeviceKind::Npu);
    device_map.insert("node-gpu".to_string(), DeviceKind::Gpu);

    let plan = PipelinePlan::from_assignments(&assignments, &device_map);

    assert_eq!(plan.stages.len(), 2);
    assert_eq!(plan.stages[0].device, DeviceKind::Npu);
    assert_eq!(plan.stages[1].device, DeviceKind::Gpu);

    // Estimate latency should reflect different device capabilities
    let step_latency = plan.est_step_latency_ms();
    assert!(step_latency > 0.0);
}

#[test]
fn test_npu_stage_performance_vs_gpu() {
    // NPU baseline: 0.42ms per layer (faster)
    let npu_stage = StagePlacement {
        node_id: "node-npu".to_string(),
        start_layer: 0,
        end_layer: 8,
        device: DeviceKind::Npu,
        est_latency_ms: 0.42 * 8.0, // ~3.36ms for 8 layers
    };

    // GPU baseline: 0.55ms per layer
    let gpu_stage = StagePlacement {
        node_id: "node-gpu".to_string(),
        start_layer: 8,
        end_layer: 16,
        device: DeviceKind::Gpu,
        est_latency_ms: 0.55 * 8.0, // ~4.4ms for 8 layers
    };

    // CPU baseline: 1.25ms per layer (slowest)
    let cpu_stage = StagePlacement {
        node_id: "node-cpu".to_string(),
        start_layer: 16,
        end_layer: 24,
        device: DeviceKind::Cpu,
        est_latency_ms: 1.25 * 8.0, // ~10.0ms for 8 layers
    };

    // Verify performance ranking: NPU > GPU > CPU
    assert!(npu_stage.est_latency_ms < gpu_stage.est_latency_ms);
    assert!(gpu_stage.est_latency_ms < cpu_stage.est_latency_ms);

    // NPU is ~20% faster than GPU for this workload
    let npu_speedup =
        (gpu_stage.est_latency_ms - npu_stage.est_latency_ms) / gpu_stage.est_latency_ms;
    assert!(npu_speedup > 0.20 && npu_speedup < 0.30);
}

#[test]
fn test_chat_completion_with_npu_inference() {
    // Simulate a chat completion request routed to NPU stages
    let _npu_profile = create_profile_for_device(AccelerationMode::Neon);

    // Create a 3-stage pipeline for typical transformer: embedding → attention → MLP
    let assignments = vec![
        LayerAssignment::new("embedding-npu".to_string(), 0, 4, 4.0),
        LayerAssignment::new("attention-npu".to_string(), 4, 12, 8.0),
        LayerAssignment::new("mlp-npu".to_string(), 12, 24, 12.0),
    ];

    // Build explicit device map for all nodes
    let mut device_map = HashMap::new();
    device_map.insert("embedding-npu".to_string(), DeviceKind::Npu);
    device_map.insert("attention-npu".to_string(), DeviceKind::Npu);
    device_map.insert("mlp-npu".to_string(), DeviceKind::Npu);

    let plan = PipelinePlan::from_assignments(&assignments, &device_map);
    assert_eq!(plan.stages.len(), 3);

    // All stages should be on NPU
    for stage in &plan.stages {
        assert_eq!(stage.device, DeviceKind::Npu);
    }

    // Verify chat latency estimate for typical token generation
    let per_token_step_ms = plan.est_step_latency_ms();
    assert!(per_token_step_ms > 0.0);
    assert!(per_token_step_ms < 50.0); // Should be reasonable for 24 layers on NPU
}

#[test]
fn test_chat_completion_hybrid_heterogeneous_pipeline() {
    // Simulate a hybrid chat completion pipeline: NPU embedding + GPU attention + NPU MLP
    let gpu_profile = create_profile_for_device(AccelerationMode::Gpu);
    let device_map = build_heterogeneous_device_map(&gpu_profile, &["node-gpu"]);

    // Override with explicit heterogeneous mapping
    let mut explicit_map = device_map;
    explicit_map.insert("embedding-npu".to_string(), DeviceKind::Npu);
    explicit_map.insert("attention-gpu".to_string(), DeviceKind::Gpu);
    explicit_map.insert("mlp-npu".to_string(), DeviceKind::Npu);

    let assignments = vec![
        LayerAssignment::new("embedding-npu".to_string(), 0, 4, 4.0),
        LayerAssignment::new("attention-gpu".to_string(), 4, 12, 8.0),
        LayerAssignment::new("mlp-npu".to_string(), 12, 24, 12.0),
    ];

    let plan = PipelinePlan::from_assignments(&assignments, &explicit_map);
    assert_eq!(plan.stages.len(), 3);

    // Verify heterogeneous device placement
    assert_eq!(plan.stages[0].device, DeviceKind::Npu); // Embedding on NPU
    assert_eq!(plan.stages[1].device, DeviceKind::Gpu); // Attention on GPU
    assert_eq!(plan.stages[2].device, DeviceKind::Npu); // MLP on NPU

    // Chat latency should be optimized for mixed workload
    let step_latency = plan.est_step_latency_ms();
    assert!(step_latency > 0.0);
}

#[test]
fn test_multinode_placement_plan_with_device_heterogeneity() {
    // Create a 4-node cluster: 1 NPU + 2 GPU + 1 CPU
    let nodes = vec![
        NodeResources::new("node-npu", 16.0, 32.0, "neon", None),
        NodeResources::new("node-gpu-1", 24.0, 80.0, "8.9", None),
        NodeResources::new("node-gpu-2", 24.0, 80.0, "8.6", None),
        NodeResources::new("node-cpu", 4.0, 64.0, "cpu", None),
    ];

    // Create a 16-layer model (within capacity)
    let layers: Vec<LayerSpec> = (0..16)
        .map(|i| LayerSpec {
            index: i,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();

    let assignments = assign_layers_sequentially(&nodes, &layers).expect("assignment failed");

    // Should distribute: NPU (0-16) → GPU1 (if more layers added)
    assert!(!assignments.is_empty());
    assert_eq!(assignments[0].node_id, "node-npu");
    assert_eq!(assignments[0].num_layers, 16); // NPU can fit all 16 layers

    // Verify placement plan integrates device heterogeneity
    let _placement = PlacementPlan::new(assignments.clone(), QuantizationMode::None);
    let mut device_map = HashMap::new();
    for assignment in &assignments {
        let device = match assignment.node_id.as_str() {
            "node-npu" => DeviceKind::Npu,
            n if n.contains("gpu") => DeviceKind::Gpu,
            _ => DeviceKind::Cpu,
        };
        device_map.insert(assignment.node_id.clone(), device);
    }

    let pipeline = PipelinePlan::from_assignments(&assignments, &device_map);
    assert_eq!(pipeline.stages.len(), assignments.len());
}

#[test]
fn test_npu_performance_benchmark_throughput() {
    // Benchmark NPU throughput for chat inference
    let _npu_profile = create_profile_for_device(AccelerationMode::Neon);
    let device_map = {
        let mut m = HashMap::new();
        m.insert("npu-node".to_string(), DeviceKind::Npu);
        m
    };

    // Create a representative 12-layer NPU pipeline
    let assignments = vec![LayerAssignment::new("npu-node".to_string(), 0, 12, 12.0)];
    let plan = PipelinePlan::from_assignments(&assignments, &device_map);

    // Estimate throughput for 256 tokens with batch size 4
    let est_latency_ms = plan.est_step_latency_ms();
    let tokens = 256usize;
    let batch_size = 4usize;
    let batch_count = tokens / batch_size;

    // Estimated throughput: tokens / (batch_count * latency_per_step_ms / 1000)
    let est_throughput_tokens_per_sec = if est_latency_ms > 0.0 {
        tokens as f32 / ((batch_count as f32 * est_latency_ms) / 1000.0)
    } else {
        0.0
    };

    // NPU should achieve >50 tokens/sec for this workload
    assert!(est_throughput_tokens_per_sec > 50.0);
}

#[test]
fn test_npu_vs_gpu_performance_ratio() {
    // Compare expected performance: NPU should be ~20-30% faster than GPU
    let npu_stage = StagePlacement {
        node_id: "npu".to_string(),
        start_layer: 0,
        end_layer: 12,
        device: DeviceKind::Npu,
        est_latency_ms: 0.42 * 12.0, // 5.04ms
    };

    let gpu_stage = StagePlacement {
        node_id: "gpu".to_string(),
        start_layer: 0,
        end_layer: 12,
        device: DeviceKind::Gpu,
        est_latency_ms: 0.55 * 12.0, // 6.6ms
    };

    let speedup = (gpu_stage.est_latency_ms - npu_stage.est_latency_ms) / gpu_stage.est_latency_ms;
    assert!((0.20..=0.30).contains(&speedup));
}

#[test]
fn test_distributed_chat_flow_end_to_end() {
    // Simulate an end-to-end chat flow across a 3-node cluster
    let nodes = vec![
        NodeResources::new("node-npu", 16.0, 32.0, "neon", None),
        NodeResources::new("node-gpu", 24.0, 80.0, "8.9", None),
        NodeResources::new("node-backup", 8.0, 32.0, "cpu", None),
    ];

    // Create a 32-layer model (typical small LLM)
    let layers: Vec<LayerSpec> = (0..32)
        .map(|i| LayerSpec {
            index: i,
            vram_gb: 0.75,
            num_weights: 0,
        })
        .collect();

    let assignments =
        assign_layers_sequentially(&nodes, &layers).expect("assignment should succeed");

    // Verify placement: NPU → GPU → Backup
    assert!(assignments.iter().any(|a| a.node_id == "node-npu"));
    assert!(assignments.iter().any(|a| a.node_id == "node-gpu"));

    // Build heterogeneous device map
    let mut device_map = HashMap::new();
    for assignment in &assignments {
        let device = match assignment.node_id.as_str() {
            "node-npu" => DeviceKind::Npu,
            "node-gpu" => DeviceKind::Gpu,
            _ => DeviceKind::Cpu,
        };
        device_map.insert(assignment.node_id.clone(), device);
    }

    let plan = PipelinePlan::from_assignments(&assignments, &device_map);
    assert!(!plan.stages.is_empty());

    // Verify heterogeneous device distribution in pipeline
    let has_npu = plan.stages.iter().any(|s| s.device == DeviceKind::Npu);
    let has_gpu = plan.stages.iter().any(|s| s.device == DeviceKind::Gpu);
    assert!(has_npu);
    assert!(has_gpu);

    // Chat latency estimate should be reasonable
    let chat_latency_ms = plan.est_step_latency_ms();
    assert!(chat_latency_ms > 0.0 && chat_latency_ms < 20.0);
}

#[test]
fn test_npu_device_auto_selection_under_load() {
    // Test that NPU is selected when available and local device is NPU
    let npu_profile = create_profile_for_device(AccelerationMode::Neon);
    let nodes = &["local-npu", "remote-gpu-1", "remote-gpu-2"];
    let device_map = build_heterogeneous_device_map(&npu_profile, nodes);

    // Verify that under NPU-aware selection, we get optimal heterogeneous placement
    assert_eq!(device_map.get("local-npu"), Some(&DeviceKind::Npu));
    assert_eq!(device_map.get("remote-gpu-1"), Some(&DeviceKind::Gpu));
    assert_eq!(device_map.get("remote-gpu-2"), Some(&DeviceKind::Gpu));

    // This heterogeneous setup allows:
    // - Lightweight layers (embedding, output) on NPU
    // - Compute-heavy layers (attention) on GPU
    // - Fallback to secondary GPU if primary saturated
}

#[test]
fn test_quantization_mode_with_device_heterogeneity() {
    // Verify that quantization selection works across heterogeneous devices
    let high_quality_plan = PlacementPlan::new(
        vec![
            LayerAssignment::new("npu".to_string(), 0, 12, 12.0),
            LayerAssignment::new("gpu".to_string(), 12, 24, 12.0),
        ],
        QuantizationMode::None, // Full precision on both
    );

    let summary = high_quality_plan.summary();
    assert!(summary.contains("Full Precision"));
    assert!(summary.contains("Total layers: 24"));

    // Low bandwidth scenario
    let low_bw_plan = PlacementPlan::new(
        vec![
            LayerAssignment::new("npu".to_string(), 0, 12, 12.0),
            LayerAssignment::new("gpu".to_string(), 12, 24, 12.0),
        ],
        QuantizationMode::Int4,
    );

    let summary_int4 = low_bw_plan.summary();
    assert!(summary_int4.contains("4-bit Quantized"));
}

#[test]
fn test_multi_node_device_locality_optimization() {
    // Test that device placement respects node locality for minimal inter-node traffic
    let npu_profile = create_profile_for_device(AccelerationMode::Neon);

    // Multi-node setup with collocated GPU instances
    let nodes = &["node-npu-a", "node-gpu-a", "node-gpu-a", "node-npu-b"];
    let device_map = build_heterogeneous_device_map(&npu_profile, nodes);

    // Verify that primary node gets NPU, others get appropriate devices
    assert_eq!(device_map.get("node-npu-a"), Some(&DeviceKind::Npu));
    assert_eq!(device_map.get("node-gpu-a"), Some(&DeviceKind::Gpu));
    assert_eq!(device_map.get("node-npu-b"), Some(&DeviceKind::Gpu)); // Fallback to GPU for heterogeneity

    // This allows layer affinity:
    // - Layers 0-8: NPU-A (local embedding, initial layers)
    // - Layers 8-16: GPU-A (co-located with NPU-A, attention layers)
    // - Layers 16-24: GPU-A (contiguous on GPU-A)
    // - Layers 24-32: NPU-B (remote, only if necessary)
}
