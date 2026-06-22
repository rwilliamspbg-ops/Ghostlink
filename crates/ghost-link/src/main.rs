//! Ghost-Link CLI Demo
//! 
//! Command-line interface for demonstrating Ghost-Link primitives:
//! - `plan` - Generate layer placement plan
//! - `join` - Broadcast discovery frame to join cluster
//! - `dashboard` - Display ASCII cluster dashboard

use anyhow::Result;
use ghostlink_core::cluster::{ClusterState, NodeMetrics};
use ghostlink_core::dashboard::Dashboard;
use ghostlink_core::protocol::NodeResources;
use ghostlink_core::planning::{assign_layers_sequentially, select_quantization_mode, LayerSpec};
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind};

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().init();

    let mut args = std::env::args().skip(1);
    
    match args.next().as_deref() {
        Some("plan") => print_plan()?,
        Some("join") => print_join(args.next().as_deref().unwrap_or("node-01"))?,
        Some("dashboard") => print_dashboard()?,
        Some("help" | "--help" | "-h") => print_help(),
        _ => {
            eprintln!("Usage: ghost-link <plan|join|dashboard|help>");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  plan      - Generate layer placement plan");
            eprintln!("  join [id] - Broadcast discovery frame to join cluster");
            eprintln!("  dashboard - Display ASCII cluster dashboard");
            eprintln!("  help      - Show this help message");
        }
    }

    Ok(())
}

fn print_help() {
    println!("ghost-link CLI Demo\n");
    println!("Ghost-Link is an open-source scaffold for a zero-config LAN fabric");
    println!("that turns spare local GPUs into a shared execution surface.");
    println!();
    println!("Commands:");
    println!("  plan      - Generate layer placement plan across nodes");
    println!("  join [id] - Broadcast discovery frame to join cluster");
    println!("  dashboard - Display ASCII cluster dashboard");
    println!("  help      - Show this help message");
    println!();
    println!("Examples:");
    println!("  $ ghost-link plan");
    println!("  $ ghost-link join node-02");
    println!("  $ ghost-link dashboard");
}

fn print_plan() -> Result<()> {
    // Create sample nodes
    let nodes = vec![
        NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
        NodeResources::new("node-b", 12.0, 32.0, "8.6", None),
    ];

    // Create sample layers (Llama-7B has ~33 layers)
    let layers: Vec<LayerSpec> = (0..33)
        .map(|index| LayerSpec { 
            index, 
            vram_gb: 1.0,
            num_weights: 0, 
        })
        .collect();

    // Assign layers sequentially
    let assignments = assign_layers_sequentially(&nodes, &layers)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("Ghost-Link Layer Placement Plan\n");
    println!("================================\n");

    for assignment in &assignments {
        println!(
            "- {} => layers {}-{} ({:.1} GB)",
            assignment.node_id,
            assignment.start_layer,
            assignment.end_layer,
            assignment.used_vram_gb
        );
    }

    // Demonstrate adaptive quantization trigger
    println!("\nAdaptive Quantization Trigger:\n");
    for ratio in [0.98_f32, 0.90, 0.75] {
        println!(
            "delivery_ratio={ratio:.2} => {:?}",
            select_quantization_mode(ratio)
        );
    }

    Ok(())
}

fn print_join(node_id: &str) -> Result<()> {
    // Create discovery frame with node resources
    let frame = DiscoveryFrame {
        kind: FrameKind::Join,
        node: NodeResources::new(
            node_id, 
            12.0, 
            32.0, 
            "8.6".to_string(),
            None,
        ),
    };

    let encoded = frame.encode();
    let decoded = DiscoveryFrame::decode(&encoded)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("Broadcasting Ghost-Link Join Frame\n");
    println!("====================================\n");
    println!("Frame Size: {} bytes", encoded.len());
    println!("EtherType: 0x{:04X}", crate::protocol::GHOSTLINK_ETHERTYPE);
    println!();
    println!("Node Information:\n");
    println!("  ID: {}", decoded.node.id);
    println!("  VRAM: {:.1} GB", decoded.node.vram_gb);
    println!("  System Memory: {:.1} GB", decoded.node.system_memory_gb);
    println!("  Compute Capability: {}", decoded.node.compute_capability);

    // Show encoded frame (first 50 bytes for brevity)
    if !encoded.is_empty() {
        let preview = &encoded[..std::cmp::min(50, encoded.len())];
        println!("\nEncoded Frame Preview (hex):\n");
        for byte in preview.iter() {
            print!("{:02x} ", byte);
        }
        println!();
    }

    Ok(())
}

fn print_dashboard() -> Result<()> {
    // Create sample cluster state
    let cluster = ClusterState::new();
    cluster.register(NodeResources::new("NODE-01", 24.0, 64.0, "8.9", Some("RTX4090".to_string())));
    cluster.register(NodeResources::new("NODE-02", 12.0, 32.0, "8.6", Some("RTX3080".to_string())));

    // Update metrics for each node
    cluster.get_metrics_mut("NODE-01", |metrics| {
        metrics.record_vram_usage(22.4);
        metrics.set_streaming_layers(0, 24);
        metrics.record_latency(1.2);
        metrics.record_throughput(9.8);
    });

    cluster.get_metrics_mut("NODE-02", |metrics| {
        metrics.record_vram_usage(7.2);
    });

    // Collect nodes metrics for display
    let nodes_metrics: Vec<NodeMetrics> = cluster.nodes_snapshot()
        .iter()
        .filter_map(|n| cluster.get_metrics(&n.id))
        .collect();

    // Create and render dashboard
    let dashboard = Dashboard::new(
        cluster.clone(),
        63,
        42,
        nodes_metrics,
    );

    println!("{}", dashboard.render_ascii());

    Ok(())
}

// Re-export protocol module for use in main.rs
mod protocol {
    pub use ghostlink_core::protocol::GHOSTLINK_ETHERTYPE;
}
