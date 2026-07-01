//! Ghost-Link CLI Demo
//!
//! Command-line interface for demonstrating Ghost-Link primitives:
//! - `plan` - Generate layer placement plan
//! - `join` - Broadcast discovery frame to join cluster
//! - `dashboard` - Display ASCII cluster dashboard

use anyhow::Result;
use ghostlink_core::cluster::{ClusterState, NodeMetrics};
use ghostlink_core::dashboard::Dashboard;
use ghostlink_core::discovery::{
    broadcast_and_collect, respond_once, serve_discovery, serve_discovery_with_stats,
    UdpDiscoveryConfig, DEFAULT_DISCOVERY_PORT,
};
use ghostlink_core::health::NetworkHealthMonitor;
use ghostlink_core::host::{detect_runtime_profile, detect_runtime_profile_full, ProbeMode};
use ghostlink_core::load_balance::LoadBalancer;
use ghostlink_core::planning::{
    assign_layers_with_runtime_profile, select_quantization_mode, LayerSpec, PlacementPlan,
    QuantizationMode, RebalanceTrigger,
};
use ghostlink_core::protocol::NodeResources;
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind};
use ghostlink_core::runtime::{
    build_token_schedule, execute_pipeline_tcp_loopback_with_config,
    execute_pipeline_with_rebalance_and_measured, DeviceKind, PipelinePlan, TcpTransportConfig,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    flow: Option<FlowDefaults>,
    cluster_start: Option<ClusterStartDefaults>,
    discovery: Option<DiscoveryDefaults>,
    tcp: Option<TcpDefaults>,
    gui: Option<GuiDefaults>,
}

#[derive(Debug, Default, Deserialize)]
struct FlowDefaults {
    local_id: Option<String>,
    remote_id: Option<String>,
    remote_vram_gb: Option<f32>,
    remote_system_memory_gb: Option<f32>,
    execution_tokens: Option<usize>,
    micro_batch: Option<usize>,
    transport: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClusterStartDefaults {
    node_count: Option<usize>,
    base_port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
struct DiscoveryDefaults {
    listen: Option<String>,
    broadcast: Option<String>,
    timeout_ms: Option<u64>,
    auth_token: Option<String>,
    allow_legacy_crc32: Option<bool>,
    max_replies: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct TcpDefaults {
    max_inflight: Option<usize>,
    reconnect_attempts: Option<usize>,
    reconnect_backoff_ms: Option<u64>,
    auth_token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GuiDefaults {
    python: Option<String>,
}

#[derive(Debug)]
struct BootstrapArgs {
    command_args: Vec<String>,
    config_path: Option<PathBuf>,
}

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

struct FlowOptions<'a> {
    local_id: &'a str,
    remote_id: &'a str,
    remote_vram_gb: f32,
    remote_system_memory_gb: f32,
    execution_tokens: usize,
    micro_batch: usize,
    transport_mode: FlowTransportMode,
    top_k: usize,
    penalty: f32,
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().init();

    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    let bootstrap = extract_bootstrap_args(raw_args)?;

    if let Some(config_path) = resolve_config_path(bootstrap.config_path.as_deref()) {
        let config = load_file_config(&config_path)?;
        apply_file_config_to_env(&config);
        println!("Loaded config defaults from {}", config_path.display());
    }

    let command = match parse_cli(bootstrap.command_args.into_iter()) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("Error: {err}");
            print_usage();
            std::process::exit(2);
        }
    };

    if let Err(err) = execute_command(command) {
        eprintln!("Error: {err}");
        std::process::exit(2);
    }

    Ok(())
}

fn extract_bootstrap_args(args: Vec<String>) -> Result<BootstrapArgs> {
    let mut command_args = Vec::new();
    let mut config_path = None;
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];
        if arg == "--config" {
            let Some(value) = args.get(i + 1) else {
                anyhow::bail!("--config requires a path value");
            };
            config_path = Some(PathBuf::from(value));
            i += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--config=") {
            if value.is_empty() {
                anyhow::bail!("--config requires a non-empty path value");
            }
            config_path = Some(PathBuf::from(value));
            i += 1;
            continue;
        }

        command_args.push(arg.clone());
        i += 1;
    }

    Ok(BootstrapArgs {
        command_args,
        config_path,
    })
}

fn resolve_config_path(cli_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = cli_path {
        return Some(path.to_path_buf());
    }

    if let Some(path) = std::env::var("GHOSTLINK_CONFIG")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(PathBuf::from(path));
    }

    let default_path = PathBuf::from("./ghostlink.toml");
    if default_path.exists() {
        return Some(default_path);
    }

    None
}

fn load_file_config(path: &Path) -> Result<FileConfig> {
    let raw = fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("failed to read config {}: {}", path.display(), err))?;
    toml::from_str::<FileConfig>(&raw)
        .map_err(|err| anyhow::anyhow!("failed to parse config {}: {}", path.display(), err))
}

fn set_env_if_absent(key: &str, value: String) {
    if std::env::var(key)
        .ok()
        .filter(|existing| !existing.trim().is_empty())
        .is_none()
    {
        std::env::set_var(key, value);
    }
}

fn should_apply_gui_python_override(value: &str) -> bool {
    let normalized = value.trim();
    !normalized.is_empty() && !matches!(normalized, "python3" | "python")
}

fn apply_file_config_to_env(config: &FileConfig) {
    if let Some(flow) = &config.flow {
        if let Some(value) = &flow.local_id {
            set_env_if_absent("GHOSTLINK_FLOW_DEFAULT_LOCAL_ID", value.clone());
        }
        if let Some(value) = &flow.remote_id {
            set_env_if_absent("GHOSTLINK_FLOW_DEFAULT_REMOTE_ID", value.clone());
        }
        if let Some(value) = flow.remote_vram_gb {
            set_env_if_absent("GHOSTLINK_FLOW_DEFAULT_REMOTE_VRAM_GB", value.to_string());
        }
        if let Some(value) = flow.remote_system_memory_gb {
            set_env_if_absent("GHOSTLINK_FLOW_DEFAULT_REMOTE_MEM_GB", value.to_string());
        }
        if let Some(value) = flow.execution_tokens {
            set_env_if_absent("GHOSTLINK_FLOW_DEFAULT_EXEC_TOKENS", value.to_string());
        }
        if let Some(value) = flow.micro_batch {
            set_env_if_absent("GHOSTLINK_FLOW_DEFAULT_MICRO_BATCH", value.to_string());
        }
        if let Some(value) = &flow.transport {
            set_env_if_absent("GHOSTLINK_FLOW_DEFAULT_TRANSPORT", value.clone());
        }
    }

    if let Some(cluster_start) = &config.cluster_start {
        if let Some(value) = cluster_start.node_count {
            set_env_if_absent(
                "GHOSTLINK_CLUSTER_START_DEFAULT_NODE_COUNT",
                value.to_string(),
            );
        }
        if let Some(value) = cluster_start.base_port {
            set_env_if_absent(
                "GHOSTLINK_CLUSTER_START_DEFAULT_BASE_PORT",
                value.to_string(),
            );
        }
    }

    if let Some(discovery) = &config.discovery {
        if let Some(value) = &discovery.listen {
            set_env_if_absent("GHOSTLINK_DISCOVERY_LISTEN", value.clone());
        }
        if let Some(value) = &discovery.broadcast {
            set_env_if_absent("GHOSTLINK_DISCOVERY_BROADCAST", value.clone());
        }
        if let Some(value) = discovery.timeout_ms {
            set_env_if_absent("GHOSTLINK_DISCOVERY_TIMEOUT_MS", value.to_string());
        }
        if let Some(value) = &discovery.auth_token {
            set_env_if_absent("GHOSTLINK_DISCOVERY_AUTH_TOKEN", value.clone());
        }
        if let Some(value) = discovery.allow_legacy_crc32 {
            set_env_if_absent("GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32", value.to_string());
        }
        if let Some(value) = discovery.max_replies {
            set_env_if_absent("GHOSTLINK_DISCOVERY_MAX_REPLIES", value.to_string());
        }
    }

    if let Some(tcp) = &config.tcp {
        if let Some(value) = tcp.max_inflight {
            set_env_if_absent("GHOSTLINK_TCP_MAX_INFLIGHT", value.to_string());
        }
        if let Some(value) = tcp.reconnect_attempts {
            set_env_if_absent("GHOSTLINK_TCP_RECONNECT_ATTEMPTS", value.to_string());
        }
        if let Some(value) = tcp.reconnect_backoff_ms {
            set_env_if_absent("GHOSTLINK_TCP_RECONNECT_BACKOFF_MS", value.to_string());
        }
        if let Some(value) = &tcp.auth_token {
            set_env_if_absent("GHOSTLINK_TCP_AUTH_TOKEN", value.clone());
        }
    }

    if let Some(gui) = &config.gui {
        if let Some(value) = &gui.python {
            if should_apply_gui_python_override(value) {
                set_env_if_absent("GHOSTLINK_PYTHON", value.clone());
            }
        }
    }
}

fn env_default_string(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn env_default_f32(key: &str, fallback: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(fallback)
}

fn env_default_usize(key: &str, fallback: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(fallback)
}

fn env_default_u16(key: &str, fallback: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(fallback)
}

fn env_default_bool(key: &str, fallback: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(fallback)
}

#[derive(Debug, PartialEq)]
enum CliCommand {
    Plan,
    Join {
        node_id: String,
    },
    Listen {
        node_id: String,
        once: bool,
    },
    Gui {
        args: Vec<String>,
    },
    GuiCheck {
        strict: bool,
    },
    GuiDiagnose {
        strict: bool,
    },
    Doctor(DoctorOptions),
    Dashboard,
    ClusterStart {
        node_count: usize,
        base_port: u16,
    },
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
        top_k: usize,
        penalty: f32,
    },
    Serve {
        port: u16,
        host: String,
    },
    Help,
}

#[derive(Debug, PartialEq)]
struct DoctorOptions {
    strict: bool,
    json_out: Option<PathBuf>,
    network_probe: bool,
    network_target: String,
}

fn execute_command(command: CliCommand) -> Result<()> {
    match command {
        CliCommand::Plan => print_plan()?,
        CliCommand::Join { node_id } => print_join(&node_id)?,
        CliCommand::Listen { node_id, once } => print_discovery_listener(&node_id, once)?,
        CliCommand::Gui { args } => launch_mohawk_gui(&args)?,
        CliCommand::GuiCheck { strict } => print_gui_readiness(strict)?,
        CliCommand::GuiDiagnose { strict } => print_gui_diagnostics(strict)?,
        CliCommand::Doctor(options) => print_doctor_report(&options)?,
        CliCommand::Dashboard => print_dashboard()?,
        CliCommand::ClusterStart {
            node_count,
            base_port,
        } => print_cluster_start(node_count, base_port)?,
        CliCommand::Probe { node_id, mode } => print_probe(&node_id, mode)?,
        CliCommand::Flow {
            local_id,
            remote_id,
            remote_vram_gb,
            remote_system_memory_gb,
            execution_tokens,
            micro_batch,
            transport_mode,
            top_k,
            penalty,
        } => print_flow(FlowOptions {
            local_id: &local_id,
            remote_id: &remote_id,
            remote_vram_gb,
            remote_system_memory_gb,
            execution_tokens,
            micro_batch,
            transport_mode,
            top_k,
            penalty,
        })?,
        CliCommand::Serve { port, host } => start_openai_api_server(port, &host)?,
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
            node_id: args
                .next()
                .unwrap_or_else(|| env_default_string("GHOSTLINK_JOIN_DEFAULT_NODE_ID", "node-01")),
        }),
        "listen" => {
            let node_id = args.next().unwrap_or_else(|| {
                env_default_string("GHOSTLINK_LISTEN_DEFAULT_NODE_ID", "local-node")
            });
            let once = args.any(|arg| arg == "--once");
            Ok(CliCommand::Listen { node_id, once })
        }
        "gui" => Ok(CliCommand::Gui {
            args: args.collect(),
        }),
        "gui-check" => {
            let strict = args.any(|arg| arg == "--strict");
            Ok(CliCommand::GuiCheck { strict })
        }
        "gui-diagnose" => {
            let strict = args.any(|arg| arg == "--strict");
            Ok(CliCommand::GuiDiagnose { strict })
        }
        "doctor" => Ok(CliCommand::Doctor(parse_doctor_options(args)?)),
        "dashboard" => Ok(CliCommand::Dashboard),
        "cluster-start" => {
            let node_count = args
                .next()
                .as_deref()
                .map(parse_usize_arg)
                .transpose()?
                .unwrap_or_else(|| {
                    env_default_usize("GHOSTLINK_CLUSTER_START_DEFAULT_NODE_COUNT", 3)
                })
                .max(1);
            let base_port = args
                .next()
                .as_deref()
                .map(parse_u16_arg)
                .transpose()?
                .unwrap_or_else(|| {
                    env_default_u16("GHOSTLINK_CLUSTER_START_DEFAULT_BASE_PORT", 46000)
                });
            Ok(CliCommand::ClusterStart {
                node_count,
                base_port,
            })
        }
        "probe" => {
            let node_id = args.next().unwrap_or_else(|| {
                env_default_string("GHOSTLINK_PROBE_DEFAULT_NODE_ID", "local-node")
            });
            let mode = parse_probe_mode(args.next().as_deref())?;
            Ok(CliCommand::Probe { node_id, mode })
        }
        "flow" => {
            let local_id = args.next().unwrap_or_else(|| {
                env_default_string("GHOSTLINK_FLOW_DEFAULT_LOCAL_ID", "iprada-16gb")
            });
            let remote_id = args.next().unwrap_or_else(|| {
                env_default_string("GHOSTLINK_FLOW_DEFAULT_REMOTE_ID", "zenbook-32gb")
            });
            let remote_vram_gb = args
                .next()
                .as_deref()
                .map(parse_f32_arg)
                .transpose()?
                .unwrap_or_else(|| env_default_f32("GHOSTLINK_FLOW_DEFAULT_REMOTE_VRAM_GB", 32.0));
            let remote_system_memory_gb = args
                .next()
                .as_deref()
                .map(parse_f32_arg)
                .transpose()?
                .unwrap_or_else(|| env_default_f32("GHOSTLINK_FLOW_DEFAULT_REMOTE_MEM_GB", 32.0));
            let execution_tokens = args
                .next()
                .as_deref()
                .map(parse_usize_arg)
                .transpose()?
                .unwrap_or_else(|| env_default_usize("GHOSTLINK_FLOW_DEFAULT_EXEC_TOKENS", 32));
            let micro_batch = args
                .next()
                .as_deref()
                .map(parse_usize_arg)
                .transpose()?
                .unwrap_or_else(|| env_default_usize("GHOSTLINK_FLOW_DEFAULT_MICRO_BATCH", 1))
                .max(1);
            let env_transport = std::env::var("GHOSTLINK_FLOW_DEFAULT_TRANSPORT").ok();
            let cli_transport = args.next();
            let transport_mode =
                parse_flow_transport_mode(cli_transport.as_deref().or(env_transport.as_deref()))?;

            Ok(CliCommand::Flow {
                local_id,
                remote_id,
                remote_vram_gb,
                remote_system_memory_gb,
                execution_tokens,
                micro_batch,
                transport_mode,
                top_k: 40,
                penalty: 1.1,
            })
        }
        "serve" => {
            let host = args.next().unwrap_or_else(|| "127.0.0.1".to_string());
            let port = args
                .next()
                .as_deref()
                .map(parse_u16_arg)
                .transpose()?
                .unwrap_or(8000);
            Ok(CliCommand::Serve { host, port })
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

fn parse_u16_arg(value: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .map_err(|_| anyhow::anyhow!("invalid port value: {value}"))
}

fn parse_doctor_options<I>(args: I) -> Result<DoctorOptions>
where
    I: Iterator<Item = String>,
{
    let mut strict = false;
    let mut json_out = None;
    let mut network_probe = false;
    let mut network_target = "127.0.0.1:8003".to_string();

    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--strict" => strict = true,
            "--network-probe" => network_probe = true,
            "--json" => {
                let Some(path) = iter.next() else {
                    anyhow::bail!("--json requires a file path");
                };
                if path.trim().is_empty() {
                    anyhow::bail!("--json requires a non-empty file path");
                }
                json_out = Some(PathBuf::from(path));
            }
            "--network-target" => {
                let Some(target) = iter.next() else {
                    anyhow::bail!("--network-target requires a host:port value");
                };
                if target.trim().is_empty() {
                    anyhow::bail!("--network-target requires a non-empty host:port value");
                }
                network_target = target;
            }
            _ if arg.starts_with("--json=") => {
                let value = arg.trim_start_matches("--json=");
                if value.trim().is_empty() {
                    anyhow::bail!("--json requires a non-empty file path");
                }
                json_out = Some(PathBuf::from(value));
            }
            _ if arg.starts_with("--network-target=") => {
                let value = arg.trim_start_matches("--network-target=");
                if value.trim().is_empty() {
                    anyhow::bail!("--network-target requires a non-empty host:port value");
                }
                network_target = value.to_string();
            }
            _ => anyhow::bail!("unknown doctor option: {}", arg),
        }
    }

    Ok(DoctorOptions {
        strict,
        json_out,
        network_probe,
        network_target,
    })
}

fn maybe_write_flow_metrics_json(
    execution: &ghostlink_core::runtime::ExecutionResult,
    transport_mode: FlowTransportMode,
    tcp_config: Option<&TcpTransportConfig>,
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
            "{{\"stage_idx\":{},\"processed_batches\":{},\"avg_compute_ms\":{:.6},\"avg_recv_wait_ms\":{:.6},\"avg_send_wait_ms\":{:.6},\"avg_bridge_write_ms\":{:.6},\"avg_bridge_read_ms\":{:.6}}}",
            stage.stage_idx,
            stage.processed_batches,
            stage.avg_compute_ms,
            stage.avg_recv_wait_ms,
            stage.avg_send_wait_ms,
            stage.avg_bridge_write_ms,
            stage.avg_bridge_read_ms
        ));
    }

    let tcp_max_inflight = tcp_config
        .map(|cfg| cfg.max_inflight_batches.to_string())
        .unwrap_or_else(|| "null".to_string());
    let tcp_reconnect_attempts = tcp_config
        .map(|cfg| cfg.reconnect_attempts.to_string())
        .unwrap_or_else(|| "null".to_string());
    let tcp_reconnect_backoff_ms = tcp_config
        .map(|cfg| cfg.reconnect_backoff_ms.to_string())
        .unwrap_or_else(|| "null".to_string());

    let payload = format!(
        "{{\n  \"transport_mode\": \"{}\",\n  \"token_count\": {},\n  \"micro_batch\": {},\n  \"batch_count\": {},\n  \"stage_count\": {},\n  \"total_time_ms\": {:.6},\n  \"throughput_tokens_per_sec\": {:.6},\n  \"avg_token_latency_ms\": {:.6},\n  \"p95_token_latency_ms\": {:.6},\n  \"tcp_max_inflight_batches\": {},\n  \"tcp_reconnect_attempts\": {},\n  \"tcp_reconnect_backoff_ms\": {},\n  \"stage_stats\": [{}]\n}}\n",
        transport_mode.as_str(),
        execution.token_count,
        execution.micro_batch,
        execution.batch_count,
        execution.stage_count,
        execution.total_time_ms,
        execution.throughput_tokens_per_sec,
        execution.avg_token_latency_ms,
        execution.p95_token_latency_ms,
        tcp_max_inflight,
        tcp_reconnect_attempts,
        tcp_reconnect_backoff_ms,
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
        .unwrap_or(512)
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
        ..Default::default()
    }
}

fn is_env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn normalize_tcp_autotune_candidates(parsed: Vec<usize>, base_inflight: usize) -> Vec<usize> {
    let mut unique = if parsed.is_empty() {
        let base_inflight = base_inflight.max(1);
        let mut defaults = vec![32, 64, 128, 256, base_inflight];
        if base_inflight > 32 {
            defaults.push((base_inflight / 2).max(1));
        }
        if let Some(double_inflight) = base_inflight.checked_mul(2) {
            defaults.push(double_inflight);
        }
        defaults
    } else {
        parsed
    };

    unique.retain(|value| *value > 0);
    unique.sort_unstable();
    unique.dedup();
    unique
}

fn tcp_autotune_candidates_from_env(base_inflight: usize) -> Vec<usize> {
    let parsed = std::env::var("GHOSTLINK_TCP_AUTOTUNE_CANDIDATES")
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|part| part.trim().parse::<usize>().ok())
                .filter(|value| *value > 0)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    normalize_tcp_autotune_candidates(parsed, base_inflight)
}

fn tcp_autotune_cache_path() -> PathBuf {
    std::env::var("GHOSTLINK_TCP_AUTOTUNE_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./tmp/tcp_autotune_cache.tsv"))
}

fn tcp_autotune_key(
    plan: &PipelinePlan,
    tune_tokens: usize,
    tune_micro_batch: usize,
    candidates: &[usize],
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tune_tokens.hash(&mut hasher);
    tune_micro_batch.hash(&mut hasher);
    candidates.hash(&mut hasher);
    plan.stages.len().hash(&mut hasher);
    for stage in &plan.stages {
        stage.node_id.hash(&mut hasher);
        stage.start_layer.hash(&mut hasher);
        stage.end_layer.hash(&mut hasher);
        stage.device.as_str().hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn load_cached_autotune_inflight(cache_key: &str, candidates: &[usize]) -> Option<usize> {
    let cache_path = tcp_autotune_cache_path();
    let raw = fs::read_to_string(cache_path).ok()?;
    for line in raw.lines() {
        let mut parts = line.splitn(2, '\t');
        let Some(key) = parts.next() else {
            continue;
        };
        let Some(value) = parts.next() else {
            continue;
        };
        if key != cache_key {
            continue;
        }
        let parsed = value.trim().parse::<usize>().ok()?;
        if candidates.contains(&parsed) {
            return Some(parsed);
        }
    }
    None
}

fn store_cached_autotune_inflight(cache_key: &str, inflight: usize) -> Result<()> {
    let cache_path = tcp_autotune_cache_path();
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            anyhow::anyhow!(
                "failed to create autotune cache directory {}: {}",
                parent.display(),
                err
            )
        })?;
    }

    let mut lines = Vec::new();
    if let Ok(existing) = fs::read_to_string(&cache_path) {
        for line in existing.lines() {
            if let Some((key, _)) = line.split_once('\t') {
                if key == cache_key {
                    continue;
                }
            }
            lines.push(line.to_string());
        }
    }
    lines.push(format!("{}\t{}", cache_key, inflight));
    fs::write(&cache_path, lines.join("\n") + "\n").map_err(|err| {
        anyhow::anyhow!(
            "failed to write autotune cache {}: {}",
            cache_path.display(),
            err
        )
    })
}

fn autotune_tcp_transport_config(
    plan: &PipelinePlan,
    execution_tokens: usize,
    micro_batch: usize,
    base: TcpTransportConfig,
) -> Result<TcpTransportConfig> {
    let tune_tokens = std::env::var("GHOSTLINK_TCP_AUTOTUNE_TOKENS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64)
        .max(16)
        .min(execution_tokens.max(16));
    let tune_micro_batch = std::env::var("GHOSTLINK_TCP_AUTOTUNE_MICRO_BATCH")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(micro_batch)
        .max(1);
    let tune_runs = std::env::var("GHOSTLINK_TCP_AUTOTUNE_RUNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);
    let candidates = tcp_autotune_candidates_from_env(base.max_inflight_batches);
    let refresh_cache = is_env_truthy("GHOSTLINK_TCP_AUTOTUNE_REFRESH");
    let cache_key = tcp_autotune_key(plan, tune_tokens, tune_micro_batch, &candidates);

    if !refresh_cache {
        if let Some(cached_inflight) = load_cached_autotune_inflight(&cache_key, &candidates) {
            let mut cached_cfg = base.clone();
            cached_cfg.max_inflight_batches = cached_inflight;
            println!(
                "TCP autotune reused cached max_inflight={} (key={})",
                cached_inflight, cache_key
            );
            return Ok(cached_cfg);
        }
    }

    let mut best_cfg = base.clone();
    let mut best_throughput = 0.0_f32;
    let mut best_p95 = f32::MAX;
    for candidate in candidates {
        let mut candidate_cfg = base.clone();
        candidate_cfg.max_inflight_batches = candidate;
        let mut throughput_sum = 0.0_f32;
        let mut p95_sum = 0.0_f32;
        for _ in 0..tune_runs {
            let sample = execute_pipeline_tcp_loopback_with_config(
                plan,
                tune_tokens,
                tune_micro_batch,
                candidate_cfg.clone(),
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            throughput_sum += sample.throughput_tokens_per_sec;
            p95_sum += sample.p95_token_latency_ms;
        }

        let avg_throughput = throughput_sum / tune_runs as f32;
        let avg_p95 = p95_sum / tune_runs as f32;
        if avg_throughput > best_throughput
            || ((avg_throughput - best_throughput).abs() <= 0.01 && avg_p95 < best_p95)
        {
            best_throughput = avg_throughput;
            best_p95 = avg_p95;
            best_cfg = candidate_cfg;
        }
    }

    println!(
        "TCP autotune selected max_inflight={} from candidate sweep (avg throughput {:.2} tok/s, avg p95 {:.2} ms, runs={})",
        best_cfg.max_inflight_batches, best_throughput, best_p95, tune_runs
    );

    let _ = store_cached_autotune_inflight(&cache_key, best_cfg.max_inflight_batches);

    Ok(best_cfg)
}

fn print_usage() {
    eprintln!(
        "Usage: ghost-link [--config <path>] <plan|join|listen|gui|gui-check|gui-diagnose|doctor|dashboard|cluster-start|probe|flow|help>"
    );
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  plan      - Generate layer placement plan");
    eprintln!("  join [id] - Broadcast discovery frame to join cluster");
    eprintln!("  listen [id] [--once] - Reply to UDP discovery requests");
    eprintln!("  gui [args...] - Launch vendored Mohawk GUI (Python/PyQt6)");
    eprintln!("  gui-check [--strict] - Validate GUI readiness and dependencies");
    eprintln!("  gui-diagnose [--strict] - Emit categorized GUI diagnostics report");
    eprintln!(
        "  doctor [--strict] [--json <path>] [--network-probe] [--network-target <host:port>] - Run unified troubleshooting checks"
    );
    eprintln!("  dashboard - Display ASCII cluster dashboard");
    eprintln!(
        "  cluster-start [node_count] [base_port] - Start local discovery listeners and run a quick join/reply validation"
    );
    eprintln!(
        "  probe [id] [fast|full|--fast|--full] - Detect local workers and acceleration profile"
    );
    eprintln!(
        "  flow [local_id] [remote_id] [remote_vram_gb] [remote_mem_gb] [exec_tokens] [micro_batch] [transport=tcp|inmem] - Run full 30B planning flow"
    );
    eprintln!("  help      - Show this help message");
    eprintln!();
    eprintln!("Config:");
    eprintln!("  --config <path> - Load default values from a TOML config file");
    eprintln!("  Env fallback     - Set GHOSTLINK_CONFIG to a config file path");
}

fn print_help() {
    println!("ghost-link CLI Demo\n");
    println!("Ghost-Link is an open-source scaffold for a zero-config LAN fabric");
    println!("that turns spare local GPUs into a shared execution surface.");
    println!();
    println!("Commands:");
    println!("  plan      - Generate layer placement plan across nodes");
    println!("  join [id] - Broadcast discovery frame to join cluster");
    println!("  listen [id] [--once] - Reply to UDP discovery requests");
    println!("  gui [args...] - Launch vendored Mohawk GUI (Python/PyQt6)");
    println!("  gui-check [--strict] - Validate GUI readiness and dependencies");
    println!("  gui-diagnose [--strict] - Emit categorized GUI diagnostics report");
    println!(
        "  doctor [--strict] [--json <path>] [--network-probe] [--network-target <host:port>] - Run unified troubleshooting checks"
    );
    println!("  dashboard - Display ASCII cluster dashboard");
    println!(
        "  cluster-start [node_count] [base_port] - Start local discovery listeners and run a quick join/reply validation"
    );
    println!(
        "  probe [id] [fast|full|--fast|--full] - Detect local workers and acceleration profile"
    );
    println!(
        "  flow [local_id] [remote_id] [remote_vram_gb] [remote_mem_gb] [exec_tokens] [micro_batch] [transport=tcp|inmem] - Run full 30B planning flow"
    );
    println!("  serve [host] [port] - Start OpenAI-compatible API server");
    println!("  help      - Show this help message");
    println!();
    println!("Config:");
    println!("  --config <path> - Load default values from a TOML config file");
    println!("  Env fallback     - Set GHOSTLINK_CONFIG to a config file path");
    println!();
    println!("Examples:");
    println!("  $ ghost-link plan");
    println!("  $ ghost-link join node-02");
    println!("  $ ghost-link listen workstation-a --once");
    println!("  $ ghost-link gui --host 0.0.0.0 --port 8003");
    println!("  $ ghost-link gui-check --strict");
    println!("  $ ghost-link gui-diagnose --strict");
    println!("  $ ghost-link doctor --strict");
    println!("  $ ghost-link doctor --strict --json ./tmp/doctor-report.json");
    println!("  $ ghost-link doctor --network-probe --network-target 127.0.0.1:8003");
    println!("  $ ghost-link dashboard");
    println!("  $ ghost-link cluster-start 3 46000");
    println!("  $ ghost-link --config ./ghostlink.toml flow");
    println!("  $ ghost-link probe workstation-a fast");
    println!("  $ ghost-link probe workstation-a --full");
    println!("  $ ghost-link flow iprada-16gb zenbook-32gb 32 32 64 4 tcp");
    println!("  $ ghost-link flow iprada-16gb zenbook-32gb 32 32 64 4 inmem");
}

fn print_flow(opts: FlowOptions) -> Result<()> {
    let local_profile = detect_runtime_profile(opts.local_id);
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
        opts.remote_id,
        opts.remote_vram_gb,
        opts.remote_system_memory_gb,
        "auto".to_string(),
        Some("remote-host".to_string()),
    ));

    // Seed baseline metrics so health monitor can classify status immediately.
    cluster.get_metrics_mut(opts.local_id, |metrics| {
        metrics.record_latency(2.5);
        metrics.record_delivery_ratio(0.97);
        metrics.record_throughput(8.0);
    });
    cluster.get_metrics_mut(opts.remote_id, |metrics| {
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

    let device_map = build_device_map(&local_profile, opts.local_id, opts.remote_id);
    let pipeline_plan = PipelinePlan::from_assignments(&assignments, &device_map);
    let placement_context = PlacementPlan::new(assignments.clone(), QuantizationMode::None);
    let rebalance_trigger = RebalanceTrigger::default();

    let schedule_preview_tokens = opts.execution_tokens.min(8);
    let token_schedule = build_token_schedule(pipeline_plan.stages.len(), schedule_preview_tokens);
    let mut selected_tcp_cfg: Option<TcpTransportConfig> = None;
    let execution = match opts.transport_mode {
        FlowTransportMode::TcpLoopback => {
            let base_tcp_cfg = tcp_transport_config_from_env();
            let tcp_cfg = if is_env_truthy("GHOSTLINK_TCP_AUTOTUNE") {
                autotune_tcp_transport_config(
                    &pipeline_plan,
                    opts.execution_tokens,
                    opts.micro_batch,
                    base_tcp_cfg,
                )?
            } else {
                base_tcp_cfg
            };
            selected_tcp_cfg = Some(tcp_cfg.clone());
            execute_pipeline_tcp_loopback_with_config(
                &pipeline_plan,
                opts.execution_tokens,
                opts.micro_batch,
                tcp_cfg,
            )
        }
        FlowTransportMode::InMemory => Ok(execute_pipeline_with_rebalance_and_measured(
            &pipeline_plan,
            opts.execution_tokens,
            opts.micro_batch,
            Some(&rebalance_trigger),
            Some(&cluster),
            Some(&placement_context),
        )),
    };

    let load_balancer =
        LoadBalancer::with_runtime_profile(Arc::new(cluster.clone()), &local_profile);
    let distribution = load_balancer
        .distribute_layers_with_runtime_profile(&layers, &local_profile)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("Ghost-Link 30B Multi-Host Runtime Flow\n");
    println!("====================================\n");
    println!("Local node: {}", local_profile.node_resources.id);
    println!("Remote node: {}", opts.remote_id);
    println!(
        "Local acceleration: {}",
        local_profile.acceleration_mode.as_str()
    );
    println!("Local workers: {}", local_profile.recommended_workers);
    println!("Total cluster nodes: {}\n", cluster.node_count());

    println!("Health Summary:\n{}", health_monitor.get_health_summary());

    if is_env_truthy("GHOSTLINK_DISTRIBUTED_SMOKE") {
        println!("Running Distributed Runtime Validation...");
        let placement = PlacementPlan::new(assignments.clone(), QuantizationMode::None);
        let dist_execution = ghostlink_core::runtime::execute_pipeline_distributed(
            &pipeline_plan,
            opts.execution_tokens,
            opts.micro_batch,
            tcp_transport_config_from_env(),
            &cluster,
            Some(&placement),
            None,
        )
        .map_err(|e| anyhow::anyhow!(e))?;
        println!("Distributed Smoke Result:");
        println!("{}", dist_execution.summary());
    }

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
    println!(
        "Inference Parameters: top_k={} penalty={:.1}",
        opts.top_k, opts.penalty
    );
    let execution = execution.map_err(|e| anyhow::anyhow!(e))?;
    println!("{}", execution.summary());
    maybe_write_flow_metrics_json(&execution, opts.transport_mode, selected_tcp_cfg.as_ref())?;

    println!("Execution Modes:");
    println!("- NPU/GPU/CPU backend selection is runtime-profile driven");
    println!("- Flow currently provides transparent planning and health-driven orchestration");
    println!(
        "- Inter-stage transport mode: {} (real runtime wiring)",
        opts.transport_mode.as_str()
    );
    println!("- Use tcp for socket-backed transport or inmem for channel-backed baseline\n");

    if matches!(opts.transport_mode, FlowTransportMode::TcpLoopback) {
        println!(
            "TCP transport controls: GHOSTLINK_TCP_MAX_INFLIGHT, GHOSTLINK_TCP_RECONNECT_ATTEMPTS, GHOSTLINK_TCP_RECONNECT_BACKOFF_MS, GHOSTLINK_TCP_AUTH_TOKEN, GHOSTLINK_TCP_AUTOTUNE\n"
        );
    }

    Ok(())
}

fn start_openai_api_server(port: u16, host: &str) -> Result<()> {
    use axum::{
        extract::{Path, State},
        routing::{get, post},
        Json, Router,
    };
    use serde::{Deserialize, Serialize};
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower_http::cors::CorsLayer;

    #[derive(Debug, Deserialize)]
    struct ChatCompletionRequest {
        model: String,
        #[allow(dead_code)]
        messages: Vec<serde_json::Value>,
        #[allow(dead_code)]
        stream: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    struct GuiChatRequest {
        message: String,
        #[allow(dead_code)]
        temperature: Option<f32>,
        #[allow(dead_code)]
        top_p: Option<f32>,
        #[allow(dead_code)]
        top_k: Option<usize>,
        #[allow(dead_code)]
        penalty: Option<f32>,
        #[allow(dead_code)]
        max_tokens: Option<usize>,
        #[allow(dead_code)]
        system_prompt: Option<String>,
        #[allow(dead_code)]
        mcp: Option<serde_json::Value>,
    }

    #[derive(Debug, Deserialize)]
    struct ModelLoadRequest {
        model: String,
    }

    #[derive(Debug, Deserialize)]
    struct ModelDownloadRequest {
        model_id: String,
    }

    #[derive(Debug, Deserialize)]
    struct WorkerAddRequest {
        host: String,
        port: u16,
    }

    #[derive(Debug, Clone, Serialize)]
    struct ModelRecord {
        name: String,
        size_gb: f32,
        model_type: String,
        quantization: String,
        status: String,
    }

    #[derive(Debug, Clone, Serialize)]
    struct WorkerRecord {
        id: String,
        host: String,
        port: u16,
        status: String,
        model: String,
        threads: usize,
        load: u8,
    }

    #[derive(Debug, Clone, Serialize)]
    struct SessionRecord {
        id: String,
        model: String,
        status: String,
        throughput: usize,
        latency: u32,
        tokens: usize,
    }

    #[derive(Debug)]
    struct BackendState {
        models: Vec<ModelRecord>,
        current_model: String,
        workers: Vec<WorkerRecord>,
        sessions: Vec<SessionRecord>,
        queue_depth: usize,
        chat_requests: u64,
        last_latency_ms: f32,
        started_at: Instant,
        backend_url: String,
    }

    #[derive(Debug, Serialize)]
    struct ChatCompletionResponse {
        id: String,
        object: String,
        created: u64,
        model: String,
        choices: Vec<Choice>,
    }

    #[derive(Debug, Serialize)]
    struct Choice {
        index: usize,
        message: serde_json::Value,
        finish_reason: String,
    }

    fn lock_state(state: &Arc<Mutex<BackendState>>) -> std::sync::MutexGuard<'_, BackendState> {
        state.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    async fn handle_chat_completions(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ChatCompletionRequest>,
    ) -> Json<ChatCompletionResponse> {
        let mut backend = lock_state(&state);
        backend.chat_requests = backend.chat_requests.saturating_add(1);
        let model = if req.model.trim().is_empty() {
            backend.current_model.clone()
        } else {
            req.model.clone()
        };

        Json(ChatCompletionResponse {
            id: format!("chatcmpl-{}", rand::random::<u32>()),
            object: "chat.completion".to_string(),
            created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: model.clone(),
            choices: vec![Choice {
                index: 0,
                message: serde_json::json!({
                    "role": "assistant",
                    "content": format!(
                        "Ghostlink backend is online. Model '{}' handled request #{}.",
                        model,
                        backend.chat_requests
                    )
                }),
                finish_reason: "stop".to_string(),
            }],
        })
    }

    async fn handle_models(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let data = backend
            .models
            .iter()
            .map(|model| {
                serde_json::json!({
                    "id": model.name,
                    "object": "model",
                    "created": 1700000000,
                    "owned_by": "ghostlink"
                })
            })
            .collect::<Vec<_>>();

        Json(serde_json::json!({
            "object": "list",
            "data": data
        }))
    }

    async fn handle_gui_models(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let models = backend
            .models
            .iter()
            .map(|model| {
                serde_json::json!({
                    "name": model.name,
                    "size_gb": model.size_gb,
                    "type": model.model_type,
                    "quantization": model.quantization,
                    "status": model.status,
                })
            })
            .collect::<Vec<_>>();

        Json(serde_json::json!({
            "models": models,
            "current_model": backend.current_model
        }))
    }

    async fn handle_gui_model_load(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ModelLoadRequest>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        let mut found = false;
        for model in &backend.models {
            if model.name == req.model {
                found = true;
                break;
            }
        }

        if !found {
            return Json(serde_json::json!({
                "error": format!("model '{}' not found", req.model)
            }));
        }

        backend.current_model = req.model.clone();
        Json(serde_json::json!({
            "status": "ok",
            "model": req.model,
            "loaded": true
        }))
    }

    async fn handle_gui_model_download(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ModelDownloadRequest>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        if !backend
            .models
            .iter()
            .any(|model| model.name == req.model_id)
        {
            backend.models.push(ModelRecord {
                name: req.model_id.clone(),
                size_gb: 4.0,
                model_type: "LLM".to_string(),
                quantization: "Q4_K_M".to_string(),
                status: "Ready".to_string(),
            });
        }

        Json(serde_json::json!({
            "status": "ok",
            "model_id": req.model_id,
            "queued": true
        }))
    }

    async fn handle_gui_workers(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let workers = backend
            .workers
            .iter()
            .map(|worker| {
                serde_json::json!({
                    "id": worker.id,
                    "host": worker.host,
                    "port": worker.port,
                    "status": worker.status,
                    "model": worker.model,
                    "threads": worker.threads,
                    "load": worker.load,
                })
            })
            .collect::<Vec<_>>();

        Json(serde_json::json!({
            "workers": workers
        }))
    }

    async fn handle_gui_workers_connect(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        let current_model = backend.current_model.clone();
        let mut connected = 0_usize;
        for worker in &mut backend.workers {
            let reachable =
                is_gui_backend_reachable(&worker.host, worker.port, Duration::from_millis(250));
            worker.status = if reachable {
                connected = connected.saturating_add(1);
                "Connected".to_string()
            } else {
                "Disconnected".to_string()
            };
            worker.model = current_model.clone();
            worker.load = if reachable { 35 } else { 0 };
        }

        Json(serde_json::json!({
            "status": "ok",
            "connected": connected,
            "total": backend.workers.len()
        }))
    }

    async fn handle_gui_workers_add(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<WorkerAddRequest>,
    ) -> Json<serde_json::Value> {
        if req.host.trim().is_empty() {
            return Json(serde_json::json!({"error": "host cannot be empty"}));
        }

        let mut backend = lock_state(&state);
        let current_model = backend.current_model.clone();
        let duplicate = backend
            .workers
            .iter()
            .any(|worker| worker.host == req.host && worker.port == req.port);

        if duplicate {
            return Json(serde_json::json!({
                "status": "ok",
                "worker": {
                    "host": req.host,
                    "port": req.port
                },
                "duplicate": true
            }));
        }

        let worker_id = format!("worker_{:03}", backend.workers.len() + 1);
        backend.workers.push(WorkerRecord {
            id: worker_id,
            host: req.host.clone(),
            port: req.port,
            status: "Disconnected".to_string(),
            model: current_model,
            threads: 4,
            load: 0,
        });

        Json(serde_json::json!({
            "status": "ok",
            "worker": {
                "host": req.host,
                "port": req.port
            }
        }))
    }

    async fn handle_gui_metrics(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let workers_online = backend
            .workers
            .iter()
            .filter(|worker| worker.status == "Connected")
            .count();
        let throughput = (workers_online.max(1) * 1200) + (backend.chat_requests as usize * 20);
        let latency_p50 = backend.last_latency_ms.max(1.0).round() as usize;
        let latency_p95 = (backend.last_latency_ms * 1.4).max(2.0).round() as usize;
        let cpu = (18 + workers_online * 9 + backend.queue_depth.min(20)).min(95);
        let memory = (24 + workers_online * 7 + (backend.sessions.len() * 2)).min(96);
        let gpu = (32 + workers_online * 11 + (backend.chat_requests as usize % 15)).min(98);

        Json(serde_json::json!({
            "metrics": {
                "throughput": throughput,
                "cpu": cpu,
                "memory": memory,
                "gpu": gpu,
                "latency_p50": latency_p50,
                "latency_p95": latency_p95
            }
        }))
    }

    async fn handle_gui_sessions(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let sessions = backend
            .sessions
            .iter()
            .map(|session| {
                serde_json::json!({
                    "id": session.id,
                    "model": session.model,
                    "status": session.status,
                    "throughput": session.throughput,
                    "latency": session.latency,
                    "tokens": session.tokens,
                })
            })
            .collect::<Vec<_>>();

        Json(serde_json::json!({
            "sessions": sessions
        }))
    }

    async fn handle_gui_session_cancel(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(session_id): Path<String>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        let mut cancelled = false;
        for session in &mut backend.sessions {
            if session.id == session_id {
                session.status = "Cancelled".to_string();
                cancelled = true;
                break;
            }
        }

        Json(serde_json::json!({
            "status": "ok",
            "session_id": session_id,
            "cancelled": cancelled
        }))
    }

    async fn handle_gui_queue(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        backend.queue_depth = backend.queue_depth.saturating_add(1);

        Json(serde_json::json!({
            "status": "ok",
            "queued": true,
            "queue_depth": backend.queue_depth
        }))
    }

    async fn handle_gui_jwt_refresh() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "status": "ok",
            "refreshed": true
        }))
    }

    async fn handle_gui_pqc_enable() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "status": "ok",
            "pqc": "enabled"
        }))
    }

    async fn handle_gui_chat(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<GuiChatRequest>,
    ) -> Json<serde_json::Value> {
        let started = Instant::now();
        let mut backend = lock_state(&state);
        let current_model = backend.current_model.clone();
        backend.chat_requests = backend.chat_requests.saturating_add(1);

        let token_estimate = req.message.split_whitespace().count().max(1);
        let maybe_session = backend.sessions.first_mut();
        if let Some(session) = maybe_session {
            session.tokens = session.tokens.saturating_add(token_estimate);
            session.throughput = session.throughput.saturating_add(12);
            session.latency = session.latency.max(1);
            session.model = current_model.clone();
            session.status = "Running".to_string();
        } else {
            backend.sessions.push(SessionRecord {
                id: "sess_local_001".to_string(),
                model: current_model.clone(),
                status: "Running".to_string(),
                throughput: 1200,
                latency: 2,
                tokens: token_estimate,
            });
        }

        backend.last_latency_ms = (started.elapsed().as_secs_f32() * 1000.0).max(1.0);

        Json(serde_json::json!({
            "response": format!(
                "[{}] {}",
                current_model,
                req.message
            ),
            "request_id": format!("req-{}", backend.chat_requests),
            "tokens_estimated": token_estimate,
        }))
    }

    async fn handle_health(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let uptime_s = backend.started_at.elapsed().as_secs();
        Json(serde_json::json!({
            "status": "healthy",
            "version": "0.1.0-alpha.0",
            "backend_url": backend.backend_url,
            "uptime_s": uptime_s,
            "current_model": backend.current_model,
        }))
    }

    println!("Ghostlink Studio API - Starting OpenAI-compatible server...");
    println!("Listening on http://{}:{}", host, port);
    println!("Routes:");
    println!("  - POST /v1/chat/completions");
    println!("  - GET  /v1/models");
    println!("  - GET  /health");
    println!("  - GET  /api/models");
    println!("  - POST /api/models/load");
    println!("  - POST /api/models/download");
    println!("  - GET  /api/workers");
    println!("  - POST /api/workers/connect");
    println!("  - POST /api/workers/add");
    println!("  - GET  /api/metrics");
    println!("  - GET  /api/sessions");
    println!("  - POST /api/sessions/:session_id/cancel");
    println!("  - POST /api/queue");
    println!("  - POST /api/security/jwt/refresh");
    println!("  - POST /api/security/pqc/enable");
    println!("  - POST /api/inference/chat");

    let profile = detect_runtime_profile("studio-api");
    let backend_url = format!("http://{}:{}", host, port);
    println!(
        "Inference Core: {} workers, {} acceleration",
        profile.recommended_workers,
        profile.acceleration_mode.as_str()
    );

    if std::env::var("GHOSTLINK_CI_RUN").is_ok() {
        return Ok(());
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let state = Arc::new(Mutex::new(BackendState {
        models: vec![
            ModelRecord {
                name: "ghostlink-30b-v1".to_string(),
                size_gb: 30.0,
                model_type: "LLM".to_string(),
                quantization: "Q4_K_M".to_string(),
                status: "Ready".to_string(),
            },
            ModelRecord {
                name: "mistral-7b-instruct".to_string(),
                size_gb: 7.0,
                model_type: "LLM".to_string(),
                quantization: "Q8_0".to_string(),
                status: "Ready".to_string(),
            },
        ],
        current_model: "ghostlink-30b-v1".to_string(),
        workers: vec![WorkerRecord {
            id: "local-node".to_string(),
            host: host.to_string(),
            port,
            status: "Connected".to_string(),
            model: "ghostlink-30b-v1".to_string(),
            threads: profile.recommended_workers.max(1),
            load: 35,
        }],
        sessions: vec![],
        queue_depth: 0,
        chat_requests: 0,
        last_latency_ms: 2.0,
        started_at: Instant::now(),
        backend_url,
    }));

    rt.block_on(async {
        let app = Router::new()
            .route("/v1/chat/completions", post(handle_chat_completions))
            .route("/v1/models", get(handle_models))
            .route("/health", get(handle_health))
            .route("/api/models", get(handle_gui_models))
            .route("/api/models/load", post(handle_gui_model_load))
            .route("/api/models/download", post(handle_gui_model_download))
            .route("/api/workers", get(handle_gui_workers))
            .route("/api/workers/connect", post(handle_gui_workers_connect))
            .route("/api/workers/add", post(handle_gui_workers_add))
            .route("/api/metrics", get(handle_gui_metrics))
            .route("/api/sessions", get(handle_gui_sessions))
            .route(
                "/api/sessions/{session_id}/cancel",
                post(handle_gui_session_cancel),
            )
            .route("/api/queue", post(handle_gui_queue))
            .route("/api/security/jwt/refresh", post(handle_gui_jwt_refresh))
            .route("/api/security/pqc/enable", post(handle_gui_pqc_enable))
            .route("/api/inference/chat", post(handle_gui_chat))
            .with_state(state)
            .layer(CorsLayer::permissive());

        let addr: SocketAddr = format!("{}:{}", host, port).parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        println!("\nAPI Server Online. Ready for connections.");

        axum::serve(listener, app).await.unwrap();
    });

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

    let auth_token = std::env::var("GHOSTLINK_DISCOVERY_AUTH_TOKEN")
        .ok()
        .filter(|token| !token.is_empty());
    let broadcast_addr = std::env::var("GHOSTLINK_DISCOVERY_BROADCAST")
        .ok()
        .and_then(|raw| raw.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| SocketAddr::from(([255, 255, 255, 255], DEFAULT_DISCOVERY_PORT)));
    let timeout_ms = std::env::var("GHOSTLINK_DISCOVERY_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(750);
    let discovery_cfg = UdpDiscoveryConfig {
        broadcast_addr,
        response_timeout: Duration::from_millis(timeout_ms),
        auth_token,
        allow_legacy_crc32: env_default_bool("GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32", false),
        ..UdpDiscoveryConfig::default()
    };

    let discovery_replies = broadcast_and_collect(&frame, &discovery_cfg)
        .map_err(|e| anyhow::anyhow!("UDP discovery broadcast failed: {e}"))?;

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
    println!("  UDP Broadcast Target: {}", discovery_cfg.broadcast_addr);
    println!("  Discovery Timeout: {} ms", timeout_ms);
    println!(
        "  Discovery Auth: {}",
        if discovery_cfg.auth_token.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!("  Replies Received: {}", discovery_replies.len());

    for (peer_frame, peer_addr) in discovery_replies {
        println!(
            "    - {} at {} (VRAM {:.1} GB, RAM {:.1} GB, CC {}, GPU {})",
            peer_frame.node.id,
            peer_addr,
            peer_frame.node.vram_gb,
            peer_frame.node.system_memory_gb,
            peer_frame.node.compute_capability,
            peer_frame.node.gpu_name.as_deref().unwrap_or("unknown")
        );
    }

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

fn print_discovery_listener(node_id: &str, once: bool) -> Result<()> {
    let profile = detect_runtime_profile(node_id);

    let auth_token = std::env::var("GHOSTLINK_DISCOVERY_AUTH_TOKEN")
        .ok()
        .filter(|token| !token.is_empty());
    let listen_addr = std::env::var("GHOSTLINK_DISCOVERY_LISTEN")
        .ok()
        .and_then(|raw| raw.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], DEFAULT_DISCOVERY_PORT)));
    let timeout_ms = std::env::var("GHOSTLINK_DISCOVERY_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(1000);
    let max_replies = std::env::var("GHOSTLINK_DISCOVERY_MAX_REPLIES")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0);

    let config = UdpDiscoveryConfig {
        bind_addr: listen_addr,
        response_timeout: Duration::from_millis(timeout_ms),
        auth_token,
        allow_legacy_crc32: env_default_bool("GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32", false),
        ..UdpDiscoveryConfig::default()
    };

    println!("Ghost-Link Discovery Listener\n");
    println!("===========================\n");
    println!("Node ID: {}", profile.node_resources.id);
    println!("Listen Address: {}", config.bind_addr);
    println!("Timeout: {} ms", timeout_ms);
    println!(
        "Auth Token: {}",
        if config.auth_token.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );

    if once {
        println!("Mode: one-shot\n");
        match respond_once(&profile.node_resources, &config)
            .map_err(|e| anyhow::anyhow!("UDP discovery listener failed: {e}"))?
        {
            Some(peer) => println!("Replied to discovery request from {}", peer),
            None => println!("No discovery request received before timeout"),
        }
        return Ok(());
    }

    println!("Mode: service loop\n");
    if let Some(limit) = max_replies {
        println!("Max Replies: {}", limit);
        let stats = serve_discovery_with_stats(&profile.node_resources, &config, Some(limit))
            .map_err(|e| anyhow::anyhow!("UDP discovery listener failed: {e}"))?;
        println!("Listener stopped after {} replies", stats.replies_sent);
        println!("Drop Counters:");
        println!("  malformed: {}", stats.drops.malformed);
        println!("  auth_mismatch: {}", stats.drops.auth_mismatch);
        println!("  unsupported_kind: {}", stats.drops.unsupported_kind);
    } else {
        println!("Max Replies: unlimited (Ctrl+C to stop)");
        let _ = serve_discovery(&profile.node_resources, &config, None)
            .map_err(|e| anyhow::anyhow!("UDP discovery listener failed: {e}"))?;
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

fn print_cluster_start(node_count: usize, base_port: u16) -> Result<()> {
    let mut listeners = Vec::new();
    let self_exe = std::env::current_exe()
        .map_err(|err| anyhow::anyhow!("failed to locate current executable: {}", err))?;

    println!("Ghost-Link Local Cluster Start\n");
    println!("===============================\n");
    println!("Node count: {}", node_count);
    println!("Base port: {}", base_port);

    for i in 0..node_count {
        let node_id = format!("local-node-{}", i + 1);
        let port = base_port.saturating_add(i as u16);
        let listen_addr = format!("127.0.0.1:{}", port);

        let child = Command::new(&self_exe)
            .arg("listen")
            .arg(&node_id)
            .arg("--once")
            .env("GHOSTLINK_DISCOVERY_LISTEN", &listen_addr)
            .env("GHOSTLINK_DISCOVERY_TIMEOUT_MS", "2500")
            .spawn()
            .map_err(|err| {
                anyhow::anyhow!(
                    "failed to spawn listener {} at {}: {}",
                    node_id,
                    listen_addr,
                    err
                )
            })?;
        listeners.push((node_id, listen_addr, child));
    }

    std::thread::sleep(Duration::from_millis(300));

    let controller = detect_runtime_profile("cluster-controller");
    let join = DiscoveryFrame {
        kind: FrameKind::Join,
        node: controller.node_resources,
    };

    let mut total_replies = 0usize;
    for (node_id, listen_addr, _child) in &listeners {
        let target = listen_addr
            .parse::<SocketAddr>()
            .map_err(|err| anyhow::anyhow!("invalid listen addr {}: {}", listen_addr, err))?;

        let cfg = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: target,
            response_timeout: Duration::from_millis(800),
            allow_legacy_crc32: env_default_bool("GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32", false),
            ..UdpDiscoveryConfig::default()
        };

        let replies = broadcast_and_collect(&join, &cfg)
            .map_err(|err| anyhow::anyhow!("join probe failed for {}: {}", node_id, err))?;
        println!(
            "{} at {} replied {} time(s)",
            node_id,
            listen_addr,
            replies.len()
        );
        total_replies += replies.len();
    }

    for (node_id, listen_addr, mut child) in listeners {
        let status = child.wait().map_err(|err| {
            anyhow::anyhow!(
                "failed waiting for listener {} ({}) to exit: {}",
                node_id,
                listen_addr,
                err
            )
        })?;
        if !status.success() {
            anyhow::bail!(
                "listener {} ({}) exited with status {}",
                node_id,
                listen_addr,
                status
            );
        }
    }

    if total_replies < node_count {
        anyhow::bail!(
            "cluster-start validation incomplete: expected at least {} replies, got {}",
            node_count,
            total_replies
        );
    }

    println!(
        "\nCluster-start validation passed: {} replies across {} local nodes",
        total_replies, node_count
    );
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

impl DoctorStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Debug)]
struct DoctorCheck {
    area: &'static str,
    name: &'static str,
    status: DoctorStatus,
    detail: String,
    fix: Option<String>,
    context_json: Option<String>,
}

fn push_doctor_check(
    checks: &mut Vec<DoctorCheck>,
    area: &'static str,
    name: &'static str,
    status: DoctorStatus,
    detail: impl Into<String>,
    fix: Option<String>,
) {
    push_doctor_check_with_context(checks, area, name, status, detail, fix, None);
}

fn push_doctor_check_with_context(
    checks: &mut Vec<DoctorCheck>,
    area: &'static str,
    name: &'static str,
    status: DoctorStatus,
    detail: impl Into<String>,
    fix: Option<String>,
    context_json: Option<String>,
) {
    checks.push(DoctorCheck {
        area,
        name,
        status,
        detail: detail.into(),
        fix,
        context_json,
    });
}

fn run_command_capture(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| anyhow::anyhow!("failed to execute {}: {}", program, err))?;

    if !output.status.success() {
        anyhow::bail!(
            "{} exited with status {}",
            program,
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string())
        );
    }

    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    Ok(text.trim().to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PythonResolutionSource {
    ConfiguredOverride,
    RepoVenv,
    SystemFallback,
}

impl PythonResolutionSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ConfiguredOverride => "configured-override",
            Self::RepoVenv => "repo-venv",
            Self::SystemFallback => "system-fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PythonResolution {
    executable: String,
    source: PythonResolutionSource,
}

fn resolve_python_for_root(repo_root: &Path, configured: Option<String>) -> PythonResolution {
    if let Some(configured) = configured.filter(|value| !value.trim().is_empty()) {
        return PythonResolution {
            executable: configured,
            source: PythonResolutionSource::ConfiguredOverride,
        };
    }

    let venv_python = repo_root.join(".venv").join("bin").join("python");
    if venv_python.is_file() {
        return PythonResolution {
            executable: venv_python.display().to_string(),
            source: PythonResolutionSource::RepoVenv,
        };
    }

    PythonResolution {
        executable: "python3".to_string(),
        source: PythonResolutionSource::SystemFallback,
    }
}

fn resolve_python_executable_for_root(repo_root: &Path, configured: Option<String>) -> String {
    resolve_python_for_root(repo_root, configured).executable
}

fn run_planner_accuracy_check() -> Result<String> {
    let profile = detect_runtime_profile("doctor-local");
    let local_id = "doctor-local";
    let remote_id = "doctor-remote";
    let nodes = vec![
        NodeResources::new(
            local_id,
            profile.node_resources.vram_gb.max(16.0),
            profile.node_resources.system_memory_gb.max(16.0),
            profile.node_resources.compute_capability.clone(),
            profile.node_resources.gpu_name.clone(),
        ),
        NodeResources::new(
            remote_id,
            32.0,
            32.0,
            "auto",
            Some("remote-host".to_string()),
        ),
    ];
    let layers: Vec<LayerSpec> = (0..60)
        .map(|index| LayerSpec {
            index,
            vram_gb: 0.5,
            num_weights: 500_000_000 / 60,
        })
        .collect();
    let assignments = assign_layers_with_runtime_profile(&nodes, &layers, &profile)
        .map_err(|err| anyhow::anyhow!(err))?;

    let mut coverage = vec![0usize; layers.len()];
    for assignment in &assignments {
        for layer in assignment.start_layer..assignment.end_layer {
            if let Some(entry) = coverage.get_mut(layer) {
                *entry += 1;
            } else {
                anyhow::bail!("assignment references out-of-range layer index {}", layer);
            }
        }
    }

    let missing = coverage.iter().filter(|count| **count == 0).count();
    let overlaps = coverage.iter().filter(|count| **count > 1).count();
    if missing > 0 || overlaps > 0 {
        anyhow::bail!(
            "planner coverage mismatch (missing_layers={}, overlapped_layers={})",
            missing,
            overlaps
        );
    }

    Ok(format!(
        "{} assignments cover {} layers with no gaps/overlap",
        assignments.len(),
        layers.len()
    ))
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn write_doctor_report_json(
    path: &Path,
    checks: &[DoctorCheck],
    pass_count: usize,
    warn_count: usize,
    fail_count: usize,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| {
                anyhow::anyhow!(
                    "failed to create doctor report directory {}: {}",
                    parent.display(),
                    err
                )
            })?;
        }
    }

    let checks_json = checks
        .iter()
        .map(|check| {
            let fix_json = check
                .fix
                .as_ref()
                .map(|value| format!("\"{}\"", json_escape(value)))
                .unwrap_or_else(|| "null".to_string());
            let context_json = check
                .context_json
                .as_ref()
                .cloned()
                .unwrap_or_else(|| "null".to_string());
            format!(
                "{{\"area\":\"{}\",\"name\":\"{}\",\"status\":\"{}\",\"detail\":\"{}\",\"fix\":{},\"context\":{}}}",
                json_escape(check.area),
                json_escape(check.name),
                check.status.as_str(),
                json_escape(&check.detail),
                fix_json,
                context_json
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let payload = format!(
        "{{\n  \"summary\": {{\"pass\": {}, \"warn\": {}, \"fail\": {}}},\n  \"checks\": [{}]\n}}\n",
        pass_count, warn_count, fail_count, checks_json
    );

    fs::write(path, payload).map_err(|err| {
        anyhow::anyhow!(
            "failed to write doctor report JSON {}: {}",
            path.display(),
            err
        )
    })
}

const DOCTOR_NETWORK_PROBE_TIMEOUT_MS: u64 = 350;
const DOCTOR_NETWORK_PROBE_WARN_LATENCY_MS: f64 = 150.0;

#[derive(Debug, Clone, PartialEq)]
enum NetworkProbeOutcome {
    Reachable {
        resolved: SocketAddr,
        latency_ms: f64,
    },
    Unreachable {
        resolved: SocketAddr,
        error: String,
    },
    InvalidTarget(String),
}

fn probe_network_target(target: &str, timeout: Duration) -> NetworkProbeOutcome {
    let Some((host, port_str)) = target.rsplit_once(':') else {
        return NetworkProbeOutcome::InvalidTarget(format!(
            "invalid network target '{}', expected host:port",
            target
        ));
    };

    if host.is_empty() || port_str.is_empty() {
        return NetworkProbeOutcome::InvalidTarget(format!(
            "invalid network target '{}', expected host:port",
            target
        ));
    }

    let Ok(port) = port_str.parse::<u16>() else {
        return NetworkProbeOutcome::InvalidTarget(format!(
            "invalid network target '{}', expected numeric port",
            target
        ));
    };

    let Ok(resolved_addrs) = (host, port).to_socket_addrs() else {
        return NetworkProbeOutcome::InvalidTarget(format!(
            "invalid network target '{}', hostname resolution failed",
            target
        ));
    };

    let resolved_addrs = resolved_addrs.collect::<Vec<_>>();
    if resolved_addrs.is_empty() {
        return NetworkProbeOutcome::InvalidTarget(format!(
            "invalid network target '{}', no socket addresses resolved",
            target
        ));
    }

    let mut last_error = None;
    for resolved in resolved_addrs {
        let started_at = Instant::now();
        match TcpStream::connect_timeout(&resolved, timeout) {
            Ok(stream) => {
                let _ = stream.shutdown(Shutdown::Both);
                return NetworkProbeOutcome::Reachable {
                    resolved,
                    latency_ms: started_at.elapsed().as_secs_f64() * 1000.0,
                };
            }
            Err(err) => last_error = Some((resolved, err.to_string())),
        }
    }

    let (resolved, error) = last_error.expect("resolved address list should not be empty");
    NetworkProbeOutcome::Unreachable { resolved, error }
}

fn run_optional_network_probe(target: &str, checks: &mut Vec<DoctorCheck>) {
    match probe_network_target(target, Duration::from_millis(DOCTOR_NETWORK_PROBE_TIMEOUT_MS)) {
        NetworkProbeOutcome::Reachable {
            resolved,
            latency_ms,
        } => {
            let degraded = latency_ms > DOCTOR_NETWORK_PROBE_WARN_LATENCY_MS;
            push_doctor_check_with_context(
                checks,
                "accessibility",
                "network-probe",
                if degraded {
                    DoctorStatus::Warn
                } else {
                    DoctorStatus::Pass
                },
                format!(
                    "target {} reachable via {} ({:.2} ms)",
                    target, resolved, latency_ms
                ),
                if degraded {
                    Some(
                        "Network path is reachable but latency is elevated; inspect host load and RTT before rollout"
                            .to_string(),
                    )
                } else {
                    None
                },
                Some(format!(
                    "{{\"target\":\"{}\",\"resolved\":\"{}\",\"reachable\":true,\"latency_ms\":{:.2},\"timeout_ms\":{}}}",
                    json_escape(target),
                    json_escape(&resolved.to_string()),
                    latency_ms,
                    DOCTOR_NETWORK_PROBE_TIMEOUT_MS
                )),
            )
        }
        NetworkProbeOutcome::Unreachable { resolved, error } => push_doctor_check_with_context(
            checks,
            "accessibility",
            "network-probe",
            DoctorStatus::Warn,
            format!("target {} resolved to {} but is not reachable ({})", target, resolved, error),
            Some(
                "Start a listener on the target and retry with --network-probe --network-target <host:port>"
                    .to_string(),
            ),
            Some(format!(
                "{{\"target\":\"{}\",\"resolved\":\"{}\",\"reachable\":false,\"timeout_ms\":{},\"error\":\"{}\"}}",
                json_escape(target),
                json_escape(&resolved.to_string()),
                DOCTOR_NETWORK_PROBE_TIMEOUT_MS,
                json_escape(&error)
            )),
        ),
        NetworkProbeOutcome::InvalidTarget(detail) => push_doctor_check_with_context(
            checks,
            "accessibility",
            "network-probe",
            DoctorStatus::Warn,
            detail.clone(),
            Some("Use --network-target <host:port> with a valid hostname or socket address".to_string()),
            Some(format!(
                "{{\"target\":\"{}\",\"reachable\":false,\"timeout_ms\":{},\"error\":\"{}\"}}",
                json_escape(target),
                DOCTOR_NETWORK_PROBE_TIMEOUT_MS,
                json_escape(&detail)
            )),
        ),
    }
}

fn print_doctor_report(options: &DoctorOptions) -> Result<()> {
    let mut checks: Vec<DoctorCheck> = Vec::new();
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root.join("..").join("..");

    let python =
        resolve_python_executable_for_root(&repo_root, std::env::var("GHOSTLINK_PYTHON").ok());

    match run_command_capture("cargo", &["--version"]) {
        Ok(version) => push_doctor_check(
            &mut checks,
            "environment",
            "cargo",
            DoctorStatus::Pass,
            version,
            None,
        ),
        Err(err) => push_doctor_check(
            &mut checks,
            "environment",
            "cargo",
            DoctorStatus::Warn,
            err.to_string(),
            Some("Install Rust: curl https://sh.rustup.rs -sSf | sh -s -- -y".to_string()),
        ),
    }

    let python_ok = match run_command_capture(&python, &["--version"]) {
        Ok(version) => {
            push_doctor_check(
                &mut checks,
                "environment",
                "python-runtime",
                DoctorStatus::Pass,
                version,
                None,
            );
            true
        }
        Err(err) => {
            push_doctor_check(
                &mut checks,
                "environment",
                "python-runtime",
                DoctorStatus::Warn,
                err.to_string(),
                Some("Install Python 3.10+ and set GHOSTLINK_PYTHON if needed".to_string()),
            );
            false
        }
    };

    let example_config = repo_root.join("ghostlink.example.toml");
    if example_config.exists() {
        push_doctor_check(
            &mut checks,
            "readiness",
            "config-template",
            DoctorStatus::Pass,
            format!("found {}", example_config.display()),
            None,
        );
    } else {
        push_doctor_check(
            &mut checks,
            "readiness",
            "config-template",
            DoctorStatus::Fail,
            format!("missing {}", example_config.display()),
            Some("Restore ghostlink.example.toml from repository".to_string()),
        );
    }

    let local_config = repo_root.join("ghostlink.toml");
    push_doctor_check(
        &mut checks,
        "readiness",
        "local-config",
        if local_config.exists() {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Warn
        },
        if local_config.exists() {
            format!("using {}", local_config.display())
        } else {
            "not found (quickstart will auto-create it)".to_string()
        },
        if local_config.exists() {
            None
        } else {
            Some("Run: bash scripts/quickstart.sh".to_string())
        },
    );

    if let Some(last_check) = checks.last_mut() {
        last_check.context_json = Some(format!(
            "{{\"path\":\"{}\",\"exists\":{}}}",
            json_escape(&local_config.display().to_string()),
            local_config.exists()
        ));
    }

    let gui_entry = repo_root
        .join("third_party")
        .join("mohawk_gui")
        .join("main.py");
    let gui_requirements = repo_root
        .join("third_party")
        .join("mohawk_gui")
        .join("requirements.txt");
    if gui_entry.exists() && gui_requirements.exists() {
        push_doctor_check(
            &mut checks,
            "readiness",
            "gui-assets",
            DoctorStatus::Pass,
            "GUI entrypoint and requirements present".to_string(),
            None,
        );
    } else {
        push_doctor_check(
            &mut checks,
            "readiness",
            "gui-assets",
            DoctorStatus::Fail,
            "missing vendored GUI files".to_string(),
            Some("Ensure third_party/mohawk_gui is checked out".to_string()),
        );
    }

    if python_ok {
        match detect_missing_gui_python_modules(&python) {
            Ok(missing) if missing.is_empty() => push_doctor_check_with_context(
                &mut checks,
                "readiness",
                "gui-python-modules",
                DoctorStatus::Pass,
                "PyQt6, requests, pyqtgraph available".to_string(),
                None,
                Some("{\"missing\":[],\"python_ok\":true}".to_string()),
            ),
            Ok(missing) => push_doctor_check_with_context(
                &mut checks,
                "readiness",
                "gui-python-modules",
                DoctorStatus::Warn,
                format!("missing: {}", missing.join(", ")),
                Some(format!(
                    "Install with: {} -m pip install -r third_party/mohawk_gui/requirements-runtime.txt",
                    python
                )),
                Some(format!(
                    "{{\"missing\":[{}],\"python_ok\":true}}",
                    missing
                        .iter()
                        .map(|module| format!("\"{}\"", json_escape(module)))
                        .collect::<Vec<_>>()
                        .join(",")
                )),
            ),
            Err(err) => push_doctor_check_with_context(
                &mut checks,
                "readiness",
                "gui-python-modules",
                DoctorStatus::Warn,
                err.to_string(),
                Some("Verify Python environment and package installation".to_string()),
                Some(format!(
                    "{{\"python_ok\":false,\"error\":\"{}\"}}",
                    json_escape(&err.to_string())
                )),
            ),
        }
    }

    let has_display = std::env::var("DISPLAY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
        || std::env::var("WAYLAND_DISPLAY")
            .ok()
            .filter(|v| !v.is_empty())
            .is_some();

    if has_display {
        push_doctor_check_with_context(
            &mut checks,
            "accessibility",
            "display-session",
            DoctorStatus::Pass,
            "DISPLAY/WAYLAND session detected".to_string(),
            None,
            Some(format!(
                "{{\"has_display\":true,\"display\":{},\"wayland_display\":{}}}",
                std::env::var("DISPLAY")
                    .ok()
                    .map(|value| format!("\"{}\"", json_escape(&value)))
                    .unwrap_or_else(|| "null".to_string()),
                std::env::var("WAYLAND_DISPLAY")
                    .ok()
                    .map(|value| format!("\"{}\"", json_escape(&value)))
                    .unwrap_or_else(|| "null".to_string())
            )),
        );
    } else {
        let xvfb_ok = run_command_capture("xvfb-run", &["--help"]).is_ok();
        push_doctor_check_with_context(
            &mut checks,
            "accessibility",
            "display-session",
            if xvfb_ok {
                DoctorStatus::Warn
            } else {
                DoctorStatus::Fail
            },
            if xvfb_ok {
                "headless session; xvfb-run available for GUI diagnostics".to_string()
            } else {
                "headless session and xvfb-run unavailable".to_string()
            },
            if xvfb_ok {
                Some("Run GUI checks with: xvfb-run -a cargo run -p ghost-link -- gui-diagnose --strict".to_string())
            } else {
                Some("Install xvfb and rerun GUI diagnostics for headless hosts".to_string())
            },
            Some(format!(
                "{{\"has_display\":false,\"xvfb_available\":{}}}",
                xvfb_ok
            )),
        );
    }

    for (name, rel_path) in [
        ("deployment-guide", "docs/DEPLOYMENT.md"),
        (
            "systemd-template",
            "deploy/systemd/ghost-link-listener@.service",
        ),
        (
            "docker-local-demo",
            "deploy/docker/docker-compose.local.yml",
        ),
    ] {
        let path = repo_root.join(rel_path);
        push_doctor_check(
            &mut checks,
            "accessibility",
            name,
            if path.exists() {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Warn
            },
            if path.exists() {
                format!("found {}", path.display())
            } else {
                format!("missing {}", path.display())
            },
            if path.exists() {
                None
            } else {
                Some("Restore deployment assets for multi-device onboarding".to_string())
            },
        );
        if let Some(last_check) = checks.last_mut() {
            last_check.context_json = Some(format!(
                "{{\"path\":\"{}\",\"exists\":{}}}",
                json_escape(&path.display().to_string()),
                path.exists()
            ));
        }
    }

    if options.network_probe {
        run_optional_network_probe(&options.network_target, &mut checks);
    }

    match run_planner_accuracy_check() {
        Ok(summary) => push_doctor_check(
            &mut checks,
            "accuracy",
            "planner-layer-coverage",
            DoctorStatus::Pass,
            summary,
            None,
        ),
        Err(err) => push_doctor_check(
            &mut checks,
            "accuracy",
            "planner-layer-coverage",
            DoctorStatus::Fail,
            err.to_string(),
            Some("Inspect assign_layers_with_runtime_profile behavior".to_string()),
        ),
    }

    for rel_path in [
        "scripts/validate_flow_metrics.py",
        "scripts/validate_stage_tail_metrics.py",
        "scripts/validate_flow_canary.py",
        "docs/PERF_BASELINE.json",
    ] {
        let path = repo_root.join(rel_path);
        push_doctor_check(
            &mut checks,
            "accuracy",
            "validation-artifacts",
            if path.exists() {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Warn
            },
            if path.exists() {
                format!("found {}", path.display())
            } else {
                format!("missing {}", path.display())
            },
            None,
        );
        if let Some(last_check) = checks.last_mut() {
            last_check.context_json = Some(format!(
                "{{\"path\":\"{}\",\"exists\":{}}}",
                json_escape(&path.display().to_string()),
                path.exists()
            ));
        }
    }

    if python_ok {
        let api_contract_script = repo_root
            .join("scripts")
            .join("validate_gui_api_contract.py");
        match Command::new(&python).arg(&api_contract_script).status() {
            Ok(status) if status.success() => push_doctor_check(
                &mut checks,
                "accuracy",
                "gui-api-contract",
                DoctorStatus::Pass,
                "validate_gui_api_contract.py passed".to_string(),
                None,
            ),
            Ok(status) => push_doctor_check(
                &mut checks,
                "accuracy",
                "gui-api-contract",
                DoctorStatus::Fail,
                format!("script exited with status {}", status),
                Some(
                    "Run python3 scripts/validate_gui_api_contract.py and review missing APIs"
                        .to_string(),
                ),
            ),
            Err(err) => push_doctor_check(
                &mut checks,
                "accuracy",
                "gui-api-contract",
                DoctorStatus::Warn,
                format!("failed to execute: {}", err),
                Some("Verify Python executable and script path".to_string()),
            ),
        }
    }

    println!("Ghost-Link Doctor Report\n");
    println!("========================\n");

    for area in ["environment", "readiness", "accessibility", "accuracy"] {
        println!("{}:", area);
        for check in checks.iter().filter(|check| check.area == area) {
            println!(
                "- [{}] {}: {}",
                check.status.as_str(),
                check.name,
                check.detail
            );
            if let Some(fix) = &check.fix {
                println!("  FIX: {}", fix);
            }
        }
        println!();
    }

    let pass_count = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Pass)
        .count();
    let warn_count = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Warn)
        .count();
    let fail_count = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Fail)
        .count();

    println!(
        "Summary: {} pass, {} warn, {} fail",
        pass_count, warn_count, fail_count
    );

    if let Some(path) = options.json_out.as_deref() {
        write_doctor_report_json(path, &checks, pass_count, warn_count, fail_count)?;
        println!("Doctor report JSON written to: {}", path.display());
    }

    println!("\nReview areas for multi-device accessibility:");
    println!("- GUI path: desktop display or headless xvfb-run fallback");
    println!("- Deployment path: Docker local demo, systemd service template, staged LAN guide");
    println!("- Discovery path: cluster-start for local multi-node behavior");

    println!("\nReview areas for accuracy:");
    println!("- Planner layer coverage integrity (no gaps/overlap)");
    println!("- GUI API contract parity checks");
    println!("- Runtime SLO/canary/perf-drift validators and baseline presence");

    if options.strict && fail_count > 0 {
        anyhow::bail!(
            "doctor strict mode failed with {} failing checks",
            fail_count
        );
    }

    Ok(())
}

fn launch_mohawk_gui(args: &[String]) -> Result<()> {
    let skip_preflight = args.iter().any(|arg| arg == "--help" || arg == "-h");
    let forwarded_args = args
        .iter()
        .filter(|arg| arg.as_str() != "--no-auto-backend")
        .cloned()
        .collect::<Vec<_>>();

    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gui_entry = crate_root
        .join("..")
        .join("..")
        .join("third_party")
        .join("mohawk_gui")
        .join("main.py");

    if !gui_entry.exists() {
        anyhow::bail!(
            "Mohawk GUI entrypoint not found at {}. Ensure third_party/mohawk_gui is present.",
            gui_entry.display()
        );
    }

    let repo_root = crate_root.join("..").join("..");
    let python =
        resolve_python_executable_for_root(&repo_root, std::env::var("GHOSTLINK_PYTHON").ok());

    if !skip_preflight {
        run_gui_preflight_checks()?;
        run_gui_python_preflight(&python)?;
    }

    let (backend_host, backend_port) = parse_gui_backend_target(args);
    let backend_url = format!("http://{}:{}", backend_host, backend_port);
    let mut managed_backend = maybe_spawn_managed_gui_backend(args, &backend_host, backend_port)?;

    println!("Launching Mohawk GUI from {}", gui_entry.display());
    println!("Python executable: {}", python);
    println!("GUI backend target: {}", backend_url);

    let status = Command::new(&python)
        .arg(&gui_entry)
        .env("GHOSTLINK_GUI_BASE_URL", &backend_url)
        .args(&forwarded_args)
        .status()
        .map_err(|err| anyhow::anyhow!("failed to launch Mohawk GUI with {}: {}", python, err))?;

    if let Some(child) = managed_backend.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }

    if !status.success() {
        anyhow::bail!(
            "Mohawk GUI exited with status {}. Install dependencies from third_party/mohawk_gui and retry.",
            status
        );
    }

    Ok(())
}

fn parse_gui_backend_target(args: &[String]) -> (String, u16) {
    let mut host = "localhost".to_string();
    let mut port = 8003_u16;
    let mut i = 0_usize;

    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                if let Some(value) = args.get(i + 1) {
                    if !value.trim().is_empty() {
                        host = value.clone();
                    }
                }
                i += 1;
            }
            "--port" => {
                if let Some(value) = args.get(i + 1) {
                    if let Ok(parsed) = value.parse::<u16>() {
                        port = parsed;
                    }
                }
                i += 1;
            }
            _ if args[i].starts_with("--host=") => {
                let value = args[i].trim_start_matches("--host=").trim();
                if !value.is_empty() {
                    host = value.to_string();
                }
            }
            _ if args[i].starts_with("--port=") => {
                let value = args[i].trim_start_matches("--port=").trim();
                if let Ok(parsed) = value.parse::<u16>() {
                    port = parsed;
                }
            }
            _ if args[i].starts_with("--backend-url=") => {
                if let Some((parsed_host, parsed_port)) =
                    parse_host_port_from_backend_url(args[i].trim_start_matches("--backend-url="))
                {
                    host = parsed_host;
                    port = parsed_port;
                }
            }
            "--backend-url" => {
                if let Some(value) = args.get(i + 1) {
                    if let Some((parsed_host, parsed_port)) =
                        parse_host_port_from_backend_url(value)
                    {
                        host = parsed_host;
                        port = parsed_port;
                    }
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    (host, port)
}

fn parse_host_port_from_backend_url(value: &str) -> Option<(String, u16)> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next()?.trim();

    if host_port.is_empty() {
        return None;
    }

    if let Some((host, port)) = host_port.rsplit_once(':') {
        let parsed_port = port.parse::<u16>().ok()?;
        if host.trim().is_empty() {
            return None;
        }
        return Some((host.trim().to_string(), parsed_port));
    }

    Some((host_port.to_string(), 8003))
}

fn maybe_spawn_managed_gui_backend(
    args: &[String],
    host: &str,
    port: u16,
) -> Result<Option<Child>> {
    if args.iter().any(|arg| arg == "--no-auto-backend") {
        return Ok(None);
    }

    if is_gui_backend_reachable(host, port, Duration::from_millis(200)) {
        return Ok(None);
    }

    println!(
        "No backend detected at {}:{}; starting managed Ghostlink API backend...",
        host, port
    );

    let executable = std::env::current_exe().map_err(|err| {
        anyhow::anyhow!("failed to resolve current executable for auto-backend launch: {err}")
    })?;

    let mut child = Command::new(&executable)
        .arg("serve")
        .arg(host)
        .arg(port.to_string())
        .spawn()
        .map_err(|err| {
            anyhow::anyhow!(
                "failed to auto-start backend with {} serve {} {}: {}",
                executable.display(),
                host,
                port,
                err
            )
        })?;

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if is_gui_backend_reachable(host, port, Duration::from_millis(200)) {
            println!("Managed backend online at http://{}:{}", host, port);
            return Ok(Some(child));
        }
        std::thread::sleep(Duration::from_millis(125));
    }

    let _ = child.kill();
    let _ = child.wait();
    anyhow::bail!(
        "managed backend did not become reachable at http://{}:{} within startup timeout",
        host,
        port
    );
}

fn is_gui_backend_reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let addr = format!("{}:{}", host, port);
    if let Ok(mut addrs) = addr.to_socket_addrs() {
        if let Some(sock_addr) = addrs.next() {
            return TcpStream::connect_timeout(&sock_addr, timeout).is_ok();
        }
    }
    false
}

fn run_gui_python_preflight(python: &str) -> Result<()> {
    let missing = detect_missing_gui_python_modules(python)?;
    if !missing.is_empty() {
        anyhow::bail!(
            "GUI preflight failed: required Python GUI modules are missing: {}. Install with: {} -m pip install -r third_party/mohawk_gui/requirements-runtime.txt",
            missing.join(", "),
            python,
        );
    }

    Ok(())
}

fn print_gui_diagnostics(strict: bool) -> Result<()> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gui_entry = crate_root
        .join("..")
        .join("..")
        .join("third_party")
        .join("mohawk_gui")
        .join("main.py");
    let requirements = crate_root
        .join("..")
        .join("..")
        .join("third_party")
        .join("mohawk_gui")
        .join("requirements.txt");
    let repo_root = crate_root.join("..").join("..");
    let python_resolution =
        resolve_python_for_root(&repo_root, std::env::var("GHOSTLINK_PYTHON").ok());
    let python = python_resolution.executable.clone();

    let mut categories: Vec<(String, String)> = Vec::new();
    if !gui_entry.exists() {
        categories.push((
            "missing_files".to_string(),
            format!("Missing GUI entrypoint: {}", gui_entry.display()),
        ));
    }
    if !requirements.exists() {
        categories.push((
            "missing_files".to_string(),
            format!("Missing requirements file: {}", requirements.display()),
        ));
    }
    if Command::new(&python).arg("--version").output().is_err() {
        categories.push((
            "python_runtime".to_string(),
            format!("Python executable is not runnable: {}", python),
        ));
    }

    let mut missing_python_modules: Vec<String> = Vec::new();
    let mut python_module_probe_error: Option<String> = None;
    match detect_missing_gui_python_modules(&python) {
        Ok(missing) if !missing.is_empty() => {
            missing_python_modules = missing.clone();
            categories.push((
                "python_modules".to_string(),
                format!("Missing Python modules: {}", missing.join(", ")),
            ));
        }
        Err(err) => {
            python_module_probe_error = Some(err.to_string());
            categories.push((
                "python_modules".to_string(),
                format!("Python module probe failed: {}", err),
            ));
        }
        _ => {}
    }

    #[cfg(target_os = "linux")]
    let has_libgl = has_linux_libgl();
    #[cfg(target_os = "linux")]
    let has_libxkb = has_linux_libxkbcommon();
    #[cfg(target_os = "linux")]
    {
        if !has_libgl {
            categories.push((
                "system_libs".to_string(),
                "Missing libGL.so.1 (install libgl1)".to_string(),
            ));
        }
        if !has_libxkb {
            categories.push((
                "system_libs".to_string(),
                "Missing libxkbcommon.so.0 (install libxkbcommon0)".to_string(),
            ));
        }
    }

    let has_display = std::env::var("DISPLAY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
        || std::env::var("WAYLAND_DISPLAY")
            .ok()
            .filter(|v| !v.is_empty())
            .is_some();
    let xvfb_available = run_command_capture("xvfb-run", &["--help"]).is_ok();
    if !has_display {
        categories.push((
            "display_session".to_string(),
            "No DISPLAY/WAYLAND session detected (headless)".to_string(),
        ));
    }

    println!("Ghost-Link GUI Diagnostics\n");
    println!("==========================\n");
    println!("GUI entry: {}", gui_entry.display());
    println!("Requirements: {}", requirements.display());
    println!("Python executable: {}", python);
    println!("Python source: {}", python_resolution.source.as_str());
    println!(
        "Display session: {}",
        if has_display { "detected" } else { "none" }
    );

    if categories.is_empty() {
        println!("\nDiagnostics: PASS");
    } else {
        println!("\nDiagnostics: FAIL");
        for (kind, message) in &categories {
            println!("- [{}] {}", kind, message);
        }
    }

    if let Some(path) = std::env::var("GHOSTLINK_GUI_DIAG_JSON")
        .ok()
        .filter(|value| !value.is_empty())
    {
        let escaped = categories
            .iter()
            .map(|(kind, msg)| {
                format!(
                    "{{\"category\":\"{}\",\"message\":\"{}\"}}",
                    kind.replace('"', "\\\""),
                    msg.replace('"', "\\\"")
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        #[cfg(target_os = "linux")]
        let linux_libgl_json = if has_libgl { "true" } else { "false" };
        #[cfg(not(target_os = "linux"))]
        let linux_libgl_json = "null";
        #[cfg(target_os = "linux")]
        let linux_libxkb_json = if has_libxkb { "true" } else { "false" };
        #[cfg(not(target_os = "linux"))]
        let linux_libxkb_json = "null";
        let payload = format!(
            "{{\"ok\":{},\"python\":\"{}\",\"python_source\":\"{}\",\"gui_entry\":\"{}\",\"requirements\":\"{}\",\"has_display\":{},\"xvfb_available\":{},\"missing_python_modules\":[{}],\"python_module_probe_error\":{},\"linux_libgl_present\":{},\"linux_libxkbcommon_present\":{},\"issues\":[{}]}}\n",
            if categories.is_empty() { "true" } else { "false" },
            python.replace('"', "\\\""),
            python_resolution.source.as_str(),
            gui_entry.display().to_string().replace('"', "\\\""),
            requirements.display().to_string().replace('"', "\\\""),
            if has_display { "true" } else { "false" },
            if xvfb_available { "true" } else { "false" },
            missing_python_modules
                .iter()
                .map(|module| format!("\"{}\"", module.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(","),
            python_module_probe_error
                .as_ref()
                .map(|value| format!("\"{}\"", value.replace('"', "\\\"")))
                .unwrap_or_else(|| "null".to_string()),
            linux_libgl_json,
            linux_libxkb_json,
            escaped
        );
        fs::write(&path, payload).map_err(|err| {
            anyhow::anyhow!("failed to write GUI diagnostics JSON to {}: {}", path, err)
        })?;
        println!("Diagnostics JSON written to: {}", path);
    }

    if strict && !categories.is_empty() {
        anyhow::bail!("GUI diagnostics failed in strict mode");
    }

    Ok(())
}

fn print_gui_readiness(strict: bool) -> Result<()> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gui_entry = crate_root
        .join("..")
        .join("..")
        .join("third_party")
        .join("mohawk_gui")
        .join("main.py");
    let requirements = crate_root
        .join("..")
        .join("..")
        .join("third_party")
        .join("mohawk_gui")
        .join("requirements.txt");
    let repo_root = crate_root.join("..").join("..");
    let python =
        resolve_python_executable_for_root(&repo_root, std::env::var("GHOSTLINK_PYTHON").ok());

    let mut issues: Vec<String> = Vec::new();

    println!("Ghost-Link GUI Readiness Report\n");
    println!("===============================\n");
    println!("GUI entry: {}", gui_entry.display());
    println!("Requirements: {}", requirements.display());
    println!("Python executable: {}", python);

    if !gui_entry.exists() {
        issues.push(format!("Missing GUI entrypoint: {}", gui_entry.display()));
    }

    if !requirements.exists() {
        issues.push(format!(
            "Missing GUI requirements file: {}",
            requirements.display()
        ));
    }

    match Command::new(&python).arg("--version").output() {
        Ok(output) => {
            let version = String::from_utf8_lossy(if output.stdout.is_empty() {
                &output.stderr
            } else {
                &output.stdout
            });
            println!("Python version: {}", version.trim());
        }
        Err(err) => {
            issues.push(format!("Python executable is not runnable: {}", err));
        }
    }

    match detect_missing_gui_python_modules(&python) {
        Ok(missing) if missing.is_empty() => {
            println!("Python modules: OK (PyQt6, requests, pyqtgraph)");
        }
        Ok(missing) => {
            issues.push(format!("Missing Python modules: {}", missing.join(", ")));
        }
        Err(err) => {
            issues.push(format!("Unable to validate Python modules: {}", err));
        }
    }

    #[cfg(target_os = "linux")]
    {
        let has_libgl = has_linux_libgl();
        let has_libxkb = has_linux_libxkbcommon();
        println!(
            "Linux OpenGL runtime (libGL.so.1): {}",
            if has_libgl { "present" } else { "missing" }
        );
        println!(
            "Linux XKB runtime (libxkbcommon.so.0): {}",
            if has_libxkb { "present" } else { "missing" }
        );
        if !has_libgl {
            issues.push("Missing libGL.so.1 system dependency (install `libgl1`)".to_string());
        }
        if !has_libxkb {
            issues.push(
                "Missing libxkbcommon.so.0 system dependency (install `libxkbcommon0`)".to_string(),
            );
        }
    }

    let has_display = std::env::var("DISPLAY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
        || std::env::var("WAYLAND_DISPLAY")
            .ok()
            .filter(|v| !v.is_empty())
            .is_some();
    println!(
        "Display session: {}",
        if has_display {
            "detected"
        } else {
            "not detected (headless)"
        }
    );

    if issues.is_empty() {
        println!("\nReadiness: PASS");
        return Ok(());
    }

    println!("\nReadiness: FAIL");
    println!("Issues:");
    for issue in &issues {
        println!("- {}", issue);
    }

    println!("\nSuggested fixes:");
    println!(
        "- Install Python deps: {} -m pip install -r {}",
        python,
        requirements.display()
    );
    #[cfg(target_os = "linux")]
    println!(
        "- Install system libs: sudo apt-get update && sudo apt-get install -y libgl1 libxkbcommon0"
    );

    if strict {
        anyhow::bail!("GUI readiness check failed in strict mode");
    }

    Ok(())
}

fn detect_missing_gui_python_modules(python: &str) -> Result<Vec<String>> {
    let output = Command::new(python)
        .args([
            "-c",
            "import importlib.util as u;mods=['PyQt6','requests','pyqtgraph'];missing=[m for m in mods if u.find_spec(m) is None];print(','.join(missing))",
        ])
        .output()
        .map_err(|err| anyhow::anyhow!("unable to execute Python '{}': {}", python, err))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "python module check failed with status {}",
            output.status
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let missing = stdout
        .trim()
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    Ok(missing)
}

#[cfg(target_os = "linux")]
fn has_linux_libgl() -> bool {
    let libgl_candidates = [
        "/usr/lib/x86_64-linux-gnu/libGL.so.1",
        "/usr/lib64/libGL.so.1",
        "/usr/lib/libGL.so.1",
    ];

    libgl_candidates.iter().any(|path| Path::new(path).exists())
}

#[cfg(target_os = "linux")]
fn has_linux_libxkbcommon() -> bool {
    let xkb_candidates = [
        "/usr/lib/x86_64-linux-gnu/libxkbcommon.so.0",
        "/usr/lib64/libxkbcommon.so.0",
        "/usr/lib/libxkbcommon.so.0",
    ];

    xkb_candidates.iter().any(|path| Path::new(path).exists())
}

fn run_gui_preflight_checks() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if !has_linux_libgl() {
            anyhow::bail!(
                "GUI preflight failed: required OpenGL runtime library libGL.so.1 is missing. \
Install system dependency (Debian/Ubuntu): sudo apt-get update && sudo apt-get install -y libgl1"
            );
        }

        if !has_linux_libxkbcommon() {
            anyhow::bail!(
                "GUI preflight failed: required XKB runtime library libxkbcommon.so.0 is missing. \
Install system dependency (Debian/Ubuntu): sudo apt-get update && sudo apt-get install -y libxkbcommon0"
            );
        }
    }

    Ok(())
}

// Re-export protocol module for use in main.rs
mod protocol {
    pub use ghostlink_core::protocol::GHOSTLINK_ETHERTYPE;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostlink_core::host::AccelerationMode;
    use ghostlink_core::host::RuntimeProfile;
    use ghostlink_core::protocol::NodeResources;
    use std::io::Read;
    use std::net::TcpListener;
    use std::time::{SystemTime, UNIX_EPOCH};

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
            parse_cli(args(&["listen", "node-l", "--once"])).unwrap(),
            CliCommand::Listen {
                node_id: "node-l".to_string(),
                once: true,
            }
        );
        assert_eq!(
            parse_cli(args(&["gui", "--port", "8003"])).unwrap(),
            CliCommand::Gui {
                args: vec!["--port".to_string(), "8003".to_string()],
            }
        );
        assert_eq!(
            parse_cli(args(&["gui-check", "--strict"])).unwrap(),
            CliCommand::GuiCheck { strict: true }
        );
        assert_eq!(
            parse_cli(args(&["gui-diagnose", "--strict"])).unwrap(),
            CliCommand::GuiDiagnose { strict: true }
        );
        assert_eq!(
            parse_cli(args(&["doctor", "--strict"])).unwrap(),
            CliCommand::Doctor(DoctorOptions {
                strict: true,
                json_out: None,
                network_probe: false,
                network_target: "127.0.0.1:8003".to_string(),
            })
        );
        assert_eq!(
            parse_cli(args(&[
                "doctor",
                "--strict",
                "--network-probe",
                "--network-target",
                "127.0.0.1:18765",
                "--json",
                "./tmp/doctor.json",
            ]))
            .unwrap(),
            CliCommand::Doctor(DoctorOptions {
                strict: true,
                json_out: Some(PathBuf::from("./tmp/doctor.json")),
                network_probe: true,
                network_target: "127.0.0.1:18765".to_string(),
            })
        );
        assert_eq!(
            parse_cli(args(&["cluster-start", "4", "46010"])).unwrap(),
            CliCommand::ClusterStart {
                node_count: 4,
                base_port: 46010,
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
                top_k: 40,
                penalty: 1.1,
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
                top_k: 40,
                penalty: 1.1,
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
            parse_cli(args(&["listen"])).unwrap(),
            CliCommand::Listen {
                node_id: "local-node".to_string(),
                once: false,
            }
        );
        assert_eq!(
            parse_cli(args(&["gui"])).unwrap(),
            CliCommand::Gui { args: vec![] }
        );
        assert_eq!(
            parse_cli(args(&["gui-check"])).unwrap(),
            CliCommand::GuiCheck { strict: false }
        );
        assert_eq!(
            parse_cli(args(&["gui-diagnose"])).unwrap(),
            CliCommand::GuiDiagnose { strict: false }
        );
        assert_eq!(
            parse_cli(args(&["doctor"])).unwrap(),
            CliCommand::Doctor(DoctorOptions {
                strict: false,
                json_out: None,
                network_probe: false,
                network_target: "127.0.0.1:8003".to_string(),
            })
        );
        assert_eq!(
            parse_cli(args(&["cluster-start"])).unwrap(),
            CliCommand::ClusterStart {
                node_count: 3,
                base_port: 46000,
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
                top_k: 40,
                penalty: 1.1,
            }
        );
    }

    #[test]
    fn tcp_autotune_default_candidates_include_base_and_neighbors() {
        assert_eq!(
            normalize_tcp_autotune_candidates(Vec::new(), 512),
            vec![32, 64, 128, 256, 512, 1024]
        );
        assert_eq!(
            normalize_tcp_autotune_candidates(Vec::new(), 96),
            vec![32, 48, 64, 96, 128, 192, 256]
        );
    }

    #[test]
    fn tcp_autotune_explicit_candidates_stay_authoritative() {
        assert_eq!(
            normalize_tcp_autotune_candidates(vec![256, 64, 256, 0, 32], 512),
            vec![32, 64, 256]
        );
    }

    #[test]
    fn python_resolution_prefers_repo_venv_then_falls_back() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ghostlink-python-root-{unique}"));
        let venv_bin = root.join(".venv").join("bin");
        std::fs::create_dir_all(&venv_bin).unwrap();
        let venv_python = venv_bin.join("python");
        std::fs::write(&venv_python, "#!/bin/sh\n").unwrap();

        let venv_resolved = resolve_python_for_root(&root, None);
        assert_eq!(venv_resolved.executable, venv_python.display().to_string());
        assert_eq!(venv_resolved.source, PythonResolutionSource::RepoVenv);
        let configured = resolve_python_for_root(&root, Some("custom-python".to_string()));
        assert_eq!(configured.executable, "custom-python");
        assert_eq!(
            configured.source,
            PythonResolutionSource::ConfiguredOverride
        );

        std::fs::remove_file(&venv_python).unwrap();
        let fallback = resolve_python_for_root(&root, None);
        assert_eq!(fallback.executable, "python3");
        assert_eq!(fallback.source, PythonResolutionSource::SystemFallback);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gui_python_override_skips_generic_python_defaults() {
        assert!(!should_apply_gui_python_override("python3"));
        assert!(!should_apply_gui_python_override("python"));
        assert!(!should_apply_gui_python_override("   "));
        assert!(should_apply_gui_python_override(
            "/workspaces/Ghostlink/.venv/bin/python"
        ));
        assert!(should_apply_gui_python_override("python3.12"));
    }

    #[test]
    fn parse_gui_backend_target_prefers_backend_url_over_host_port() {
        let (host, port) = parse_gui_backend_target(&[
            "--host".to_string(),
            "ignored.local".to_string(),
            "--port".to_string(),
            "9001".to_string(),
            "--backend-url".to_string(),
            "http://127.0.0.1:8123".to_string(),
        ]);

        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 8123);
    }

    #[test]
    fn parse_gui_backend_target_uses_defaults_and_parses_equals_form() {
        let (default_host, default_port) = parse_gui_backend_target(&[]);
        assert_eq!(default_host, "localhost");
        assert_eq!(default_port, 8003);

        let (host, port) =
            parse_gui_backend_target(&["--host=10.0.0.8".to_string(), "--port=8111".to_string()]);
        assert_eq!(host, "10.0.0.8");
        assert_eq!(port, 8111);
    }

    #[test]
    fn network_probe_rejects_invalid_target() {
        let result = probe_network_target("not-a-target", Duration::from_millis(50));
        assert!(matches!(result, NetworkProbeOutcome::InvalidTarget(_)));
    }

    #[test]
    fn network_probe_accepts_hostname_and_reports_reachable_latency() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 1];
            let _ = stream.read(&mut buf);
        });

        let result = probe_network_target(
            &format!("localhost:{}", addr.port()),
            Duration::from_millis(250),
        );

        assert!(matches!(
            result,
            NetworkProbeOutcome::Reachable { latency_ms, .. } if latency_ms >= 0.0
        ));
        handle.join().unwrap();
    }

    #[test]
    fn doctor_json_includes_structured_network_probe_context() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 1];
            let _ = stream.read(&mut buf);
        });

        let mut checks = Vec::new();
        run_optional_network_probe(&format!("localhost:{}", addr.port()), &mut checks);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ghostlink-doctor-{unique}.json"));
        write_doctor_report_json(&path, &checks, 1, 0, 0).unwrap();
        let payload = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(payload.contains("\"name\":\"network-probe\""));
        assert!(payload.contains("\"context\":{"));
        assert!(payload.contains("\"target\":\"localhost:"));
        assert!(payload.contains("\"reachable\":true"));
        assert!(payload.contains("\"latency_ms\":"));
        handle.join().unwrap();
    }

    #[test]
    fn doctor_json_serializes_generic_check_context() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ghostlink-doctor-generic-{unique}.json"));

        let checks = vec![DoctorCheck {
            area: "readiness",
            name: "local-config",
            status: DoctorStatus::Pass,
            detail: "using ./ghostlink.toml".to_string(),
            fix: None,
            context_json: Some("{\"path\":\"./ghostlink.toml\",\"exists\":true}".to_string()),
        }];

        write_doctor_report_json(&path, &checks, 1, 0, 0).unwrap();
        let payload = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(payload.contains("\"name\":\"local-config\""));
        assert!(payload.contains("\"context\":{"));
        assert!(payload.contains("\"path\":\"./ghostlink.toml\""));
        assert!(payload.contains("\"exists\":true"));
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(parse_cli(args(&[])).is_err());
        assert!(parse_cli(args(&["unknown"])).is_err());
        assert!(parse_cli(args(&["probe", "n1", "nonsense"])).is_err());
        assert!(parse_cli(args(&["flow", "a", "b", "32", "64", "bad"])).is_err());
        assert!(parse_cli(args(&["flow", "a", "b", "32", "64", "64", "2", "bad-mode"])).is_err());
        assert!(parse_cli(args(&["cluster-start", "2", "not-a-port"])).is_err());
        assert!(parse_cli(args(&["doctor", "--json"])).is_err());
        assert!(parse_cli(args(&["doctor", "--network-target"])).is_err());
        assert!(parse_cli(args(&["doctor", "--nope"])).is_err());
    }

    #[test]
    fn maps_neon_profile_to_npu_device_kind() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("local", 0.0, 16.0, "cpu", None),
            logical_cores: 8,
            recommended_workers: 4,
            acceleration_mode: AccelerationMode::Neon,
            xdp_supported: true,
            detection_source: "test".to_string(),
            probe_mode: ProbeMode::Fast,
        };

        let map = build_device_map(&profile, "local", "remote");
        assert_eq!(map.get("local"), Some(&DeviceKind::Npu));
        assert_eq!(map.get("remote"), Some(&DeviceKind::Gpu));
    }

    #[test]
    fn bootstrap_extracts_config_argument() {
        let bootstrap = extract_bootstrap_args(vec![
            "--config".to_string(),
            "./ghostlink.toml".to_string(),
            "flow".to_string(),
            "node-a".to_string(),
        ])
        .unwrap();

        assert_eq!(
            bootstrap.config_path,
            Some(PathBuf::from("./ghostlink.toml"))
        );
        assert_eq!(bootstrap.command_args, vec!["flow", "node-a"]);
    }

    #[test]
    fn bootstrap_rejects_missing_config_value() {
        let result = extract_bootstrap_args(vec!["--config".to_string()]);
        assert!(result.is_err());
    }
}
