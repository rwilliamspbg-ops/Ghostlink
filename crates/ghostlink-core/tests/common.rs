//! Shared test utilities for Ghostlink test suite
//!
//! This module provides common test helpers to eliminate duplication:
//! - Sample data generators (layers, nodes, clusters)
//! - Assertion helpers for cluster state validation
//! - Frame corruption utilities for protocol testing

use ghostlink_core::cluster::{ClusterState, NodeResources};
use ghostlink_core::planning::LayerSpec;
use std::sync::Arc;

/// Create sample layer specifications with realistic weight counts
pub fn sample_layers(count: usize, vram_gb: f32) -> Vec<LayerSpec> {
    (0..count)
        .map(|index| LayerSpec {
            index,
            vram_gb,
            num_weights: (index as u32 + 1) * 1_000_000,
        })
        .collect()
}

/// Create sample node resources with heterogeneous capacities
/// Increments VRAM by 12GB per node to simulate varied hardware
pub fn sample_nodes(count: usize, base_vram_gb: f32) -> Vec<NodeResources> {
    (0..count)
        .map(|i| {
            NodeResources::new(
                format!("node-{}", i),
                base_vram_gb + (i as f32 * 12.0),
                64.0,
                format!("8.{}", 6 + (i % 4)),
                if i % 2 == 0 {
                    Some(format!("RTX{}", 4000 + (i * 100)))
                } else {
                    None
                },
            )
        })
        .collect()
}

/// Create and populate a cluster with nodes
pub fn setup_cluster(node_count: usize, base_vram: f32) -> Arc<ClusterState> {
    let cluster = Arc::new(ClusterState::new());
    for node in sample_nodes(node_count, base_vram) {
        cluster.register(node);
    }
    cluster
}

/// Assert cluster has expected node count and total VRAM
pub fn assert_cluster_state(cluster: &ClusterState, expected_nodes: usize, expected_vram: f32) {
    let nodes = cluster.nodes();
    assert_eq!(
        nodes.len(),
        expected_nodes,
        "Expected {} nodes, got {}",
        expected_nodes,
        nodes.len()
    );

    let total = cluster.total_vram_gb();
    assert!(
        (total - expected_vram).abs() < 0.1,
        "Expected {:.1} GB total VRAM, got {:.1} GB",
        expected_vram,
        total
    );
}

/// Simulate packet corruption for protocol testing
/// Flips random bits in a frame to test corruption detection
pub fn corrupt_frame_bits(frame: &mut [u8], bit_count: usize) {
    use std::collections::HashSet;

    let total_bits = frame.len() * 8;
    let mut corrupted = HashSet::new();

    for idx in 0..bit_count.min(total_bits) {
        let bit_index = (idx * 7) % total_bits;

        let byte_idx = bit_index / 8;
        let bit_idx = bit_index % 8;

        if !corrupted.contains(&bit_index) {
            frame[byte_idx] ^= 1 << bit_idx;
            corrupted.insert(bit_index);
        }
    }
}

/// Assert delivery ratio falls within threshold
pub fn assert_delivery_ratio(ratio: f32, min: f32, max: f32, context: &str) {
    assert!(
        ratio >= min && ratio <= max,
        "[{}] Delivery ratio {:.2}% outside range [{:.2}%, {:.2}%]",
        context,
        ratio * 100.0,
        min * 100.0,
        max * 100.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_layers_generates_correct_count() {
        let layers = sample_layers(10, 2.0);
        assert_eq!(layers.len(), 10);
        assert!(layers.iter().all(|l| (l.vram_gb - 2.0).abs() < 0.001));
        assert_eq!(layers[0].num_weights, 1_000_000);
        assert_eq!(layers[9].num_weights, 10_000_000);
    }

    #[test]
    fn sample_nodes_are_heterogeneous() {
        let nodes = sample_nodes(3, 24.0);
        assert_eq!(nodes.len(), 3);

        assert_eq!(nodes[0].vram_gb, 24.0);
        assert_eq!(nodes[1].vram_gb, 36.0);
        assert_eq!(nodes[2].vram_gb, 48.0);

        assert!(nodes[0].gpu_name.is_some());
        assert!(nodes[1].gpu_name.is_none());
        assert!(nodes[2].gpu_name.is_some());
    }

    #[test]
    fn setup_cluster_registers_all_nodes() {
        let cluster = setup_cluster(5, 24.0);
        assert_cluster_state(&cluster, 5, 24.0 + 36.0 + 48.0 + 60.0 + 72.0);
    }

    #[test]
    fn corrupt_frame_bits_modifies_data() {
        let mut frame = vec![0u8; 10];
        frame[5] = 0xFF;
        let original = frame.clone();

        corrupt_frame_bits(&mut frame, 3);

        assert_ne!(frame, original);
    }

    #[test]
    fn assert_delivery_ratio_accepts_valid_range() {
        assert_delivery_ratio(0.95, 0.90, 1.00, "test_pass");
        assert_delivery_ratio(0.80, 0.75, 0.85, "boundary_low");
        assert_delivery_ratio(0.85, 0.75, 0.85, "boundary_high");
    }

    #[test]
    #[should_panic(expected = "Delivery ratio")]
    fn assert_delivery_ratio_rejects_low() {
        assert_delivery_ratio(0.70, 0.75, 0.85, "test_fail");
    }

    #[test]
    #[should_panic(expected = "Delivery ratio")]
    fn assert_delivery_ratio_rejects_high() {
        assert_delivery_ratio(0.90, 0.75, 0.85, "test_fail");
    }
}
