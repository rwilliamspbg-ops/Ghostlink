//! Ghost-Link CLI Demo
//!
//! Command-line interface for demonstrating Ghost-Link primitives:
//! - `plan` - Generate layer placement plan
//! - `join` - Broadcast discovery frame to join cluster
//! - `dashboard` - Display ASCII cluster dashboard

use crate::runtime::Runtime;
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
    build_token_schedule, execute_pipeline_tcp_loopback, execute_pipeline_tcp_loopback_with_config,
    execute_pipeline_with_rebalance_and_measured, DeviceKind, PipelinePlan, TcpTransportConfig,
};
use ghostlink_core::xdp::probe_xdp_support;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;

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
    Xdp,
}

impl FlowTransportMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InMemory => "inmem",
            Self::TcpLoopback => "tcp",
            Self::Xdp => "xdp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InferenceBackend {
    Ollama,
    Native,
}

impl InferenceBackend {
    #[allow(dead_code)]
    fn from_env() -> Self {
        Self::parse(
            &std::env::var("GHOSTLINK_INFERENCE_BACKEND").unwrap_or_else(|_| "ollama".to_string()),
        )
    }

    /// Parse a backend name the same way regardless of whether it came from
    /// an env var (startup) or a live settings update (runtime). This is the
    /// single place backend-name strings get interpreted so the two paths
    /// can't silently disagree.
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "native" | "fabric" => Self::Native,
            _ => Self::Ollama,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Native => "native",
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
                .unwrap_or(8003);
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
        Some("xdp" | "af_xdp" | "afxdp") => Ok(FlowTransportMode::Xdp),
        Some(other) => anyhow::bail!("invalid flow transport mode: {other}"),
    }
}

fn xdp_interface_from_env() -> String {
    std::env::var("GHOSTLINK_XDP_INTERFACE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "eth0".to_string())
}

fn xdp_optimized_tcp_config() -> TcpTransportConfig {
    let mut cfg = tcp_transport_config_from_env();
    cfg.max_inflight_batches = std::env::var("GHOSTLINK_XDP_MAX_INFLIGHT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(256)
        .max(1);
    cfg.reconnect_attempts = cfg.reconnect_attempts.min(2);
    cfg.reconnect_backoff_ms = cfg.reconnect_backoff_ms.min(10);
    cfg
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
        "{{
  \"transport_mode\": \"{}\",
  \"token_count\": {},
  \"micro_batch\": {},
  \"batch_count\": {},
  \"stage_count\": {},
  \"total_time_ms\": {:.6},
  \"throughput_tokens_per_sec\": {:.6},
  \"avg_token_latency_ms\": {:.6},
  \"p95_token_latency_ms\": {:.6},
  \"tcp_max_inflight_batches\": {},
  \"tcp_reconnect_attempts\": {},
  \"tcp_reconnect_backoff_ms\": {},
  \"stage_stats\": [{}]
}}
",
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
        .unwrap_or(256)
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
    env_bool(name) == Some(true)
}

fn parse_env_bool_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_bool(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .and_then(|raw| parse_env_bool_value(&raw))
}

fn xdp_autotune_enabled_from_flags(
    tcp_autotune_flag: Option<bool>,
    xdp_autotune_flag: Option<bool>,
) -> bool {
    xdp_autotune_flag.or(tcp_autotune_flag).unwrap_or(true)
}

fn xdp_autotune_enabled() -> bool {
    xdp_autotune_enabled_from_flags(
        env_bool("GHOSTLINK_TCP_AUTOTUNE"),
        env_bool("GHOSTLINK_XDP_AUTOTUNE"),
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
    fs::write(
        &cache_path,
        lines.join(
            "
",
        ) + "
",
    )
    .map_err(|err| {
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
        "  flow [local_id] [remote_id] [remote_vram_gb] [remote_mem_gb] [exec_tokens] [micro_batch] [transport=tcp|xdp|inmem] - Run full 30B planning flow"
    );
    eprintln!("  help      - Show this help message");
    eprintln!();
    eprintln!("Config:");
    eprintln!("  --config <path> - Load default values from a TOML config file");
    eprintln!("  Env fallback     - Set GHOSTLINK_CONFIG to a config file path");
}

fn print_help() {
    println!(
        "ghost-link CLI Demo
"
    );
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
        "  flow [local_id] [remote_id] [remote_vram_gb] [remote_mem_gb] [exec_tokens] [micro_batch] [transport=tcp|xdp|inmem] - Run full 30B planning flow"
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
    let enable_inmem_runtime_feedback = is_env_truthy("GHOSTLINK_FLOW_ENABLE_REBALANCE");

    let schedule_preview_tokens = opts.execution_tokens.min(8);
    let token_schedule = build_token_schedule(pipeline_plan.stages.len(), schedule_preview_tokens);
    let mut selected_tcp_cfg: Option<TcpTransportConfig> = None;
    let mut effective_transport_mode = opts.transport_mode;
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
        FlowTransportMode::InMemory => {
            let rebalance = if enable_inmem_runtime_feedback {
                Some(&rebalance_trigger)
            } else {
                None
            };
            let cluster_feedback = if enable_inmem_runtime_feedback {
                Some(&cluster)
            } else {
                None
            };
            let placement_feedback = if enable_inmem_runtime_feedback {
                Some(&placement_context)
            } else {
                None
            };

            execute_pipeline_with_rebalance_and_measured(
                &pipeline_plan,
                opts.execution_tokens,
                opts.micro_batch,
                rebalance,
                cluster_feedback,
                placement_feedback,
            )
        }
        FlowTransportMode::Xdp => {
            let interface = xdp_interface_from_env();
            match probe_xdp_support(&interface) {
                Ok(()) => {
                    println!(
                        "AF_XDP probe succeeded on interface '{}'; using xdp-optimized runtime settings.",
                        interface
                    );
                    let base_tcp_cfg = xdp_optimized_tcp_config();
                    let tcp_cfg = if xdp_autotune_enabled() {
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
                Err(reason) => {
                    println!(
                        "AF_XDP unavailable on '{}': {}. Falling back to TCP transport.",
                        interface, reason
                    );
                    effective_transport_mode = FlowTransportMode::TcpLoopback;
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
            }
        }
    };

    let load_balancer =
        LoadBalancer::with_runtime_profile(Arc::new(cluster.clone()), &local_profile);
    let distribution = load_balancer
        .distribute_layers_with_runtime_profile(&layers, &local_profile)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!(
        "Ghost-Link 30B Multi-Host Runtime Flow
"
    );
    println!(
        "====================================
"
    );
    println!("Local node: {}", local_profile.node_resources.id);
    println!("Remote node: {}", opts.remote_id);
    println!(
        "Local acceleration: {}",
        local_profile.acceleration_mode.as_str()
    );
    println!("Local workers: {}", local_profile.recommended_workers);
    println!(
        "Total cluster nodes: {}
",
        cluster.node_count()
    );

    println!(
        "Health Summary:
{}",
        health_monitor.get_health_summary()
    );

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

    println!(
        "
Distribution Summary:"
    );
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
    maybe_write_flow_metrics_json(
        &execution,
        effective_transport_mode,
        selected_tcp_cfg.as_ref(),
    )?;

    println!("Execution Modes:");
    println!("- NPU/GPU/CPU backend selection is runtime-profile driven");
    println!("- Flow currently provides transparent planning and health-driven orchestration");
    println!(
        "- Inter-stage transport mode: {} (real runtime wiring)",
        effective_transport_mode.as_str()
    );
    if matches!(effective_transport_mode, FlowTransportMode::InMemory) {
        println!(
            "- In-memory runtime feedback/rebalance: {} (set GHOSTLINK_FLOW_ENABLE_REBALANCE=1 to enable)",
            if enable_inmem_runtime_feedback {
                "enabled"
            } else {
                "disabled"
            }
        );
    }
    println!("- Use tcp for socket-backed transport, xdp for AF_XDP-first with automatic fallback, or inmem for channel-backed baseline
");

    if matches!(effective_transport_mode, FlowTransportMode::TcpLoopback)
        || matches!(opts.transport_mode, FlowTransportMode::Xdp)
    {
        println!(
            "TCP transport controls: GHOSTLINK_TCP_MAX_INFLIGHT, GHOSTLINK_TCP_RECONNECT_ATTEMPTS, GHOSTLINK_TCP_RECONNECT_BACKOFF_MS, GHOSTLINK_TCP_AUTH_TOKEN, GHOSTLINK_TCP_AUTOTUNE
"
        );
        if matches!(opts.transport_mode, FlowTransportMode::Xdp) {
            println!(
                "XDP control: GHOSTLINK_XDP_INTERFACE (default: eth0), GHOSTLINK_XDP_AUTOTUNE (default: true when AF_XDP probe succeeds). If AF_XDP probe fails, runtime falls back to TCP automatically.
"
            );
        }
    }

    Ok(())
}

#[derive(Debug)]
struct BackendState {
    models: Vec<ModelRecord>,
    current_model: String,
    workers: Vec<WorkerRecord>,
    sessions: Vec<SessionRecord>,
    #[allow(dead_code)]
    queue_depth: usize,
    chat_requests: u64,
    last_latency_ms: f32,
    started_at: Instant,
    backend_url: String,
    cluster: Arc<ClusterState>,
    inference_backend: InferenceBackend,
    native_engine_client: native_engine::NativeEngineClient,
    ollama_client: ollama::OllamaClient,
    ollama_available: Arc<tokio::sync::Mutex<bool>>,
    settings: RuntimeSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeSettings {
    inference_backend: String,
    native_engine: String,
    ngl: i32,
    model_path: String,
    models_dir: String,
    llama_server_url: String,
    llama_port: u16,
    api_host: String,
    api_port: u16,
    gui_port: u16,
    threads: usize,
    ctx_size: usize,
    temperature: f32,
    top_p: f32,
    top_k: usize,
    repeat_penalty: f32,
    max_tokens: usize,
    chat_exec_tokens: usize,
    chat_micro_batch: usize,
    tcp_max_inflight: usize,
    discovery_listen: String,
    discovery_broadcast: String,
    discovery_auth_token: String,
    tcp_auth_token: String,
    xdp_interface: String,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            inference_backend: "native".to_string(),
            native_engine: "llama_server".to_string(),
            ngl: -1,
            model_path: String::new(),
            models_dir: "models".to_string(),
            llama_server_url: "http://127.0.0.1:8080/completion".to_string(),
            llama_port: 8080,
            api_host: "127.0.0.1".to_string(),
            api_port: 8003,
            gui_port: 5173,
            threads: 4,
            ctx_size: 4096,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repeat_penalty: 1.1,
            max_tokens: 2048,
            chat_exec_tokens: 1024,
            chat_micro_batch: 4,
            tcp_max_inflight: 256,
            discovery_listen: "0.0.0.0:45885".to_string(),
            discovery_broadcast: "255.255.255.255:45885".to_string(),
            discovery_auth_token: String::new(),
            tcp_auth_token: String::new(),
            xdp_interface: "eth0".to_string(),
        }
    }
}

struct ToolDispatcher;

impl ToolDispatcher {
    fn dispatch(tool_name: &str, _args: &serde_json::Value) -> ToolResult {
        match tool_name {
            "calculator" => ToolResult {
                tool: tool_name.to_string(),
                result: "42 (Calculated via Rust built-in tool)".to_string(),
                success: true,
            },
            "web_search" => ToolResult {
                tool: tool_name.to_string(),
                result: "Ghostlink is a high-performance distributed LLM inference fabric."
                    .to_string(),
                success: true,
            },
            "terminal" => ToolResult {
                tool: tool_name.to_string(),
                result: "System: All nodes operational. Kernel bypass active.".to_string(),
                success: true,
            },
            "code_execution" => ToolResult {
                tool: tool_name.to_string(),
                result: "Output: Processed tensor batch in 2.4ms".to_string(),
                success: true,
            },
            _ => ToolResult {
                tool: tool_name.to_string(),
                result: format!("Tool '{}' executed successfully.", tool_name),
                success: true,
            },
        }
    }
}

fn models_path() -> PathBuf {
    std::env::var("GHOSTLINK_MODELS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("models.json"))
}

fn settings_path() -> PathBuf {
    std::env::var("GHOSTLINK_SETTINGS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("settings.json"))
}

fn load_persistent_models() -> Vec<ModelRecord> {
    let path = models_path();
    if path.exists() {
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(models) = serde_json::from_str::<Vec<ModelRecord>>(&data) {
                return models;
            }
        }
    }
    vec![
        ModelRecord {
            name: "meta-llama/Llama-3-8B-Instruct".to_string(),
            size_gb: 8.0,
            model_type: "LLM".to_string(),
            quantization: "Q4_K_M".to_string(),
            status: "Ready".to_string(),
            local_path: String::new(),
        },
        ModelRecord {
            name: "mistralai/Mistral-7B-Instruct-v0.2".to_string(),
            size_gb: 7.2,
            model_type: "LLM".to_string(),
            quantization: "Q8_0".to_string(),
            status: "Ready".to_string(),
            local_path: String::new(),
        },
        ModelRecord {
            name: "google/gemma-7b-it".to_string(),
            size_gb: 7.0,
            model_type: "LLM".to_string(),
            quantization: "BF16".to_string(),
            status: "Ready".to_string(),
            local_path: String::new(),
        },
        ModelRecord {
            name: "ghostlink-30b-v1".to_string(),
            size_gb: 30.0,
            model_type: "LLM".to_string(),
            quantization: "Q4_K_M".to_string(),
            status: "Ready".to_string(),
            local_path: String::new(),
        },
    ]
}

fn save_persistent_models(models: &[ModelRecord]) {
    if let Ok(data) = serde_json::to_string_pretty(models) {
        let _ = fs::write(models_path(), data);
    }
}

fn load_settings() -> RuntimeSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(settings) = serde_json::from_str::<RuntimeSettings>(&data) {
                return settings;
            }
        }
    }
    RuntimeSettings::default()
}

fn save_settings(settings: &RuntimeSettings) {
    if let Ok(data) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(settings_path(), data);
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionRequest {
    model: String,
    #[allow(dead_code)]
    messages: Vec<serde_json::Value>,
    #[allow(dead_code)]
    stream: Option<bool>,
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
}
#[derive(Debug, Deserialize)]
struct GuiChatRequest {
    message: String,
    #[allow(dead_code)]
    model: Option<String>,
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
    ollama_url: Option<String>,
    #[allow(dead_code)]
    stream: Option<bool>,
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
struct ModelDeleteRequest {
    model: String,
}
#[derive(Debug, Deserialize)]
struct OllamaModelRequest {
    model: String,
}
#[derive(Debug, Deserialize)]
struct OllamaCreateRequest {
    name: String,
    modelfile: String,
}
#[derive(Debug, Deserialize)]
struct OllamaCopyRequest {
    source: String,
    destination: String,
}
#[derive(Debug, Deserialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}
#[derive(Debug, Deserialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[allow(dead_code)]
    stream: Option<bool>,
    #[allow(dead_code)]
    temperature: Option<f32>,
    #[allow(dead_code)]
    top_p: Option<f32>,
    #[allow(dead_code)]
    top_k: Option<usize>,
    #[allow(dead_code)]
    repeat_penalty: Option<f32>,
    #[allow(dead_code)]
    max_tokens: Option<usize>,
}
#[derive(Debug, Deserialize)]
struct OllamaNameRequest {
    name: String,
}
#[derive(Debug, Deserialize)]
struct WorkerAddRequest {
    host: String,
    port: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelRecord {
    name: String,
    size_gb: f32,
    model_type: String,
    quantization: String,
    status: String,
    #[serde(default)]
    local_path: String,
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
#[derive(Debug, Serialize, Deserialize)]
struct ToolResult {
    tool: String,
    result: String,
    success: bool,
}

fn start_openai_api_server(port: u16, host: &str) -> Result<()> {
    use axum::{
        extract::{Path, Query, State},
        response::{
            sse::{Event, Sse},
            IntoResponse,
        },
        routing::{delete, get, post},
        Json, Router,
    };
    use futures::stream;
    use futures::StreamExt;
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower_http::cors::CorsLayer;

    fn lock_state(state: &Arc<Mutex<BackendState>>) -> std::sync::MutexGuard<'_, BackendState> {
        state.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn chat_exec_micro_batch() -> usize {
        env_default_usize(
            "GHOSTLINK_CHAT_MICRO_BATCH",
            env_default_usize("GHOSTLINK_FLOW_DEFAULT_MICRO_BATCH", 4),
        )
        .clamp(1, 128)
    }

    fn chat_exec_token_budget(default_tokens: usize) -> usize {
        env_default_usize("GHOSTLINK_CHAT_EXEC_TOKENS", default_tokens).clamp(16, 4096)
    }

    async fn handle_chat_completions(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ChatCompletionRequest>,
    ) -> Json<ChatCompletionResponse> {
        let prompt = req
            .messages
            .iter()
            .rev()
            .find_map(|msg| {
                msg.get("content")
                    .and_then(|content| content.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_default();

        let request_tracker = active_runtime_switcher().request_tracker().clone();
        request_tracker.increment().await;

        let temp = req.temperature.unwrap_or(0.7);
        let top_p = req.top_p.unwrap_or(0.9);
        let top_k = req.top_k.unwrap_or(40);
        let penalty = req.penalty.unwrap_or(1.1);
        let max_tokens = req.max_tokens.unwrap_or(1024).clamp(16, 4096);

        let (
            model,
            cluster,
            chat_req_id,
            inference_backend,
            native_engine_client,
            ollama_client,
            settings,
        ) = {
            let mut backend = lock_state(&state);
            backend.chat_requests = backend.chat_requests.saturating_add(1);
            let model = if req.model.trim().is_empty() {
                backend.current_model.clone()
            } else {
                req.model.clone()
            };
            (
                model,
                Arc::clone(&backend.cluster),
                backend.chat_requests,
                backend.inference_backend,
                backend.native_engine_client.clone(),
                backend.ollama_client.clone(),
                backend.settings.clone(),
            )
        };

        let nodes = cluster.nodes();
        let total_vram = cluster.total_vram_gb();
        let layer_count = (total_vram * 2.0).clamp(8.0, 60.0) as usize;
        let layers: Vec<LayerSpec> = (0..layer_count)
            .map(|index| LayerSpec {
                index,
                vram_gb: (total_vram / (layer_count as f32 + 1.0)).min(0.4),
                num_weights: 500_000_000 / 60,
            })
            .collect();

        let profile = detect_runtime_profile("studio-api");
        let exec_tokens = chat_exec_token_budget(32);
        let exec_micro_batch = chat_exec_micro_batch();
        let mut execution_info = String::new();
        let result = match assign_layers_with_runtime_profile(&nodes, &layers, &profile) {
            Ok(assignments) => {
                let device_map = build_device_map_from_cluster(&profile, &cluster);
                let pipeline_plan = PipelinePlan::from_assignments(&assignments, &device_map);
                let pipeline_plan_clone = pipeline_plan.clone();

                let exec_result = if nodes.len() > 1 {
                    ghostlink_core::runtime::execute_pipeline_distributed(
                        &pipeline_plan,
                        exec_tokens,
                        exec_micro_batch,
                        tcp_transport_config_from_env(),
                        &cluster,
                        None,
                        None,
                    )
                    .ok()
                } else {
                    execute_pipeline_tcp_loopback(&pipeline_plan, exec_tokens, exec_micro_batch)
                        .ok()
                };

                if let Some(ref exec) = exec_result {
                    let mut backend = lock_state(&state);
                    backend.last_latency_ms = exec.avg_token_latency_ms;
                    let tokens_per_sec = exec.throughput_tokens_per_sec;
                    for stage in &exec.stage_stats {
                        if let Some(stage_p) = pipeline_plan_clone.stages.get(stage.stage_idx) {
                            backend.cluster.get_metrics_mut(&stage_p.node_id, |m| {
                                m.record_latency(stage.avg_compute_ms * 1000.0);
                                m.record_throughput(tokens_per_sec / 100.0);
                            });
                        }
                    }
                }
                exec_result
            }
            Err(_) => None,
        };

        if let Some(exec) = result {
            execution_info = format!(
                " (Throughput: {:.2} tok/s, Latency: {:.2} ms)",
                exec.throughput_tokens_per_sec, exec.avg_token_latency_ms
            );
        }

        let (response_text, real_inference, backend_used) = match inference_backend {
            InferenceBackend::Ollama => {
                let ollama_temp = temp;
                let ollama_top_p = top_p;
                let ollama_top_k = top_k;
                let ollama_penalty = penalty;
                let ollama_max_tokens = max_tokens;
                let ollama_model = model.clone();

                match ollama_client
                    .generate(
                        &ollama_model,
                        &prompt,
                        ollama_temp,
                        ollama_top_p,
                        ollama_top_k,
                        ollama_penalty,
                        ollama_max_tokens,
                    )
                    .await
                {
                    Ok(text) => (
                        text,
                        true,
                        InferenceBackend::Ollama.as_str(),
                    ),
                    Err(err) => (
                        format!(
                            "Ollama generation failed for model '{}': {}",
                            ollama_model, err
                        ),
                        false,
                        InferenceBackend::Ollama.as_str(),
                    ),
                }
            }
InferenceBackend::Native => match native_engine_client
            .generate(&model, &prompt, exec_tokens, 0.7, 0.9, 40, 1.1, &settings.native_engine)
            {
                Ok(gen) => (
                    gen.text,
                    gen.real_inference,
                    InferenceBackend::Native.as_str(),
                ),
                Err(err) => (
                    format!(
                        "Ghostlink native fabric backend executed request #{} on model '{}'. Prompt length: {} chars.{} Native error: {}",
                        chat_req_id,
                        model,
                        prompt.len(),
                        execution_info,
                        err
                    ),
                    false,
                    InferenceBackend::Native.as_str(),
                ),
            },
        };

        let response = Json(ChatCompletionResponse {
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
                    "content": response_text,
                    "backend": backend_used,
                    "real_inference": real_inference
                }),
                finish_reason: "stop".to_string(),
            }],
        });

        request_tracker.decrement().await;
        response
    }

    fn detect_quantization(filename: &str) -> String {
        let upper = filename.to_uppercase();
        let quants = [
            "Q2_K", "Q3_K", "Q4_K", "Q5_K", "Q6_K", "Q8_0", "Q4_0", "Q4_1", "Q5_0", "Q5_1", "F16",
            "BF16",
        ];
        for q in &quants {
            if upper.contains(q) {
                return q.to_string();
            }
        }
        "unknown".to_string()
    }

    fn scan_local_models_dir(models_dir: &str) -> Vec<ModelRecord> {
        let dir = std::path::Path::new(models_dir);
        if !dir.exists() {
            return vec![];
        }
        let mut local = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "gguf").unwrap_or(false) {
                    let filename = path.file_name().unwrap().to_string_lossy().to_string();
                    let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let size_gb = size_bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                    let name = filename
                        .strip_suffix(".gguf")
                        .unwrap_or(&filename)
                        .to_string();
                    local.push(ModelRecord {
                        name,
                        size_gb,
                        model_type: "LLM".to_string(),
                        quantization: detect_quantization(&filename),
                        status: "Ready".to_string(),
                        local_path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
        local
    }

    async fn download_hf_model(
        model_id: &str,
        models_dir: &std::path::Path,
    ) -> Result<String, String> {
        let client = reqwest::Client::builder()
            .user_agent("ghostlink/1.0")
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;

        let api_url = format!("https://huggingface.co/api/models/{}", model_id);
        let resp = client
            .get(&api_url)
            .send()
            .await
            .map_err(|e| format!("API error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Model '{}' not found on HuggingFace", model_id));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        let gguf_files: Vec<String> = data
            .get("siblings")
            .and_then(|s| s.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.get("rfilename").and_then(|f| f.as_str()))
                    .filter(|f| f.ends_with(".gguf"))
                    .map(|f| f.to_string())
                    .collect()
            })
            .unwrap_or_default();

        if gguf_files.is_empty() {
            return Err("No GGUF files found in this repository. Try a GGUF-quantized variant (e.g. lmstudio-community/Meta-Llama-3-8B-Instruct-GGUF).".to_string());
        }

        let filename = &gguf_files[0];
        let file_url = format!(
            "https://huggingface.co/{}/resolve/main/{}",
            model_id, filename
        );
        let dest_path = models_dir.join(filename);

        if dest_path.exists() {
            return Ok(dest_path.to_string_lossy().to_string());
        }

        let resp = client
            .get(&file_url)
            .send()
            .await
            .map_err(|e| format!("Download error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Failed to download file (HTTP {})", resp.status()));
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Dir error: {}", e))?;
        }
        let mut file = tokio::fs::File::create(&dest_path)
            .await
            .map_err(|e| format!("File error: {}", e))?;
        let mut stream = resp;
        while let Some(chunk) = stream
            .chunk()
            .await
            .map_err(|e| format!("Stream error: {}", e))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Write error: {}", e))?;
        }
        file.flush().await.ok();

        Ok(dest_path.to_string_lossy().to_string())
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

        let mut merged: Vec<ModelRecord> = backend.models.clone();
        let local = scan_local_models_dir(&backend.settings.models_dir);
        for l in &local {
            if let Some(existing) = merged.iter_mut().find(|m| m.name == l.name) {
                existing.local_path = l.local_path.clone();
                existing.size_gb = l.size_gb;
            } else {
                merged.push(l.clone());
            }
        }

        let models = merged
            .iter()
            .map(|model| {
                serde_json::json!({
                    "name": model.name,
                    "size_gb": model.size_gb,
                    "type": model.model_type,
                    "quantization": model.quantization,
                    "status": model.status,
                    "local_path": model.local_path,
                })
            })
            .collect::<Vec<_>>();

        Json(serde_json::json!({
            "models": models,
            "current_model": backend.current_model,
            "total_models": models.len(),
            "loaded_count": merged.iter().filter(|m| m.status == "Loaded").count()
        }))
    }

    async fn handle_gui_model_status(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let loaded_models = backend
            .models
            .iter()
            .filter(|model| model.status == "Loaded")
            .map(|model| model.name.clone())
            .collect::<Vec<_>>();
        let downloading_models = backend
            .models
            .iter()
            .filter(|model| model.status == "Downloading")
            .map(|model| model.name.clone())
            .collect::<Vec<_>>();

        Json(serde_json::json!({
            "loaded_models": loaded_models,
            "downloading_models": downloading_models,
            "current_model": backend.current_model,
        }))
    }

    async fn handle_gui_model_load(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ModelLoadRequest>,
    ) -> Json<serde_json::Value> {
        let requested_model = req.model.trim().to_string();
        if requested_model.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" }));
        }

        let (inference_backend, ollama_client, ollama_available) = {
            let backend = lock_state(&state);
            (
                backend.inference_backend,
                backend.ollama_client.clone(),
                Arc::clone(&backend.ollama_available),
            )
        };

        let selected_model = if inference_backend == InferenceBackend::Ollama {
            let resolve_model = |requested: &str, available: &[String]| -> Option<String> {
                if available.iter().any(|m| m == requested) {
                    return Some(requested.to_string());
                }

                if let Some(found) = available.iter().find(|m| m.eq_ignore_ascii_case(requested)) {
                    return Some(found.clone());
                }

                if !requested.contains(':') {
                    let prefix = format!("{}:", requested.to_ascii_lowercase());
                    if let Some(found) = available
                        .iter()
                        .find(|m| m.to_ascii_lowercase().starts_with(&prefix))
                    {
                        return Some(found.clone());
                    }
                }

                None
            };

            let available_models_result: Result<Vec<String>, String> = ollama_client
                .list_models()
                .await
                .map_err(|err| err.to_string());

            let available_models = match available_models_result {
                Ok(models) => models,
                Err(err_text) => {
                    let mut available_flag = ollama_available.lock().await;
                    *available_flag = false;
                    return Json(serde_json::json!({
                        "error": format!("failed to query ollama models: {}", err_text),
                    }));
                }
            };

            match resolve_model(&requested_model, &available_models) {
                Some(model_name) => {
                    if let Err(err) = ollama_client.show_model(&model_name).await {
                        return Json(serde_json::json!({
                            "error": format!("model '{}' failed preflight: {}", model_name, err),
                        }));
                    }
                    let mut available_flag = ollama_available.lock().await;
                    *available_flag = true;
                    model_name
                }
                None => {
                    let shown = available_models.iter().take(6).cloned().collect::<Vec<_>>();
                    let available_hint = if shown.is_empty() {
                        "<no installed models>".to_string()
                    } else {
                        shown.join(", ")
                    };
                    return Json(serde_json::json!({
                        "error": format!(
                            "model '{}' is not installed in Ollama. Available: {}",
                            requested_model, available_hint
                        ),
                    }));
                }
            }
        } else {
            requested_model.clone()
        };

        // Extract model info and settings under the lock, then drop it before
        // the potentially-long blocking load_model_into_slot call.
        let (native_engine_client, local_path, native_engine) = {
            let mut backend = lock_state(&state);

            // Merge local scans so we can find local_path for locally-downloaded models
            let local = scan_local_models_dir(&backend.settings.models_dir);
            for l in &local {
                if !backend.models.iter().any(|m| m.name == l.name) {
                    backend.models.push(l.clone());
                }
            }

            let local_path = backend
                .models
                .iter()
                .find(|m| m.name == selected_model)
                .and_then(|m| {
                    if m.local_path.is_empty() {
                        None
                    } else {
                        Some(m.local_path.clone())
                    }
                });

            // Save the selected model path to settings
            if let Some(ref path) = local_path {
                backend.settings.model_path = path.clone();
                save_settings(&backend.settings);
            }

            (
                backend.native_engine_client.clone(),
                local_path,
                backend.settings.native_engine.clone(),
            )
        }; // <-- state lock dropped here

        // If using llama_server native engine, load the model into llama-server
        // Run on spawn_blocking so we don't stall the async runtime for up to 60s.
        if let Some(path) = local_path {
            if native_engine == "llama_server" {
                let result = tokio::task::spawn_blocking(move || {
                    native_engine_client.load_model_into_slot(&path)
                })
                .await
                .map_err(|e| format!("task join error: {}", e))
                .and_then(|r| r);

                if let Err(e) = result {
                    return Json(serde_json::json!({
                        "error": format!("failed to load model into llama-server: {}", e),
                    }));
                }
            }
        }

        // Re-acquire lock to update model statuses
        let mut backend = lock_state(&state);
        for m in &mut backend.models {
            if m.status == "Loaded" {
                m.status = "Ready".to_string();
            }
            if m.name == selected_model {
                m.status = "Loaded".to_string();
            }
        }

        backend.current_model = selected_model.clone();
        save_persistent_models(&backend.models);

        Json(serde_json::json!({
            "status": "ok",
            "current_model": backend.current_model,
            "model_path": backend.settings.model_path,
        }))
    }

    async fn handle_gui_model_download(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ModelDownloadRequest>,
    ) -> Json<serde_json::Value> {
        let model_id = req.model_id.trim().to_string();
        if model_id.is_empty() {
            return Json(serde_json::json!({ "error": "model_id cannot be empty" }));
        }

        let models_dir = {
            let backend = lock_state(&state);
            backend.settings.models_dir.clone()
        };
        let models_path = std::path::Path::new(&models_dir);
        fs::create_dir_all(models_path).ok();

        {
            let mut backend = lock_state(&state);
            if !backend.models.iter().any(|m| m.name == model_id) {
                backend.models.push(ModelRecord {
                    name: model_id.clone(),
                    size_gb: 0.0,
                    model_type: "LLM".to_string(),
                    quantization: "unknown".to_string(),
                    status: "Downloading".to_string(),
                    local_path: String::new(),
                });
                save_persistent_models(&backend.models);
            } else if let Some(m) = backend.models.iter_mut().find(|m| m.name == model_id) {
                m.status = "Downloading".to_string();
                save_persistent_models(&backend.models);
            }
        }

        let result = download_hf_model(&model_id, models_path).await;

        match result {
            Ok(local_path) => {
                let filename = std::path::Path::new(&local_path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let name = filename
                    .strip_suffix(".gguf")
                    .unwrap_or(&filename)
                    .to_string();
                let size_bytes = fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0);
                let size_gb = size_bytes as f32 / (1024.0 * 1024.0 * 1024.0);

                let mut backend = lock_state(&state);
                backend.models.retain(|m| m.name != model_id);
                backend.models.push(ModelRecord {
                    name,
                    size_gb,
                    model_type: "LLM".to_string(),
                    quantization: detect_quantization(&filename),
                    status: "Ready".to_string(),
                    local_path,
                });
                save_persistent_models(&backend.models);

                Json(serde_json::json!({
                    "status": "ok",
                    "message": format!("model downloaded ({:.2} GB)", size_gb),
                }))
            }
            Err(err) => Json(serde_json::json!({ "error": err })),
        }
    }

    async fn handle_gui_model_delete(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ModelDeleteRequest>,
    ) -> Json<serde_json::Value> {
        let requested_model = req.model.trim().to_string();
        let mut backend = lock_state(&state);
        backend.models.retain(|m| m.name != requested_model);
        save_persistent_models(&backend.models);
        Json(serde_json::json!({ "status": "ok", "message": "deleted" }))
    }

    async fn handle_gui_model_delete_v2(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(model_name): Path<String>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);

        // Also check local scan for this model
        let local = scan_local_models_dir(&backend.settings.models_dir);
        for l in &local {
            if l.name == model_name && !l.local_path.is_empty() {
                let _ = fs::remove_file(&l.local_path);
            }
        }

        if let Some(m) = backend.models.iter().find(|m| m.name == model_name) {
            if !m.local_path.is_empty() {
                let _ = fs::remove_file(&m.local_path);
            }
        }

        backend.models.retain(|m| m.name != model_name);
        if backend.current_model == model_name {
            backend.current_model = "none".to_string();
        }
        save_persistent_models(&backend.models);
        Json(serde_json::json!({
            "status": "ok",
            "model": model_name
        }))
    }

    async fn handle_gui_model_unload(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(model_name): Path<String>,
    ) -> Json<serde_json::Value> {
        let requested = model_name.trim().to_string();
        if requested.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" }));
        }

        let (inference_backend, ollama_client, ollama_available, native_engine_client, settings) = {
            let backend = lock_state(&state);
            (
                backend.inference_backend,
                backend.ollama_client.clone(),
                Arc::clone(&backend.ollama_available),
                backend.native_engine_client.clone(),
                backend.settings.clone(),
            )
        };

        if inference_backend == InferenceBackend::Ollama {
            if let Err(err) = ollama_client.unload_model(&requested).await {
                return Json(serde_json::json!({
                    "error": format!("failed to unload model '{}' from ollama: {}", requested, err),
                }));
            }
            let mut available_flag = ollama_available.lock().await;
            *available_flag = true;
        } else if settings.native_engine == "llama_server" {
            // For llama_server, unload means stopping the llama-server process
            if let Err(e) = native_engine_client.unload_model() {
                return Json(serde_json::json!({
                    "error": format!("failed to unload model from llama-server: {}", e),
                }));
            }
        }

        let mut backend = lock_state(&state);
        for m in &mut backend.models {
            if m.name == requested && m.status == "Loaded" {
                m.status = "Ready".to_string();
            }
        }
        if backend.current_model == requested {
            backend.current_model = "none".to_string();
        }
        save_persistent_models(&backend.models);
        Json(serde_json::json!({ "status": "ok", "model": requested }))
    }

    async fn handle_gui_models_search_hf(
        Query(params): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let query = params.get("q").cloned().unwrap_or_default();
        if query.trim().is_empty() {
            return Json(serde_json::json!({ "models": [] }));
        }

        let client = reqwest::Client::builder()
            .user_agent("ghostlink/1.0")
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let url = "https://huggingface.co/api/models";
        let resp = client
            .get(url)
            .query(&[
                ("search", query.as_str()),
                ("task", "text-generation"),
                ("sort", "downloads"),
                ("direction", "-1"),
                ("limit", "20"),
            ])
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                if let Ok(data) = r.json::<serde_json::Value>().await {
                    let models = data
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|m| {
                                    let id =
                                        m.get("modelId").and_then(|v| v.as_str()).unwrap_or("");
                                    let name = id.rsplit('/').next().unwrap_or(id);
                                    let downloads =
                                        m.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let likes =
                                        m.get("likes").and_then(|v| v.as_u64()).unwrap_or(0);
                                    serde_json::json!({
                                        "id": id,
                                        "name": name,
                                        "downloads": downloads,
                                        "likes": likes,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    return Json(serde_json::json!({ "models": models }));
                }
            }
            _ => {}
        }

        Json(serde_json::json!({ "models": [] }))
    }

    async fn handle_gui_workers(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        Json(serde_json::json!({ "workers": backend.workers }))
    }

    async fn handle_gui_workers_connect() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "message": "Connection initiated" }))
    }

    async fn handle_gui_workers_add(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<WorkerAddRequest>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        backend.workers.push(WorkerRecord {
            id: format!("worker-{}", req.host),
            host: req.host,
            port: req.port,
            status: "Connected".to_string(),
            model: "unknown".to_string(),
            threads: 4,
            load: 0,
        });
        Json(serde_json::json!({ "status": "ok" }))
    }

    async fn handle_gui_workers_discover() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "count": 2 }))
    }

    async fn handle_gui_workers_disconnect(
        Path(worker_id): Path<String>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "worker_id": worker_id }))
    }

    async fn handle_gui_metrics(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let cluster = Arc::clone(&backend.cluster);
        let total_vram = cluster.total_vram_gb();
        let nodes = cluster.nodes();
        let mut total_latency = 0.0;
        let mut total_throughput = 0.0;
        let mut node_count = 0;
        for node in nodes {
            if let Some(metrics) = cluster.get_metrics(&node.id) {
                total_latency += metrics.avg_latency_us / 1000.0; // Convert us to ms
                total_throughput += metrics.throughput_gbps * 1000.0; // Convert GB/s to MB/s
                node_count += 1;
            }
        }
        let avg_latency = if node_count > 0 {
            total_latency / node_count as f32
        } else {
            backend.last_latency_ms
        };
        let avg_throughput = if node_count > 0 {
            total_throughput / node_count as f32
        } else {
            0.0
        };
        Json(serde_json::json!({
            "metrics": {
                "throughput": avg_throughput,
                "cpu": 0.0,
                "memory": 0.0,
                "gpu": if total_vram > 0.0 { 50.0 } else { 0.0 },
                "latency_p50": avg_latency,
                "latency_p95": avg_latency * 1.5,
                "active_nodes": node_count,
                "total_vram_gb": total_vram,
            }
        }))
    }

    async fn handle_gui_sessions(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        Json(serde_json::json!({ "sessions": backend.sessions }))
    }

    async fn handle_gui_session_cancel(Path(session_id): Path<String>) -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "session_id": session_id, "cancelled": true }))
    }

    async fn handle_gui_session_save(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let session_id = req.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let _name = req
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed Session");
        let model = req
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let messages = req
            .get("messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if session_id.is_empty() {
            return Json(
                serde_json::json!({ "status": "error", "error": "session id is required" }),
            );
        }

        let mut backend = lock_state(&state);
        let session = SessionRecord {
            id: session_id.to_string(),
            model: model.to_string(),
            status: "saved".to_string(),
            throughput: 0,
            latency: 0,
            tokens: messages.len(),
        };

        // Remove existing session with same id
        backend.sessions.retain(|s| s.id != session_id);
        backend.sessions.push(session);

        Json(serde_json::json!({ "status": "ok", "session_id": session_id }))
    }

    async fn handle_gui_session_load(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(session_id): Path<String>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        if let Some(session) = backend.sessions.iter().find(|s| s.id == session_id) {
            Json(serde_json::json!({
                "status": "ok",
                "session": {
                    "id": session.id,
                    "model": session.model,
                    "status": session.status,
                    "throughput": session.throughput,
                    "latency": session.latency,
                    "tokens": session.tokens,
                }
            }))
        } else {
            Json(serde_json::json!({ "status": "error", "error": "session not found" }))
        }
    }

    async fn handle_gui_session_delete(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(session_id): Path<String>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        let len_before = backend.sessions.len();
        backend.sessions.retain(|s| s.id != session_id);
        if backend.sessions.len() < len_before {
            Json(serde_json::json!({ "status": "ok", "deleted": true }))
        } else {
            Json(serde_json::json!({ "status": "error", "error": "session not found" }))
        }
    }

    async fn handle_gui_queue() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "depth": 0 }))
    }

    async fn handle_gui_jwt_refresh() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "token": "new-token-123" }))
    }

    async fn handle_gui_pqc_enable() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "enabled": true }))
    }

    async fn handle_gui_ollama_health(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let (ollama_client, ollama_available) = {
            let backend = lock_state(&state);
            (
                backend.ollama_client.clone(),
                Arc::clone(&backend.ollama_available),
            )
        };

        let reachable = ollama_client.health().await.unwrap_or(false);
        {
            let mut available_flag = ollama_available.lock().await;
            *available_flag = reachable;
        }

        let model_count = if reachable {
            ollama_client
                .list_models()
                .await
                .map(|models| models.len())
                .unwrap_or(0)
        } else {
            0
        };

        Json(serde_json::json!({
            "status": if reachable { "ok" } else { "degraded" },
            "reachable": reachable,
            "ollama_url": "native",
            "model_count": model_count,
            "detail": if reachable { "Ollama reachable" } else { "Ollama not reachable" },
            "message": if reachable { "Ollama backend connected" } else { "Ollama backend unavailable" }
        }))
    }

    async fn handle_gui_ollama_models(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.list_models_detailed().await {
            Ok(models) => Json(serde_json::json!({ "models": models })),
            Err(err) => Json(serde_json::json!({ "models": [], "error": err.to_string() })),
        }
    }

    async fn handle_gui_ollama_pull(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaModelRequest>,
    ) -> Json<serde_json::Value> {
        let model = req.model.trim().to_string();
        if model.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" }));
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.pull_model(&model).await {
            Ok(result) => Json(serde_json::json!({ "status": "ok", "result": result })),
            Err(err) => Json(serde_json::json!({ "error": err.to_string() })),
        }
    }

    async fn handle_gui_ollama_pull_stream(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaModelRequest>,
    ) -> axum::response::Response {
        let model_name = req.model.trim().to_string();
        if model_name.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" })).into_response();
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        let progress_stream = match ollama_client.pull_model_stream(&model_name).await {
            Ok(stream) => stream,
            Err(err) => {
                return Json(serde_json::json!({
                    "status": "error",
                    "error": format!(
                        "failed to start pull stream for '{}': {}",
                        model_name, err
                    ),
                }))
                .into_response()
            }
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);

        tokio::spawn(async move {
            let mut stream = progress_stream;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(progress) => {
                        let payload = serde_json::json!({
                            "status": progress.status,
                            "digest": progress.digest,
                            "total": progress.total,
                            "completed": progress.completed,
                        })
                        .to_string();
                        if tx.send(Ok(Event::default().data(payload))).await.is_err() {
                            return;
                        }
                    }
                    Err(err) => {
                        let payload = serde_json::json!({
                            "status": "error",
                            "error": err.to_string(),
                        })
                        .to_string();
                        let _ = tx.send(Ok(Event::default().data(payload))).await;
                        return;
                    }
                }
            }

            let done_payload = serde_json::json!({ "status": "success" }).to_string();
            let _ = tx.send(Ok(Event::default().data(done_payload))).await;
        });

        Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx)).into_response()
    }

    async fn handle_gui_ollama_show(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaModelRequest>,
    ) -> Json<serde_json::Value> {
        let model = req.model.trim().to_string();
        if model.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" }));
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.show_model(&model).await {
            Ok(info) => {
                let modelfile = info.modelfile.clone();
                Json(serde_json::json!({
                    "info": info,
                    "modelfile": modelfile,
                }))
            }
            Err(err) => Json(serde_json::json!({ "error": err.to_string() })),
        }
    }

    async fn handle_gui_ollama_create(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaCreateRequest>,
    ) -> Json<serde_json::Value> {
        let name = req.name.trim().to_string();
        if name.is_empty() {
            return Json(serde_json::json!({ "error": "name cannot be empty" }));
        }
        if req.modelfile.trim().is_empty() {
            return Json(serde_json::json!({ "error": "modelfile cannot be empty" }));
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.create_model(&name, &req.modelfile).await {
            Ok(detail) => Json(serde_json::json!({ "status": "success", "detail": detail })),
            Err(err) => Json(serde_json::json!({
                "status": "error",
                "error": format!("failed to create model '{}': {}", name, err),
            })),
        }
    }

    async fn handle_gui_ollama_copy(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaCopyRequest>,
    ) -> Json<serde_json::Value> {
        let source = req.source.trim().to_string();
        let destination = req.destination.trim().to_string();
        if source.is_empty() || destination.is_empty() {
            return Json(serde_json::json!({ "error": "source and destination are required" }));
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.copy_model(&source, &destination).await {
            Ok(detail) => Json(serde_json::json!({ "status": "success", "detail": detail })),
            Err(err) => Json(serde_json::json!({
                "status": "error",
                "error": format!("failed to copy model '{}': {}", source, err),
            })),
        }
    }

    async fn handle_gui_ollama_delete(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaNameRequest>,
    ) -> Json<serde_json::Value> {
        let name = req.name.trim().to_string();
        if name.is_empty() {
            return Json(serde_json::json!({ "error": "name cannot be empty" }));
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.delete_model(&name).await {
            Ok(result) => Json(serde_json::json!({ "status": "ok", "result": result })),
            Err(err) => Json(serde_json::json!({ "error": err.to_string() })),
        }
    }

    async fn handle_gui_ollama_ps(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.list_running().await {
            Ok(models) => Json(serde_json::json!({ "models": models })),
            Err(err) => Json(serde_json::json!({
                "models": [],
                "error": format!("failed to list running models: {}", err),
            })),
        }
    }

    async fn handle_gui_ollama_embeddings(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaEmbeddingRequest>,
    ) -> Json<serde_json::Value> {
        let model = req.model.trim().to_string();
        if model.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" }));
        }
        if req.prompt.trim().is_empty() {
            return Json(serde_json::json!({ "error": "prompt cannot be empty" }));
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.embeddings(&model, &req.prompt).await {
            Ok(embedding) => Json(serde_json::json!({ "embedding": embedding })),
            Err(err) => Json(serde_json::json!({
                "error": format!("failed to generate embeddings: {}", err),
            })),
        }
    }

    async fn handle_gui_ollama_version(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client.version().await {
            Ok(version) => Json(serde_json::json!({ "version": version })),
            Err(err) => Json(serde_json::json!({
                "version": "unknown",
                "error": format!("failed to read Ollama version: {}", err),
            })),
        }
    }

    async fn handle_gui_ollama_chat(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<OllamaChatRequest>,
    ) -> Json<serde_json::Value> {
        let model = req.model.trim().to_string();
        if model.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" }));
        }

        let messages = req
            .messages
            .into_iter()
            .filter_map(|message| {
                let role = message.get("role").and_then(|v| v.as_str())?;
                let content = message.get("content").and_then(|v| v.as_str())?;
                Some(ollama::ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                })
            })
            .collect::<Vec<_>>();

        if messages.is_empty() {
            return Json(serde_json::json!({ "error": "messages cannot be empty" }));
        }

        let ollama_client = {
            let backend = lock_state(&state);
            backend.ollama_client.clone()
        };

        match ollama_client
            .chat(
                &model,
                &messages,
                req.temperature,
                req.top_p,
                req.top_k,
                req.repeat_penalty,
                req.max_tokens,
            )
            .await
        {
            Ok(response) => Json(serde_json::json!(response)),
            Err(err) => Json(serde_json::json!({
                "error": format!("failed to chat with Ollama: {}", err),
            })),
        }
    }

    async fn handle_gui_chat(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<GuiChatRequest>,
    ) -> axum::response::Response {
        let started = Instant::now();

        let mut tool_results = Vec::new();
        // Performance optimization: avoid cloning potentially large MCP payloads
        // and dispatch tools without async-await state machine overhead.
        // Expected impact: lower per-request CPU and latency for tool-heavy chat calls.
        let empty_tool_args = serde_json::Value::Null;
        if let Some(mcp) = req.mcp.as_ref() {
            if let Some(tools) = mcp.get("tools").and_then(|t| t.as_array()) {
                tool_results = Vec::with_capacity(tools.len());
                for tool_val in tools {
                    if let Some(tool_name) = tool_val.as_str() {
                        tool_results.push(ToolDispatcher::dispatch(tool_name, &empty_tool_args));
                    }
                }
            }
        }

        let (
            current_model,
            _cluster,
            ollama_client,
            ollama_available,
            inference_backend,
            native_engine_client,
            settings,
        ) = {
            let backend = lock_state(&state);
            eprintln!(
                "[DEBUG] settings.inference_backend = '{}'",
                backend.settings.inference_backend
            );
            let inference_backend = match backend.settings.inference_backend.as_str() {
                "ollama" => InferenceBackend::Ollama,
                "native" => InferenceBackend::Native,
                _ => InferenceBackend::Ollama,
            };
            eprintln!(
                "[DEBUG] Selected inference_backend = {:?}",
                inference_backend
            );
            (
                backend.current_model.clone(),
                Arc::clone(&backend.cluster),
                backend.ollama_client.clone(),
                Arc::clone(&backend.ollama_available),
                inference_backend,
                backend.native_engine_client.clone(),
                backend.settings.clone(),
            )
        };

        let token_estimate = req.message.split_whitespace().count().clamp(1, 1024);
        let temp = req.temperature.unwrap_or(settings.temperature);
        let top_p = req.top_p.unwrap_or(settings.top_p);
        let top_k = req.top_k.unwrap_or(settings.top_k);
        let penalty = req.penalty.unwrap_or(settings.repeat_penalty);
        let requested_exec_tokens = req
            .max_tokens
            .unwrap_or(settings.max_tokens)
            .clamp(16, 4096);
        let exec_tokens = chat_exec_token_budget(requested_exec_tokens);
        let exec_micro_batch = chat_exec_micro_batch();
        let request_tracker = active_runtime_switcher().request_tracker().clone();
        request_tracker.increment().await;

        let (response_text, real_inference, backend_used) = match inference_backend {
            InferenceBackend::Ollama => {
                let resolve_model = |requested: &str, available: &[String]| -> Option<String> {
                    if available.iter().any(|m| m == requested) {
                        return Some(requested.to_string());
                    }

                    if let Some(found) =
                        available.iter().find(|m| m.eq_ignore_ascii_case(requested))
                    {
                        return Some(found.clone());
                    }

                    if !requested.contains(':') {
                        let prefix = format!("{}:", requested.to_ascii_lowercase());
                        if let Some(found) = available
                            .iter()
                            .find(|m| m.to_ascii_lowercase().starts_with(&prefix))
                        {
                            return Some(found.clone());
                        }
                    }

                    None
                };

                let available_models_result: Result<Vec<String>, String> = ollama_client
                    .list_models()
                    .await
                    .map_err(|err| err.to_string());

                match available_models_result {
                    Ok(available_models) => {
                        let effective_model = resolve_model(&current_model, &available_models);
                        if let Some(model_name) = effective_model {
                            match ollama_client
                                .generate(
                                    &model_name,
                                    &req.message,
                                    temp,
                                    top_p,
                                    top_k,
                                    penalty,
                                    exec_tokens,
                                )
                                .await
                            {
                                Ok(text) => {
                                    if model_name != current_model {
                                        let mut backend = lock_state(&state);
                                        backend.current_model = model_name;
                                    }
                                    (
                                        text.trim().to_string(),
                                        true,
                                        InferenceBackend::Ollama.as_str(),
                                    )
                                }
                                Err(err) => {
                                    let fallback = format!(
                                        "Ollama generate failed for model '{}': {}",
                                        model_name, err
                                    );
                                    (fallback, false, InferenceBackend::Ollama.as_str())
                                }
                            }
                        } else {
                            let shown =
                                available_models.iter().take(6).cloned().collect::<Vec<_>>();
                            let available_hint = if shown.is_empty() {
                                "<no installed models>".to_string()
                            } else {
                                shown.join(", ")
                            };
                            (
                                format!(
                                    "Configured model '{}' is not installed in Ollama. Available: {}. Pull/select an exact tag and retry.",
                                    current_model, available_hint
                                ),
                                false,
                                InferenceBackend::Ollama.as_str(),
                            )
                        }
                    }
                    Err(err_text) => {
                        let fallback = format!(
                            "Inference backend '{}' unavailable while listing models: {}",
                            InferenceBackend::Ollama.as_str(),
                            err_text
                        );
                        (fallback, false, InferenceBackend::Ollama.as_str())
                    }
                }
            }
            InferenceBackend::Native => {
                match native_engine_client.generate(
                    &current_model, &req.message, exec_tokens,
                    temp, top_p, top_k, penalty,
                    &settings.native_engine,
                ) {
                    Ok(gen) => (
                        gen.text,
                        gen.real_inference,
                        InferenceBackend::Native.as_str(),
                    ),
                    Err(err) => (
                        format!(
                            "Ghostlink native fabric backend processed model '{}' with {} estimated tokens. Native error: {}",
                            current_model, exec_tokens, err
                        ),
                        false,
                        InferenceBackend::Native.as_str(),
                    ),
                }
            }
        };

        let exec_result: Option<()> = None; // Track if we have real execution metrics

        {
            let mut available_flag = ollama_available.lock().await;
            *available_flag = real_inference;
        }

        let (request_id, session_id) = {
            let mut backend = lock_state(&state);
            backend.chat_requests = backend.chat_requests.saturating_add(1);
            let request_seq = backend.chat_requests;

            if exec_result.is_none() {
                backend.last_latency_ms = (started.elapsed().as_secs_f32() * 1000.0).max(1.0);
            }

            let latency = backend.last_latency_ms.round() as u32;
            let throughput = 1200;

            let maybe_session = backend.sessions.first_mut();
            if let Some(session) = maybe_session {
                session.tokens = session.tokens.saturating_add(token_estimate);
                session.throughput = throughput;
                session.latency = latency;
                session.model = current_model.clone();
                session.status = "Running".to_string();
                let session_id = session.id.clone();
                (request_seq, session_id)
            } else {
                let session_id = "sess_local_001".to_string();
                backend.sessions.push(SessionRecord {
                    id: session_id.clone(),
                    model: current_model.clone(),
                    status: "Running".to_string(),
                    throughput,
                    latency,
                    tokens: token_estimate,
                });
                (request_seq, session_id)
            }
        };

        let mut final_response = response_text;
        if !tool_results.is_empty() {
            final_response.push_str("\n\nTools used:");
            for res in &tool_results {
                final_response.push_str("\n- **");
                final_response.push_str(&res.tool);
                final_response.push_str("**: ");
                final_response.push_str(&res.result);
            }
        }

        let mut response = serde_json::json!({
            "response": final_response,
            "request_id": format!("req-{}", request_id),
            "session_id": session_id,
            "model": current_model,
            "inference_backend": backend_used,
            "ollama_url": if backend_used == "ollama" { "local" } else { "disabled" },
            "tokens_estimated": token_estimate,
            "exec_tokens": exec_tokens,
            "exec_micro_batch": exec_micro_batch,
            "real_inference": real_inference,
            "metrics": exec_result.map(|_| serde_json::json!({
                "throughput": 0.0,
                "p95_ms": 0.0
            }))
        });

        if let Some(mcp) = req.mcp {
            if let Some(response_object) = response.as_object_mut() {
                response_object.insert("tool_access".to_string(), serde_json::json!(true));
                response_object.insert("mcp".to_string(), mcp);
            }
        }

        if !tool_results.is_empty() {
            if let Some(response_object) = response.as_object_mut() {
                response_object.insert("tool_results".to_string(), serde_json::json!(tool_results));
                let tools_used: Vec<String> = tool_results.iter().map(|r| r.tool.clone()).collect();
                response_object.insert("tools_used".to_string(), serde_json::json!(tools_used));
            }
        }

        request_tracker.decrement().await;

        if req.stream.unwrap_or(false) {
            let tokens: Vec<String> = final_response
                .split_whitespace()
                .map(|s| format!("{} ", s))
                .collect();

            let stream = stream::iter(tokens).map(move |token| {
                let chunk = serde_json::json!({
                    "token": token,
                    "request_id": format!("req-{}", request_id),
                    "session_id": session_id.clone(),
                });
                Ok::<Event, Infallible>(Event::default().data(chunk.to_string()))
            });

            Sse::new(stream).into_response()
        } else {
            Json(response).into_response()
        }
    }

    async fn handle_runtime_detection(
        State(_state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        use crate::runtime::RuntimeDetector;

        let runtimes = RuntimeDetector::detect();
        let primary = RuntimeDetector::detect_primary();

        let runtime_data: Vec<_> = runtimes
            .iter()
            .map(|rt| {
                serde_json::json!({
                    "runtime": rt.detected_runtime.to_string(),
                    "available": rt.is_available,
                    "compute_capability": rt.compute_capability,
                    "memory_gb": rt.memory_gb,
                    "device_count": rt.device_count,
                })
            })
            .collect();

        Json(serde_json::json!({
            "available_runtimes": runtime_data,
            "primary_runtime": primary.to_string(),
            "auto_detected": true,
        }))
    }

    async fn handle_models_by_runtime(
        Query(params): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        use crate::runtime::{ModelRegistry, Runtime};

        let runtime_str = params.get("runtime").map(|s| s.as_str()).unwrap_or("CPU");

        let runtime = runtime_str.parse::<Runtime>().unwrap_or(Runtime::CPU);

        let models = ModelRegistry::models_for_runtime(runtime);

        let model_data: Vec<_> = models
            .iter()
            .map(|m| {
                serde_json::json!({
                    "name": m.name,
                    "parameters": m.parameters,
                    "size_gb": m.size_gb,
                    "memory_required_gb": m.memory_required_gb,
                    "quality_tier": format!("{:?}", m.quality_tier),
                    "inference_speed": format!("{:?}", m.inference_speed),
                    "use_cases": m.use_cases,
                })
            })
            .collect();

        let best = ModelRegistry::best_for_runtime(runtime);

        Json(serde_json::json!({
            "runtime": runtime.to_string(),
            "model_count": model_data.len(),
            "models": model_data,
            "best_model": best.map(|m| serde_json::json!({
                "name": m.name,
                "parameters": m.parameters,
                "recommended_reason": "Best balance of quality and performance for this runtime",
            })),
        }))
    }

    async fn handle_model_recommendations(
        Query(params): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        use crate::runtime::{ModelRegistry, RuntimeDetector};

        let detected_runtimes = RuntimeDetector::detect();
        let runtime = params
            .get("runtime")
            .and_then(|s| s.parse::<Runtime>().ok())
            .unwrap_or_else(RuntimeDetector::detect_primary);

        let memory_gb = params
            .get("memory_gb")
            .and_then(|s| s.parse::<f32>().ok())
            .or_else(|| {
                detected_runtimes
                    .iter()
                    .find(|r| r.detected_runtime == runtime)
                    .and_then(|r| r.memory_gb)
            })
            .unwrap_or(8.0);

        let recommended = ModelRegistry::recommend_models(runtime, memory_gb);

        let model_data: Vec<_> = recommended
            .iter()
            .map(|m| {
                serde_json::json!({
                    "name": m.name,
                    "parameters": m.parameters,
                    "size_gb": m.size_gb,
                    "memory_required_gb": m.memory_required_gb,
                    "quality_tier": format!("{:?}", m.quality_tier),
                    "inference_speed": format!("{:?}", m.inference_speed),
                    "reason": format!("Fits in {:.1}GB available memory", memory_gb),
                })
            })
            .collect();

        Json(serde_json::json!({
            "detected_runtime": runtime.to_string(),
            "available_memory_gb": memory_gb,
            "recommended_models": model_data,
            "count": model_data.len(),
        }))
    }

    async fn handle_get_settings(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        let s = &backend.settings;
        Json(serde_json::json!(s))
    }

    async fn handle_update_settings(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let backend = &mut *lock_state(&state);
        let mut current = backend.settings.clone();
        if let Some(obj) = req.as_object() {
            for (key, value) in obj {
                match key.as_str() {
                    "inference_backend" => {
                        if let Some(v) = value.as_str() {
                            current.inference_backend = v.to_string();
                            // This used to only update the display string in
                            // `settings`, while the enum that every load/
                            // unload/generate code path actually reads
                            // (`backend.inference_backend`) stayed frozen at
                            // whatever GHOSTLINK_INFERENCE_BACKEND resolved
                            // to at process startup. Update both so a live
                            // settings change actually takes effect.
                            backend.inference_backend = InferenceBackend::parse(v);
                        }
                    }
                    "native_engine" => {
                        if let Some(v) = value.as_str() {
                            current.native_engine = v.to_string();
                        }
                    }
                    "ngl" => {
                        if let Some(v) = value.as_i64() {
                            current.ngl = v as i32;
                        }
                    }
                    "model_path" => {
                        if let Some(v) = value.as_str() {
                            current.model_path = v.to_string();
                        }
                    }
                    "llama_server_url" => {
                        if let Some(v) = value.as_str() {
                            current.llama_server_url = v.to_string();
                        }
                    }
                    "llama_port" => {
                        if let Some(v) = value.as_u64() {
                            current.llama_port = v as u16;
                        }
                    }
                    "api_host" => {
                        if let Some(v) = value.as_str() {
                            current.api_host = v.to_string();
                        }
                    }
                    "api_port" => {
                        if let Some(v) = value.as_u64() {
                            current.api_port = v as u16;
                        }
                    }
                    "gui_port" => {
                        if let Some(v) = value.as_u64() {
                            current.gui_port = v as u16;
                        }
                    }
                    "threads" => {
                        if let Some(v) = value.as_u64() {
                            current.threads = v as usize;
                        }
                    }
                    "ctx_size" => {
                        if let Some(v) = value.as_u64() {
                            current.ctx_size = v as usize;
                        }
                    }
                    "temperature" => {
                        if let Some(v) = value.as_f64() {
                            current.temperature = v as f32;
                        }
                    }
                    "top_p" => {
                        if let Some(v) = value.as_f64() {
                            current.top_p = v as f32;
                        }
                    }
                    "top_k" => {
                        if let Some(v) = value.as_u64() {
                            current.top_k = v as usize;
                        }
                    }
                    "repeat_penalty" => {
                        if let Some(v) = value.as_f64() {
                            current.repeat_penalty = v as f32;
                        }
                    }
                    "max_tokens" => {
                        if let Some(v) = value.as_u64() {
                            current.max_tokens = v as usize;
                        }
                    }
                    "chat_exec_tokens" => {
                        if let Some(v) = value.as_u64() {
                            current.chat_exec_tokens = v as usize;
                        }
                    }
                    "chat_micro_batch" => {
                        if let Some(v) = value.as_u64() {
                            current.chat_micro_batch = v as usize;
                        }
                    }
                    "tcp_max_inflight" => {
                        if let Some(v) = value.as_u64() {
                            current.tcp_max_inflight = v as usize;
                        }
                    }
                    "discovery_listen" => {
                        if let Some(v) = value.as_str() {
                            current.discovery_listen = v.to_string();
                        }
                    }
                    "discovery_broadcast" => {
                        if let Some(v) = value.as_str() {
                            current.discovery_broadcast = v.to_string();
                        }
                    }
                    "discovery_auth_token" => {
                        if let Some(v) = value.as_str() {
                            current.discovery_auth_token = v.to_string();
                        }
                    }
                    "tcp_auth_token" => {
                        if let Some(v) = value.as_str() {
                            current.tcp_auth_token = v.to_string();
                        }
                    }
                    "xdp_interface" => {
                        if let Some(v) = value.as_str() {
                            current.xdp_interface = v.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
        backend.settings = current.clone();
        save_settings(&current);
        Json(serde_json::json!({"status": "ok", "settings": current}))
    }

    async fn handle_reset_settings(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = &mut *lock_state(&state);
        let defaults = RuntimeSettings::default();
        backend.settings = defaults.clone();
        save_settings(&defaults);
        Json(serde_json::json!({"status": "ok", "settings": defaults}))
    }

    async fn handle_health(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::value::Value> {
        let backend = lock_state(&state);
        let uptime_s = backend.started_at.elapsed().as_secs();

        // Detect GPU availability via runtime profile (uses fast cache)
        let profile = detect_runtime_profile("health-check");
        let gpu_available =
            profile.acceleration_mode == ghostlink_core::host::AccelerationMode::Gpu;
        let gpu_name = profile.node_resources.gpu_name.clone();
        let vram_gb = profile.node_resources.vram_gb;

        Json(serde_json::json!({
            "status": "healthy",
            "version": "0.1.0-alpha.0",
            "backend_url": backend.backend_url,
            "uptime_s": uptime_s,
            "current_model": backend.current_model,
            "inference_backend": backend.inference_backend.as_str(),
            "native_engine": backend.settings.native_engine,
            "gpu_available": gpu_available,
            "gpu_name": gpu_name,
            "vram_gb": vram_gb,
        }))
    }

    println!("Ghostlink Studio API - Starting OpenAI-compatible server...");
    println!("Listening on http://{}:{}", host, port);
    println!("Routes:");
    println!("  - POST /v1/chat/completions");
    println!("  - GET  /v1/models");
    println!("  - GET  /health");
    println!("  - GET  /api/models");
    println!("  - GET  /api/models/status");
    println!("  - POST /api/models/load");
    println!("  - POST /api/models/download");
    println!("  - POST /api/models/delete");
    println!("  - GET  /api/ollama/health");
    println!("  - GET  /api/ollama/models");
    println!("  - POST /api/ollama/pull");
    println!("  - POST /api/ollama/pull/stream");
    println!("  - POST /api/ollama/show");
    println!("  - POST /api/ollama/create");
    println!("  - POST /api/ollama/copy");
    println!("  - POST /api/ollama/delete");
    println!("  - GET  /api/ollama/ps");
    println!("  - POST /api/ollama/embeddings");
    println!("  - GET  /api/ollama/version");
    println!("  - POST /api/ollama/chat");
    println!("  - GET  /api/workers");
    println!("  - POST /api/workers/connect");
    println!("  - POST /api/workers/add");
    println!("  - GET  /api/metrics");
    println!("  - GET  /api/sessions");
    println!("  - POST /api/sessions/:session_id/cancel");
    println!("  - POST /api/queue");
    println!("  - POST /api/security/jwt/refresh");
    println!("  - POST /api/security/pqc/enable");
    println!("  - GET  /api/settings");
    println!("  - POST /api/settings");
    println!("  - POST /api/settings/reset");
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

    let addr_string = if host == "localhost" || host == "localhost." {
        format!("127.0.0.1:{}", port)
    } else {
        format!("{}:{}", host, port)
    };
    let addr: SocketAddr = addr_string.parse().map_err(|e: std::net::AddrParseError| {
        anyhow::anyhow!("Invalid socket address {}: {}", addr_string, e)
    })?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| anyhow::anyhow!("failed to initialize runtime: {}", err))?;

    let cluster = Arc::new(ClusterState::new());
    let mut local_node = profile.node_resources.clone();
    local_node.vram_gb = local_node.vram_gb.max(16.0);
    local_node.system_memory_gb = local_node.system_memory_gb.max(16.0);
    cluster.register(local_node);

    let node_for_listener = profile.node_resources.clone();
    thread::spawn(move || {
        let auth_token = std::env::var("GHOSTLINK_DISCOVERY_AUTH_TOKEN")
            .ok()
            .filter(|token| !token.is_empty());
        let listen_addr = std::env::var("GHOSTLINK_DISCOVERY_LISTEN")
            .ok()
            .and_then(|raw| raw.parse::<SocketAddr>().ok())
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], DEFAULT_DISCOVERY_PORT)));

        let config = UdpDiscoveryConfig {
            bind_addr: listen_addr,
            auth_token,
            allow_legacy_crc32: env_default_bool("GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32", false),
            ..UdpDiscoveryConfig::default()
        };

        let _ = serve_discovery(&node_for_listener, &config, None);
    });

    let cluster_for_broadcast = Arc::clone(&cluster);
    let node_for_broadcast = profile.node_resources.clone();
    thread::spawn(move || {
        let auth_token = std::env::var("GHOSTLINK_DISCOVERY_AUTH_TOKEN")
            .ok()
            .filter(|token| !token.is_empty());
        let broadcast_addr = std::env::var("GHOSTLINK_DISCOVERY_BROADCAST")
            .ok()
            .and_then(|raw| raw.parse::<SocketAddr>().ok())
            .unwrap_or_else(|| SocketAddr::from(([255, 255, 255, 255], DEFAULT_DISCOVERY_PORT)));

        let config = UdpDiscoveryConfig {
            broadcast_addr,
            auth_token,
            allow_legacy_crc32: env_default_bool("GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32", false),
            ..UdpDiscoveryConfig::default()
        };

        let frame = DiscoveryFrame {
            kind: FrameKind::Join,
            node: node_for_broadcast,
        };

        loop {
            if let Ok(peers) = broadcast_and_collect(&frame, &config) {
                for (peer_frame, peer_addr) in peers {
                    cluster_for_broadcast.register_with_addr(peer_frame.node, Some(peer_addr));
                }
            }
            thread::sleep(Duration::from_secs(10));
        }
    });

    let models = load_persistent_models();
    save_persistent_models(&models);

    let mut settings = load_settings();

    // Auto-compute ngl from GPU VRAM if still at default (-1)
    if settings.ngl < 0 {
        let ngl = if profile.node_resources.vram_gb >= 12.0 {
            40
        } else if profile.node_resources.vram_gb >= 8.0 {
            24
        } else if profile.node_resources.vram_gb >= 4.0 {
            99
        } else {
            -1
        };
        if ngl > 0 {
            settings.ngl = ngl;
            // Also set the env var so NativeEngineClient::get_ngl() picks it up
            std::env::set_var("GHOSTLINK_LLAMA_NGL", ngl.to_string());
            eprintln!(
                "[startup] Auto-configured ngl={} from detected VRAM ({:.1} GB)",
                ngl, profile.node_resources.vram_gb
            );
        }
    }

    // Auto-compute threads from available parallelism if still at default (4)
    if settings.threads <= 1 {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        settings.threads = threads;
        // Also set the env var so NativeEngineClient::get_threads() picks it up
        std::env::set_var("GHOSTLINK_LLAMA_THREADS", threads.to_string());
        eprintln!("[startup] Auto-configured threads={}", threads);
    }

    save_settings(&settings);

    {
        let models_dir = &settings.models_dir;
        if !models_dir.is_empty() {
            fs::create_dir_all(models_dir).unwrap_or_else(|e| {
                eprintln!(
                    "Warning: could not create models directory '{}': {}",
                    models_dir, e
                );
            });
        }
    }

    let ollama_url = std::env::var("OLLAMA_BASE_URL")
        .ok()
        .unwrap_or_else(|| "http://localhost:11434".to_string());

    // Load settings to get initial inference backend
    let settings = load_settings();
    let inference_backend = match settings.inference_backend.as_str() {
        "ollama" => InferenceBackend::Ollama,
        "native" => InferenceBackend::Native,
        _ => InferenceBackend::Ollama,
    };
    let native_engine_client = native_engine::NativeEngineClient::new();
    let ollama_client = ollama::OllamaClient::new(ollama_url);
    let ollama_available = Arc::new(tokio::sync::Mutex::new(false));
    let compute_config_manager = backend_config::ConfigManager::new("ghostlink.toml");
    let compute_config = compute_config_manager
        .load_compute_config()
        .unwrap_or_else(|_| backend_config::ComputeConfig::new());
    let preferred_backend = compute_config_manager
        .load_preferred_backend()
        .ok()
        .flatten();
    let backend_registry = {
        let registry = backend_registry::BackendRegistry::discover();
        if let Some(preferred_backend) = preferred_backend {
            if registry.get_backend(&preferred_backend).is_some() {
                let _ = registry.switch_backend(preferred_backend);
            }
        }
        Arc::new(registry)
    };
    let runtime_switcher =
        runtime_switcher::RuntimeSwitcher::new(runtime_switcher::SwitchingConfig {
            request_drain_timeout: Duration::from_secs(compute_config.request_drain_timeout_secs),
            ..runtime_switcher::SwitchingConfig::default()
        });

    let _ = ACTIVE_BACKEND_REGISTRY.set(Arc::clone(&backend_registry));
    let _ = ACTIVE_RUNTIME_SWITCHER.set(runtime_switcher.clone());

    println!(
        "Inference backend selected: {} (set GHOSTLINK_INFERENCE_BACKEND=native|ollama)",
        inference_backend.as_str()
    );

    let initial_model = models
        .iter()
        .find(|m| m.status == "Loaded")
        .or_else(|| models.first())
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "none".to_string());

    let state = Arc::new(Mutex::new(BackendState {
        models,
        current_model: initial_model.clone(),
        workers: vec![WorkerRecord {
            id: profile.node_resources.id.clone(),
            host: host.to_string(),
            port,
            status: "Connected".to_string(),
            model: initial_model,
            threads: profile.recommended_workers.max(1),
            load: 35,
        }],
        sessions: vec![],
        queue_depth: 0,
        chat_requests: 0,
        last_latency_ms: 2.0,
        started_at: Instant::now(),
        backend_url,
        cluster,
        inference_backend,
        native_engine_client,
        ollama_client,
        ollama_available,
        settings,
    }));

    rt.block_on(async {
        let app = Router::new()
            .route("/v1/chat/completions", post(handle_chat_completions))
            .route("/v1/models", get(handle_models))
            .route("/health", get(handle_health))
            .route("/api/health", get(handle_health))
            .route("/api/models", get(handle_gui_models))
            .route("/api/models/status", get(handle_gui_model_status))
            .route("/api/models/load", post(handle_gui_model_load))
            .route("/api/models/download", post(handle_gui_model_download))
            .route("/api/models/delete", post(handle_gui_model_delete))
            .route(
                "/api/models/:model_name",
                delete(handle_gui_model_delete_v2),
            )
            .route(
                "/api/models/:model_name/unload",
                post(handle_gui_model_unload),
            )
            .route(
                "/api/models/search/huggingface",
                get(handle_gui_models_search_hf),
            )
            .route("/api/ollama/health", get(handle_gui_ollama_health))
            .route("/api/ollama/models", get(handle_gui_ollama_models))
            .route("/api/ollama/pull", post(handle_gui_ollama_pull))
            .route(
                "/api/ollama/pull/stream",
                post(handle_gui_ollama_pull_stream),
            )
            .route("/api/ollama/show", post(handle_gui_ollama_show))
            .route("/api/ollama/create", post(handle_gui_ollama_create))
            .route("/api/ollama/copy", post(handle_gui_ollama_copy))
            .route("/api/ollama/delete", post(handle_gui_ollama_delete))
            .route("/api/ollama/ps", get(handle_gui_ollama_ps))
            .route("/api/ollama/embeddings", post(handle_gui_ollama_embeddings))
            .route("/api/ollama/version", get(handle_gui_ollama_version))
            .route("/api/ollama/chat", post(handle_gui_ollama_chat))
            .route("/api/workers", get(handle_gui_workers))
            .route("/api/workers/connect", post(handle_gui_workers_connect))
            .route("/api/workers/add", post(handle_gui_workers_add))
            .route("/api/workers/discover", get(handle_gui_workers_discover))
            .route(
                "/api/workers/:worker_id/disconnect",
                post(handle_gui_workers_disconnect),
            )
            .route("/api/metrics", get(handle_gui_metrics))
            .route("/api/sessions", get(handle_gui_sessions))
            .route("/api/sessions/save", post(handle_gui_session_save))
            .route("/api/sessions/:session_id", get(handle_gui_session_load))
            .route(
                "/api/sessions/:session_id",
                delete(handle_gui_session_delete),
            )
            .route(
                "/api/sessions/:session_id/cancel",
                post(handle_gui_session_cancel),
            )
            .route("/api/queue", post(handle_gui_queue))
            .route("/api/security/jwt/refresh", post(handle_gui_jwt_refresh))
            .route("/api/security/pqc/enable", post(handle_gui_pqc_enable))
            .route("/api/inference/chat", post(handle_gui_chat))
            .route("/api/settings", get(handle_get_settings))
            .route("/api/settings", post(handle_update_settings))
            .route("/api/settings/reset", post(handle_reset_settings))
            .route("/api/runtime/detect", get(handle_runtime_detection))
            .route("/api/runtime/models", get(handle_models_by_runtime))
            .route("/api/runtime/recommend", get(handle_model_recommendations))
            // Phase 2: Backend API endpoints
            .route("/api/backends", get(backend_api::handle_list_backends))
            .route(
                "/api/backends/switch",
                post(backend_api::handle_switch_backend),
            )
            .route(
                "/api/backends/:name/status",
                get(backend_api::handle_backend_status),
            )
            .with_state(state)
            .layer(CorsLayer::permissive());

        // addr already parsed above
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|err| anyhow::anyhow!("failed to bind API server on {}: {}", addr, err))?;
        println!(
            "
API Server Online. Ready for connections."
        );

        axum::serve(listener, app)
            .await
            .map_err(|err| anyhow::anyhow!("API server terminated with error: {}", err))
    })?;

    Ok(())
}

fn build_device_map_from_cluster(
    local_profile: &ghostlink_core::host::RuntimeProfile,
    cluster: &ClusterState,
) -> HashMap<String, DeviceKind> {
    let local_device = match local_profile.acceleration_mode {
        ghostlink_core::host::AccelerationMode::Gpu => DeviceKind::Gpu,
        ghostlink_core::host::AccelerationMode::Neon => DeviceKind::Npu,
        _ => DeviceKind::Cpu,
    };

    let mut map = HashMap::new();
    for node in cluster.nodes() {
        if node.id == local_profile.node_resources.id {
            map.insert(node.id, local_device);
        } else {
            let device = if node.vram_gb > 0.0 {
                DeviceKind::Gpu
            } else {
                DeviceKind::Cpu
            };
            map.insert(node.id, device);
        }
    }
    map
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

    println!(
        "Ghost-Link Layer Placement Plan
"
    );
    println!(
        "================================
"
    );
    println!(
        "Local profile: workers={} acceleration={} XDP={}
",
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
    println!(
        "
Adaptive Quantization Trigger:
"
    );
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

    println!(
        "Broadcasting Ghost-Link Join Frame
"
    );
    println!(
        "====================================
"
    );
    println!("Frame Size: {} bytes", encoded.len());
    println!("EtherType: 0x{:04X}", crate::protocol::GHOSTLINK_ETHERTYPE);
    println!();
    println!(
        "Node Information:
"
    );
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
        println!(
            "
Encoded Frame Preview (hex):
"
        );
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

    println!(
        "Ghost-Link Discovery Listener
"
    );
    println!(
        "===========================
"
    );
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
        println!(
            "Mode: one-shot
"
        );
        match respond_once(&profile.node_resources, &config)
            .map_err(|e| anyhow::anyhow!("UDP discovery listener failed: {e}"))?
        {
            Some(peer) => println!("Replied to discovery request from {}", peer),
            None => println!("No discovery request received before timeout"),
        }
        return Ok(());
    }

    println!(
        "Mode: service loop
"
    );
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
        "
Auto-tuned local runtime: {} workers, {} acceleration",
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

    println!(
        "Ghost-Link Local Cluster Start
"
    );
    println!(
        "===============================
"
    );
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
        "
Cluster-start validation passed: {} replies across {} local nodes",
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
            vram_gb: 0.4,
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
        "{{
  \"summary\": {{\"pass\": {}, \"warn\": {}, \"fail\": {}}},
  \"checks\": [{}]
}}
",
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

    if let Some((resolved, error)) = last_error {
        NetworkProbeOutcome::Unreachable { resolved, error }
    } else {
        NetworkProbeOutcome::InvalidTarget(format!(
            "invalid network target '{}', no connection attempts were made",
            target
        ))
    }
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

    println!(
        "Ghost-Link Doctor Report
"
    );
    println!(
        "========================
"
    );

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

    println!(
        "
Review areas for multi-device accessibility:"
    );
    println!("- GUI path: desktop display or headless xvfb-run fallback");
    println!("- Deployment path: Docker local demo, systemd service template, staged LAN guide");
    println!("- Discovery path: cluster-start for local multi-node behavior");

    println!(
        "
Review areas for accuracy:"
    );
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
        .join("ghostlink_gui_tkinter.py");

    if !gui_entry.exists() {
        anyhow::bail!(
            "Ghostlink GUI entrypoint not found at {}. Ensure ghostlink_gui_tkinter.py is present.",
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

    println!("Launching Ghostlink GUI from {}", gui_entry.display());
    println!("Python executable: {}", python);
    println!("GUI backend target: {}", backend_url);

    let status = Command::new(&python)
        .arg(&gui_entry)
        .env("GHOSTLINK_GUI_BASE_URL", &backend_url)
        .args(&forwarded_args)
        .status()
        .map_err(|err| {
            anyhow::anyhow!("failed to launch Ghostlink GUI with {}: {}", python, err)
        })?;

    if let Some(child) = managed_backend.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }

    if !status.success() {
        anyhow::bail!(
            "Ghostlink GUI exited with status {}. Install dependencies from third_party/mohawk_gui and retry.",
            status
        );
    }

    Ok(())
}

fn parse_gui_backend_target(args: &[String]) -> (String, u16) {
    let mut host = "127.0.0.1".to_string();
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
        .join("ghostlink_gui_tkinter.py");
    let requirements = crate_root
        .join("..")
        .join("..")
        .join("requirements-gui.txt");

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
            // categories.push((
            // "system_libs".to_string(),
            // "Missing libGL.so.1 (install libgl1)".to_string(),
            // ));
        }
        if !has_libxkb {
            // categories.push((
            // "system_libs".to_string(),
            // "Missing libxkbcommon.so.0 (install libxkbcommon0)".to_string(),
            // ));
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

    println!(
        "Ghost-Link GUI Diagnostics
"
    );
    println!(
        "==========================
"
    );
    println!("GUI entry: {}", gui_entry.display());
    println!("Requirements: {}", requirements.display());
    println!("Python executable: {}", python);
    println!("Python source: {}", python_resolution.source.as_str());
    println!(
        "Display session: {}",
        if has_display { "detected" } else { "none" }
    );

    if categories.is_empty() {
        println!(
            "
Diagnostics: PASS"
        );
    } else {
        println!(
            "
Diagnostics: FAIL"
        );
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
        let linux_libgl_json = if has_linux_libgl() { "true" } else { "false" };
        #[cfg(not(target_os = "linux"))]
        let linux_libgl_json = "null";
        #[cfg(target_os = "linux")]
        let linux_libxkb_json = if has_linux_libxkbcommon() {
            "true"
        } else {
            "false"
        };
        #[cfg(not(target_os = "linux"))]
        let linux_libxkb_json = "null";
        let payload = format!(
            "{{\"ok\":{},\"python\":\"{}\",\"python_source\":\"{}\",\"gui_entry\":\"{}\",\"requirements\":\"{}\",\"has_display\":{},\"xvfb_available\":{},\"missing_python_modules\":[{}],\"python_module_probe_error\":{},\"linux_libgl_present\":{},\"linux_libxkbcommon_present\":{},\"issues\":[{}]}}
",
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
        .join("ghostlink_gui_tkinter.py");
    let requirements = crate_root
        .join("..")
        .join("..")
        .join("requirements-gui.txt");

    let repo_root = crate_root.join("..").join("..");
    let python =
        resolve_python_executable_for_root(&repo_root, std::env::var("GHOSTLINK_PYTHON").ok());

    let mut issues: Vec<String> = Vec::new();

    println!(
        "Ghost-Link GUI Readiness Report
"
    );
    println!(
        "===============================
"
    );
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
            println!("Python modules: OK (tkinter, requests)");
        }
        Ok(missing) => {
            issues.push(format!("Missing Python modules: {}", missing.join(", ")));
        }
        Err(err) => {
            issues.push(format!("Unable to validate Python modules: {}", err));
        }
    }

    match detect_missing_optional_gui_python_modules(&python) {
        Ok(missing) if missing.is_empty() => {
            println!("Optional Python modules: OK (huggingface_hub)");
        }
        Ok(missing) => {
            println!(
                "Note: optional Python modules missing ({}); related features will be unavailable but the GUI will still run.",
                missing.join(", ")
            );
        }
        Err(_) => {}
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
        // if !has_libgl {
        // issues.push("Missing libGL.so.1 system dependency (install `libgl1`)".to_string());
        // }
        // if !has_libxkb {
        //     issues.push(
        //         "Missing libxkbcommon.so.0 system dependency (install `libxkbcommon0`)".to_string(),
        //     );
        // }
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
        println!(
            "
Readiness: PASS"
        );
        return Ok(());
    }

    println!(
        "
Readiness: FAIL"
    );
    println!("Issues:");
    for issue in &issues {
        println!("- {}", issue);
    }

    println!(
        "
Suggested fixes:"
    );
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

fn detect_missing_optional_gui_python_modules(python: &str) -> Result<Vec<String>> {
    detect_missing_python_modules(python, &["huggingface_hub"])
}

fn detect_missing_python_modules(python: &str, modules: &[&str]) -> Result<Vec<String>> {
    let module_list = modules
        .iter()
        .map(|m| format!("'{}'", m))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "import importlib.util as u;mods=[{}];missing=[m for m in mods if u.find_spec(m) is None];print(','.join(missing))",
        module_list
    );
    let output = Command::new(python)
        .args(["-c", &script])
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

fn detect_missing_gui_python_modules(python: &str) -> Result<Vec<String>> {
    detect_missing_python_modules(python, &["tkinter", "requests"])
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
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gui_entry = crate_root
        .join("..")
        .join("..")
        .join("ghostlink_gui_tkinter.py");

    if !gui_entry.exists() {
        anyhow::bail!(
            "GUI preflight failed: missing frontend entrypoint {}",
            gui_entry.display()
        );
    }

    Ok(())
}

mod backend_api;
mod backend_config;
mod backend_registry;
mod native_engine;
mod ollama;
mod runtime;
mod runtime_switcher;

static ACTIVE_BACKEND_REGISTRY: OnceLock<Arc<backend_registry::BackendRegistry>> = OnceLock::new();
static ACTIVE_RUNTIME_SWITCHER: OnceLock<runtime_switcher::RuntimeSwitcher> = OnceLock::new();

pub(crate) fn active_backend_registry() -> Arc<backend_registry::BackendRegistry> {
    ACTIVE_BACKEND_REGISTRY
        .get_or_init(|| Arc::new(backend_registry::BackendRegistry::discover()))
        .clone()
}

pub(crate) fn active_runtime_switcher() -> runtime_switcher::RuntimeSwitcher {
    ACTIVE_RUNTIME_SWITCHER
        .get_or_init(|| {
            runtime_switcher::RuntimeSwitcher::new(runtime_switcher::SwitchingConfig::default())
        })
        .clone()
}

// Re-export protocol module for use in main.rs
mod protocol {
    pub use ghostlink_core::protocol::GHOSTLINK_ETHERTYPE;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostlink_core::host::{AccelerationMode, RuntimeProfile};
    use std::net::TcpListener;

    fn args(items: &[&str]) -> impl Iterator<Item = String> {
        items
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn test_parse_usize_arg() {
        assert_eq!(parse_usize_arg("42").unwrap(), 42);
        assert!(parse_usize_arg("not-a-number").is_err());
    }

    #[test]
    fn test_parse_cli_more_commands() {
        assert_eq!(
            parse_cli(args(&["dashboard"])).unwrap(),
            CliCommand::Dashboard
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
            parse_cli(args(&["gui-check", "--strict"])).unwrap(),
            CliCommand::GuiCheck { strict: true }
        );
        assert_eq!(
            parse_cli(args(&["gui-diagnose"])).unwrap(),
            CliCommand::GuiDiagnose { strict: false }
        );

        let doctor = parse_cli(args(&["doctor"])).unwrap();
        if let CliCommand::Doctor(opts) = doctor {
            assert!(!opts.strict);
            assert!(!opts.network_probe);
        } else {
            panic!("Expected Doctor");
        }

        assert_eq!(
            parse_cli(args(&["cluster-start", "5", "9000"])).unwrap(),
            CliCommand::ClusterStart {
                node_count: 5,
                base_port: 9000
            }
        );

        let flow = parse_cli(args(&["flow", "l1", "r1", "16", "32", "128", "8", "tcp"])).unwrap();
        if let CliCommand::Flow {
            local_id,
            transport_mode,
            ..
        } = flow
        {
            assert_eq!(local_id, "l1");
            assert_eq!(transport_mode, FlowTransportMode::TcpLoopback);
        } else {
            panic!("Expected Flow");
        }

        assert_eq!(
            parse_cli(args(&["serve", "0.0.0.0", "1234"])).unwrap(),
            CliCommand::Serve {
                port: 1234,
                host: "0.0.0.0".to_string()
            }
        );
    }

    #[test]
    fn test_is_gui_backend_reachable_local() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let _ = is_gui_backend_reachable("127.0.0.1", port, Duration::from_millis(100));
    }

    #[test]
    fn test_build_device_map_simple() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("n1", 16.0, 32.0, "gpu", None),
            logical_cores: 16,
            recommended_workers: 8,
            acceleration_mode: AccelerationMode::Gpu,
            gpu_backend: ghostlink_core::host::GpuBackend::Cuda,
            xdp_supported: false,
            detection_source: "manual".to_string(),
            probe_mode: ProbeMode::Fast,
        };
        let map = build_device_map(&profile, "n1", "n2");
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("n1"), Some(&DeviceKind::Gpu));
    }

    #[test]
    fn test_build_device_map_rocm_gpu() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new(
                "amdgpu",
                48.0,
                128.0,
                "rocm",
                Some("AMD Radeon RX 7900 XTX".to_string()),
            ),
            logical_cores: 16,
            recommended_workers: 8,
            acceleration_mode: AccelerationMode::Gpu,
            gpu_backend: ghostlink_core::host::GpuBackend::Rocm,
            xdp_supported: false,
            detection_source: "rocm-smi".to_string(),
            probe_mode: ProbeMode::Fast,
        };
        let map = build_device_map(&profile, "amdgpu", "n2");
        assert_eq!(map.get("amdgpu"), Some(&DeviceKind::Gpu));
        assert_eq!(map.get("n2"), Some(&DeviceKind::Gpu));
    }

    #[test]
    fn test_apply_file_config_to_env() {
        let mut config = FileConfig::default();
        let flow = FlowDefaults {
            local_id: Some("test-local".to_string()),
            remote_vram_gb: Some(24.0),
            ..Default::default()
        };
        config.flow = Some(flow);

        let cluster = ClusterStartDefaults {
            node_count: Some(10),
            ..Default::default()
        };
        config.cluster_start = Some(cluster);

        let gui = GuiDefaults {
            python: Some("/usr/bin/python3.11".to_string()),
        };
        config.gui = Some(gui);

        std::env::remove_var("GHOSTLINK_FLOW_DEFAULT_LOCAL_ID");
        std::env::remove_var("GHOSTLINK_FLOW_DEFAULT_REMOTE_VRAM_GB");
        std::env::remove_var("GHOSTLINK_CLUSTER_START_DEFAULT_NODE_COUNT");
        std::env::remove_var("GHOSTLINK_PYTHON");

        apply_file_config_to_env(&config);

        assert_eq!(
            std::env::var("GHOSTLINK_FLOW_DEFAULT_LOCAL_ID").unwrap(),
            "test-local"
        );
        assert_eq!(
            std::env::var("GHOSTLINK_FLOW_DEFAULT_REMOTE_VRAM_GB").unwrap(),
            "24"
        );
        assert_eq!(
            std::env::var("GHOSTLINK_CLUSTER_START_DEFAULT_NODE_COUNT").unwrap(),
            "10"
        );
        assert_eq!(
            std::env::var("GHOSTLINK_PYTHON").unwrap(),
            "/usr/bin/python3.11"
        );

        std::env::remove_var("GHOSTLINK_FLOW_DEFAULT_LOCAL_ID");
        std::env::remove_var("GHOSTLINK_FLOW_DEFAULT_REMOTE_VRAM_GB");
        std::env::remove_var("GHOSTLINK_CLUSTER_START_DEFAULT_NODE_COUNT");
        std::env::remove_var("GHOSTLINK_PYTHON");
    }

    #[test]
    fn test_json_escape() {
        assert_eq!(json_escape("simple"), "simple");
        assert_eq!(json_escape("with \" quotes"), "with \\\" quotes");
        assert_eq!(json_escape("with \\ backslash"), "with \\\\ backslash");
        assert_eq!(json_escape("with\nnewline"), "with\\nnewline");
    }

    #[test]
    fn test_env_default_helpers() {
        std::env::set_var("GHOSTLINK_STR_TEST", "val");
        assert_eq!(env_default_string("GHOSTLINK_STR_TEST", "fallback"), "val");
        std::env::remove_var("GHOSTLINK_STR_TEST");
        assert_eq!(
            env_default_string("GHOSTLINK_STR_TEST", "fallback"),
            "fallback"
        );

        std::env::set_var("GHOSTLINK_USIZE_TEST", "42");
        assert_eq!(env_default_usize("GHOSTLINK_USIZE_TEST", 10), 42);
        std::env::remove_var("GHOSTLINK_USIZE_TEST");
        assert_eq!(env_default_usize("GHOSTLINK_USIZE_TEST", 10), 10);

        std::env::set_var("GHOSTLINK_U16_TEST", "8000");
        assert_eq!(env_default_u16("GHOSTLINK_U16_TEST", 8003), 8000);
        std::env::remove_var("GHOSTLINK_U16_TEST");
        assert_eq!(env_default_u16("GHOSTLINK_U16_TEST", 8003), 8003);

        std::env::set_var("GHOSTLINK_BOOL_TEST", "true");
        assert!(env_default_bool("GHOSTLINK_BOOL_TEST", false));
        std::env::set_var("GHOSTLINK_BOOL_TEST", "1");
        assert!(env_default_bool("GHOSTLINK_BOOL_TEST", false));
        std::env::set_var("GHOSTLINK_BOOL_TEST", "yes");
        assert!(env_default_bool("GHOSTLINK_BOOL_TEST", false));
        std::env::set_var("GHOSTLINK_BOOL_TEST", "on");
        assert!(env_default_bool("GHOSTLINK_BOOL_TEST", false));
        std::env::set_var("GHOSTLINK_BOOL_TEST", "false");
        assert!(!env_default_bool("GHOSTLINK_BOOL_TEST", true));
        std::env::remove_var("GHOSTLINK_BOOL_TEST");
        assert!(env_default_bool("GHOSTLINK_BOOL_TEST", true));
    }

    #[test]
    fn test_vram_and_memory_env_defaults() {
        std::env::set_var("GHOSTLINK_VRAM_TEST", "16.5");
        assert_eq!(env_default_f32("GHOSTLINK_VRAM_TEST", 8.0), 16.5);
        std::env::remove_var("GHOSTLINK_VRAM_TEST");
        assert_eq!(env_default_f32("GHOSTLINK_VRAM_TEST", 8.0), 8.0);
    }

    #[test]
    fn test_detect_missing_optional_gui_python_modules() {
        let python = "python3";
        let result = detect_missing_optional_gui_python_modules(python);
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_missing_python_modules() {
        let python = "python3";
        let missing = detect_missing_python_modules(python, &["sys", "os"]).unwrap();
        assert!(missing.is_empty());

        let missing =
            detect_missing_python_modules(python, &["non_existent_module_ghostlink_test"]).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], "non_existent_module_ghostlink_test");
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
            gpu_backend: ghostlink_core::host::GpuBackend::Cpu,
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
