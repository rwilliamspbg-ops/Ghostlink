use ghostlink_core::cluster::NodeResources;
use ghostlink_core::dashboard::{DashboardSnapshot, NodeMetrics};
use ghostlink_core::planning::{assign_layers_sequentially, select_quantization_mode, LayerSpec};
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("plan") => print_plan(),
        Some("join") => print_join(args.next().as_deref().unwrap_or("node-01")),
        Some("dashboard") => print_dashboard(),
        _ => print_help(),
    }
}

fn print_help() {
    println!("ghost-link <plan|join|dashboard>");
}

fn print_plan() {
    let nodes = vec![
        NodeResources::new("node-a", 24.0, 64.0, "8.9"),
        NodeResources::new("node-b", 12.0, 32.0, "8.6"),
    ];
    let layers = (0..33)
        .map(|index| LayerSpec {
            index,
            vram_gb: 1.0,
        })
        .collect::<Vec<_>>();

    let assignments = assign_layers_sequentially(&nodes, &layers).expect("plan should fit");

    println!("Ghost-Link layer placement plan:");
    for assignment in assignments {
        println!(
            "- {} => layers {}-{} ({:.1} GB)",
            assignment.node_id,
            assignment.start_layer,
            assignment.end_layer,
            assignment.used_vram_gb
        );
    }

    for ratio in [0.98_f32, 0.90, 0.75] {
        println!(
            "delivery_ratio={ratio:.2} => {:?}",
            select_quantization_mode(ratio)
        );
    }
}

fn print_join(node_id: &str) {
    let frame = DiscoveryFrame {
        kind: FrameKind::Join,
        node: NodeResources::new(node_id, 12.0, 32.0, "8.6"),
    };
    let encoded = frame.encode();
    let decoded = DiscoveryFrame::decode(&encoded).expect("encoded discovery frame should decode");

    println!(
        "Broadcasting Ghost-Link join frame ({} bytes)",
        encoded.len()
    );
    println!(
        "node={} vram={}GB system={}GB capability={}",
        decoded.node.id,
        decoded.node.vram_gb,
        decoded.node.system_memory_gb,
        decoded.node.compute_capability
    );
}

fn print_dashboard() {
    let snapshot = DashboardSnapshot {
        status: "ACTIVE".into(),
        ring_fill_percent: 63,
        gradient_steps: 42,
        nodes: vec![
            NodeMetrics {
                name: "NODE-01".into(),
                gpu_name: "RTX4090".into(),
                used_vram_gb: 22.4,
                total_vram_gb: 24.0,
                streaming_layers: Some((0, 24)),
                af_xdp_gbps: 9.8,
                latency_micros: 1.2,
            },
            NodeMetrics {
                name: "NODE-02".into(),
                gpu_name: "RTX3080".into(),
                used_vram_gb: 7.2,
                total_vram_gb: 10.0,
                streaming_layers: None,
                af_xdp_gbps: 0.0,
                latency_micros: 0.0,
            },
        ],
    };

    println!("{}", snapshot.render_ascii());
}
