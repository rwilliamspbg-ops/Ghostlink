//! Ghost-Link CLI Demo
//!
//! Command-line interface for demonstrating Ghost-Link primitives:
//! - `plan` - Generate layer placement plan
//! - `join` - Broadcast discovery frame to join cluster
//! - `dashboard` - Display ASCII cluster dashboard

use crate::runtime::Runtime;
use anyhow::Result;
use ghostlink_core::autotune::AutoTuner;
use ghostlink_core::cluster::{ClusterState, NodeMetrics, NodeStatus};
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
    execute_pipeline_with_rebalance_and_measured, execute_pipeline_with_remote_stage,
    run_stage_worker, DeviceKind, PipelinePlan, TcpTransportConfig,
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
    /// Real cross-process execution when set: `remote_id`'s stage runs on
    /// a separate `ghost-link stage-worker --bind <this address>` process
    /// (started ahead of time by the caller) instead of being simulated
    /// in-process like every other transport mode here. `None` keeps the
    /// existing local-simulation behavior, now honestly labeled as such in
    /// the printed output rather than implying a real second node.
    remote_addr: Option<SocketAddr>,
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
    StageWorker {
        bind: String,
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
        /// When set, `remote_id`'s stage runs for real on a separate
        /// `ghost-link stage-worker` process at this address instead of
        /// being simulated locally. See `FlowOptions::remote_addr`.
        remote_addr: Option<SocketAddr>,
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
        CliCommand::StageWorker { bind } => print_stage_worker(&bind)?,
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
            remote_addr,
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
            remote_addr,
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
        "stage-worker" => {
            let bind = args.next().unwrap_or_else(|| {
                env_default_string("GHOSTLINK_STAGE_WORKER_DEFAULT_BIND", "0.0.0.0:9500")
            });
            Ok(CliCommand::StageWorker { bind })
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

            // Trailing flag, not positional (unlike everything above it) —
            // scan whatever's left rather than consuming a fixed slot, so
            // it can be added/omitted without shifting the existing
            // positional argument order.
            let remaining: Vec<String> = args.collect();
            let remote_addr = remaining
                .iter()
                .position(|a| a == "--remote-addr")
                .and_then(|i| remaining.get(i + 1))
                .map(|s| {
                    s.parse::<SocketAddr>()
                        .map_err(|e| anyhow::anyhow!("invalid --remote-addr '{s}': {e}"))
                })
                .transpose()?;

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
                remote_addr,
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
    // Remote metrics are seeded with placeholder constants ONLY for the
    // simulated (no --remote-addr) path, where there's no real connection to
    // measure anything from — the health monitor still needs *some* number
    // to classify status against. When --remote-addr is given, these get
    // replaced below with figures derived from the actual execution result
    // instead of being presented as if they were real from the start.
    if opts.remote_addr.is_none() {
        cluster.get_metrics_mut(opts.remote_id, |metrics| {
            metrics.record_latency(3.2);
            metrics.record_delivery_ratio(0.95);
            metrics.record_throughput(7.4);
        });
    }

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

    let nodes = cluster.nodes_snapshot();
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
    let execution = if let Some(remote_addr) = opts.remote_addr {
        // Real path: remote_id's stage(s) run on a separate stage-worker
        // process — merged into one logical stage internally, since the
        // real layer-assignment algorithm splits one node's range into
        // several raw stages rather than exactly one (verified: a 60-layer/
        // 2-node plan produces ~11 raw stages). See
        // execute_pipeline_with_remote_stage's doc comment.
        println!(
            "REAL execution: {}'s stage runs on a separate process at {remote_addr} \
             (start it first with `ghost-link stage-worker --bind {remote_addr}`)",
            opts.remote_id
        );
        execute_pipeline_with_remote_stage(
            &pipeline_plan,
            opts.local_id,
            opts.remote_id,
            opts.execution_tokens,
            opts.micro_batch,
            tcp_transport_config_from_env(),
            remote_addr,
        )
    } else {
        println!(
            "SIMULATED execution: single process, no second machine involved. \
             Pass --remote-addr <host:port> (with a `ghost-link stage-worker` \
             already running there) for real cross-process execution."
        );
        match opts.transport_mode {
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
        }
    };

    // Replace the remote node's placeholder health-monitor metrics (never
    // seeded in the first place for this path — see above) with figures
    // derived from what actually happened, now that it's known.
    if let (Some(_), Ok(result)) = (opts.remote_addr, &execution) {
        if let Some(remote_stats) = result.stage_stats.get(1) {
            cluster.get_metrics_mut(opts.remote_id, |metrics| {
                metrics.record_latency(remote_stats.avg_bridge_write_ms.max(0.01) * 1000.0);
                metrics.record_delivery_ratio(1.0);
                metrics.record_throughput(result.throughput_tokens_per_sec);
            });
        }
    }

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
    last_tokens_per_sec: f32,
    inference_metrics: host_metrics::InferenceMetrics,
    started_at: Instant,
    backend_url: String,
    cluster: Arc<ClusterState>,
    inference_backend: InferenceBackend,
    native_engine_client: native_engine::NativeEngineClient,
    ollama_client: ollama::OllamaClient,
    ollama_available: Arc<tokio::sync::Mutex<bool>>,
    settings: RuntimeSettings,
    mcp_registry: Arc<mcp::McpRegistry>,
    pending_tool_calls: Arc<tokio::sync::Mutex<HashMap<String, PendingToolCall>>>,
    download_progress: HashMap<String, DownloadProgressInfo>,
    /// Serializes the full model load/unload sequence end to end — the
    /// llama-server stage/kill/spawn dance in `NativeEngineClient::load_model_into_slot`
    /// plus the `BackendState` updates (`current_model`, model status, settings)
    /// that follow it. Without this, two overlapping `/api/models/load` (or
    /// load+unload) requests each independently ran `free_llama_port`'s
    /// system-wide `taskkill /F /IM llama-server.exe` and raced to bind the
    /// same port — confirmed by firing concurrent load requests and seeing
    /// each response report a *different* `current_model`, since the final
    /// `backend.current_model = selected_model` write was a plain
    /// last-writer-wins race with no relation to which process actually
    /// survived the taskkill/spawn collision.
    model_lifecycle_lock: Arc<tokio::sync::Mutex<()>>,
}

/// Real byte-level progress for an in-flight (or just-finished) model download.
/// Updated from the background download task, read by the GUI's polling loop.
#[derive(Debug, Clone, Serialize, Default)]
struct DownloadProgressInfo {
    bytes_downloaded: u64,
    total_bytes: u64,
    status: String,
    error: Option<String>,
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
    /// Token budget for conversation history sent to the model, distinct from
    /// `max_tokens` (per-response length). `#[serde(default)]` so settings.json
    /// files saved before this field existed still deserialize instead of
    /// falling back to `RuntimeSettings::default()` in its entirety.
    #[serde(default = "default_conversation_token_limit")]
    conversation_token_limit: usize,
}

// Kept as named constants (rather than inlined in both `RuntimeSettings::default()`
// and `default_conversation_token_limit()`) so the two defaults can't drift out
// of sync — see the derivation below.
const DEFAULT_CTX_SIZE: usize = 4096;
const DEFAULT_MAX_TOKENS: usize = 2048;
/// Slack reserved on top of `max_tokens` for role/formatting overhead —
/// mirrors the per-message `+ 4` in `build_conversation_prompt`, just applied
/// once at the whole-budget level here.
const CONVERSATION_TOKEN_MARGIN: usize = 128;

/// Derived from the *default* `ctx_size`/`max_tokens`, not a flat guess — a
/// flat number here previously left the stock settings.json tripping the
/// Settings tab's own "history + max_tokens > ctx_size" warning out of the
/// box. Still just the *default*: `handle_gui_chat` separately clamps
/// whatever value ends up in `conversation_token_limit` (default or
/// user-edited) to `ctx_size` before using it, so a manually misconfigured
/// value can't overflow the context window either.
fn default_conversation_token_limit() -> usize {
    DEFAULT_CTX_SIZE
        .saturating_sub(DEFAULT_MAX_TOKENS)
        .saturating_sub(CONVERSATION_TOKEN_MARGIN)
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
            ctx_size: DEFAULT_CTX_SIZE,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repeat_penalty: 1.1,
            max_tokens: DEFAULT_MAX_TOKENS,
            chat_exec_tokens: 1024,
            chat_micro_batch: 4,
            tcp_max_inflight: 256,
            discovery_listen: "0.0.0.0:45885".to_string(),
            discovery_broadcast: "255.255.255.255:45885".to_string(),
            discovery_auth_token: String::new(),
            tcp_auth_token: String::new(),
            xdp_interface: "eth0".to_string(),
            conversation_token_limit: default_conversation_token_limit(),
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
            quantization: "Q4_K_M".to_string(),
            status: "Ready".to_string(),
            local_path: String::new(),
        },
        ModelRecord {
            name: "google/gemma-7b-it".to_string(),
            size_gb: 7.0,
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

fn sessions_path() -> PathBuf {
    std::env::var("GHOSTLINK_SESSIONS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("sessions.json"))
}

fn load_persistent_sessions() -> Vec<SessionRecord> {
    let path = sessions_path();
    if path.exists() {
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(sessions) = serde_json::from_str::<Vec<SessionRecord>>(&data) {
                return sessions;
            }
        }
    }
    vec![]
}

fn save_persistent_sessions(sessions: &[SessionRecord]) {
    if let Ok(data) = serde_json::to_string_pretty(sessions) {
        let _ = fs::write(sessions_path(), data);
    }
}

fn load_settings() -> RuntimeSettings {
    let path = settings_path();
    let mut settings = if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str::<RuntimeSettings>(&data).unwrap_or_default()
        } else {
            RuntimeSettings::default()
        }
    } else {
        RuntimeSettings::default()
    };

    // Ensure native engine env mirrors persisted settings when launchers did not set them.
    // Critical for Linux launch.sh and dynamic model load/unload.
    if std::env::var("GHOSTLINK_LLAMA_SERVER_URL")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
        && !settings.llama_server_url.trim().is_empty()
    {
        std::env::set_var(
            "GHOSTLINK_LLAMA_SERVER_URL",
            settings.llama_server_url.trim(),
        );
    }
    if std::env::var("GHOSTLINK_NATIVE_ENGINE")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
        && !settings.native_engine.trim().is_empty()
    {
        std::env::set_var("GHOSTLINK_NATIVE_ENGINE", settings.native_engine.trim());
    }

    if let Ok(val) = std::env::var("GHOSTLINK_INFERENCE_BACKEND") {
        let v = val.trim().to_ascii_lowercase();
        if v == "native" || v == "ollama" {
            settings.inference_backend = v;
        }
    }
    if let Ok(val) = std::env::var("GHOSTLINK_NATIVE_ENGINE") {
        let v = val.trim().to_string();
        if !v.is_empty() {
            settings.native_engine = v;
        }
    }
    if let Ok(val) = std::env::var("GHOSTLINK_LLAMA_SERVER_URL") {
        let v = val.trim().to_string();
        if !v.is_empty() {
            settings.llama_server_url = v;
        }
    }
    if let Ok(val) = std::env::var("GHOSTLINK_LLAMA_NGL") {
        if let Ok(n) = val.trim().parse::<i32>() {
            settings.ngl = n;
        }
    }

    settings
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
#[derive(Debug, Deserialize, Clone)]
struct GuiChatMessage {
    role: String,
    content: String,
}
#[derive(Debug, Deserialize)]
struct GuiChatRequest {
    /// Latest turn's text. Kept for older GUI builds that haven't picked up
    /// `messages` yet — used only when `messages` is absent.
    message: String,
    /// Full conversation transcript (oldest first, including the latest
    /// turn). Preferred over `message` when present — without it the model
    /// never sees anything before the current turn.
    #[serde(default)]
    messages: Option<Vec<GuiChatMessage>>,
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
    system_prompt: Option<String>,
    #[allow(dead_code)]
    ollama_url: Option<String>,
    #[allow(dead_code)]
    stream: Option<bool>,
    #[allow(dead_code)]
    mcp: Option<serde_json::Value>,
}

/// Cheap chars/4 heuristic — no tokenizer wired in, but good enough to
/// budget conversation history against `conversation_token_limit` without
/// pulling in a model-specific vocab.
fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(if text.trim().is_empty() { 0 } else { 1 })
}

/// `/v1/embeddings`'s `input` field accepts either a single string or an
/// array of strings per the OpenAI spec — normalizes both into one list so
/// the handler has a single code path. Non-string array entries are
/// dropped rather than erroring the whole batch; an all-invalid or empty
/// input yields an empty `Vec`, which the caller turns into a 400.
fn normalize_embeddings_input(input: &serde_json::Value) -> Vec<String> {
    match input {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// Formats conversation history into a plain-text transcript the raw
/// completion endpoint (llama-server `/completion`, Ollama `/generate`)
/// expects, keeping as many of the newest turns as fit under `token_limit`
/// once `reserved_response_tokens` is set aside for the reply. Walks
/// newest-first so a long conversation loses its oldest turns first, never
/// its most recent ones — and always keeps at least the single newest
/// message even if it alone blows the budget.
///
/// Returns `(prompt, was_truncated)`; the caller surfaces `was_truncated`
/// back to the GUI so a dropped-history turn doesn't look like the model
/// silently forgot something.
fn build_conversation_prompt(
    system_prompt: Option<&str>,
    history: &[GuiChatMessage],
    token_limit: usize,
    reserved_response_tokens: usize,
) -> (String, bool) {
    let budget = token_limit
        .saturating_sub(reserved_response_tokens)
        .max(256);

    let mut kept: Vec<&GuiChatMessage> = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;

    for msg in history.iter().rev() {
        // +4 covers the "Role: " / newline formatting overhead below.
        let cost = estimate_tokens(&msg.content) + 4;
        if !kept.is_empty() && used + cost > budget {
            truncated = true;
            break;
        }
        used += cost;
        kept.push(msg);
    }
    kept.reverse();

    let mut prompt = String::new();
    if let Some(sys) = system_prompt {
        let sys = sys.trim();
        if !sys.is_empty() {
            prompt.push_str("System: ");
            prompt.push_str(sys);
            prompt.push_str("\n\n");
        }
    }
    for msg in &kept {
        let role_label = match msg.role.as_str() {
            "assistant" => "Assistant",
            "system" => "System",
            _ => "User",
        };
        prompt.push_str(role_label);
        prompt.push_str(": ");
        prompt.push_str(&msg.content);
        prompt.push('\n');
    }
    prompt.push_str("Assistant:");
    (prompt, truncated)
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
struct ModelDownloadProgressQuery {
    model_id: String,
}
#[derive(Debug, Deserialize)]
struct ModelDeleteRequest {
    model: String,
}
#[derive(Debug, Deserialize)]
struct DiscardPartialRequest {
    filename: String,
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionRecord {
    id: String,
    #[serde(default)]
    name: String,
    model: String,
    status: String,
    throughput: usize,
    latency: u32,
    tokens: usize,
    // Only returned by the single-session load endpoint — the list endpoint
    // (`/api/sessions`, shared with SessionsTab's active-session view)
    // builds its own trimmed JSON rather than serializing this field, so
    // listing saved chats doesn't ship every message body over the wire
    // just to render a summary card.
    #[serde(default)]
    messages: Vec<serde_json::Value>,
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

/// OpenAI's legacy (non-chat) completions request: a plain `prompt` string
/// rather than a `messages` array. `handle_completions` mirrors
/// `handle_chat_completions` almost exactly, just skipping the
/// last-message extraction step since there's nothing to extract from.
#[derive(Debug, Deserialize)]
struct CompletionRequest {
    model: String,
    prompt: String,
    #[allow(dead_code)]
    stream: Option<bool>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<usize>,
    penalty: Option<f32>,
    max_tokens: Option<usize>,
}
#[derive(Debug, Serialize)]
struct CompletionResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<CompletionChoice>,
}
#[derive(Debug, Serialize)]
struct CompletionChoice {
    text: String,
    index: usize,
    finish_reason: String,
}

/// OpenAI's `/v1/embeddings` request. `input` is `Value` rather than
/// `String`/`Vec<String>` because the real API accepts either a single
/// string or an array of strings — `handle_embeddings` normalizes both
/// into the same code path rather than needing two request types.
#[derive(Debug, Deserialize)]
struct EmbeddingsRequest {
    model: String,
    input: serde_json::Value,
}
#[derive(Debug, Serialize)]
struct EmbeddingsResponse {
    object: String,
    data: Vec<EmbeddingData>,
    model: String,
    usage: EmbeddingsUsage,
}
#[derive(Debug, Serialize)]
struct EmbeddingData {
    object: String,
    embedding: Vec<f32>,
    index: usize,
}
#[derive(Debug, Serialize)]
struct EmbeddingsUsage {
    prompt_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolResult {
    tool: String,
    server: String,
    result: String,
    success: bool,
}

/// Generation parameters captured once per chat request so the tool-calling loop
/// (see `run_tool_loop`) can call the same backend repeatedly across iterations —
/// and so a confirmation round-trip (`handle_gui_chat_tool_confirm`) can resume
/// generation later using the exact same settings the original request used.
#[derive(Debug, Clone)]
struct GenerationParams {
    inference_backend: InferenceBackend,
    current_model: String,
    native_engine_client: native_engine::NativeEngineClient,
    ollama_client: ollama::OllamaClient,
    settings: RuntimeSettings,
    temperature: f32,
    top_p: f32,
    top_k: usize,
    repeat_penalty: f32,
    exec_tokens: usize,
    /// Enabled tools, offered to the Ollama backend's native `tools` API
    /// (`generate_once` prefers this over ReAct-marker parsing for models that
    /// declare tool support; the native backend has no equivalent API today, so
    /// it always uses the ReAct prompt fallback regardless of this field).
    tool_schemas: Vec<mcp::McpToolSchema>,
}

/// A tool call the model requested that requires explicit user approval
/// (`McpServerConfig.requires_confirmation`) before it runs — e.g. terminal or
/// code_execution once Docker MCP Toolkit backs them. Stored server-side, keyed by
/// a request id, so the GUI's approve/deny click can resume the same chat turn.
#[derive(Debug, Clone)]
struct PendingToolCall {
    gen: GenerationParams,
    effective_prompt: String,
    iteration: usize,
    tool_owner: HashMap<String, String>,
    enabled_tool_names: Vec<String>,
    tool_results_so_far: Vec<ToolResult>,
    tool: String,
    server: String,
    args: serde_json::Value,
}

/// Rough size/quality rank for common GGUF quantization tags — higher
/// means bigger file / higher fidelity. Used to pick a quantization that
/// actually fits the local machine instead of an arbitrary file from the
/// repo. Matched by substring against the uppercased filename; ties are
/// broken by preferring the longest matching tag (so "Q3_K_M" doesn't
/// get miscategorized by a shorter unrelated prefix).
fn quant_rank(name_upper: &str) -> Option<i32> {
    const TIERS: &[(&str, i32)] = &[
        ("IQ1", 0),
        ("IQ2_XXS", 1),
        ("IQ2_XS", 2),
        ("IQ2_S", 3),
        ("IQ2_M", 3),
        ("Q2_K", 3),
        ("IQ3_XXS", 4),
        ("IQ3_XS", 5),
        ("Q3_K_S", 5),
        ("IQ3_S", 5),
        ("IQ3_M", 6),
        ("Q3_K_M", 6),
        ("Q3_K_L", 7),
        ("IQ4_XS", 7),
        ("IQ4_NL", 8),
        ("Q4_0", 8),
        ("Q4_1", 8),
        ("Q4_K_S", 8),
        ("Q4_K_M", 9),
        ("Q5_0", 10),
        ("Q5_1", 10),
        ("Q5_K_S", 10),
        ("Q5_K_M", 11),
        ("Q6_K", 12),
        ("Q8_0", 13),
        ("F16", 14),
        ("BF16", 14),
        ("F32", 15),
    ];
    TIERS
        .iter()
        .filter(|(tag, _)| name_upper.contains(tag))
        .max_by_key(|(tag, _)| tag.len())
        .map(|(_, rank)| *rank)
}

/// Target quantization rank (see `quant_rank`) for a given amount of
/// VRAM — aims for a comfortable sweet spot rather than the smallest or
/// largest option technically available. Below 4GB in particular, most
/// 7B+ full-precision or Q6/Q8 files won't fit at all, so favor a much
/// smaller quantization there rather than picking whatever the repo
/// happens to list first (previously: always `gguf_files[0]`, with no
/// regard for size at all).
fn target_quant_rank_for_vram(vram_gb: f32) -> i32 {
    if vram_gb >= 16.0 {
        12 // Q6_K
    } else if vram_gb >= 12.0 {
        11 // Q5_K_M
    } else if vram_gb >= 8.0 || vram_gb >= 4.0 {
        9 // Q4_K_M — the common "fits almost anywhere with partial offload" sweet spot
    } else {
        6 // Q3_K_M / IQ3_M for genuinely constrained VRAM
    }
}

/// HuggingFace GGUF repos commonly split a single quantization across
/// multiple files named `<name>-00001-of-00005.gguf`. Groups filenames
/// that belong to the same split set together (by filename with the
/// shard suffix stripped) so all shards of a chosen quantization get
/// downloaded — previously only `gguf_files[0]` was ever fetched, which
/// for a split model meant downloading one shard of a multi-file model
/// and silently leaving it unloadable (llama.cpp requires every shard
/// alongside the first). Each returned group is sorted by shard index.
fn group_gguf_shards(files: &[String]) -> Vec<Vec<String>> {
    use std::collections::BTreeMap;

    let shard_re = |name: &str| -> Option<(String, u32)> {
        // Matches "...-00001-of-00005.gguf" (case-insensitive "of").
        let lower = name.to_ascii_lowercase();
        let gguf_pos = lower.rfind(".gguf")?;
        let stem = &name[..gguf_pos];
        let of_pos = stem.to_ascii_lowercase().rfind("-of-")?;
        let (before_of, after_of) = stem.split_at(of_pos);
        let _total: u32 = after_of[4..].parse().ok()?;
        let dash_pos = before_of.rfind('-')?;
        let shard_idx: u32 = before_of[dash_pos + 1..].parse().ok()?;
        let base = before_of[..dash_pos].to_string();
        Some((base, shard_idx))
    };

    let mut groups: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    for file in files {
        match shard_re(file) {
            Some((base, idx)) => groups.entry(base).or_default().push((idx, file.clone())),
            None => groups
                .entry(file.clone())
                .or_default()
                .push((0, file.clone())),
        }
    }

    groups
        .into_values()
        .map(|mut shards| {
            shards.sort_by_key(|(idx, _)| *idx);
            shards.into_iter().map(|(_, name)| name).collect()
        })
        .collect()
}

/// Picks the shard-group whose quantization rank is closest to the
/// target for the detected VRAM, preferring a rank at or below the
/// target (safe) over one above it (might not fit) when both are
/// equally close. Falls back to the smallest available quantization if
/// none can be ranked (unusual filenames) rather than failing outright.
fn select_best_gguf_group(groups: &[Vec<String>], vram_gb: f32) -> Option<Vec<String>> {
    let target = target_quant_rank_for_vram(vram_gb);
    groups
        .iter()
        .filter(|g| !g.is_empty())
        .min_by_key(|g| {
            let upper = g[0].to_ascii_uppercase();
            match quant_rank(&upper) {
                Some(rank) => {
                    let diff = (rank - target).abs();
                    // Prefer <= target over > target on a tie in absolute distance.
                    (diff, if rank > target { 1 } else { 0 }, rank)
                }
                None => (i32::MAX, 1, i32::MAX),
            }
        })
        .cloned()
        .or_else(|| groups.first().cloned())
}

fn start_openai_api_server(port: u16, host: &str) -> Result<()> {
    use axum::{
        extract::{Path, Query, State},
        http::StatusCode,
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

    /// Waits for Ctrl+C, then force-tears-down every connected MCP server before
    /// `axum::serve`'s graceful shutdown lets the process exit. Without this, a
    /// `cmd /C npx ...`-spawned server's child processes (see mcp::client) would
    /// otherwise be orphaned when Ghostlink exits.
    async fn mcp_shutdown_on_ctrl_c(mcp_registry: Arc<mcp::McpRegistry>) {
        let _ = tokio::signal::ctrl_c().await;
        mcp_registry.shutdown_all().await;
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

        let (model, chat_req_id, inference_backend, native_engine_client, ollama_client, settings) = {
            let mut backend = lock_state(&state);
            backend.chat_requests = backend.chat_requests.saturating_add(1);
            let model = if req.model.trim().is_empty() {
                backend.current_model.clone()
            } else {
                req.model.clone()
            };
            (
                model,
                backend.chat_requests,
                backend.inference_backend,
                backend.native_engine_client.clone(),
                backend.ollama_client.clone(),
                backend.settings.clone(),
            )
        };

        let exec_tokens = chat_exec_token_budget(32);
        let gen_started = Instant::now();

        let (response_text, real_inference, backend_used, gen_tokens, gen_tps, gen_latency_ms) =
            match inference_backend {
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
                            None,
                            None,
                            None,
                        ),
                        Err(err) => (
                            format!(
                                "Ollama generation failed for model '{}': {}",
                                ollama_model, err
                            ),
                            false,
                            InferenceBackend::Ollama.as_str(),
                            None,
                            None,
                            None,
                        ),
                    }
                }
                InferenceBackend::Native => match native_engine_client
                    .generate(&model, &prompt, exec_tokens, 0.7, 0.9, 40, 1.1, &settings.native_engine)
                    .await
                {
                    Ok(gen) => (
                        gen.text,
                        gen.real_inference,
                        InferenceBackend::Native.as_str(),
                        gen.tokens_generated,
                        gen.tokens_per_sec,
                        gen.latency_ms,
                    ),
                    Err(err) => (
                        format!(
                            "Ghostlink native backend executed request #{} on model '{}'. Prompt length: {} chars. Native error: {}",
                            chat_req_id,
                            model,
                            prompt.len(),
                            err
                        ),
                        false,
                        InferenceBackend::Native.as_str(),
                        None,
                        None,
                        None,
                    ),
                },
            };

        // Real, measured latency/throughput from this actual generation call.
        // Previously this handler also ran a full synthetic fabric-pipeline
        // simulation (execute_pipeline_tcp_loopback/_distributed) on every
        // request purely to produce a decorative throughput string — pure
        // overhead (measured via `cargo bench`: ~0.3ms up to 40ms+ per request
        // depending on stage count) that never reflected the real backend.
        // This mirrors the pattern `finish_chat_response` already uses for
        // the GUI chat path: prefer the backend's own reported numbers, fall
        // back to wall-clock/word-count estimates otherwise.
        let wall_ms = (gen_started.elapsed().as_secs_f32() * 1000.0).max(0.1);
        let latency_ms = gen_latency_ms.unwrap_or(wall_ms);
        let tokens_out =
            gen_tokens.unwrap_or_else(|| (response_text.split_whitespace().count() as u32).max(1));
        let tokens_per_sec = gen_tps.or_else(|| {
            if real_inference && latency_ms > 0.0 {
                Some(tokens_out as f32 / (latency_ms / 1000.0))
            } else {
                None
            }
        });

        {
            let mut backend = lock_state(&state);
            backend.last_latency_ms = latency_ms;
            if let Some(tps) = tokens_per_sec {
                backend.last_tokens_per_sec = tps;
            }
            backend.inference_metrics.record(
                latency_ms,
                tokens_out,
                tokens_per_sec,
                real_inference,
            );
        }

        let response = Json(ChatCompletionResponse {
            id: format!("chatcmpl-{}", rand::random::<u32>()),
            object: "chat.completion".to_string(),
            created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
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

    /// OpenAI's legacy `/v1/completions` endpoint: same backend dispatch as
    /// `handle_chat_completions`, just reading a plain `prompt` string
    /// instead of extracting one from a `messages` array. No session
    /// context here (this is the stateless REST surface, not the GUI's
    /// conversation), so no slot/cache-prompt reuse — matches
    /// `handle_chat_completions`'s existing behavior.
    async fn handle_completions(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<CompletionRequest>,
    ) -> Json<CompletionResponse> {
        let prompt = req.prompt;

        let request_tracker = active_runtime_switcher().request_tracker().clone();
        request_tracker.increment().await;

        let temp = req.temperature.unwrap_or(0.7);
        let top_p = req.top_p.unwrap_or(0.9);
        let top_k = req.top_k.unwrap_or(40);
        let penalty = req.penalty.unwrap_or(1.1);
        let max_tokens = req.max_tokens.unwrap_or(1024).clamp(16, 4096);

        let (model, inference_backend, native_engine_client, ollama_client, settings) = {
            let mut backend = lock_state(&state);
            backend.chat_requests = backend.chat_requests.saturating_add(1);
            let model = if req.model.trim().is_empty() {
                backend.current_model.clone()
            } else {
                req.model.clone()
            };
            (
                model,
                backend.inference_backend,
                backend.native_engine_client.clone(),
                backend.ollama_client.clone(),
                backend.settings.clone(),
            )
        };

        let exec_tokens = chat_exec_token_budget(32);

        let response_text = match inference_backend {
            InferenceBackend::Ollama => ollama_client
                .generate(&model, &prompt, temp, top_p, top_k, penalty, max_tokens)
                .await
                .unwrap_or_else(|err| {
                    format!("Ollama generation failed for model '{}': {}", model, err)
                }),
            InferenceBackend::Native => native_engine_client
                .generate(
                    &model,
                    &prompt,
                    exec_tokens,
                    temp,
                    top_p,
                    top_k,
                    penalty,
                    &settings.native_engine,
                )
                .await
                .map(|gen| gen.text)
                .unwrap_or_else(|err| {
                    format!(
                        "Ghostlink native backend error on model '{}': {}",
                        model, err
                    )
                }),
        };

        let response = Json(CompletionResponse {
            id: format!("cmpl-{}", rand::random::<u32>()),
            object: "text_completion".to_string(),
            created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            model,
            choices: vec![CompletionChoice {
                text: response_text,
                index: 0,
                finish_reason: "stop".to_string(),
            }],
        });

        request_tracker.decrement().await;
        response
    }

    /// OpenAI's `/v1/embeddings` endpoint, backed by
    /// `OllamaClient::embeddings()` (already used internally by
    /// `/api/ollama/embeddings`). The native engine has no embedding
    /// support today — that would need a second llama-server instance
    /// launched with `--embedding`, out of scope here — so a native-backend
    /// request gets a real 501 explaining that, rather than a silent
    /// failure or a faked response.
    async fn handle_embeddings(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<EmbeddingsRequest>,
    ) -> axum::response::Response {
        let inputs = normalize_embeddings_input(&req.input);
        if inputs.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "message": "input must be a non-empty string or array of strings", "type": "invalid_request_error" }
                })),
            )
                .into_response();
        }

        let (inference_backend, ollama_client) = {
            let backend = lock_state(&state);
            (backend.inference_backend, backend.ollama_client.clone())
        };

        if inference_backend != InferenceBackend::Ollama {
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(serde_json::json!({
                    "error": {
                        "message": "embeddings are only available with the Ollama backend today — the native llama-server engine has no embedding support wired in",
                        "type": "not_implemented"
                    }
                })),
            )
                .into_response();
        }

        let mut data = Vec::with_capacity(inputs.len());
        let mut prompt_tokens = 0usize;
        for (index, text) in inputs.iter().enumerate() {
            match ollama_client.embeddings(&req.model, text).await {
                Ok(embedding) => {
                    prompt_tokens += estimate_tokens(text);
                    data.push(EmbeddingData {
                        object: "embedding".to_string(),
                        embedding,
                        index,
                    });
                }
                Err(err) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(serde_json::json!({
                            "error": { "message": format!("embedding generation failed: {err}"), "type": "upstream_error" }
                        })),
                    )
                        .into_response();
                }
            }
        }

        Json(EmbeddingsResponse {
            object: "list".to_string(),
            data,
            model: req.model,
            usage: EmbeddingsUsage {
                prompt_tokens,
                total_tokens: prompt_tokens,
            },
        })
        .into_response()
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
        mut on_progress: impl FnMut(u64, u64) + Send,
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

        // Pick a quantization that fits this machine's VRAM instead of
        // blindly taking whatever file the repo listed first, and pull every
        // shard of a split GGUF instead of just one.
        let vram_gb = detect_runtime_profile("hf-download").node_resources.vram_gb;
        let groups = group_gguf_shards(&gguf_files);
        let chosen_files = select_best_gguf_group(&groups, vram_gb)
            .ok_or_else(|| "No usable GGUF file found in this repository".to_string())?;

        // `filename` comes straight from the remote HuggingFace API response
        // (`rfilename`) and may contain nested-path separators or, in the
        // worst case, an absolute path. `PathBuf::join` silently discards the
        // base directory when given an absolute path, and any path
        // separators could otherwise let a crafted repo write outside
        // `models_dir`. Only the file's base name is ever meaningful here.
        let files: Vec<(String, std::path::PathBuf)> = chosen_files
            .iter()
            .map(|filename| {
                let safe_filename = std::path::Path::new(filename.as_str())
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .filter(|f| !f.is_empty())
                    .unwrap_or_else(|| "download.gguf".to_string());
                let dest_path = models_dir.join(&safe_filename);
                (filename.clone(), dest_path)
            })
            .collect();

        // Sizing pass: HEAD every shard up front so the progress callback can
        // report a real fraction from the very first update instead of sitting
        // at an unknowable 0/0 until the last shard finishes.
        let mut expected_sizes: Vec<u64> = Vec::with_capacity(files.len());
        let mut total_bytes: u64 = 0;
        for (filename, _) in &files {
            let file_url = format!(
                "https://huggingface.co/{}/resolve/main/{}",
                model_id, filename
            );
            let len = client
                .head(&file_url)
                .send()
                .await
                .ok()
                .and_then(|r| r.content_length())
                .unwrap_or(0);
            expected_sizes.push(len);
            total_bytes += len;
        }
        on_progress(0, total_bytes);

        let mut downloaded_bytes: u64 = 0;
        let mut first_dest_path: Option<std::path::PathBuf> = None;
        let mut last_report = std::time::Instant::now();
        for (idx, (filename, dest_path)) in files.iter().enumerate() {
            let expected_len = expected_sizes[idx];
            let existing_len = tokio::fs::metadata(dest_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            // A file only counts as already-downloaded if its size matches what
            // HuggingFace reports. Previously any existing file — including one
            // truncated by a dropped connection — was treated as complete and
            // silently never retried.
            let already_complete =
                dest_path.exists() && (expected_len == 0 || existing_len == expected_len);

            if !already_complete {
                let file_url = format!(
                    "https://huggingface.co/{}/resolve/main/{}",
                    model_id, filename
                );

                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("Dir error: {}", e))?;
                }

                // Stream into a `.part` sibling instead of the real `.gguf`
                // name. Previously a hard network error partway through
                // (`stream.chunk()` returning `Err`, e.g. a dropped
                // connection) propagated via `?` immediately, skipping the
                // cleanup that only ran for the "clean EOF but short byte
                // count" case below — leaving a truncated file sitting at
                // the final filename. `scan_local_models_dir` only looks for
                // `.gguf` files and has no integrity check of its own, so
                // that truncated file was then listed as a normal "Ready"
                // model and would fail or crash whenever actually loaded.
                // Writing to `.part` and only renaming into place after a
                // verified-complete download means an interrupted transfer
                // — for any reason, not just this one error path — can never
                // produce a corrupt file at the name the rest of the app
                // trusts.
                let mut tmp_os = dest_path.clone().into_os_string();
                tmp_os.push(".part");
                let tmp_path = std::path::PathBuf::from(tmp_os);

                let download_result: Result<u64, String> = async {
                    let resp = client
                        .get(&file_url)
                        .send()
                        .await
                        .map_err(|e| format!("Download error: {}", e))?;

                    if !resp.status().is_success() {
                        return Err(format!("Failed to download file (HTTP {})", resp.status()));
                    }

                    let mut file = tokio::fs::File::create(&tmp_path)
                        .await
                        .map_err(|e| format!("File error: {}", e))?;
                    let mut stream = resp;
                    let mut file_bytes: u64 = 0;
                    while let Some(chunk) = stream
                        .chunk()
                        .await
                        .map_err(|e| format!("Stream error: {}", e))?
                    {
                        file.write_all(&chunk)
                            .await
                            .map_err(|e| format!("Write error: {}", e))?;
                        file_bytes += chunk.len() as u64;
                        if last_report.elapsed() >= std::time::Duration::from_millis(200) {
                            on_progress(downloaded_bytes + file_bytes, total_bytes);
                            last_report = std::time::Instant::now();
                        }
                    }
                    file.flush()
                        .await
                        .map_err(|e| format!("Write error: {}", e))?;
                    drop(file);

                    // The connection can also be dropped mid-transfer (client
                    // timeout, network blip) without `chunk()` ever returning
                    // an `Err` — the stream just ends early. Compare against
                    // the size HuggingFace actually advertised instead of
                    // trusting an early EOF.
                    if expected_len > 0 && file_bytes != expected_len {
                        return Err(format!(
                            "Download incomplete for {}: got {} of {} bytes",
                            filename, file_bytes, expected_len
                        ));
                    }
                    Ok(file_bytes)
                }
                .await;

                let file_bytes = match download_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        return Err(e);
                    }
                };

                // Only now — a fully-verified file on disk — does it become
                // visible at the real `.gguf` name.
                tokio::fs::rename(&tmp_path, dest_path)
                    .await
                    .map_err(|e| format!("Finalize error: {}", e))?;

                downloaded_bytes += file_bytes;
            } else {
                downloaded_bytes += existing_len;
            }

            on_progress(downloaded_bytes, total_bytes);

            if first_dest_path.is_none() {
                first_dest_path = Some(dest_path.clone());
            }
        }

        Ok(first_dest_path
            .expect("chosen_files is non-empty")
            .to_string_lossy()
            .to_string())
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

        // Held for the rest of this handler so an overlapping load/unload
        // request queues up instead of racing it — see the field doc on
        // `model_lifecycle_lock` for what went wrong without this.
        let lifecycle_lock = {
            let backend = lock_state(&state);
            Arc::clone(&backend.model_lifecycle_lock)
        };
        let _lifecycle_guard = lifecycle_lock.lock().await;

        let (inference_backend, ollama_client, ollama_available) = {
            let backend = lock_state(&state);
            // Prefer live settings string (updated by Settings UI / runtime select)
            // so load and chat cannot disagree on which backend is active.
            let inference_backend = InferenceBackend::parse(&backend.settings.inference_backend);
            (
                inference_backend,
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
        let (native_engine_client, local_path, native_engine, selected_model) = {
            let mut backend = lock_state(&state);

            // Merge local scans so we can find local_path for locally-downloaded models
            let local = scan_local_models_dir(&backend.settings.models_dir);
            for l in &local {
                if !backend.models.iter().any(|m| m.name == l.name) {
                    backend.models.push(l.clone());
                }
            }

            // Resolve local GGUF path: exact name, basename match, or path suffix.
            let resolve_local_path =
                |models: &[ModelRecord], requested: &str| -> Option<(String, String)> {
                    let req = requested.trim();
                    let req_base = std::path::Path::new(req)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(req);
                    // Exact name with path
                    if let Some(m) = models
                        .iter()
                        .find(|m| m.name == req && !m.local_path.is_empty())
                    {
                        return Some((m.name.clone(), m.local_path.clone()));
                    }
                    // Case-insensitive name
                    if let Some(m) = models
                        .iter()
                        .find(|m| m.name.eq_ignore_ascii_case(req) && !m.local_path.is_empty())
                    {
                        return Some((m.name.clone(), m.local_path.clone()));
                    }
                    // Basename / stem match (gemma-4-E4B-it-Q4_K_M vs full HF id)
                    if let Some(m) = models.iter().find(|m| {
                        !m.local_path.is_empty()
                            && (m.name.eq_ignore_ascii_case(req_base)
                                || std::path::Path::new(&m.local_path)
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .map(|s| s.eq_ignore_ascii_case(req_base))
                                    .unwrap_or(false)
                                || m.local_path.replace('\\', "/").ends_with(req)
                                || m.name.ends_with(req_base))
                    }) {
                        return Some((m.name.clone(), m.local_path.clone()));
                    }
                    None
                };

            let mut selected = selected_model.clone();
            let mut local_path = resolve_local_path(&backend.models, &selected).map(|(_, p)| p);

            // Prefer canonical local model name when a GGUF match is found
            if let Some((name, path)) = resolve_local_path(&backend.models, &selected) {
                selected = name;
                local_path = Some(path);
            }

            // Save the selected model path to settings
            if let Some(ref path) = local_path {
                backend.settings.model_path = path.clone();
                save_settings(&backend.settings);
            }

            (
                backend.native_engine_client.clone(),
                local_path,
                backend.settings.native_engine.clone(),
                selected,
            )
        }; // <-- state lock dropped here

        // Native llama_server requires a real on-disk GGUF — never report fake success.
        if inference_backend == InferenceBackend::Native
            && (native_engine == "llama_server" || native_engine == "llama-server")
        {
            let Some(path) = local_path.clone() else {
                return Json(serde_json::json!({
                    "error": format!(
                        "model '{}' has no local GGUF path. Download a .gguf into models/ or select a local model.",
                        selected_model
                    ),
                }));
            };

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
        } else if let Some(path) = local_path.clone() {
            if native_engine == "llama_server" || native_engine == "llama-server" {
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
        let models_path = std::path::PathBuf::from(&models_dir);
        fs::create_dir_all(&models_path).ok();

        {
            let mut backend = lock_state(&state);
            // A download already running for this model — don't start a second
            // one racing it on the same destination file.
            if let Some(existing) = backend.download_progress.get(&model_id) {
                if existing.status == "downloading" {
                    return Json(serde_json::json!({
                        "status": "started",
                        "message": format!("{} is already downloading", model_id),
                    }));
                }
            }
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
            backend.download_progress.insert(
                model_id.clone(),
                DownloadProgressInfo {
                    bytes_downloaded: 0,
                    total_bytes: 0,
                    status: "downloading".to_string(),
                    error: None,
                },
            );
        }

        // The actual transfer runs detached from this request/response cycle so
        // a multi-GB model isn't bound by the frontend's HTTP client timeout —
        // that used to abort (and silently truncate) any download that took
        // longer than the client's timeout window. The GUI now polls
        // /api/models/download/progress for real byte-level progress instead.
        let spawn_state = Arc::clone(&state);
        let spawn_model_id = model_id.clone();
        tokio::spawn(async move {
            let progress_state = Arc::clone(&spawn_state);
            let progress_model_id = spawn_model_id.clone();
            let result =
                download_hf_model(&spawn_model_id, &models_path, move |downloaded, total| {
                    let mut backend = lock_state(&progress_state);
                    if let Some(p) = backend.download_progress.get_mut(&progress_model_id) {
                        p.bytes_downloaded = downloaded;
                        p.total_bytes = total;
                    }
                })
                .await;

            match result {
                Ok(local_path) => {
                    let filename = std::path::Path::new(&local_path)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| local_path.clone());
                    let name = filename
                        .strip_suffix(".gguf")
                        .unwrap_or(&filename)
                        .to_string();
                    let size_bytes = fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0);
                    let size_gb = size_bytes as f32 / (1024.0 * 1024.0 * 1024.0);

                    let mut backend = lock_state(&spawn_state);
                    backend.models.retain(|m| m.name != spawn_model_id);
                    backend.models.push(ModelRecord {
                        name,
                        size_gb,
                        model_type: "LLM".to_string(),
                        quantization: detect_quantization(&filename),
                        status: "Ready".to_string(),
                        local_path,
                    });
                    save_persistent_models(&backend.models);
                    backend.download_progress.insert(
                        spawn_model_id.clone(),
                        DownloadProgressInfo {
                            bytes_downloaded: size_bytes,
                            total_bytes: size_bytes,
                            status: "completed".to_string(),
                            error: None,
                        },
                    );
                }
                Err(err) => {
                    let mut backend = lock_state(&spawn_state);
                    if let Some(model) =
                        backend.models.iter_mut().find(|m| m.name == spawn_model_id)
                    {
                        model.status = "Failed".to_string();
                    }
                    save_persistent_models(&backend.models);
                    if let Some(p) = backend.download_progress.get_mut(&spawn_model_id) {
                        p.status = "failed".to_string();
                        p.error = Some(err);
                    }
                }
            }
        });

        Json(serde_json::json!({
            "status": "started",
            "message": format!("Downloading {}", model_id),
        }))
    }

    async fn handle_gui_model_download_progress(
        State(state): State<Arc<Mutex<BackendState>>>,
        Query(params): Query<ModelDownloadProgressQuery>,
    ) -> Json<serde_json::Value> {
        let model_id = params.model_id.trim().to_string();
        if model_id.is_empty() {
            return Json(serde_json::json!({
                "progress": 0.0,
                "status": "unknown",
                "error": "model_id cannot be empty",
            }));
        }

        let backend = lock_state(&state);
        // Real byte-level progress from the in-flight (or just-finished)
        // background download task — this is the source of truth whenever
        // it's available. The model-status heuristic below only covers
        // entries left over from before this map existed (e.g. across a
        // server restart mid-download).
        if let Some(p) = backend.download_progress.get(&model_id) {
            let progress = if p.total_bytes > 0 {
                (p.bytes_downloaded as f64 / p.total_bytes as f64).min(1.0)
            } else {
                0.0
            };
            return Json(serde_json::json!({
                "progress": progress,
                "status": p.status,
                "bytes_downloaded": p.bytes_downloaded,
                "total_bytes": p.total_bytes,
                "error": p.error,
            }));
        }

        if let Some(model) = backend.models.iter().find(|m| m.name == model_id) {
            let (progress, status) = match model.status.as_str() {
                "Downloading" => (0.0, "downloading"),
                "Failed" => (0.0, "failed"),
                "Ready" | "Loaded" => (1.0, "completed"),
                _ => (0.0, "unknown"),
            };
            return Json(serde_json::json!({
                "progress": progress,
                "status": status,
            }));
        }

        // Downloads currently transform a HuggingFace repo id into a local GGUF model name.
        // If the original id entry no longer exists and isn't downloading, treat it as done.
        let still_downloading = backend
            .models
            .iter()
            .any(|m| m.name == model_id && m.status == "Downloading");

        if still_downloading {
            Json(serde_json::json!({
                "progress": 0.0,
                "status": "downloading",
            }))
        } else {
            Json(serde_json::json!({
                "progress": 1.0,
                "status": "completed",
            }))
        }
    }

    // Shared by both delete routes so they can't drift apart again — the JSON-body
    // route (below) used to only remove the in-memory record and never touch the
    // .gguf on disk, while the path-param route did the real deletion. A model
    // "deleted" via the body route never actually freed disk space and reappeared
    // on the next /api/models refresh (scan_local_models_dir re-discovers it).
    fn delete_model_files_and_record(backend: &mut BackendState, model_name: &str) {
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
    }

    async fn handle_gui_model_delete(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ModelDeleteRequest>,
    ) -> Json<serde_json::Value> {
        let requested_model = req.model.trim().to_string();
        let mut backend = lock_state(&state);
        delete_model_files_and_record(&mut backend, &requested_model);
        Json(serde_json::json!({ "status": "ok", "message": "deleted" }))
    }

    async fn handle_gui_model_delete_v2(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(model_name): Path<String>,
    ) -> Json<serde_json::Value> {
        let mut backend = lock_state(&state);
        delete_model_files_and_record(&mut backend, &model_name);
        Json(serde_json::json!({
            "status": "ok",
            "model": model_name
        }))
    }

    // Surfaces `.gguf.part` files left behind by an interrupted download
    // (see download_hf_model's atomic-rename fix) — previously invisible
    // dead weight sitting in the models directory with no way to see or
    // reclaim it from the GUI.
    async fn handle_gui_models_partial(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let models_dir = {
            let backend = lock_state(&state);
            backend.settings.models_dir.clone()
        };
        let dir = std::path::Path::new(&models_dir);
        let mut partials = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let filename = path.file_name().map(|f| f.to_string_lossy().to_string());
                let Some(filename) = filename else { continue };
                if !filename.ends_with(".gguf.part") {
                    continue;
                }
                let metadata = fs::metadata(&path).ok();
                let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let age_secs = metadata
                    .and_then(|m| m.modified().ok())
                    .and_then(|m| m.elapsed().ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                partials.push(serde_json::json!({
                    "filename": filename,
                    "size_bytes": size_bytes,
                    "age_secs": age_secs,
                }));
            }
        }
        Json(serde_json::json!({ "partial_downloads": partials }))
    }

    async fn handle_gui_models_partial_discard(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<DiscardPartialRequest>,
    ) -> Json<serde_json::Value> {
        let models_dir = {
            let backend = lock_state(&state);
            backend.settings.models_dir.clone()
        };
        // Same defensive stance as the downloader's own filename handling —
        // only a bare, well-formed `.gguf.part` basename is ever accepted,
        // never a path that could escape models_dir.
        let safe_name = std::path::Path::new(&req.filename)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        if safe_name.is_empty() || !safe_name.ends_with(".gguf.part") {
            return Json(serde_json::json!({ "status": "error", "error": "invalid filename" }));
        }
        let path = std::path::Path::new(&models_dir).join(&safe_name);
        match fs::remove_file(&path) {
            Ok(()) => Json(serde_json::json!({ "status": "ok" })),
            Err(e) => Json(serde_json::json!({ "status": "error", "error": e.to_string() })),
        }
    }

    async fn handle_gui_model_unload(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(model_name): Path<String>,
    ) -> Json<serde_json::Value> {
        let requested = model_name.trim().to_string();
        if requested.is_empty() {
            return Json(serde_json::json!({ "error": "model cannot be empty" }));
        }

        // See `model_lifecycle_lock`'s field doc — same race as the load
        // handler applies here (an unload racing a concurrent load).
        let lifecycle_lock = {
            let backend = lock_state(&state);
            Arc::clone(&backend.model_lifecycle_lock)
        };
        let _lifecycle_guard = lifecycle_lock.lock().await;

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
        let (manual_workers, cluster) = {
            let backend = lock_state(&state);
            (backend.workers.clone(), Arc::clone(&backend.cluster))
        };

        // Merge manually-added workers with peers found via UDP auto-discovery.
        // `cluster` is kept live by the background broadcast/listen threads
        // started in `serve` and by explicit /api/workers/discover calls.
        let mut seen: std::collections::HashSet<String> =
            manual_workers.iter().map(|w| w.id.clone()).collect();
        let mut workers = manual_workers;

        for node in cluster.nodes_snapshot().iter() {
            if !seen.insert(node.id.clone()) {
                continue;
            }
            let metrics = cluster.get_metrics(&node.id);
            let (host, port) = metrics
                .as_ref()
                .and_then(|m| m.ip_address)
                .map(|addr| (addr.ip().to_string(), addr.port()))
                .unwrap_or_else(|| ("unknown".to_string(), 0));
            let status = match metrics.as_ref().map(|m| m.status) {
                Some(NodeStatus::Failed) => "Disconnected",
                Some(NodeStatus::Degraded) => "Degraded",
                _ => "Connected",
            };
            workers.push(WorkerRecord {
                id: node.id.clone(),
                host,
                port,
                status: status.to_string(),
                model: node
                    .gpu_name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                threads: 0,
                load: 0,
            });
        }

        Json(serde_json::json!({ "workers": workers }))
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

    async fn handle_gui_workers_discover(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let (cluster, discovery_broadcast, discovery_auth_token) = {
            let backend = lock_state(&state);
            (
                Arc::clone(&backend.cluster),
                backend.settings.discovery_broadcast.clone(),
                backend.settings.discovery_auth_token.clone(),
            )
        };

        let broadcast_addr = discovery_broadcast
            .parse::<SocketAddr>()
            .unwrap_or_else(|_| SocketAddr::from(([255, 255, 255, 255], DEFAULT_DISCOVERY_PORT)));
        let auth_token = if discovery_auth_token.is_empty() {
            None
        } else {
            Some(discovery_auth_token)
        };
        let local_node = detect_runtime_profile("workers-discover").node_resources;

        // broadcast_and_collect blocks on a UDP recv loop for response_timeout —
        // run it off the async runtime so it can't stall other requests.
        let peers = tokio::task::spawn_blocking(move || {
            let config = UdpDiscoveryConfig {
                broadcast_addr,
                auth_token,
                response_timeout: Duration::from_millis(1200),
                allow_legacy_crc32: env_default_bool(
                    "GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32",
                    false,
                ),
                ..UdpDiscoveryConfig::default()
            };
            let frame = DiscoveryFrame {
                kind: FrameKind::Join,
                node: local_node,
            };
            broadcast_and_collect(&frame, &config)
        })
        .await
        .unwrap_or_else(|_| Ok(Vec::new()))
        .unwrap_or_default();

        let discovered = peers.len();
        for (peer_frame, peer_addr) in peers {
            cluster.register_with_addr(peer_frame.node, Some(peer_addr));
        }

        Json(serde_json::json!({
            "status": "ok",
            "discovered": discovered,
            "count": cluster.node_count(),
        }))
    }

    async fn handle_gui_workers_disconnect(
        Path(worker_id): Path<String>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "worker_id": worker_id }))
    }

    async fn handle_gui_metrics(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        // Host snapshot is lock-free relative to BackendState (background sampler).
        let host = host_metrics::current_host_snapshot();

        let (inf, node_count, cluster_vram, uptime_s, backend_name) = {
            let backend = lock_state(&state);
            let cluster = Arc::clone(&backend.cluster);
            let node_count = cluster.node_count();
            let cluster_vram = cluster.total_vram_gb();
            let inf = backend.inference_metrics.snapshot();
            let uptime_s = backend.started_at.elapsed().as_secs_f32();
            let backend_name = backend.inference_backend.as_str().to_string();
            (inf, node_count, cluster_vram, uptime_s, backend_name)
        };

        let total_vram = if host.total_vram_gb > 0.0 {
            host.total_vram_gb
        } else {
            cluster_vram
        };
        let gpu = if host.gpu_available {
            host.gpu
        } else if total_vram > 0.0 {
            // Accelerator present but util probe unavailable — don't fake 50%.
            0.0
        } else {
            0.0
        };

        Json(serde_json::json!({
            "metrics": {
                "throughput": inf.tokens_per_sec,
                "cpu": host.cpu,
                "memory": host.memory,
                "gpu": gpu,
                "latency_p50": inf.latency_p50_ms,
                "latency_p95": inf.latency_p95_ms,
                "active_nodes": node_count,
                "total_vram_gb": total_vram,
                "total_memory_gb": host.total_memory_gb,
                "used_memory_gb": host.used_memory_gb,
                "gpu_available": host.gpu_available || total_vram > 0.0,
                "real_inference": inf.real_inference,
                "samples": inf.samples,
                "last_latency_ms": inf.last_latency_ms,
                "last_tokens": inf.last_tokens,
                "uptime_s": uptime_s,
                "inference_backend": backend_name,
            }
        }))
    }

    async fn handle_gui_sessions(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let backend = lock_state(&state);
        // Summary only — omits `messages` so listing saved chats doesn't ship
        // every message body over the wire just to render a summary card.
        let sessions: Vec<serde_json::Value> = backend
            .sessions
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "model": s.model,
                    "status": s.status,
                    "throughput": s.throughput,
                    "latency": s.latency,
                    "tokens": s.tokens,
                })
            })
            .collect();
        Json(serde_json::json!({ "sessions": sessions }))
    }

    async fn handle_gui_session_cancel(Path(session_id): Path<String>) -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok", "session_id": session_id, "cancelled": true }))
    }

    async fn handle_gui_session_save(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let session_id = req.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let name = req
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
            name: name.to_string(),
            model: model.to_string(),
            status: "saved".to_string(),
            throughput: 0,
            latency: 0,
            tokens: messages.len(),
            messages,
        };

        // Remove existing session with same id
        backend.sessions.retain(|s| s.id != session_id);
        backend.sessions.push(session);
        save_persistent_sessions(&backend.sessions);

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
                    "name": session.name,
                    "model": session.model,
                    "status": session.status,
                    "throughput": session.throughput,
                    "latency": session.latency,
                    "tokens": session.tokens,
                    "messages": session.messages,
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
            save_persistent_sessions(&backend.sessions);
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

    async fn handle_gui_pqc_state() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "enabled": true, "algorithm": "ml-kem-768" }))
    }

    async fn handle_gui_audit_log() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "entries": [] }))
    }

    async fn handle_gui_runtime_select(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let runtime = req.get("runtime").and_then(|v| v.as_str()).unwrap_or("cpu");
        let normalized = runtime.trim().to_ascii_lowercase();
        let (inference, native_engine) = match normalized.as_str() {
            "native" | "llama_server" | "llama-server" => ("native", Some("llama_server")),
            "ollama" => ("ollama", None),
            "directml" | "cpu" | "cuda" | "rocm" | "vulkan" | "metal" => {
                ("native", Some("llama_server"))
            }
            other => (other, None),
        };

        // When switching to Ollama, pick a valid installed tag if current model is a local GGUF.
        let mut auto_model: Option<String> = None;
        if inference == "ollama" {
            let ollama_client = {
                let backend = lock_state(&state);
                backend.ollama_client.clone()
            };
            if let Ok(models) = ollama_client.list_models().await {
                let backend = lock_state(&state);
                let current = backend.current_model.clone();
                let current_ok = models.iter().any(|m| {
                    m == &current
                        || m.eq_ignore_ascii_case(&current)
                        || m.starts_with(&format!("{current}:"))
                });
                if !current_ok {
                    auto_model = models.into_iter().next();
                }
            }
        }

        let mut backend = lock_state(&state);
        backend.settings.inference_backend = inference.to_string();
        backend.inference_backend = InferenceBackend::parse(inference);
        if let Some(engine) = native_engine {
            backend.settings.native_engine = engine.to_string();
            std::env::set_var("GHOSTLINK_NATIVE_ENGINE", engine);
        }
        if let Some(ref model) = auto_model {
            backend.current_model = model.clone();
        }
        std::env::set_var("GHOSTLINK_INFERENCE_BACKEND", inference);
        save_settings(&backend.settings);
        Json(serde_json::json!({
            "status": "ok",
            "runtime": inference,
            "inference_backend": inference,
            "native_engine": backend.settings.native_engine,
            "current_model": backend.current_model,
        }))
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
                    tool_calls: None,
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
                None,
            )
            .await
        {
            Ok(response) => Json(serde_json::json!(response)),
            Err(err) => Json(serde_json::json!({
                "error": format!("failed to chat with Ollama: {}", err),
            })),
        }
    }

    /// Single inference call against whichever backend is active. Pulled out of
    /// `handle_gui_chat` so the tool-calling loop (`run_tool_loop`) can call it
    /// repeatedly across iterations with a growing prompt, instead of once.
    /// Converts our MCP tool schemas into Ollama's OpenAI-style function-tool
    /// format so models with native tool-calling support in their chat template
    /// can use it directly, instead of relying solely on the ReAct text marker.
    fn ollama_tools_json(schemas: &[mcp::McpToolSchema]) -> Vec<serde_json::Value> {
        schemas
            .iter()
            .map(|schema| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": schema.name,
                        "description": schema.description,
                        "parameters": schema.input_schema,
                    }
                })
            })
            .collect()
    }

    async fn generate_once(
        state: &Arc<Mutex<BackendState>>,
        gen: &GenerationParams,
        prompt: &str,
    ) -> (
        String,
        bool,
        &'static str,
        Option<u32>,
        Option<f32>,
        Option<f32>,
    ) {
        match gen.inference_backend {
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

                let available_models_result: Result<Vec<String>, String> = gen
                    .ollama_client
                    .list_models()
                    .await
                    .map_err(|err| err.to_string());

                match available_models_result {
                    Ok(available_models) => {
                        let effective_model = resolve_model(&gen.current_model, &available_models);
                        if let Some(model_name) = effective_model {
                            if gen.tool_schemas.is_empty() {
                                match gen
                                    .ollama_client
                                    .generate(
                                        &model_name,
                                        prompt,
                                        gen.temperature,
                                        gen.top_p,
                                        gen.top_k,
                                        gen.repeat_penalty,
                                        gen.exec_tokens,
                                    )
                                    .await
                                {
                                    Ok(text) => {
                                        if model_name != gen.current_model {
                                            let mut backend = lock_state(state);
                                            backend.current_model = model_name;
                                        }
                                        let text = text.trim().to_string();
                                        let gen_tokens =
                                            Some((text.split_whitespace().count() as u32).max(1));
                                        (
                                            text,
                                            true,
                                            InferenceBackend::Ollama.as_str(),
                                            gen_tokens,
                                            None,
                                            None,
                                        )
                                    }
                                    Err(err) => {
                                        let fallback = format!(
                                            "Ollama generate failed for model '{}': {}",
                                            model_name, err
                                        );
                                        (
                                            fallback,
                                            false,
                                            InferenceBackend::Ollama.as_str(),
                                            None,
                                            None,
                                            None,
                                        )
                                    }
                                }
                            } else {
                                // Native tool-calling path: offer Ollama's `tools` param
                                // (best-effort — only models whose chat template
                                // declares tool support will actually populate
                                // `message.tool_calls`). When present, synthesize our
                                // internal `TOOL_CALL: {...}` marker so the existing
                                // ReAct parser in `run_tool_loop` handles it identically
                                // either way; otherwise fall back to the model's plain
                                // text (which may itself contain a ReAct-style marker).
                                let messages = vec![ollama::ChatMessage {
                                    role: "user".to_string(),
                                    content: prompt.to_string(),
                                    tool_calls: None,
                                }];
                                let tools_json = ollama_tools_json(&gen.tool_schemas);

                                match gen
                                    .ollama_client
                                    .chat(
                                        &model_name,
                                        &messages,
                                        Some(gen.temperature),
                                        Some(gen.top_p),
                                        Some(gen.top_k),
                                        Some(gen.repeat_penalty),
                                        Some(gen.exec_tokens),
                                        Some(tools_json),
                                    )
                                    .await
                                {
                                    Ok(response) => {
                                        if model_name != gen.current_model {
                                            let mut backend = lock_state(state);
                                            backend.current_model = model_name;
                                        }
                                        let native_call = response
                                            .message
                                            .tool_calls
                                            .as_ref()
                                            .and_then(|calls| calls.first());
                                        let text = match native_call {
                                            Some(call) => {
                                                let name = call
                                                    .get("function")
                                                    .and_then(|f| f.get("name"))
                                                    .and_then(|n| n.as_str())
                                                    .unwrap_or_default();
                                                let args = call
                                                    .get("function")
                                                    .and_then(|f| f.get("arguments"))
                                                    .cloned()
                                                    .unwrap_or(serde_json::Value::Null);
                                                format!(
                                                    "TOOL_CALL: {{\"tool\": \"{name}\", \"args\": {args}}}"
                                                )
                                            }
                                            None => response.message.content.trim().to_string(),
                                        };
                                        let gen_tokens =
                                            Some((text.split_whitespace().count() as u32).max(1));
                                        (
                                            text,
                                            true,
                                            InferenceBackend::Ollama.as_str(),
                                            gen_tokens,
                                            None,
                                            None,
                                        )
                                    }
                                    Err(err) => {
                                        let fallback = format!(
                                            "Ollama chat failed for model '{}': {}",
                                            model_name, err
                                        );
                                        (
                                            fallback,
                                            false,
                                            InferenceBackend::Ollama.as_str(),
                                            None,
                                            None,
                                            None,
                                        )
                                    }
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
                                    gen.current_model, available_hint
                                ),
                                false,
                                InferenceBackend::Ollama.as_str(),
                                None, None, None,
                            )
                        }
                    }
                    Err(err_text) => {
                        let fallback = format!(
                            "Inference backend '{}' unavailable while listing models: {}",
                            InferenceBackend::Ollama.as_str(),
                            err_text
                        );
                        (
                            fallback,
                            false,
                            InferenceBackend::Ollama.as_str(),
                            None,
                            None,
                            None,
                        )
                    }
                }
            }
            InferenceBackend::Native => {
                match gen
                    .native_engine_client
                    .generate(
                        &gen.current_model,
                        prompt,
                        gen.exec_tokens,
                        gen.temperature,
                        gen.top_p,
                        gen.top_k,
                        gen.repeat_penalty,
                        &gen.settings.native_engine,
                    )
                    .await
                {
                    Ok(native_gen) => {
                        let gen_tokens = native_gen.tokens_generated.or_else(|| {
                            Some((native_gen.text.split_whitespace().count() as u32).max(1))
                        });
                        (
                            native_gen.text,
                            native_gen.real_inference,
                            InferenceBackend::Native.as_str(),
                            gen_tokens,
                            native_gen.tokens_per_sec,
                            native_gen.latency_ms,
                        )
                    }
                    Err(err) => (
                        format!(
                            "Ghostlink native fabric backend processed model '{}' with {} estimated tokens. Native error: {}",
                            gen.current_model, gen.exec_tokens, err
                        ),
                        false,
                        InferenceBackend::Native.as_str(),
                        None, None, None,
                    ),
                }
            }
        }
    }

    enum ToolLoopOutcome {
        Final {
            response_text: String,
            real_inference: bool,
            backend_used: &'static str,
            gen_tokens: Option<u32>,
            gen_tps: Option<f32>,
            gen_latency_ms: Option<f32>,
            tool_results: Vec<ToolResult>,
        },
        PendingConfirmation {
            request_id: String,
            tool: String,
            server: String,
            args: serde_json::Value,
        },
    }

    /// The model-driven tool-calling loop: generate, check whether the model asked
    /// for a tool (`mcp::toolcall::extract_tool_call`), execute it for real via the
    /// MCP registry, feed the result back as an "Observation", and repeat — capped
    /// at `MAX_TOOL_ITERATIONS` round-trips. Replaces the old behavior of
    /// unconditionally dispatching every checked tool regardless of what the model
    /// actually needed.
    #[allow(clippy::too_many_arguments)]
    async fn run_tool_loop(
        state: &Arc<Mutex<BackendState>>,
        mcp_registry: &Arc<mcp::McpRegistry>,
        gen: &GenerationParams,
        tool_owner: &HashMap<String, String>,
        enabled_tool_names: &[String],
        mut effective_prompt: String,
        mut iteration: usize,
        mut tool_results: Vec<ToolResult>,
    ) -> ToolLoopOutcome {
        loop {
            let (text, real_inference, backend_used, gen_tokens, gen_tps, gen_latency_ms) =
                generate_once(state, gen, &effective_prompt).await;

            let Some(call) = mcp::toolcall::extract_tool_call(&text) else {
                return ToolLoopOutcome::Final {
                    response_text: text,
                    real_inference,
                    backend_used,
                    gen_tokens,
                    gen_tps,
                    gen_latency_ms,
                    tool_results,
                };
            };

            iteration += 1;
            if iteration > mcp::toolcall::MAX_TOOL_ITERATIONS {
                return ToolLoopOutcome::Final {
                    response_text: text,
                    real_inference,
                    backend_used,
                    gen_tokens,
                    gen_tps,
                    gen_latency_ms,
                    tool_results,
                };
            }

            let Some(server) = tool_owner.get(&call.tool).cloned() else {
                effective_prompt.push_str(&mcp::toolcall::format_observation(
                    &call.tool,
                    &serde_json::json!({
                        "error": format!("unknown tool '{}': not one of the enabled tools", call.tool)
                    }),
                ));
                continue;
            };

            if mcp_registry.requires_confirmation(&server).await {
                let request_id = uuid::Uuid::new_v4().to_string();
                let pending = PendingToolCall {
                    gen: gen.clone(),
                    effective_prompt,
                    iteration,
                    tool_owner: tool_owner.clone(),
                    enabled_tool_names: enabled_tool_names.to_vec(),
                    tool_results_so_far: tool_results,
                    tool: call.tool.clone(),
                    server: server.clone(),
                    args: call.args.clone(),
                };

                let pending_map = {
                    let backend = lock_state(state);
                    Arc::clone(&backend.pending_tool_calls)
                };
                pending_map.lock().await.insert(request_id.clone(), pending);

                return ToolLoopOutcome::PendingConfirmation {
                    request_id,
                    tool: call.tool,
                    server,
                    args: call.args,
                };
            }

            let (tool_result, observation_json) = match mcp_registry
                .call_tool(&server, &call.tool, call.args.clone())
                .await
            {
                Some(outcome) => {
                    let result_str = if outcome.success {
                        serde_json::to_string(&outcome.result)
                            .unwrap_or_else(|_| "<unserializable result>".to_string())
                    } else {
                        outcome
                            .error
                            .clone()
                            .unwrap_or_else(|| "tool call failed".to_string())
                    };
                    (
                        ToolResult {
                            tool: outcome.tool.clone(),
                            server: outcome.server.clone(),
                            result: result_str,
                            success: outcome.success,
                        },
                        outcome.result.clone(),
                    )
                }
                None => {
                    let msg = format!("no MCP server available for tool '{}'", call.tool);
                    (
                        ToolResult {
                            tool: call.tool.clone(),
                            server: server.clone(),
                            result: msg.clone(),
                            success: false,
                        },
                        serde_json::json!({ "error": msg }),
                    )
                }
            };

            effective_prompt.push_str(&mcp::toolcall::format_observation(
                &call.tool,
                &observation_json,
            ));
            tool_results.push(tool_result);
        }
    }

    /// Records a finished generation's metrics/session bookkeeping (request
    /// counter, `inference_metrics`, dashboard session record) and returns
    /// `(request_id, session_id, metrics_json)` for the caller's response body.
    /// Extracted out of `finish_chat_response` so the real-streaming path
    /// (`handle_gui_chat_stream`) can share the exact same side effects
    /// instead of a second, divergence-prone copy of this bookkeeping.
    fn record_generation_metrics(
        state: &Arc<Mutex<BackendState>>,
        current_model: &str,
        latency_ms: f32,
        tokens_out: u32,
        tokens_per_sec: Option<f32>,
        real_inference: bool,
    ) -> (u64, String, serde_json::Value) {
        let mut backend = lock_state(state);
        backend.chat_requests = backend.chat_requests.saturating_add(1);
        let request_seq = backend.chat_requests;

        backend.last_latency_ms = latency_ms;
        if let Some(tps) = tokens_per_sec {
            backend.last_tokens_per_sec = tps;
        }
        backend
            .inference_metrics
            .record(latency_ms, tokens_out, tokens_per_sec, real_inference);
        let snap = backend.inference_metrics.snapshot();

        let latency_u = latency_ms.round() as u32;
        let throughput_u = tokens_per_sec.unwrap_or(0.0).round().max(0.0) as usize;

        // Matched by its own well-known id, not just "whichever session is
        // first" — that used to mean any saved chat session that happened to
        // land at index 0 got its tokens/model/status silently overwritten
        // by unrelated live-inference bookkeeping the next time any chat
        // request completed.
        let live_session_id = "sess_local_001";
        let maybe_session = backend
            .sessions
            .iter_mut()
            .find(|s| s.id == live_session_id);
        let session_id = if let Some(session) = maybe_session {
            session.tokens = session.tokens.saturating_add(tokens_out as usize);
            session.throughput = throughput_u;
            session.latency = latency_u;
            session.model = current_model.to_string();
            session.status = if real_inference {
                "Running".to_string()
            } else {
                "Degraded".to_string()
            };
            session.id.clone()
        } else {
            let session_id = live_session_id.to_string();
            backend.sessions.push(SessionRecord {
                id: session_id.clone(),
                name: String::new(),
                model: current_model.to_string(),
                status: if real_inference {
                    "Running".to_string()
                } else {
                    "Degraded".to_string()
                },
                throughput: throughput_u,
                latency: latency_u,
                tokens: tokens_out as usize,
                messages: vec![],
            });
            session_id
        };

        let metrics_json = serde_json::json!({
            "throughput": snap.tokens_per_sec,
            "p50_ms": snap.latency_p50_ms,
            "p95_ms": snap.latency_p95_ms,
            "latency_ms": latency_ms,
            "tokens": tokens_out,
            "real_inference": real_inference,
        });
        (request_seq, session_id, metrics_json)
    }

    /// Shared response assembly (metrics recording, session bookkeeping, streaming
    /// vs. plain JSON) for a finished chat turn — used both by a fresh request and
    /// by one resumed after a tool-confirmation round-trip.
    #[allow(clippy::too_many_arguments)]
    async fn finish_chat_response(
        state: &Arc<Mutex<BackendState>>,
        ollama_available: &Arc<tokio::sync::Mutex<bool>>,
        started: Instant,
        current_model: &str,
        token_estimate: usize,
        exec_tokens: usize,
        exec_micro_batch: usize,
        mcp_echo: Option<serde_json::Value>,
        stream_response: bool,
        response_text: String,
        real_inference: bool,
        backend_used: &'static str,
        gen_tokens: Option<u32>,
        gen_tps: Option<f32>,
        gen_latency_ms: Option<f32>,
        tool_results: Vec<ToolResult>,
        truncated: bool,
    ) -> axum::response::Response {
        {
            let mut available_flag = ollama_available.lock().await;
            *available_flag = real_inference;
        }

        let wall_ms = (started.elapsed().as_secs_f32() * 1000.0).max(0.1);
        let latency_ms = gen_latency_ms.unwrap_or(wall_ms);
        let tokens_out =
            gen_tokens.unwrap_or_else(|| (response_text.split_whitespace().count() as u32).max(1));
        let tokens_per_sec = gen_tps.or_else(|| {
            if real_inference && latency_ms > 0.0 {
                Some(tokens_out as f32 / (latency_ms / 1000.0))
            } else {
                None
            }
        });

        let (request_id, session_id, metrics_json) = record_generation_metrics(
            state,
            current_model,
            latency_ms,
            tokens_out,
            tokens_per_sec,
            real_inference,
        );

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
            "tokens_generated": tokens_out,
            "exec_tokens": exec_tokens,
            "exec_micro_batch": exec_micro_batch,
            "real_inference": real_inference,
            "truncated": truncated,
            "metrics": metrics_json
        });

        if let Some(mcp) = mcp_echo {
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

        if stream_response {
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

    fn pending_tool_call_response(
        outcome: &str,
        tool: &str,
        server: &str,
        args: &serde_json::Value,
        request_id: &str,
    ) -> axum::response::Response {
        Json(serde_json::json!({
            "pending_tool_call": {
                "request_id": request_id,
                "tool": tool,
                "server": server,
                "args": args,
            },
            "response": format!(
                "This turn wants to run '{tool}' on MCP server '{server}', which requires your approval before it executes.",
            ),
            "outcome": outcome,
        }))
        .into_response()
    }

    /// Real incremental streaming path for `handle_gui_chat`, used when the
    /// caller requested `stream: true` and no MCP tools are enabled for this
    /// turn (tool-call detection needs the complete text, so that case keeps
    /// using the existing buffer-then-chunk path via `run_tool_loop` — see
    /// the call site). Forwards text deltas to the client as the backend
    /// produces them, instead of `handle_gui_chat`'s other path, which waits
    /// for the entire generation to finish and then fakes streaming by
    /// splitting the already-complete text into word chunks — meaning a
    /// client saw zero output for the whole generation time before
    /// (measured: tens of seconds for a longer response), regardless of the
    /// `stream: true` request.
    #[allow(clippy::too_many_arguments)]
    async fn handle_gui_chat_stream(
        state: Arc<Mutex<BackendState>>,
        ollama_available: Arc<tokio::sync::Mutex<bool>>,
        started: Instant,
        current_model: String,
        gen: GenerationParams,
        prompt: String,
        request_tracker: runtime_switcher::RequestTracker,
        truncated: bool,
    ) -> axum::response::Response {
        use axum::response::sse::{Event, Sse};

        // Snapshot a session id to embed in each streamed chunk up front —
        // the authoritative bookkeeping (record_generation_metrics) only runs
        // once the full text is known, at stream end, and may create a new
        // session if this is the very first request. That's a cosmetic
        // identifier mismatch for that one request on a single-user desktop
        // deployment, not a correctness issue worth blocking real streaming on.
        let request_id = rand::random::<u32>();
        let session_id = {
            let backend = lock_state(&state);
            backend
                .sessions
                .first()
                .map(|s| s.id.clone())
                .unwrap_or_else(|| "sess_local_001".to_string())
        };

        let backend_stream: Result<native_engine::NativeChatStream, String> =
            match gen.inference_backend {
                InferenceBackend::Native => {
                    gen.native_engine_client
                        .generate_chat_stream(
                            &gen.current_model,
                            &prompt,
                            gen.exec_tokens,
                            gen.temperature,
                            gen.top_p,
                            gen.top_k,
                            gen.repeat_penalty,
                        )
                        .await
                }
                InferenceBackend::Ollama => gen
                    .ollama_client
                    .generate_stream(
                        &gen.current_model,
                        &prompt,
                        gen.temperature,
                        gen.top_p,
                        gen.top_k,
                        gen.repeat_penalty,
                        gen.exec_tokens,
                    )
                    .await
                    .map(|s| -> native_engine::NativeChatStream {
                        Box::pin(StreamExt::map(s, |r| r.map_err(|e| e.to_string())))
                    })
                    .map_err(|e| e.to_string()),
            };

        let mut backend_stream = match backend_stream {
            Ok(s) => s,
            Err(err) => {
                request_tracker.decrement().await;
                let chunk = serde_json::json!({
                    "token": format!("[stream error: {err}] "),
                    "request_id": format!("req-{}", request_id),
                    "session_id": session_id,
                    "error": true,
                });
                let err_stream = futures::stream::once(async move {
                    Ok::<Event, Infallible>(Event::default().data(chunk.to_string()))
                });
                return Sse::new(err_stream).into_response();
            }
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(100);
        tokio::spawn(async move {
            let mut accumulated = String::new();
            let mut real_inference = true;
            let mut client_disconnected = false;
            while let Some(item) = backend_stream.next().await {
                match item {
                    Ok(text) => {
                        accumulated.push_str(&text);
                        let chunk = serde_json::json!({
                            "token": text,
                            "request_id": format!("req-{}", request_id),
                            "session_id": session_id,
                        });
                        if tx
                            .send(Ok(Event::default().data(chunk.to_string())))
                            .await
                            .is_err()
                        {
                            client_disconnected = true;
                            break;
                        }
                    }
                    Err(err) => {
                        real_inference = false;
                        let chunk = serde_json::json!({
                            "token": format!("[stream error: {err}] "),
                            "request_id": format!("req-{}", request_id),
                            "session_id": session_id,
                            "error": true,
                        });
                        let _ = tx.send(Ok(Event::default().data(chunk.to_string()))).await;
                        break;
                    }
                }
            }

            // Runs exactly once regardless of how the loop above ended
            // (normal completion, backend error, or client disconnect) so
            // metrics/tracker release can't be skipped by an early exit.
            {
                let mut available_flag = ollama_available.lock().await;
                *available_flag = real_inference;
            }

            let wall_ms = (started.elapsed().as_secs_f32() * 1000.0).max(0.1);
            let tokens_out = (accumulated.split_whitespace().count() as u32).max(1);
            let tokens_per_sec = if real_inference && wall_ms > 0.0 {
                Some(tokens_out as f32 / (wall_ms / 1000.0))
            } else {
                None
            };
            record_generation_metrics(
                &state,
                &current_model,
                wall_ms,
                tokens_out,
                tokens_per_sec,
                real_inference,
            );
            request_tracker.decrement().await;

            if client_disconnected {
                return;
            }

            let done = serde_json::json!({
                "done": true,
                "request_id": format!("req-{}", request_id),
                "session_id": session_id,
                "truncated": truncated,
            });
            let _ = tx.send(Ok(Event::default().data(done.to_string()))).await;
        });

        Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx)).into_response()
    }

    async fn handle_gui_chat(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<GuiChatRequest>,
    ) -> axum::response::Response {
        let started = Instant::now();

        let (
            current_model,
            _cluster,
            ollama_client,
            ollama_available,
            inference_backend,
            native_engine_client,
            settings,
            mcp_registry,
        ) = {
            let backend = lock_state(&state);
            let inference_backend = InferenceBackend::parse(&backend.settings.inference_backend);
            (
                backend.current_model.clone(),
                Arc::clone(&backend.cluster),
                backend.ollama_client.clone(),
                Arc::clone(&backend.ollama_available),
                inference_backend,
                backend.native_engine_client.clone(),
                backend.settings.clone(),
                Arc::clone(&backend.mcp_registry),
            )
        };

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

        // `messages` (the full transcript) is what current GUI builds send;
        // `message` alone is the older single-turn shape, kept as a fallback
        // so an un-upgraded client still gets a response instead of a 400.
        let history: Vec<GuiChatMessage> = req.messages.clone().unwrap_or_else(|| {
            vec![GuiChatMessage {
                role: "user".to_string(),
                content: req.message.clone(),
            }]
        });
        // Clamped to `ctx_size` regardless of what's configured — a
        // `conversation_token_limit` set higher than the model's actual
        // context window (manually, or via a stale settings.json) would
        // otherwise let history overflow the window instead of just being
        // truncated harder.
        let effective_token_limit = settings.conversation_token_limit.min(settings.ctx_size);
        let (transcript, history_truncated) = build_conversation_prompt(
            req.system_prompt.as_deref(),
            &history,
            effective_token_limit,
            exec_tokens,
        );
        let token_estimate = estimate_tokens(&transcript).clamp(1, 1_000_000);

        // Gather real tool schemas for every enabled tool identifier — a legacy
        // "slot" name (calculator, file_operations, ...) or, for standalone
        // additions with no slot (sequential-thinking, Docker gateway), the
        // server's own name — so the model can see real schemas and decide for
        // itself whether and which tool to call.
        let enabled_tool_names: Vec<String> = req
            .mcp
            .as_ref()
            .and_then(|mcp| mcp.get("tools"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let mut enabled_schemas = Vec::new();
        let mut tool_owner: HashMap<String, String> = HashMap::new();
        for identifier in &enabled_tool_names {
            let server_name = match mcp_registry.server_for_slot(identifier).await {
                Some(name) => Some(name),
                None => {
                    let schemas = mcp_registry.tool_schemas_for_server(identifier).await;
                    if schemas.is_empty() {
                        None
                    } else {
                        Some(identifier.clone())
                    }
                }
            };
            if let Some(server_name) = server_name {
                for schema in mcp_registry.tool_schemas_for_server(&server_name).await {
                    tool_owner.insert(schema.name.clone(), schema.server.clone());
                    enabled_schemas.push(schema);
                }
            }
        }

        let gen = GenerationParams {
            inference_backend,
            current_model: current_model.clone(),
            native_engine_client,
            ollama_client,
            settings,
            temperature: temp,
            top_p,
            top_k,
            repeat_penalty: penalty,
            exec_tokens,
            tool_schemas: enabled_schemas.clone(),
        };

        // Real incremental streaming only when there's no tool-call text to
        // detect in the model's raw output (tool-call marker parsing needs
        // the complete generation, so that case keeps the existing
        // buffer-then-fake-stream path below via run_tool_loop).
        if req.stream.unwrap_or(false) && enabled_schemas.is_empty() {
            // Held for the generation's full duration (released inside the
            // spawned streaming task once it finishes), matching how the
            // non-streaming path below tracks in-flight work for graceful
            // backend-switch draining.
            let request_tracker = active_runtime_switcher().request_tracker().clone();
            request_tracker.increment().await;
            return handle_gui_chat_stream(
                state,
                ollama_available,
                started,
                current_model,
                gen,
                transcript,
                request_tracker,
                history_truncated,
            )
            .await;
        }

        let tool_instructions = mcp::toolcall::build_tool_instructions(&enabled_schemas);
        let effective_prompt = if tool_instructions.is_empty() {
            transcript
        } else {
            // `transcript` already carries the system prompt (if any) and the
            // full windowed history — tool instructions just go in front of it.
            format!("{tool_instructions}\n\n{transcript}")
        };

        let request_tracker = active_runtime_switcher().request_tracker().clone();
        request_tracker.increment().await;

        let outcome = run_tool_loop(
            &state,
            &mcp_registry,
            &gen,
            &tool_owner,
            &enabled_tool_names,
            effective_prompt,
            0,
            Vec::new(),
        )
        .await;

        request_tracker.decrement().await;

        match outcome {
            ToolLoopOutcome::Final {
                response_text,
                real_inference,
                backend_used,
                gen_tokens,
                gen_tps,
                gen_latency_ms,
                tool_results,
            } => {
                finish_chat_response(
                    &state,
                    &ollama_available,
                    started,
                    &current_model,
                    token_estimate,
                    exec_tokens,
                    exec_micro_batch,
                    req.mcp,
                    req.stream.unwrap_or(false),
                    response_text,
                    real_inference,
                    backend_used,
                    gen_tokens,
                    gen_tps,
                    gen_latency_ms,
                    tool_results,
                    history_truncated,
                )
                .await
            }
            ToolLoopOutcome::PendingConfirmation {
                request_id,
                tool,
                server,
                args,
            } => pending_tool_call_response(
                "awaiting_confirmation",
                &tool,
                &server,
                &args,
                &request_id,
            ),
        }
    }

    #[derive(Debug, Deserialize)]
    struct ToolConfirmRequest {
        request_id: String,
        approve: bool,
    }

    /// Resumes a chat turn that paused for tool-call approval (see
    /// `run_tool_loop`'s confirmation gate): executes (or records the denial of)
    /// the pending tool, feeds the result back as an Observation, and continues
    /// generating from exactly where the original request left off.
    async fn handle_gui_chat_tool_confirm(
        State(state): State<Arc<Mutex<BackendState>>>,
        Json(req): Json<ToolConfirmRequest>,
    ) -> axum::response::Response {
        let (pending_map, mcp_registry, ollama_available) = {
            let backend = lock_state(&state);
            (
                Arc::clone(&backend.pending_tool_calls),
                Arc::clone(&backend.mcp_registry),
                Arc::clone(&backend.ollama_available),
            )
        };

        let pending = pending_map.lock().await.remove(&req.request_id);
        let Some(pending) = pending else {
            return Json(serde_json::json!({
                "error": format!("no pending tool call with request_id '{}'", req.request_id),
            }))
            .into_response();
        };

        let PendingToolCall {
            gen,
            mut effective_prompt,
            iteration,
            tool_owner,
            enabled_tool_names,
            mut tool_results_so_far,
            tool,
            server,
            args,
        } = pending;

        if req.approve {
            let (tool_result, observation_json) =
                match mcp_registry.call_tool(&server, &tool, args).await {
                    Some(outcome) => {
                        let result_str = if outcome.success {
                            serde_json::to_string(&outcome.result)
                                .unwrap_or_else(|_| "<unserializable result>".to_string())
                        } else {
                            outcome
                                .error
                                .clone()
                                .unwrap_or_else(|| "tool call failed".to_string())
                        };
                        (
                            ToolResult {
                                tool: outcome.tool.clone(),
                                server: outcome.server.clone(),
                                result: result_str,
                                success: outcome.success,
                            },
                            outcome.result.clone(),
                        )
                    }
                    None => {
                        let msg = format!("no MCP server available for tool '{tool}'");
                        (
                            ToolResult {
                                tool: tool.clone(),
                                server: server.clone(),
                                result: msg.clone(),
                                success: false,
                            },
                            serde_json::json!({ "error": msg }),
                        )
                    }
                };
            effective_prompt.push_str(&mcp::toolcall::format_observation(&tool, &observation_json));
            tool_results_so_far.push(tool_result);
        } else {
            effective_prompt.push_str(&mcp::toolcall::format_denial(&tool));
            tool_results_so_far.push(ToolResult {
                tool: tool.clone(),
                server: server.clone(),
                result: "denied by user".to_string(),
                success: false,
            });
        }

        let started = Instant::now();
        let request_tracker = active_runtime_switcher().request_tracker().clone();
        request_tracker.increment().await;

        let current_model = gen.current_model.clone();
        let exec_tokens = gen.exec_tokens;

        let outcome = run_tool_loop(
            &state,
            &mcp_registry,
            &gen,
            &tool_owner,
            &enabled_tool_names,
            effective_prompt,
            iteration,
            tool_results_so_far,
        )
        .await;

        request_tracker.decrement().await;

        match outcome {
            ToolLoopOutcome::Final {
                response_text,
                real_inference,
                backend_used,
                gen_tokens,
                gen_tps,
                gen_latency_ms,
                tool_results,
            } => {
                finish_chat_response(
                    &state,
                    &ollama_available,
                    started,
                    &current_model,
                    0,
                    exec_tokens,
                    chat_exec_micro_batch(),
                    None,
                    false,
                    response_text,
                    real_inference,
                    backend_used,
                    gen_tokens,
                    gen_tps,
                    gen_latency_ms,
                    tool_results,
                    false,
                )
                .await
            }
            ToolLoopOutcome::PendingConfirmation {
                request_id,
                tool,
                server,
                args,
            } => pending_tool_call_response(
                "awaiting_confirmation",
                &tool,
                &server,
                &args,
                &request_id,
            ),
        }
    }

    /// Every server configured in `mcp_servers.toml` plus its live connection
    /// status — backs the GUI's MCP tab (replacing the old hardcoded 8-tool list).
    async fn handle_list_mcp_servers(
        State(state): State<Arc<Mutex<BackendState>>>,
    ) -> Json<serde_json::Value> {
        let mcp_registry = {
            let backend = lock_state(&state);
            Arc::clone(&backend.mcp_registry)
        };

        match mcp_registry.list_all_servers().await {
            Ok(servers) => Json(serde_json::json!({ "servers": servers })),
            Err(err) => Json(serde_json::json!({ "error": err })),
        }
    }

    #[derive(Debug, Deserialize)]
    struct ToggleMcpServerRequest {
        enabled: bool,
    }

    /// Enables or disables one MCP server, taking effect immediately (connects or
    /// tears it down right away — see `McpRegistry::set_enabled`).
    async fn handle_toggle_mcp_server(
        State(state): State<Arc<Mutex<BackendState>>>,
        Path(name): Path<String>,
        Json(req): Json<ToggleMcpServerRequest>,
    ) -> Json<serde_json::Value> {
        let mcp_registry = {
            let backend = lock_state(&state);
            Arc::clone(&backend.mcp_registry)
        };

        match mcp_registry.set_enabled(&name, req.enabled).await {
            Ok(()) => match mcp_registry.list_all_servers().await {
                Ok(servers) => Json(serde_json::json!({ "success": true, "servers": servers })),
                Err(err) => Json(serde_json::json!({ "success": true, "error": err })),
            },
            Err(err) => Json(serde_json::json!({ "success": false, "error": err })),
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
                            let lowered = v.trim().to_ascii_lowercase();
                            let normalized = match lowered.as_str() {
                                "native" | "llama_server" | "llama-server" => "native",
                                "ollama" => "ollama",
                                other => other,
                            };
                            current.inference_backend = normalized.to_string();
                            // This used to only update the display string in
                            // `settings`, while the enum that every load/
                            // unload/generate code path actually reads
                            // (`backend.inference_backend`) stayed frozen at
                            // whatever GHOSTLINK_INFERENCE_BACKEND resolved
                            // to at process startup. Update both so a live
                            // settings change actually takes effect.
                            backend.inference_backend = InferenceBackend::parse(normalized);
                            std::env::set_var("GHOSTLINK_INFERENCE_BACKEND", normalized);
                            if normalized == "native" && current.native_engine.trim().is_empty() {
                                current.native_engine = "llama_server".to_string();
                            }
                        }
                    }
                    "native_engine" => {
                        if let Some(v) = value.as_str() {
                            current.native_engine = v.to_string();
                            std::env::set_var("GHOSTLINK_NATIVE_ENGINE", v);
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
                            std::env::set_var("GHOSTLINK_LLAMA_SERVER_URL", v);
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
                    "conversation_token_limit" => {
                        if let Some(v) = value.as_u64() {
                            current.conversation_token_limit = v as usize;
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
    println!("  - POST /v1/completions");
    println!("  - POST /v1/embeddings");
    println!("  - GET  /v1/models");
    println!("  - GET  /health");
    println!("  - GET  /api/models");
    println!("  - GET  /api/models/status");
    println!("  - GET  /api/models/download/progress");
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
    println!("  - GET  /api/security/pqc/state");
    println!("  - GET  /api/security/audit-log");
    println!("  - POST /api/runtime/select");
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
            12
        } else {
            -1
        };
        // Apply unconditionally, including ngl == -1 (below the lowest VRAM
        // tier, or no GPU detected at all — by far the most common case on a
        // GPU-less or low-VRAM host). Previously gated on `ngl > 0`, which
        // silently skipped both the settings update and the env var for this
        // case, leaving GHOSTLINK_LLAMA_NGL unset. native_engine.rs's
        // get_ngl() then fell through to its own -1 fallback anyway, but
        // unset -1 used to make the llama-server launch omit `-ngl` entirely
        // (defaulting to CPU-only) instead of explicitly passing `-ngl -1`
        // to let llama-server auto-decide.
        settings.ngl = ngl;
        std::env::set_var("GHOSTLINK_LLAMA_NGL", ngl.to_string());
        eprintln!(
            "[startup] Auto-configured ngl={} from detected VRAM ({:.1} GB)",
            ngl, profile.node_resources.vram_gb
        );
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
    let inference_backend = InferenceBackend::parse(&settings.inference_backend);
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
        sessions: load_persistent_sessions(),
        queue_depth: 0,
        chat_requests: 0,
        last_latency_ms: 0.0,
        last_tokens_per_sec: 0.0,
        inference_metrics: host_metrics::InferenceMetrics::default(),
        started_at: Instant::now(),
        backend_url,
        cluster,
        inference_backend,
        native_engine_client,
        ollama_client,
        ollama_available,
        settings,
        mcp_registry: Arc::new(mcp::McpRegistry::new(mcp::McpConfigManager::new(
            mcp::default_config_path(),
        ))),
        pending_tool_calls: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        download_progress: HashMap::new(),
        model_lifecycle_lock: Arc::new(tokio::sync::Mutex::new(())),
    }));

    // Background CPU/RAM/GPU sampler — keeps /api/metrics non-blocking.
    host_metrics::ensure_host_sampler();

    rt.block_on(async {
        // Connect every `enabled` MCP server before the API starts serving chat
        // requests. A misconfigured server is logged and skipped (see
        // McpRegistry::connect_enabled), so this never blocks Ghostlink from launching.
        let mcp_registry = {
            let backend = lock_state(&state);
            Arc::clone(&backend.mcp_registry)
        };
        mcp_registry.connect_enabled().await;

        let app = Router::new()
            .route("/v1/chat/completions", post(handle_chat_completions))
            .route("/v1/completions", post(handle_completions))
            .route("/v1/embeddings", post(handle_embeddings))
            .route("/v1/models", get(handle_models))
            .route("/health", get(handle_health))
            .route("/api/health", get(handle_health))
            .route("/api/models", get(handle_gui_models))
            .route("/api/models/status", get(handle_gui_model_status))
            .route(
                "/api/models/download/progress",
                get(handle_gui_model_download_progress),
            )
            .route("/api/models/load", post(handle_gui_model_load))
            .route("/api/models/download", post(handle_gui_model_download))
            .route("/api/models/delete", post(handle_gui_model_delete))
            .route("/api/models/partial", get(handle_gui_models_partial))
            .route(
                "/api/models/partial/discard",
                post(handle_gui_models_partial_discard),
            )
            .route(
                "/api/models/search/huggingface",
                get(handle_gui_models_search_hf),
            )
            .route(
                "/api/models/:model_name",
                delete(handle_gui_model_delete_v2),
            )
            .route(
                "/api/models/:model_name/unload",
                post(handle_gui_model_unload),
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
            .route("/api/security/pqc/state", get(handle_gui_pqc_state))
            .route("/api/security/audit-log", get(handle_gui_audit_log))
            .route("/api/inference/chat", post(handle_gui_chat))
            .route(
                "/api/inference/chat/tool-confirm",
                post(handle_gui_chat_tool_confirm),
            )
            // Accept GET/POST/PUT for settings — some clients issue PUT and previously got 405.
            .route(
                "/api/settings",
                get(handle_get_settings)
                    .post(handle_update_settings)
                    .put(handle_update_settings),
            )
            .route("/api/settings/reset", post(handle_reset_settings))
            .route("/api/runtime/detect", get(handle_runtime_detection))
            .route("/api/runtime/models", get(handle_models_by_runtime))
            .route("/api/runtime/recommend", get(handle_model_recommendations))
            .route("/api/runtime/select", post(handle_gui_runtime_select))
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
            .route("/api/mcp/servers", get(handle_list_mcp_servers))
            .route(
                "/api/mcp/servers/:name/toggle",
                post(handle_toggle_mcp_server),
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
            .with_graceful_shutdown(mcp_shutdown_on_ctrl_c(mcp_registry))
            .await
            .map_err(|err| anyhow::anyhow!("API server terminated with error: {}", err))
    })?;

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

/// Runs one pipeline stage for real on this machine, waiting for a
/// `ghost-link flow --remote-addr <this bind address>` coordinator to
/// connect. See `ghostlink_core::runtime::run_stage_worker` for the actual
/// accept/handshake/compute loop this just binds a listener for and reports
/// the result of — this function is deliberately thin, matching how
/// `print_join`/`print_discovery_listener` above are thin wrappers around
/// `ghostlink_core::discovery`.
fn print_stage_worker(bind: &str) -> Result<()> {
    let bind_addr: SocketAddr = bind
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid --bind address '{bind}': {e}"))?;

    println!("Ghost-Link Stage Worker\n");
    println!("=======================\n");
    println!("Binding: {bind_addr}");
    println!("Waiting for one coordinator connection (this process handles exactly one, then exits — same one-shot model `cluster-start`'s child processes already use)...");

    let listener = std::net::TcpListener::bind(bind_addr)
        .map_err(|e| anyhow::anyhow!("failed to bind {bind_addr}: {e}"))?;

    let summary = run_stage_worker(listener, &tcp_transport_config_from_env())
        .map_err(|e| anyhow::anyhow!("stage worker failed: {e}"))?;

    println!(
        "Done: processed {} batch(es), avg compute {:.2} ms/batch",
        summary.batches_processed, summary.avg_compute_ms
    );
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

    // Auto-tune all subsystems from the detected profile and persist the cache
    let sp = ghostlink_core::system_profile::SystemProfile::detect_fast();
    let tuner = AutoTuner::from_system_profile(&sp);
    tuner.save_cache();
    println!(
        "Auto-tuned: {} compute workers, {} max inflight, {:.0}µs healthy latency",
        tuner.worker_pool.compute_workers,
        tuner.tcp_config.max_inflight_batches,
        tuner.health_config.healthy_latency_us,
    );
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
mod host_metrics;
mod mcp;
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
            remote_addr,
            ..
        } = flow
        {
            assert_eq!(local_id, "l1");
            assert_eq!(transport_mode, FlowTransportMode::TcpLoopback);
            assert_eq!(remote_addr, None);
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
    fn test_parse_cli_flow_remote_addr() {
        let flow = parse_cli(args(&[
            "flow",
            "l1",
            "r1",
            "16",
            "32",
            "128",
            "8",
            "tcp",
            "--remote-addr",
            "192.168.1.50:9500",
        ]))
        .unwrap();
        let CliCommand::Flow { remote_addr, .. } = flow else {
            panic!("Expected Flow");
        };
        assert_eq!(
            remote_addr,
            Some("192.168.1.50:9500".parse::<SocketAddr>().unwrap())
        );
    }

    #[test]
    fn test_parse_cli_flow_rejects_invalid_remote_addr() {
        let err = parse_cli(args(&[
            "flow",
            "l1",
            "r1",
            "16",
            "32",
            "128",
            "8",
            "tcp",
            "--remote-addr",
            "not-an-addr",
        ]))
        .unwrap_err();
        assert!(
            err.to_string().contains("invalid --remote-addr"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_cli_stage_worker() {
        assert_eq!(
            parse_cli(args(&["stage-worker", "127.0.0.1:9500"])).unwrap(),
            CliCommand::StageWorker {
                bind: "127.0.0.1:9500".to_string()
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
    fn quant_rank_orders_common_tags_by_size() {
        assert!(quant_rank("MODEL-Q2_K.GGUF") < quant_rank("MODEL-Q4_K_M.GGUF"));
        assert!(quant_rank("MODEL-Q4_K_M.GGUF") < quant_rank("MODEL-Q8_0.GGUF"));
        assert!(quant_rank("MODEL-Q8_0.GGUF") < quant_rank("MODEL-F16.GGUF"));
        // Longest-match tie-break: "Q3_K_M" must not be misread via a
        // hypothetical shorter overlapping tag.
        assert_eq!(
            quant_rank("MODEL-Q3_K_M.GGUF"),
            quant_rank("model-q3_k_m.gguf".to_uppercase().as_str())
        );
        assert_eq!(quant_rank("MODEL-UNKNOWNQUANT.GGUF"), None);
    }

    #[test]
    fn target_quant_rank_scales_down_for_low_vram() {
        let high = target_quant_rank_for_vram(24.0);
        let mid = target_quant_rank_for_vram(8.0);
        let low = target_quant_rank_for_vram(2.0);
        assert!(low < mid);
        assert!(mid <= high);
    }

    #[test]
    fn select_best_gguf_group_picks_fitting_quant_not_first_listed() {
        // Regression: previously `download_hf_model` always took whatever
        // file HuggingFace's API listed first, regardless of size. A repo
        // commonly lists a large quant before smaller ones.
        let files = vec![
            "model-F16.gguf".to_string(),
            "model-Q8_0.gguf".to_string(),
            "model-Q4_K_M.gguf".to_string(),
            "model-Q2_K.gguf".to_string(),
        ];
        let groups = group_gguf_shards(&files);
        // Below 4GB VRAM should land on a small/mid quant, never blindly on
        // F16 (index 0, and the largest/most likely to fail to load).
        let chosen = select_best_gguf_group(&groups, 3.5).expect("a group should be chosen");
        assert_ne!(chosen, vec!["model-F16.gguf".to_string()]);
        let rank = quant_rank(&chosen[0].to_ascii_uppercase()).expect("known tag");
        assert!(
            rank <= quant_rank("Q4_K_M").unwrap(),
            "expected a Q4_K_M-or-smaller pick for 3.5GB VRAM, got rank {rank}"
        );

        // Higher VRAM should land on a noticeably bigger quant than the
        // low-VRAM case, confirming the target actually scales with VRAM
        // rather than always converging on the same answer.
        let chosen_high = select_best_gguf_group(&groups, 24.0).expect("a group should be chosen");
        let rank_high = quant_rank(&chosen_high[0].to_ascii_uppercase()).expect("known tag");
        assert!(rank_high > rank);
    }

    #[test]
    fn group_gguf_shards_keeps_split_files_together_in_order() {
        // Regression: previously only the first shard of a split GGUF was
        // ever downloaded, leaving an unloadable partial model on disk.
        let files = vec![
            "model-Q8_0-00002-of-00003.gguf".to_string(),
            "model-Q4_K_M.gguf".to_string(),
            "model-Q8_0-00001-of-00003.gguf".to_string(),
            "model-Q8_0-00003-of-00003.gguf".to_string(),
        ];
        let groups = group_gguf_shards(&files);
        let split_group = groups
            .iter()
            .find(|g| g.len() > 1)
            .expect("the Q8_0 split set should be grouped together");
        assert_eq!(
            split_group,
            &vec![
                "model-Q8_0-00001-of-00003.gguf".to_string(),
                "model-Q8_0-00002-of-00003.gguf".to_string(),
                "model-Q8_0-00003-of-00003.gguf".to_string(),
            ]
        );
        assert!(groups
            .iter()
            .any(|g| g == &vec!["model-Q4_K_M.gguf".to_string()]));
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

    fn gcm(role: &str, content: &str) -> GuiChatMessage {
        GuiChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn estimate_tokens_treats_blank_as_zero_and_scales_with_length() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   "), 0);
        assert_eq!(estimate_tokens("hi"), 1);
        assert!(estimate_tokens(&"word ".repeat(100)) > estimate_tokens("word"));
    }

    #[test]
    fn normalize_embeddings_input_accepts_a_single_string() {
        let input = serde_json::json!("hello world");
        assert_eq!(
            normalize_embeddings_input(&input),
            vec!["hello world".to_string()]
        );
    }

    #[test]
    fn normalize_embeddings_input_accepts_an_array_of_strings() {
        let input = serde_json::json!(["a", "b", "c"]);
        assert_eq!(
            normalize_embeddings_input(&input),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn normalize_embeddings_input_drops_non_string_array_entries_rather_than_erroring() {
        let input = serde_json::json!(["a", 42, null, "b"]);
        assert_eq!(
            normalize_embeddings_input(&input),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn normalize_embeddings_input_returns_empty_for_unsupported_shapes() {
        assert!(normalize_embeddings_input(&serde_json::json!(42)).is_empty());
        assert!(normalize_embeddings_input(&serde_json::json!(null)).is_empty());
        assert!(normalize_embeddings_input(&serde_json::json!({"not": "a list"})).is_empty());
        assert!(normalize_embeddings_input(&serde_json::json!([])).is_empty());
    }

    #[test]
    fn build_conversation_prompt_keeps_newest_turns_and_flags_truncation() {
        let history = vec![
            gcm("user", &"oldest turn ".repeat(200)),
            gcm("assistant", &"middle turn ".repeat(200)),
            gcm("user", "newest turn"),
        ];
        // Budget far too small to hold all three, comfortably fits the last.
        let (prompt, truncated) = build_conversation_prompt(None, &history, 64, 16);

        assert!(
            truncated,
            "dropping the oldest turns should report truncated=true"
        );
        assert!(prompt.contains("newest turn"));
        assert!(!prompt.contains("oldest turn"));
    }

    #[test]
    fn build_conversation_prompt_always_keeps_the_latest_turn_even_if_oversized() {
        let history = vec![gcm("user", &"way too long ".repeat(1000))];
        let (prompt, truncated) = build_conversation_prompt(None, &history, 64, 16);

        // A single turn can't be "the oldest we dropped" — there's nothing
        // else to fall back to, so it must survive and truncated stays false.
        assert!(!truncated);
        assert!(prompt.contains("way too long"));
    }

    #[test]
    fn build_conversation_prompt_includes_system_prompt_without_tool_instructions() {
        // Regression check: the old code path only spliced `system_prompt` in
        // when tool instructions were non-empty, silently dropping it on any
        // plain (no-tools) turn.
        let history = vec![gcm("user", "hello")];
        let (prompt, _) = build_conversation_prompt(Some("Be concise."), &history, 4096, 512);

        assert!(prompt.starts_with("System: Be concise."));
        assert!(prompt.contains("User: hello"));
    }

    #[test]
    fn default_conversation_token_limit_fits_under_default_ctx_size_with_default_max_tokens() {
        // Regression guard for the bug this was derived to fix: a flat
        // default (3072) + the default max_tokens (2048) exceeded the
        // default ctx_size (4096), tripping the Settings tab's own overflow
        // warning out of the box. This must never be true again.
        let limit = default_conversation_token_limit();
        assert!(
            limit + DEFAULT_MAX_TOKENS <= DEFAULT_CTX_SIZE,
            "default conversation_token_limit ({limit}) + default max_tokens ({DEFAULT_MAX_TOKENS}) \
             must fit within default ctx_size ({DEFAULT_CTX_SIZE})"
        );
    }

    #[test]
    fn runtime_settings_default_uses_the_same_constants() {
        let settings = RuntimeSettings::default();
        assert_eq!(settings.ctx_size, DEFAULT_CTX_SIZE);
        assert_eq!(settings.max_tokens, DEFAULT_MAX_TOKENS);
        assert_eq!(
            settings.conversation_token_limit,
            default_conversation_token_limit()
        );
    }
}
