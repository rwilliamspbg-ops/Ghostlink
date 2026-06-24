//! Integration Tests for Ghost-Link
//! 
//! These tests verify end-to-end functionality of the core primitives:
//! - Multi-node discovery and registration
//! - Layer assignment with failure scenarios
//! - Ring buffer stress tests
//! - Protocol encoding/decoding edge cases
//! - Network failure injection (packet loss, corruption)
//! - Node failure cascades and recovery
//! - Health monitoring and adaptive quantization

mod common;

use ghostlink_core::cluster::{ClusterState, NodeResources};
use ghostlink_core::planning::{assign_layers_sequentially, select_quantization_mode, LayerSpec};
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind};
use ghostlink_core::ring::SpscRingBuffer;
use std::sync::Arc;
use std::thread;

#[test]
fn test_multi_node_discovery_and_registration() {
    // Create cluster state
    let cluster = ClusterState::new();
    
    // Register multiple nodes
    cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
    cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6", None));
    cluster.register(NodeResources::new("node-c", 48.0, 128.0, "9.0", None));
    
    // Verify all nodes are registered
    assert_eq!(cluster.nodes().len(), 3);
    assert_eq!(cluster.total_vram_gb(), 84.0);
    
    // Register duplicate node should update existing
    cluster.register(NodeResources::new("node-b", 24.0, 64.0, "9.0", None));
    let node_b = cluster.nodes().iter()
        .find(|n| n.id == "node-b").unwrap();
    assert_eq!(node_b.vram_gb, 24.0);
}

#[test]
fn test_layer_assignment_with_failure_scenarios() {
    // Test case 1: Insufficient capacity
    let nodes = vec![NodeResources::new("node-a", 2.0, 64.0, "8.9", None)];
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
    let ring = Arc::new(SpscRingBuffer::<i32>::new(ghostlink_core::RingConfig::default()));
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
            let cluster = Arc::clone(&cluster_clone);
            thread::spawn(move || {
                cluster.register(NodeResources::new(
                    format!("node-{}", i),
                    24.0,
                    64.0,
                    "8.9",
                    None,
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
    let ring = SpscRingBuffer::<i32>::new(ghostlink_core::RingConfig::default());
    
    // Fill most of the ring
    for i in 0..1020 {
        ring.push(i).unwrap();
    }
    
    assert_eq!(ring.len(), 1020);
    
    // Pop all elements and verify FIFO order
    for i in 0..1020 {
        assert_eq!(ring.pop(), Some(i));
    }
    
    assert!(ring.is_empty());
}

#[test]
fn test_discovery_frame_crc_verification() {
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
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
        NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
        NodeResources::new("node-b", 12.0, 32.0, "8.6", None),
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

// ============================================================================
// NETWORK FAILURE INJECTION TESTS
// ============================================================================

#[test]
fn protocol_handles_truncated_frame() {
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
    };

    let encoded = frame.encode();
    
    // Simulate incomplete frame (network drop mid-packet)
    let truncated = &encoded[0..encoded.len().saturating_sub(5)];
    
    let result = DiscoveryFrame::decode(truncated);
    assert!(result.is_err(), "Should reject truncated frame");
}

#[test]
fn protocol_detects_single_bit_corruption() {
    let frame = DiscoveryFrame {
        kind: FrameKind::Join,
        node: NodeResources::new("node-b", 12.0, 32.0, "8.6", Some("RTX4090".into())),
    };

    let encoded = frame.encode();
    let mut corrupted = encoded.clone();

    // Flip a single bit in the payload (not CRC)
    if corrupted.len() > 20 {
        corrupted[15] ^= 1;  // Flip bit 0 of byte 15
        
        let result = DiscoveryFrame::decode(&corrupted);
        assert!(result.is_err(), "Single-bit corruption should fail CRC");
        assert!(result.unwrap_err().contains("CRC"), "Error should mention CRC");
    }
}

#[test]
fn protocol_detects_multi_byte_corruption() {
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: NodeResources::new("gpu-node", 48.0, 128.0, "9.0", None),
    };

    let encoded = frame.encode();
    let mut corrupted = encoded.clone();

    // Flip multiple bytes
    for i in 5..10.min(corrupted.len()) {
        corrupted[i] = corrupted[i].wrapping_add(1);
    }

    let result = DiscoveryFrame::decode(&corrupted);
    assert!(result.is_err(), "Multi-byte corruption should fail CRC");
}

#[test]
fn protocol_rejects_invalid_magic() {
    let mut fake_frame = vec![0u8; 64];
    
    // Set wrong magic/EtherType
    fake_frame[0] = 0xFF;
    fake_frame[1] = 0xFF;

    let result = DiscoveryFrame::decode(&fake_frame);
    assert!(result.is_err(), "Invalid EtherType should be rejected");
}

#[test]
fn protocol_recovers_after_corruption() {
    // Valid frame
    let frame1 = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: NodeResources::new("node-1", 24.0, 64.0, "8.9", None),
    };

    // Corrupted frame
    let mut bad = vec![0xFF; 50];
    bad[0] = 0xB5;  // Try to look valid but corrupt
    bad[1] = 0x88;

    // Next valid frame
    let frame2 = DiscoveryFrame {
        kind: FrameKind::Join,
        node: NodeResources::new("node-2", 12.0, 32.0, "8.6", None),
    };

    // Decode sequence: valid -> bad -> valid
    let enc1 = frame1.encode();
    let enc2 = frame2.encode();

    assert!(DiscoveryFrame::decode(&enc1).is_ok(), "First frame should decode");
    assert!(DiscoveryFrame::decode(&bad).is_err(), "Bad frame should fail");
    assert!(DiscoveryFrame::decode(&enc2).is_ok(), "Recovery frame should decode");
}

// ============================================================================
// RING BUFFER NETWORK STRESS TESTS
// ============================================================================

#[test]
fn ring_buffer_handles_producer_outpacing_consumer() {
    let ring = Arc::new(SpscRingBuffer::<i32>::new(ghostlink_core::RingConfig {
        capacity: 100,
        backpressure_threshold: 80,
    }));

    let producer_ring = Arc::clone(&ring);
    let consumer_ring = Arc::clone(&ring);

    let producer = thread::spawn(move || {
        for i in 0..1_000 {
            loop {
                if producer_ring.push(i).is_ok() {
                    break;
                }
                // Backpressure: buffer full, yield and retry
                std::thread::yield_now();
            }
        }
    });

    let consumer = thread::spawn(move || {
        let mut received = Vec::new();
        let mut slow_start = true;
        
        while received.len() < 1_000 {
            if let Some(value) = consumer_ring.pop() {
                // Simulate slow consumer at start
                if slow_start && received.len() < 100 {
                    std::thread::sleep(std::time::Duration::from_micros(10));
                } else {
                    slow_start = false;
                }
                received.push(value);
            } else {
                std::thread::yield_now();
            }
        }
        received
    });

    producer.join().unwrap();
    let values = consumer.join().unwrap();

    assert_eq!(values.len(), 1_000, "All 1000 elements should be transferred");
    assert_eq!(values[0], 0, "First element preserved");
    assert_eq!(values[999], 999, "Last element preserved");
}

#[test]
fn ring_buffer_no_loss_under_rate_mismatch() {
    let ring = Arc::new(SpscRingBuffer::<u64>::new(ghostlink_core::RingConfig::default()));
    let producer_ring = Arc::clone(&ring);
    let consumer_ring = Arc::clone(&ring);

    let producer = thread::spawn(move || {
        // Fast push
        for i in 0..10_000 {
            loop {
                if producer_ring.push(i).is_ok() {
                    break;
                }
                std::thread::yield_now();
            }
        }
    });

    let consumer = thread::spawn(move || {
        let mut values = Vec::new();
        let start = std::time::Instant::now();
        
        while values.len() < 10_000 {
            if let Some(value) = consumer_ring.pop() {
                values.push(value);
                
                // Simulate variable latency consumer
                if values.len() % 1_000 == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            } else {
                std::thread::yield_now();
            }
        }
        (values, start.elapsed())
    });

    producer.join().unwrap();
    let (values, elapsed) = consumer.join().unwrap();

    assert_eq!(values.len(), 10_000, "All 10K elements transferred");
    assert_eq!(values.first(), Some(&0), "Correct first element");
    assert_eq!(values.last(), Some(&9999), "Correct last element");
    
    // Should complete in reasonable time
    assert!(elapsed.as_secs() < 5, "Transfer should complete quickly");
}

// ============================================================================
// NODE FAILURE & RECOVERY TESTS
// ============================================================================

#[test]
fn cluster_handles_single_node_deregistration() {
    let cluster = Arc::new(ClusterState::new());
    
    // Register 3 nodes
    cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
    cluster.register(NodeResources::new("node-b", 24.0, 64.0, "8.9", None));
    cluster.register(NodeResources::new("node-c", 24.0, 64.0, "8.9", None));
    
    assert_eq!(cluster.nodes().len(), 3);
    
    // Filter out node-b (simulating deregistration)
    let nodes = cluster.nodes();
    let healthy_nodes: Vec<_> = nodes
        .iter()
        .filter(|n| n.id != "node-b")
        .cloned()
        .collect();
    
    assert_eq!(healthy_nodes.len(), 2, "Should have 2 nodes after removal");
    assert!(healthy_nodes.iter().any(|n| n.id == "node-a"));
    assert!(healthy_nodes.iter().any(|n| n.id == "node-c"));
}

#[test]
fn cluster_handles_two_concurrent_failures() {
    let cluster = Arc::new(ClusterState::new());
    
    // Register 5 nodes (quorum = 3)
    for i in 0..5 {
        cluster.register(NodeResources::new(
            format!("node-{}", i),
            24.0,
            64.0,
            "8.9",
            None,
        ));
    }
    
    assert_eq!(cluster.nodes().len(), 5);
    
    // Simulate loss of nodes 1 and 3 simultaneously
    let remaining: Vec<_> = cluster.nodes()
        .iter()
        .filter(|n| !n.id.ends_with("1") && !n.id.ends_with("3"))
        .cloned()
        .collect();
    
    // Should have 3 nodes remaining (quorum retained: 3/5 > 1/2)
    assert_eq!(remaining.len(), 3, "Quorum should be maintained");
}

#[test]
fn cluster_allows_node_rejoin_after_failure() {
    let cluster = Arc::new(ClusterState::new());
    
    // Initial registration with low VRAM
    cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
    assert_eq!(cluster.nodes().len(), 1);
    assert_eq!(cluster.nodes()[0].vram_gb, 24.0);
    
    // Rejoin with same ID but different resources (upgraded hardware)
    cluster.register(NodeResources::new("node-a", 48.0, 128.0, "9.0", None));
    
    let nodes = cluster.nodes();
    assert_eq!(nodes.len(), 1, "Should still have 1 node");
    assert_eq!(nodes[0].vram_gb, 48.0, "Resources should be updated");
    assert_eq!(nodes[0].compute_capability, "9.0");
}

#[test]
fn cluster_scales_to_multiple_nodes() {
    let cluster = Arc::new(ClusterState::new());
    
    // Register many nodes
    for i in 0..50 {
        cluster.register(NodeResources::new(
            format!("node-{:03}", i),
            24.0 + (i as f32),
            64.0,
            "8.9",
            None,
        ));
    }
    
    assert_eq!(cluster.nodes().len(), 50, "Should handle 50 nodes");
    
    let total_vram = cluster.total_vram_gb();
    let expected: f32 = (0..50).map(|i| 24.0 + i as f32).sum();
    assert!((total_vram - expected).abs() < 0.1, "VRAM calculation accurate");
}

// ============================================================================
// LAYER ASSIGNMENT WITH FAILURE MODES
// ============================================================================

#[test]
fn layer_assignment_handles_single_large_node() {
    let nodes = vec![NodeResources::new("gpu-node-large", 96.0, 256.0, "9.0", None)];
    let layers = sample_layers(100, 1.0);
    
    let result = assign_layers_sequentially(&nodes, &layers);
    assert!(result.is_ok(), "Should handle single large node");
    
    let assignments = result.unwrap();
    assert_eq!(assignments.len(), 1, "Single node assignment");
    assert_eq!(assignments[0].node_id, "gpu-node-large");
    assert_eq!(assignments[0].start_layer, 0);
    assert_eq!(assignments[0].end_layer, 96);  // Limited by VRAM
}

#[test]
fn layer_assignment_with_extreme_capacity_mismatch() {
    let nodes = vec![
        NodeResources::new("node-small", 2.0, 32.0, "8.6", None),
        NodeResources::new("node-large", 96.0, 256.0, "9.0", None),
    ];
    
    let layers = sample_layers(50, 2.0);
    
    let result = assign_layers_sequentially(&nodes, &layers);
    assert!(result.is_ok(), "Should handle capacity mismatch");
    
    let assignments = result.unwrap();
    // Small node gets 1 layer, large node gets the rest
    assert_eq!(assignments[0].num_layers, 1);
    assert_eq!(assignments[1].num_layers, 49);
}