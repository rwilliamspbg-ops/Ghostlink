//! Integration Tests for Ghost-Link
//! 
//! These tests verify end-to-end functionality of the core primitives:
//! - Multi-node discovery and registration
//! - Layer assignment with failure scenarios
//! - Ring buffer stress tests
//! - Protocol encoding/decoding edge cases

use ghostlink_core::cluster::{ClusterState, NodeResources};
use ghostlink_core::planning::{assign_layers_sequentially, select_quantization_mode, LayerSpec};
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind};
use ghostlink_core::ring::SpscRingBuffer;
use std::sync::Arc;
use std::thread;

#[test]
fn test_multi_node_discovery_and_registration() {
    // Create cluster state
    let mut cluster = ClusterState::new();
    
    // Register multiple nodes
    cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9".to_string()));
    cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6".to_string()));
    cluster.register(NodeResources::new("node-c", 48.0, 128.0, "9.0".to_string()));
    
    // Verify all nodes are registered
    assert_eq!(cluster.nodes().len(), 3);
    assert_eq!(cluster.total_vram_gb(), 84.0);
    
    // Register duplicate node should update existing
    cluster.register(NodeResources::new("node-b", 24.0, 64.0, "9.0".to_string()));
    let node_b = cluster.nodes().iter()
        .find(|n| n.id == "node-b").unwrap();
    assert_eq!(node_b.vram_gb, 24.0);
}

#[test]
fn test_layer_assignment_with_failure_scenarios() {
    // Test case 1: Insufficient capacity
    let nodes = vec![NodeResources::new("node-a", 2.0, 64.0, "8.9".to_string())];
    let layers: Vec<LayerSpec> = (0..3).map(|i| LayerSpec { index: i, vram_gb: 1.0, num_weights: 0 }).collect();
    
    let result = assign_layers_sequentially(&nodes, &layers);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("insufficient cluster VRAM"));
    
    // Test case 2: Empty layers should succeed
    let empty_layers: Vec<LayerSpec> = vec![];
    let result = assign_layers_sequentially(&nodes, &empty_layers);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}

#[test]
fn test_ring_buffer_stress() {
    let ring = Arc::new(SpscRingBuffer::<i32>::new(crate::ring::RingConfig::default()));
    let producer_ring = Arc::clone(&ring);
    let consumer_ring = Arc::clone(&ring);
    
    // Producer thread
    let producer = thread::spawn(move || {
        for value in 0..10_000 {
            loop {
                if producer_ring.push(value).is_ok() {
                    break;
                }
                std::thread::yield_now();
            }
        }
    });
    
    // Consumer thread
    let consumer = thread::spawn(move || {
        let mut values = Vec::new();
        while values.len() < 10_000 {
            if let Some(value) = consumer_ring.pop() {
                values.push(value);
            } else {
                std::thread::yield_now();
            }
        }
        values
    });
    
    producer.join().unwrap();
    let values = consumer.join().unwrap();
    
    assert_eq!(values.len(), 10_000);
    assert_eq!(values.first(), Some(&0));
    assert_eq!(values.last(), Some(&9999));
}

#[test]
fn test_protocol_encoding_decoding_edge_cases() {
    // Test case 1: Valid discovery frame
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: NodeResources::new("test-node", 24.0, 64.0, "8.9".to_string(), None),
    };
    
    let encoded = frame.encode();
    let decoded = DiscoveryFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.node.id, frame.node.id);
    assert_eq!(decoded.node.vram_gb, frame.node.vram_gb);
    
    // Test case 2: Frame with GPU name
    let frame_with_gpu = DiscoveryFrame {
        kind: FrameKind::Join,
        node: NodeResources::new("gpu-node", 48.0, 128.0, "9.0".to_string(), Some("RTX4090".to_string())),
    };
    
    let encoded_gpu = frame_with_gpu.encode();
    let decoded_gpu = DiscoveryFrame::decode(&encoded_gpu).unwrap();
    assert_eq!(decoded_gpu.node.gpu_name, Some("RTX4090".to_string()));
}

#[test]
fn test_quantization_mode_selection() {
    // Test thresholds
    assert_eq!(select_quantization_mode(0.98), ghostlink_core::planning::QuantizationMode::None);
    assert_eq!(select_quantization_mode(0.95), ghostlink_core::planning::QuantizationMode::None);
    assert_eq!(select_quantization_mode(0.94), ghostlink_core::planning::QuantizationMode::Int8);
    assert_eq!(select_quantization_mode(0.85), ghostlink_core::planning::QuantizationMode::Int8);
    assert_eq!(select_quantization_mode(0.79), ghostlink_core::planning::QuantizationMode::Int4);
    assert_eq!(select_quantization_mode(0.75), ghostlink_core::planning::QuantizationMode::Int4);
}

#[test]
fn test_cluster_state_concurrent_access() {
    let cluster = Arc::new(ClusterState::new());
    let cluster_clone = Arc::clone(&cluster);
    
    // Spawn multiple threads registering nodes
    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                cluster_clone.register(NodeResources::new(
                    format!("node-{}", i),
                    24.0,
                    64.0,
                    "8.9".to_string(),
                ));
            })
        })
        .collect();
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    // All nodes should be registered
    assert_eq!(cluster.nodes().len(), 10);
}

#[test]
fn test_ring_buffer_wrap_around() {
    let ring = SpscRingBuffer::<i32>::new(crate::ring::RingConfig::default());
    
    // Fill most of the ring
    for i in 0..1020 {
        ring.push(i).unwrap();
    }
    
    assert_eq!(ring.len(), 1020);
    
    // Pop all elements and verify order
    for i in (0..1020).rev() {
        assert_eq!(ring.pop(), Some(i));
    }
    
    assert!(ring.is_empty());
}

#[test]
fn test_discovery_frame_crc_verification() {
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: NodeResources::new("node-a", 24.0, 64.0, "8.9".to_string(), None),
    };
    
    let encoded = frame.encode();
    
    // Modify the frame to corrupt CRC
    let mut corrupted = encoded.clone();
    corrupted[10] ^= 0xFF; // Modify payload
    
    // Should fail CRC verification
    let result = DiscoveryFrame::decode(&corrupted);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("CRC mismatch"));
}

#[test]
fn test_layer_assignment_greedy_strategy() {
    let nodes = vec![
        NodeResources::new("node-a", 24.0, 64.0, "8.9".to_string()),
        NodeResources::new("node-b", 12.0, 32.0, "8.6".to_string()),
    ];

    let assignments = assign_layers_sequentially(&nodes, &sample_layers(33, 1.0)).unwrap();

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].node_id, "node-a");
    assert_eq!(assignments[0].start_layer, 0);
    assert_eq!(assignments[0].end_layer, 24);
    assert_eq!(assignments[1].node_id, "node-b");
    assert_eq!(assignments[1].start_layer, 24);
    assert_eq!(assignments[1].end_layer, 33);
}

fn sample_layers(count: usize, vram_gb: f32) -> Vec<ghostlink_core::planning::LayerSpec> {
    (0..count)
        .map(|index| ghostlink_core::planning::LayerSpec { 
            index, 
            vram_gb, 
            num_weights: 0, 
        })
        .collect()
}
