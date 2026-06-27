//! Ghost-Link CLI Demo
//!
//! Command-line interface for demonstrating Ghost-Link primitives:
//! - `plan` - Generate layer placement plan
//! - `join` - Broadcast discovery frame to join cluster
//! - `dashboard` - Display ASCII cluster dashboard

use anyhow::Result;
use ghostlink_core::cluster::{ClusterState, NodeMetrics};
use ghostlink_core::dashboard::Dashboard;
use ghostlink_core::health::NetworkHealthMonitor;
use ghostlink_core::host::{detect_runtime_profile, detect_runtime_profile_full, ProbeMode};
use ghostlink_core::load_balance::LoadBalancer;
use ghostlink_core::planning::{
    assign_layers_with_runtime_profile, select_quantization_mode, LayerSpec,
};
use ghostlink_core::protocol::NodeResources;
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind};
use ghostlink_core::runtime::{
    build_token_schedule, execute_pipeline, execute_pipeline_tcp_loopback_with_config, DeviceKind,
    PipelinePlan, TcpTransportConfig,
};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowTransportMode {
    InMemory,
    TcpLoopback,
}

impl FlowTransportMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InMemory => "inmem",
            Self::TcpLoopback => "tcp",
        }
    }
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().init();

    if let Err(err) = run() {
        eprintln!("Error: {err}");
        print_usage();
        std::process::exit(2);
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
enum CliCommand {
    Plan,
    Join {
        node_id: String,
    },
    Dashboard,
    Probe {
        node_id: String,
        mode: ProbeMode,
    },
    Flow {
        local_id: String,
        remote_id: String,
        remote_vram_gb: f32,
        remote_system_memory_gb: f32,
        execution_tokens: usize,
        micro_batch: usize,
        transport_mode: FlowTransportMode,
    },
    Help,
}

fn run() -> Result<()> {
    let command = parse_cli(std::env::args().skip(1))?;

    match command {
        CliCommand::Plan => print_plan()?,
        CliCommand::Join { node_id } => print_join(&node_id)?,
        CliCommand::Dashboard => print_dashboard()?,
        CliCommand::Probe { node_id, mode } => print_probe(&node_id, mode)?,
        CliCommand::Flow {
            local_id,
            remote_id,
            remote_vram_gb,
            remote_system_memory_gb,
            execution_tokens,
            micro_batch,
            transport_mode,
        } => print_flow(
            &local_id,
            &remote_id,
            remote_vram_gb,
            remote_system_memory_gb,
            execution_tokens,
            micro_batch,
            transport_mode,
        )?,
        CliCommand::Help => print_help(),
    }

    Ok(())
}

fn parse_cli<I>(mut args: I) -> Result<CliCommand>
where
    I: Iterator<Item = String>,
{
    let Some(command) = args.next() else {
        anyhow::bail!("missing command");
    };

    match command.as_str() {
        "plan" => Ok(CliCommand::Plan),
        "join" => Ok(CliCommand::Join {
            node_id: args.next().unwrap_or_else(|| "node-01".to_string()),
        }),
        "dashboard" => Ok(CliCommand::Dashboard),
        "probe" => {
            let node_id = args.next().unwrap_or_else(|| "local-node".to_string());
            let mode = parse_probe_mode(args.next().as_deref())?;
            Ok(CliCommand::Probe { node_id, mode })
        }
        "flow" => {
            let local_id = args.next().unwrap_or_else(|| "iprada-16gb".to_string());
            let remote_id = args.next().unwrap_or_else(|| "zenbook-32gb".to_string());
            let remote_vram_gb = args
                .next()
                .as_deref()
                .map(parse_f32_arg)
                .transpose()?
                .unwrap_or(32.0);
            let remote_system_memory_gb = args
                .next()
                .as_deref()
                .map(parse_f32_arg)
                .transpose()?
                .unwrap_or(32.0);
            let execution_tokens = args
                .next()
                .as_deref()
                .map(parse_usize_arg)
                .transpose()?
                .unwrap_or(32);
            let micro_batch = args
                .next()
                .as_deref()
                .map(parse_usize_arg)
                .transpose()?
                .unwrap_or(1)
                .max(1);
            let transport_mode = parse_flow_transport_mode(args.next().as_deref())?;

            Ok(CliCommand::Flow {
                local_id,
                remote_id,
                remote_vram_gb,
                remote_system_memory_gb,
                execution_tokens,
                micro_batch,
                transport_mode,
            })
        }
        "help" | "--help" | "-h" => Ok(CliCommand::Help),
        _ => anyhow::bail!("unknown command: {command}"),
    }
}

fn parse_probe_mode(mode: Option<&str>) -> Result<ProbeMode> {
    match mode {
        Some("--full" | "full") => Ok(ProbeMode::Full),
        Some("--fast" | "fast") | None => Ok(ProbeMode::Fast),
        Some(value) => anyhow::bail!("invalid probe mode: {value}"),
    }
}

fn parse_flow_transport_mode(value: Option<&str>) -> Result<FlowTransportMode> {
    match value {
        None | Some("tcp" | "tcp-loopback") => Ok(FlowTransportMode::TcpLoopback),
        Some("inmem" | "in-memory") => Ok(FlowTransportMode::InMemory),
        Some(other) => anyhow::bail!("invalid flow transport mode: {other}"),
    }
}

fn parse_f32_arg(value: &str) -> Result<f32> {
    value
        .parse::<f32>()
        .map_err(|_| anyhow::anyhow!("invalid numeric value: {value}"))
}

fn parse_usize_arg(value: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("invalid integer value: {value}"))
}

fn maybe_write_flow_metrics_json(
    execution: &ghostlink_core::runtime::ExecutionResult,
    transport_mode: FlowTransportMode,
) -> Result<()> {
    let Some(path) = std::env::var("GHOSTLINK_FLOW_METRICS_JSON")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        return Ok(());
    };

    let mut stage_entries = String::new();
    for (idx, stage) in execution.stage_stats.iter().enumerate() {
        if idx > 0 {
            stage_entries.push(',');
        }
        stage_entries.push_str(&format!(
            "{{\"stage_idx\":{},\"processed_batches\":{},\"avg_compute_ms\":{:.6},\"avg_recv_wait_ms\":{:.6},\"avg_send_wait_ms\":{:.6}}}",
            stage.stage_idx,
            stage.processed_batches,
            stage.avg_compute_ms,
            stage.avg_recv_wait_ms,
            stage.avg_send_wait_ms
        ));
    }

    let payload = format!(
        "{{\n  \"transport_mode\": \"{}\",\n  \"token_count\": {},\n  \"micro_batch\": {},\n  \"batch_count\": {},\n  \"stage_count\": {},\n  \"total_time_ms\": {:.6},\n  \"throughput_tokens_per_sec\": {:.6},\n  \"avg_token_latency_ms\": {:.6},\n  \"p95_token_latency_ms\": {:.6},\n  \"stage_stats\": [{}]\n}}\n",
        transport_mode.as_str(),
        execution.token_count,
        execution.micro_batch,
        execution.batch_count,
        execution.stage_count,
        execution.total_time_ms,
        execution.throughput_tokens_per_sec,
        execution.avg_token_latency_ms,
        execution.p95_token_latency_ms,
        stage_entries
    );

    fs::write(&path, payload)
        .map_err(|err| anyhow::anyhow!("failed to write flow metrics json to {}: {}", path, err))?;

    println!("Flow metrics JSON written to: {}", path);
    Ok(())
}

fn tcp_transport_config_from_env() -> TcpTransportConfig {
    let max_inflight_batches = std::env::var("GHOSTLINK_TCP_MAX_INFLIGHT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64)
        .max(1);

    let reconnect_attempts = std::env::var("GHOSTLINK_TCP_RECONNECT_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);

    let reconnect_backoff_ms = std::env::var("GHOSTLINK_TCP_RECONNECT_BACKOFF_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(25)
        .max(1);

    let auth_token = std::env::var("GHOSTLINK_TCP_AUTH_TOKEN")
        .ok()
        .filter(|v| !v.is_empty());

    TcpTransportConfig {
        max_inflight_batches,
        reconnect_attempts,
        reconnect_backoff_ms,
        auth_token,
    }
}

fn print_usage() {
    eprintln!("Usage: ghost-link <plan|join|dashboard|probe|flow|help>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  plan      - Generate layer placement plan");
    eprintln!("  join [id] - Broadcast discovery frame to join cluster");
    eprintln!("  dashboard - Display ASCII cluster dashboard");
    eprintln!(
        "  probe [id] [fast|full|--fast|--full] - Detect local workers and acceleration profile"
    );
    eprintln!(
        "  flow [local_id] [remote_id] [remote_vram_gb] [remote_mem_gb] [exec_tokens] [micro_batch] [transport=tcp|inmem] - Run full 30B planning flow"
    );
    eprintln!("  help      - Show this help message");
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
    println!(
        "  probe [id] [fast|full|--fast|--full] - Detect local workers and acceleration profile"
    );
    println!(
        "  flow [local_id] [remote_id] [remote_vram_gb] [remote_mem_gb] [exec_tokens] [micro_batch] [transport=tcp|inmem] - Run full 30B planning flow"
    );
    println!("  help      - Show this help message");
    println!();
    println!("Examples:");
    println!("  $ ghost-link plan");
    println!("  $ ghost-link join node-02");
    println!("  $ ghost-link dashboard");
    println!("  $ ghost-link probe workstation-a fast");
    println!("  $ ghost-link probe workstation-a --full");
    println!("  $ ghost-link flow iprada-16gb zenbook-32gb 32 32 64 4 tcp");
    println!("  $ ghost-link flow iprada-16gb zenbook-32gb 32 32 64 4 inmem");
}

fn print_flow(
    local_id: &str,
    remote_id: &str,
    remote_vram_gb: f32,
    remote_system_memory_gb: f32,
    execution_tokens: usize,
    micro_batch: usize,
    transport_mode: FlowTransportMode,
) -> Result<()> {
    let local_profile = detect_runtime_profile(local_id);
    let local_node = NodeResources::new(
        local_profile.node_resources.id.clone(),
        local_profile.node_resources.vram_gb.max(16.0),
        local_profile.node_resources.system_memory_gb.max(16.0),
        local_profile.node_resources.compute_capability.clone(),
        local_profile.node_resources.gpu_name.clone(),
    );

    let cluster = ClusterState::new();
    cluster.register(local_node);
    cluster.register(NodeResources::new(
        remote_id,
        remote_vram_gb,
        remote_system_memory_gb,
        "auto".to_string(),
        Some("remote-host".to_string()),
    ));

    // Seed baseline metrics so health monitor can classify status immediately.
    cluster.get_metrics_mut(local_id, |metrics| {
        metrics.record_latency(2.5);
        metrics.record_delivery_ratio(0.97);
        metrics.record_throughput(8.0);
    });
    cluster.get_metrics_mut(remote_id, |metrics| {
        metrics.record_latency(3.2);
        metrics.record_delivery_ratio(0.95);
        metrics.record_throughput(7.4);
    });

    let health_monitor =
        NetworkHealthMonitor::with_runtime_profile(Arc::new(cluster.clone()), &local_profile);
    health_monitor.check_all();

    // 30B flow baseline: approximate 60-layer plan with quantized per-layer footprint.
    let layers: Vec<LayerSpec> = (0..60)
        .map(|index| LayerSpec {
            index,
            vram_gb: 0.5,
            num_weights: 500_000_000 / 60,
        })
        .collect();

    let nodes = cluster.nodes();
    let assignments = assign_layers_with_runtime_profile(&nodes, &layers, &local_profile)
        .map_err(|e| anyhow::anyhow!(e))?;

    let device_map = build_device_map(&local_profile, local_id, remote_id);
    let pipeline_plan = PipelinePlan::from_assignments(&assignments, &device_map);
    let schedule_preview_tokens = execution_tokens.min(8);
    let token_schedule = build_token_schedule(pipeline_plan.stages.len(), schedule_preview_tokens);
    let execution = match transport_mode {
        FlowTransportMode::TcpLoopback => execute_pipeline_tcp_loopback_with_config(
            &pipeline_plan,
            execution_tokens,
            micro_batch,
            tcp_transport_config_from_env(),
        ),
        FlowTransportMode::InMemory => {
            execute_pipeline(&pipeline_plan, execution_tokens, micro_batch)
        }
    };

    let load_balancer =
        LoadBalancer::with_runtime_profile(Arc::new(cluster.clone()), &local_profile);
    let distribution = load_balancer
        .distribute_layers_with_runtime_profile(&layers, &local_profile)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("Ghost-Link 30B Multi-Host Runtime Flow\n");
    println!("====================================\n");
    println!("Local node: {}", local_profile.node_resources.id);
    println!("Remote node: {}", remote_id);
    println!(
        "Local acceleration: {}",
        local_profile.acceleration_mode.as_str()
    );
    println!("Local workers: {}", local_profile.recommended_workers);
    println!("Total cluster nodes: {}\n", cluster.node_count());

    println!("Health Summary:\n{}", health_monitor.get_health_summary());

    println!("Placement Assignments (60 layers):");
    for assignment in &assignments {
        println!(
            "- {} => layers {}-{} ({:.2} GB)",
            assignment.node_id,
            assignment.start_layer,
            assignment.end_layer,
            assignment.used_vram_gb
        );
    }

    println!("\nDistribution Summary:");
    println!("{}", distribution.summary());

    println!("{}", pipeline_plan.summary());
    println!(
        "Steady-state token schedule preview: {} operations for {} tokens across {} stages",
        token_schedule.len(),
        schedule_preview_tokens,
        pipeline_plan.stages.len()
    );
    println!("{}", execution.summary());
    maybe_write_flow_metrics_json(&execution, transport_mode)?;

    println!("Execution Modes:");
    println!("- NPU/GPU/CPU backend selection is runtime-profile driven");
    println!("- Flow currently provides transparent planning and health-driven orchestration");
    println!(
        "- Inter-stage transport mode: {} (real runtime wiring)",
        transport_mode.as_str()
    );
    println!("- Use tcp for socket-backed transport or inmem for channel-backed baseline\n");

    if matches!(transport_mode, FlowTransportMode::TcpLoopback) {
        println!(
            "TCP transport controls: GHOSTLINK_TCP_MAX_INFLIGHT, GHOSTLINK_TCP_RECONNECT_ATTEMPTS, GHOSTLINK_TCP_RECONNECT_BACKOFF_MS, GHOSTLINK_TCP_AUTH_TOKEN\n"
        );
    }

    Ok(())
}

fn build_device_map(
    local_profile: &ghostlink_core::host::RuntimeProfile,
    local_id: &str,
    remote_id: &str,
) -> HashMap<String, DeviceKind> {
    let local_device = match local_profile.acceleration_mode {
        ghostlink_core::host::AccelerationMode::Gpu => DeviceKind::Gpu,
        ghostlink_core::host::AccelerationMode::Neon => DeviceKind::Npu,
        _ => DeviceKind::Cpu,
    };

    let mut map = HashMap::new();
    map.insert(local_id.to_string(), local_device);
    map.insert(remote_id.to_string(), DeviceKind::Gpu);
    map
}

fn print_plan() -> Result<()> {
    let profile = detect_runtime_profile("planner-local");

    // Create sample nodes
    let nodes = vec![
        NodeResources::new(
            profile.node_resources.id.clone(),
            profile.node_resources.vram_gb.max(24.0),
            profile.node_resources.system_memory_gb.max(64.0),
            profile.node_resources.compute_capability.clone(),
            profile.node_resources.gpu_name.clone(),
        ),
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
    let assignments = assign_layers_with_runtime_profile(&nodes, &layers, &profile)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("Ghost-Link Layer Placement Plan\n");
    println!("================================\n");
    println!(
        "Local profile: workers={} acceleration={} XDP={}\n",
        profile.recommended_workers,
        profile.acceleration_mode.as_str(),
        if profile.xdp_supported { "on" } else { "off" }
    );

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
    let profile = detect_runtime_profile(node_id);

    // Create discovery frame with node resources
    let frame = DiscoveryFrame {
        kind: FrameKind::Join,
        node: profile.node_resources.clone(),
    };

    let encoded = frame.encode();
    let decoded = DiscoveryFrame::decode(&encoded).map_err(|e| anyhow::anyhow!(e))?;

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
    println!("  Recommended Workers: {}", profile.recommended_workers);
    println!("  Acceleration: {}", profile.acceleration_mode.as_str());

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
    let profile = detect_runtime_profile("local-dashboard");

    // Create sample cluster state
    let cluster = ClusterState::new();
    cluster.register(NodeResources::new(
        "NODE-01",
        profile.node_resources.vram_gb.max(24.0),
        profile.node_resources.system_memory_gb.max(64.0),
        profile.node_resources.compute_capability.clone(),
        profile
            .node_resources
            .gpu_name
            .clone()
            .or(Some("Local Host".to_string())),
    ));
    cluster.register(NodeResources::new(
        "NODE-02",
        12.0,
        32.0,
        "8.6",
        Some("RTX3080".to_string()),
    ));

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
    let nodes_metrics: Vec<NodeMetrics> = cluster
        .nodes_snapshot()
        .iter()
        .filter_map(|n| cluster.get_metrics(&n.id))
        .collect();

    let demo_layers: Vec<LayerSpec> = (0..33)
        .map(|index| LayerSpec {
            index,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();
    let load_balancer = LoadBalancer::with_runtime_profile(Arc::new(cluster.clone()), &profile);
    let distribution_plan =
        load_balancer.distribute_layers_with_runtime_profile(&demo_layers, &profile);

    // Create and render dashboard
    let dashboard = Dashboard::new(cluster.clone(), 63, 42, nodes_metrics);

    println!("{}", dashboard.render_ascii());
    println!(
        "\nAuto-tuned local runtime: {} workers, {} acceleration",
        profile.recommended_workers,
        profile.acceleration_mode.as_str()
    );
    if let Ok(plan) = distribution_plan {
        println!("Autotuned distribution nodes: {}", plan.distributions.len());
    }

    Ok(())
}

fn print_probe(node_id: &str, probe_mode: ProbeMode) -> Result<()> {
    let profile = match probe_mode {
        ProbeMode::Fast => detect_runtime_profile(node_id),
        ProbeMode::Full => detect_runtime_profile_full(node_id),
    };
    println!("{}", profile.summary());
    Ok(())
}

// Re-export protocol module for use in main.rs
mod protocol {
    pub use ghostlink_core::protocol::GHOSTLINK_ETHERTYPE;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> std::vec::IntoIter<String> {
        values
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn parses_known_commands() {
        assert_eq!(parse_cli(args(&["plan"])).unwrap(), CliCommand::Plan);
        assert_eq!(
            parse_cli(args(&["join", "node-a"])).unwrap(),
            CliCommand::Join {
                node_id: "node-a".to_string()
            }
        );
        assert_eq!(
            parse_cli(args(&["probe", "n1", "full"])).unwrap(),
            CliCommand::Probe {
                node_id: "n1".to_string(),
                mode: ProbeMode::Full
            }
        );
        assert_eq!(
            parse_cli(args(&["flow", "a", "b", "32", "64"])).unwrap(),
            CliCommand::Flow {
                local_id: "a".to_string(),
                remote_id: "b".to_string(),
                remote_vram_gb: 32.0,
                remote_system_memory_gb: 64.0,
                execution_tokens: 32,
                micro_batch: 1,
                transport_mode: FlowTransportMode::TcpLoopback,
            }
        );
        assert_eq!(
            parse_cli(args(&["flow", "a", "b", "32", "64", "128", "4", "inmem"])).unwrap(),
            CliCommand::Flow {
                local_id: "a".to_string(),
                remote_id: "b".to_string(),
                remote_vram_gb: 32.0,
                remote_system_memory_gb: 64.0,
                execution_tokens: 128,
                micro_batch: 4,
                transport_mode: FlowTransportMode::InMemory,
            }
        );
    }

    #[test]
    fn uses_defaults_for_optional_args() {
        assert_eq!(
            parse_cli(args(&["join"])).unwrap(),
            CliCommand::Join {
                node_id: "node-01".to_string()
            }
        );
        assert_eq!(
            parse_cli(args(&["probe"])).unwrap(),
            CliCommand::Probe {
                node_id: "local-node".to_string(),
                mode: ProbeMode::Fast
            }
        );
        assert_eq!(
            parse_cli(args(&["flow"])).unwrap(),
            CliCommand::Flow {
                local_id: "iprada-16gb".to_string(),
                remote_id: "zenbook-32gb".to_string(),
                remote_vram_gb: 32.0,
                remote_system_memory_gb: 32.0,
                execution_tokens: 32,
                micro_batch: 1,
                transport_mode: FlowTransportMode::TcpLoopback,
            }
        );
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(parse_cli(args(&[])).is_err());
        assert!(parse_cli(args(&["unknown"])).is_err());
        assert!(parse_cli(args(&["probe", "n1", "nonsense"])).is_err());
        assert!(parse_cli(args(&["flow", "a", "b", "32", "64", "bad"])).is_err());
        assert!(parse_cli(args(&["flow", "a", "b", "32", "64", "64", "2", "bad-mode"])).is_err());
    }
}
