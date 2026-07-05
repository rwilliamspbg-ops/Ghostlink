#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Serialize)]
struct StudioStatus {
    app: &'static str,
    phase: &'static str,
    status: &'static str,
    repo_root: String,
}

#[tauri::command]
fn studio_status() -> StudioStatus {
    StudioStatus {
        app: "Ghostlink Studio",
        phase: "Sprint 1",
        status: "command-bridge-ready",
        repo_root: repo_root().display().to_string(),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            studio_status,
            studio_snapshot,
            cluster_preview,
            discover_workers,
            load_flow_defaults,
            list_model_presets,
            list_backend_models,
            ollama_health,
            download_backend_model,
            load_backend_model,
            run_validation_tier,
            load_ghostlink_config,
            save_ghostlink_config,
            export_studio_profile,
            import_studio_profile,
            run_doctor,
            run_doctor_with_json,
            quick_tcp_probe,
            run_probe,
            run_flow_quick,
            run_flow_between,
            run_cluster_start,
            verify_hf_repo,
            chat_infer
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ghostlink Studio");
}

#[derive(Serialize)]
struct SnapshotMetric {
    label: String,
    value: String,
}

#[derive(Serialize)]
struct StudioSnapshot {
    metrics: Vec<SnapshotMetric>,
    checks_passed: usize,
    checks_warn: usize,
    summary: String,
}

#[tauri::command]
fn studio_snapshot() -> StudioSnapshot {
    let root = repo_root();
    let mut passed = 0usize;
    let mut warn = 0usize;

    let cargo_version = command_version("cargo", &["--version"]);
    if cargo_version.is_some() {
        passed += 1;
    } else {
        warn += 1;
    }

    let python_version = command_version("python3", &["--version"])
        .or_else(|| command_version("python", &["--version"]));
    if python_version.is_some() {
        passed += 1;
    } else {
        warn += 1;
    }

    let has_local_config = root.join("ghostlink.toml").exists();
    if has_local_config {
        passed += 1;
    } else {
        warn += 1;
    }

    let has_example_config = root.join("ghostlink.example.toml").exists();
    if has_example_config {
        passed += 1;
    } else {
        warn += 1;
    }

    let last_doctor_json = root.join("tmp").join("doctor-report.json");
    let doctor_json_fresh = fs::metadata(&last_doctor_json).is_ok();
    if doctor_json_fresh {
        passed += 1;
    } else {
        warn += 1;
    }

    StudioSnapshot {
        metrics: vec![
            SnapshotMetric {
                label: "Toolchain".to_string(),
                value: cargo_version.unwrap_or_else(|| "cargo missing".to_string()),
            },
            SnapshotMetric {
                label: "Python".to_string(),
                value: python_version.unwrap_or_else(|| "python missing".to_string()),
            },
            SnapshotMetric {
                label: "Local Config".to_string(),
                value: if has_local_config {
                    "present".to_string()
                } else {
                    "missing".to_string()
                },
            },
            SnapshotMetric {
                label: "Doctor Artifact".to_string(),
                value: if doctor_json_fresh {
                    "tmp/doctor-report.json".to_string()
                } else {
                    "not generated".to_string()
                },
            },
        ],
        checks_passed: passed,
        checks_warn: warn,
        summary: format!("{} checks passed, {} checks need attention", passed, warn),
    }
}

#[derive(Serialize)]
struct CommandResult {
    command: String,
    ok: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[derive(Serialize)]
struct ConfigFileState {
    path: String,
    exists: bool,
    content: String,
}

#[derive(Serialize)]
struct DoctorCheckSummary {
    area: String,
    name: String,
    status: String,
    detail: String,
    fix: Option<String>,
}

#[derive(Serialize)]
struct DoctorJsonSummary {
    path: String,
    pass: usize,
    warn: usize,
    fail: usize,
    checks: Vec<DoctorCheckSummary>,
}

#[derive(Serialize)]
struct ModelVerifyResult {
    repo: String,
    file: String,
    ok: bool,
    stdout: String,
    stderr: String,
}

#[derive(Serialize)]
struct ChatResult {
    backend: String,
    model: String,
    response: String,
    trace: String,
}

#[derive(Serialize)]
struct ValidationStep {
    name: String,
    ok: bool,
    exit_code: Option<i32>,
    duration_ms: u128,
    stdout: String,
    stderr: String,
}

#[derive(Serialize)]
struct ValidationReport {
    tier: String,
    ok: bool,
    summary: String,
    steps: Vec<ValidationStep>,
}

#[derive(Serialize)]
struct ClusterNodeCard {
    id: String,
    acceleration: String,
    workers: usize,
    system_memory_gb: f32,
    gpu_vram_gb: f32,
    health: String,
}

#[derive(Serialize)]
struct ClusterPreview {
    nodes: Vec<ClusterNodeCard>,
    summary: String,
}

#[derive(Serialize)]
struct WorkerDiscoveryCard {
    id: String,
    available: bool,
    workers: usize,
    system_memory_gb: f32,
    gpu_vram_gb: f32,
    acceleration: String,
    health: String,
    probe_mode: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct WorkerDiscoveryResult {
    query: Vec<String>,
    available_count: usize,
    workers: Vec<WorkerDiscoveryCard>,
    summary: String,
}

#[derive(Serialize)]
struct FlowDefaults {
    local_id: String,
    remote_id: String,
    execution_tokens: u32,
    micro_batch: u32,
    transport: String,
}

#[derive(Serialize)]
struct ModelPreset {
    name: String,
    repo: String,
    default_file: String,
    quant: String,
}

#[derive(Serialize, Deserialize)]
struct StudioProfile {
    profile_name: String,
    ui_theme: String,
    font_scale: f32,
    reduced_motion: bool,
    high_contrast: bool,
    model_repo: String,
    model_file: String,
    chat_model: String,
    chat_distributed: bool,
    #[serde(default)]
    ollama_url: String,
    #[serde(default)]
    ollama_model: String,
    config_content: String,
    #[serde(default)]
    worker_probe_hints: String,
    #[serde(default)]
    worker_probe_full: bool,
    #[serde(default)]
    local_node_id: String,
    #[serde(default)]
    remote_node_id: String,
    #[serde(default)]
    flow_transport: String,
    #[serde(default)]
    flow_execution_tokens: u32,
    #[serde(default)]
    flow_micro_batch: u32,
    #[serde(default)]
    start_node_count: u32,
    #[serde(default)]
    start_base_port: u16,
    #[serde(default)]
    show_advanced_cluster_buttons: bool,
}

#[derive(Serialize)]
struct TcpProbeResult {
    host: String,
    port: u16,
    reachable: bool,
    latency_ms: Option<u128>,
    error: Option<String>,
}

#[derive(Serialize)]
struct StudioProfileExportResult {
    profile_path: String,
}

#[tauri::command]
fn cluster_preview(node_id: String, full: bool) -> Result<ClusterPreview, String> {
    let command = run_ghostlink_command(if full {
        vec!["probe", node_id.as_str(), "full"]
    } else {
        vec!["probe", node_id.as_str(), "fast"]
    })?;

    if !command.ok {
        return Err(format!(
            "probe command failed (exit code {:?})",
            command.exit_code
        ));
    }

    let parsed = parse_probe_to_node(command.stdout.as_str()).ok_or_else(|| {
        format!(
            "failed to parse live probe output for node '{}'",
            node_id.as_str()
        )
    })?;
    let nodes = vec![parsed];
    let healthy = nodes.iter().filter(|node| node.health == "healthy").count();
    let degraded = nodes.len().saturating_sub(healthy);

    Ok(ClusterPreview {
        summary: format!(
            "{} nodes total ({} healthy, {} degraded)",
            nodes.len(),
            healthy,
            degraded
        ),
        nodes,
    })
}

#[tauri::command]
fn discover_workers(node_ids: Vec<String>, full: bool) -> Result<WorkerDiscoveryResult, String> {
    let mut query = merge_worker_discovery_ids(node_ids);
    if query.is_empty() {
        query = vec!["studio-local".to_string(), "studio-remote".to_string()];
    }

    let probe_mode = if full { "full" } else { "fast" };
    let mut workers = Vec::with_capacity(query.len());
    let mut available_count = 0usize;

    for node_id in &query {
        let command = run_ghostlink_command(if full {
            vec!["probe", node_id.as_str(), "full"]
        } else {
            vec!["probe", node_id.as_str(), "fast"]
        })?;

        if command.ok {
            if let Some(parsed) = parse_probe_to_node(command.stdout.as_str()) {
                available_count = available_count.saturating_add(1);
                workers.push(WorkerDiscoveryCard {
                    id: parsed.id,
                    available: true,
                    workers: parsed.workers,
                    system_memory_gb: parsed.system_memory_gb,
                    gpu_vram_gb: parsed.gpu_vram_gb,
                    acceleration: parsed.acceleration,
                    health: parsed.health,
                    probe_mode: probe_mode.to_string(),
                    error: None,
                });
                continue;
            }
        }

        let stderr = command.stderr.trim();
        let stdout = command.stdout.trim();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "probe command produced no details"
        };

        workers.push(WorkerDiscoveryCard {
            id: node_id.clone(),
            available: false,
            workers: 0,
            system_memory_gb: 0.0,
            gpu_vram_gb: 0.0,
            acceleration: "unknown".to_string(),
            health: "unreachable".to_string(),
            probe_mode: probe_mode.to_string(),
            error: Some(detail.to_string()),
        });
    }

    Ok(WorkerDiscoveryResult {
        summary: format!("{} of {} workers reachable", available_count, workers.len()),
        query,
        available_count,
        workers,
    })
}

#[tauri::command]
fn load_flow_defaults() -> FlowDefaults {
    let defaults = parse_flow_config_defaults().unwrap_or(FlowDefaults {
        local_id: "studio-local".to_string(),
        remote_id: "studio-remote".to_string(),
        execution_tokens: 64,
        micro_batch: 2,
        transport: "tcp".to_string(),
    });

    FlowDefaults {
        local_id: sanitize_node_id(defaults.local_id.as_str(), "studio-local"),
        remote_id: sanitize_node_id(defaults.remote_id.as_str(), "studio-remote"),
        execution_tokens: defaults.execution_tokens.clamp(16, 512),
        micro_batch: defaults.micro_batch.clamp(1, 16),
        transport: sanitize_transport(defaults.transport.as_str()),
    }
}

#[tauri::command]
fn list_model_presets() -> Vec<ModelPreset> {
    vec![
        ModelPreset {
            name: "Tiny GPT-2 (smoke)".to_string(),
            repo: "sshleifer/tiny-gpt2".to_string(),
            default_file: "config.json".to_string(),
            quant: "Int8".to_string(),
        },
        ModelPreset {
            name: "Tiny Random BERT (smoke)".to_string(),
            repo: "hf-internal-testing/tiny-random-bert".to_string(),
            default_file: "config.json".to_string(),
            quant: "Int8".to_string(),
        },
        ModelPreset {
            name: "Mistral 7B".to_string(),
            repo: "mistralai/Mistral-7B-v0.1".to_string(),
            default_file: "config.json".to_string(),
            quant: "Int4".to_string(),
        },
    ]
}

#[tauri::command]
fn run_validation_tier(tier: String) -> Result<ValidationReport, String> {
    let root = repo_root();
    let tier_norm = tier.trim().to_ascii_lowercase();
    let mut steps = Vec::new();

    match tier_norm.as_str() {
        "fast" => {
            steps.push(run_command_step(
                "cargo-test-ghost-link",
                "cargo",
                &["test", "-p", "ghost-link"],
                &root,
            )?);
            steps.push(run_command_step(
                "doctor-json",
                "cargo",
                &[
                    "run",
                    "-p",
                    "ghost-link",
                    "--",
                    "doctor",
                    "--json",
                    "./tmp/studio-validation-doctor.json",
                ],
                &root,
            )?);
        }
        "full" => {
            steps.push(run_command_step(
                "full-validation-script",
                "bash",
                &["scripts/run_full_validation.sh"],
                &root,
            )?);
        }
        other => {
            return Err(format!(
                "unknown validation tier '{}'; expected 'fast' or 'full'",
                other
            ));
        }
    }

    let ok = steps.iter().all(|step| step.ok);
    let passed = steps.iter().filter(|step| step.ok).count();
    let failed = steps.len().saturating_sub(passed);

    Ok(ValidationReport {
        tier: tier_norm,
        ok,
        summary: format!("{} step(s) passed, {} step(s) failed", passed, failed),
        steps,
    })
}

#[tauri::command]
fn load_ghostlink_config() -> Result<ConfigFileState, String> {
    let root = repo_root();
    let local_path = root.join("ghostlink.toml");
    let example_path = root.join("ghostlink.example.toml");

    if local_path.exists() {
        let content = fs::read_to_string(&local_path)
            .map_err(|err| format!("failed to read {}: {}", local_path.display(), err))?;
        return Ok(ConfigFileState {
            path: local_path.display().to_string(),
            exists: true,
            content,
        });
    }

    let content = fs::read_to_string(&example_path)
        .map_err(|err| format!("failed to read {}: {}", example_path.display(), err))?;
    Ok(ConfigFileState {
        path: local_path.display().to_string(),
        exists: false,
        content,
    })
}

#[tauri::command]
fn save_ghostlink_config(content: String) -> Result<ConfigFileState, String> {
    let root = repo_root();
    let local_path = root.join("ghostlink.toml");
    fs::write(&local_path, content.as_bytes())
        .map_err(|err| format!("failed to write {}: {}", local_path.display(), err))?;

    Ok(ConfigFileState {
        path: local_path.display().to_string(),
        exists: true,
        content,
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn export_studio_profile(
    profile_name: String,
    ui_theme: String,
    font_scale: f32,
    reduced_motion: bool,
    high_contrast: bool,
    model_repo: String,
    model_file: String,
    chat_model: String,
    chat_distributed: bool,
    ollama_url: String,
    ollama_model: String,
    config_content: String,
    worker_probe_hints: String,
    worker_probe_full: bool,
    local_node_id: String,
    remote_node_id: String,
    flow_transport: String,
    flow_execution_tokens: u32,
    flow_micro_batch: u32,
    start_node_count: u32,
    start_base_port: u16,
    show_advanced_cluster_buttons: bool,
) -> Result<StudioProfileExportResult, String> {
    let root = repo_root();
    let sanitized = sanitize_profile_name(profile_name.as_str());
    let profile_dir = root.join("tmp").join("studio-profiles");
    fs::create_dir_all(&profile_dir).map_err(|err| {
        format!(
            "failed to create profile directory {}: {}",
            profile_dir.display(),
            err
        )
    })?;

    let profile = StudioProfile {
        profile_name: sanitized.clone(),
        ui_theme,
        font_scale,
        reduced_motion,
        high_contrast,
        model_repo,
        model_file,
        chat_model,
        chat_distributed,
        ollama_url,
        ollama_model,
        config_content,
        worker_probe_hints,
        worker_probe_full,
        local_node_id,
        remote_node_id,
        flow_transport,
        flow_execution_tokens,
        flow_micro_batch,
        start_node_count,
        start_base_port,
        show_advanced_cluster_buttons,
    };

    let out_path = profile_dir.join(format!("{}.json", sanitized));
    let payload = serde_json::to_string_pretty(&profile)
        .map_err(|err| format!("failed to serialize profile: {}", err))?;
    fs::write(&out_path, payload)
        .map_err(|err| format!("failed to write profile {}: {}", out_path.display(), err))?;

    Ok(StudioProfileExportResult {
        profile_path: out_path.display().to_string(),
    })
}

#[tauri::command]
fn import_studio_profile(profile_path: String) -> Result<StudioProfile, String> {
    let root = repo_root();
    let resolved = if profile_path.trim().starts_with("/") {
        PathBuf::from(profile_path.trim())
    } else {
        root.join(profile_path.trim())
    };

    let raw = fs::read_to_string(&resolved)
        .map_err(|err| format!("failed to read profile {}: {}", resolved.display(), err))?;
    serde_json::from_str::<StudioProfile>(&raw)
        .map_err(|err| format!("failed to parse profile {}: {}", resolved.display(), err))
}

#[tauri::command]
fn run_doctor(strict: bool) -> Result<CommandResult, String> {
    run_ghostlink_command(if strict {
        vec!["doctor", "--strict"]
    } else {
        vec!["doctor"]
    })
}

#[tauri::command]
fn run_doctor_with_json(strict: bool) -> Result<DoctorJsonSummary, String> {
    let root = repo_root();
    let output_path = root.join("tmp").join("studio-doctor-report.json");
    if let Some(parent) = output_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut args = vec!["doctor", "--json"];
    let output_path_str = output_path.display().to_string();
    args.push(output_path_str.as_str());
    if strict {
        args.push("--strict");
    }

    let command_result = run_ghostlink_command(args)?;
    let raw = fs::read_to_string(&output_path).map_err(|err| {
        format!(
            "doctor report missing at {}: {}",
            output_path.display(),
            err
        )
    })?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("invalid doctor json {}: {}", output_path.display(), err))?;

    let pass = value
        .get("summary")
        .and_then(|s| s.get("pass"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let warn = value
        .get("summary")
        .and_then(|s| s.get("warn"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let fail = value
        .get("summary")
        .and_then(|s| s.get("fail"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;

    let checks = value
        .get("checks")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(16)
                .map(|entry| DoctorCheckSummary {
                    area: entry
                        .get("area")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    name: entry
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unnamed")
                        .to_string(),
                    status: entry
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("UNKNOWN")
                        .to_string(),
                    detail: entry
                        .get("detail")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    fix: entry
                        .get("fix")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !command_result.ok && strict {
        return Err(format!(
            "doctor strict failed (exit code {:?})",
            command_result.exit_code
        ));
    }

    Ok(DoctorJsonSummary {
        path: output_path.display().to_string(),
        pass,
        warn,
        fail,
        checks,
    })
}

#[tauri::command]
fn quick_tcp_probe(host: String, port: u16, timeout_ms: u64) -> Result<TcpProbeResult, String> {
    let host_trimmed = host.trim();
    if host_trimmed.is_empty() {
        return Err("host cannot be empty".to_string());
    }
    if port == 0 {
        return Err("port must be > 0".to_string());
    }

    let timeout = Duration::from_millis(timeout_ms.clamp(50, 10_000));
    let address = format!("{}:{}", host_trimmed, port);
    let socket = address
        .to_socket_addrs()
        .map_err(|err| format!("failed to resolve {}: {}", address, err))?
        .next()
        .ok_or_else(|| format!("failed to resolve {}", address))?;

    let start = Instant::now();
    match TcpStream::connect_timeout(&socket, timeout) {
        Ok(_) => Ok(TcpProbeResult {
            host: host_trimmed.to_string(),
            port,
            reachable: true,
            latency_ms: Some(start.elapsed().as_millis()),
            error: None,
        }),
        Err(err) => Ok(TcpProbeResult {
            host: host_trimmed.to_string(),
            port,
            reachable: false,
            latency_ms: None,
            error: Some(err.to_string()),
        }),
    }
}

#[tauri::command]
fn run_probe(node_id: String, full: bool) -> Result<CommandResult, String> {
    run_ghostlink_command(if full {
        vec!["probe", node_id.as_str(), "full"]
    } else {
        vec!["probe", node_id.as_str(), "fast"]
    })
}

#[tauri::command]
fn run_flow_quick() -> Result<CommandResult, String> {
    run_ghostlink_command(vec![
        "flow",
        "studio-local",
        "studio-remote",
        "32",
        "32",
        "64",
        "2",
        "tcp",
    ])
}

#[tauri::command]
fn run_flow_between(
    local_id: String,
    remote_id: String,
    execution_tokens: u32,
    micro_batch: u32,
    transport: String,
) -> Result<CommandResult, String> {
    let local = sanitize_node_id(local_id.as_str(), "studio-local");
    let remote = sanitize_node_id(remote_id.as_str(), "studio-remote");
    let transport = sanitize_transport(transport.as_str());
    let tokens_arg = execution_tokens.clamp(16, 512).to_string();
    let micro_batch_arg = micro_batch.clamp(1, 16).to_string();

    run_ghostlink_command(vec![
        "flow",
        local.as_str(),
        remote.as_str(),
        "32",
        "32",
        tokens_arg.as_str(),
        micro_batch_arg.as_str(),
        transport.as_str(),
    ])
}

#[tauri::command]
fn run_cluster_start(node_count: usize, base_port: u16) -> Result<CommandResult, String> {
    run_ghostlink_command(vec![
        "cluster-start",
        &node_count.to_string(),
        &base_port.to_string(),
    ])
}

#[tauri::command]
fn verify_hf_repo(repo: String, file: Option<String>) -> Result<ModelVerifyResult, String> {
    let root = repo_root();
    let python = preferred_python();
    let resolved_file = file
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("config.json")
        .to_string();
    let output = Command::new(&python)
        .arg("scripts/verify_hf_models.py")
        .arg("--repo")
        .arg(repo.as_str())
        .arg("--file")
        .arg(resolved_file.as_str())
        .current_dir(&root)
        .output()
        .map_err(|err| format!("failed to execute verify_hf_models.py: {}", err))?;

    Ok(ModelVerifyResult {
        repo,
        file: resolved_file,
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn studio_backend_url() -> String {
    std::env::var("GHOSTLINK_BACKEND_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9999".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn backend_get_json(path: &str) -> Result<Value, String> {
    let backend_url = studio_backend_url();
    let target = format!("{}{}", backend_url, path);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| format!("failed to build backend HTTP client for {}: {}", path, err))?;
    let response = client
        .get(target.as_str())
        .send()
        .map_err(|err| format!("failed to query backend endpoint {}: {}", path, err))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|err| format!("failed to read backend response {}: {}", path, err))?;

    if !status.is_success() {
        let detail = if body.trim().is_empty() {
            format!("HTTP {}", status)
        } else {
            body.trim().to_string()
        };
        return Err(format!(
            "backend endpoint {} returned non-zero status: {}",
            path, detail
        ));
    }

    serde_json::from_str::<Value>(&body)
        .map_err(|err| format!("invalid JSON from backend endpoint {}: {}", path, err))
}

fn backend_post_json(path: &str, payload: &Value) -> Result<Value, String> {
    let backend_url = studio_backend_url();
    let target = format!("{}{}", backend_url, path);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| format!("failed to build backend HTTP client for {}: {}", path, err))?;
    let response = client
        .post(target.as_str())
        .json(payload)
        .send()
        .map_err(|err| format!("failed to post backend endpoint {}: {}", path, err))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|err| format!("failed to read backend response {}: {}", path, err))?;

    if !status.is_success() {
        let detail = if body.trim().is_empty() {
            format!("HTTP {}", status)
        } else {
            body.trim().to_string()
        };
        return Err(format!(
            "backend endpoint {} returned non-zero status: {}",
            path, detail
        ));
    }

    serde_json::from_str::<Value>(&body)
        .map_err(|err| format!("invalid JSON from backend endpoint {}: {}", path, err))
}

#[tauri::command]
fn list_backend_models() -> Result<Value, String> {
    backend_get_json("/api/models")
}

#[tauri::command]
fn ollama_health() -> Result<Value, String> {
    backend_get_json("/api/ollama/health")
}

#[tauri::command]
fn download_backend_model(model_id: String) -> Result<Value, String> {
    let normalized = model_id.trim().to_string();
    if normalized.is_empty() {
        return Err("model_id cannot be empty".to_string());
    }
    backend_post_json(
        "/api/models/download",
        &serde_json::json!({
            "model_id": normalized
        }),
    )
}

#[tauri::command]
fn load_backend_model(model: String) -> Result<Value, String> {
    let normalized = model.trim().to_string();
    if normalized.is_empty() {
        return Err("model cannot be empty".to_string());
    }
    backend_post_json(
        "/api/models/load",
        &serde_json::json!({
            "model": normalized
        }),
    )
}

#[tauri::command]
fn chat_infer(
    prompt: String,
    model: String,
    temperature: f32,
    max_tokens: u32,
    distributed: bool,
    ollama_url: Option<String>,
    ollama_model: Option<String>,
) -> Result<ChatResult, String> {
    let concise_prompt = prompt.trim();
    if concise_prompt.is_empty() {
        return Err("prompt cannot be empty".to_string());
    }

    let requested_model = model.trim();
    if requested_model.is_empty() {
        return Err("model cannot be empty".to_string());
    }

    let resolved_ollama_url = ollama_url.unwrap_or_else(|| {
        std::env::var("GHOSTLINK_OLLAMA_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string())
    });
    let resolved_ollama_model = ollama_model
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| requested_model.to_string());

    let payload = serde_json::json!({
        "message": concise_prompt,
        "model": requested_model,
        "ollama_model": resolved_ollama_model,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "ollama_url": resolved_ollama_url,
        "distributed": distributed,
    });

    let response = backend_post_json("/api/inference/chat", &payload)?;
    if let Some(error) = response.get("error").and_then(|value| value.as_str()) {
        return Err(error.to_string());
    }

    let response_text = response
        .get("response")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    let trace = format!(
        "POST /api/inference/chat\nprompt_len={} requested_max_tokens={} distributed={}\nrequest_id={} exec_tokens={} micro_batch={}",
        concise_prompt.len(),
        max_tokens,
        distributed,
        response.get("request_id").and_then(|value| value.as_str()).unwrap_or("n/a"),
        response.get("exec_tokens").and_then(|value| value.as_u64()).map(|value| value.to_string()).unwrap_or_else(|| "n/a".to_string()),
        response.get("exec_micro_batch").and_then(|value| value.as_u64()).map(|value| value.to_string()).unwrap_or_else(|| "n/a".to_string()),
    );

    Ok(ChatResult {
        backend: "http-backend-api".to_string(),
        model: response
            .get("model")
            .and_then(|value| value.as_str())
            .unwrap_or(requested_model)
            .to_string(),
        response: response_text,
        trace,
    })
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn run_ghostlink_command(args: Vec<&str>) -> Result<CommandResult, String> {
    let root = repo_root();
    let executable_name = if cfg!(windows) {
        "ghost-link.exe"
    } else {
        "ghost-link"
    };

    // Prefer direct binary execution to avoid `cargo run` rebuilds that can
    // fail on Windows when another ghost-link process already has the exe open.
    let binary_candidates = [
        root.join("target").join("release").join(executable_name),
        root.join("target").join("debug").join(executable_name),
    ];

    for binary in binary_candidates {
        if !binary.exists() {
            continue;
        }

        let output = Command::new(&binary)
            .args(&args)
            .current_dir(&root)
            .output()
            .map_err(|err| {
                format!(
                    "failed to execute ghost-link binary {}: {}",
                    binary.display(),
                    err
                )
            })?;

        return Ok(CommandResult {
            command: format!("{} {}", binary.display(), args.join(" ")),
            ok: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let rendered = format!("cargo run -p ghost-link -- {}", args.join(" "));
    let output = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("ghost-link")
        .arg("--")
        .args(args)
        .current_dir(&root)
        .output()
        .map_err(|err| format!("failed to execute ghost-link command: {}", err))?;

    Ok(CommandResult {
        command: rendered,
        ok: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn run_command_step(
    name: &str,
    program: &str,
    args: &[&str],
    root: &PathBuf,
) -> Result<ValidationStep, String> {
    let start = Instant::now();
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to execute {}: {}", name, err))?;

    Ok(ValidationStep {
        name: name.to_string(),
        ok: output.status.success(),
        exit_code: output.status.code(),
        duration_ms: start.elapsed().as_millis(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn preferred_python() -> String {
    if command_version("python3", &["--version"]).is_some() {
        "python3".to_string()
    } else {
        "python".to_string()
    }
}

fn command_version(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return Some(stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return Some(stderr);
    }

    None
}

fn parse_probe_to_node(output: &str) -> Option<ClusterNodeCard> {
    let mut id = None;
    let mut workers = None;
    let mut system_memory_gb = None;
    let mut gpu_vram_gb = None;
    let mut acceleration = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Node ID:") {
            id = Some(value.trim().to_string());
        } else if let Some(value) = trimmed.strip_prefix("Recommended workers:") {
            workers = value.trim().parse::<usize>().ok();
        } else if let Some(value) = trimmed.strip_prefix("System memory:") {
            system_memory_gb = value.split_whitespace().next()?.parse::<f32>().ok();
        } else if let Some(value) = trimmed.strip_prefix("GPU VRAM:") {
            gpu_vram_gb = value.split_whitespace().next()?.parse::<f32>().ok();
        } else if let Some(value) = trimmed.strip_prefix("Acceleration:") {
            acceleration = Some(value.trim().to_string());
        }
    }

    let node_id = id?;
    let workers = workers.unwrap_or(1);
    let memory = system_memory_gb.unwrap_or(0.0);
    let vram = gpu_vram_gb.unwrap_or(0.0);
    let acceleration = acceleration.unwrap_or_else(|| "unknown".to_string());

    Some(ClusterNodeCard {
        id: node_id,
        acceleration,
        workers,
        system_memory_gb: memory,
        gpu_vram_gb: vram,
        health: if vram > 0.0 { "healthy" } else { "degraded" }.to_string(),
    })
}

fn merge_worker_discovery_ids(mut requested: Vec<String>) -> Vec<String> {
    requested.extend(node_ids_from_config());
    requested.push("studio-local".to_string());
    requested.push("studio-remote".to_string());
    requested.push("local-node".to_string());

    let mut merged = Vec::new();
    for candidate in requested {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        if merged.iter().any(|known| known == trimmed) {
            continue;
        }
        merged.push(trimmed.to_string());
    }
    merged
}

fn node_ids_from_config() -> Vec<String> {
    let defaults = parse_flow_config_defaults();
    let mut ids = Vec::new();
    if let Some(flow) = defaults {
        if !flow.local_id.trim().is_empty() {
            ids.push(flow.local_id);
        }
        if !flow.remote_id.trim().is_empty() {
            ids.push(flow.remote_id);
        }
    }
    ids
}

fn parse_flow_config_defaults() -> Option<FlowDefaults> {
    let root = repo_root();
    let local = root.join("ghostlink.toml");
    let example = root.join("ghostlink.example.toml");
    let source = if local.exists() { local } else { example };
    let raw = fs::read_to_string(source).ok()?;

    let mut in_flow = false;
    let mut local_id = None;
    let mut remote_id = None;
    let mut execution_tokens = None;
    let mut micro_batch = None;
    let mut transport = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_flow = trimmed == "[flow]";
            continue;
        }

        if !in_flow {
            continue;
        }

        if let Some(value) = parse_toml_string_value(trimmed, "local_id") {
            local_id = Some(value);
        } else if let Some(value) = parse_toml_string_value(trimmed, "remote_id") {
            remote_id = Some(value);
        } else if let Some(value) = parse_toml_u32_value(trimmed, "execution_tokens") {
            execution_tokens = Some(value);
        } else if let Some(value) = parse_toml_u32_value(trimmed, "micro_batch") {
            micro_batch = Some(value);
        } else if let Some(value) = parse_toml_string_value(trimmed, "transport") {
            transport = Some(value);
        }
    }

    Some(FlowDefaults {
        local_id: local_id.unwrap_or_else(|| "studio-local".to_string()),
        remote_id: remote_id.unwrap_or_else(|| "studio-remote".to_string()),
        execution_tokens: execution_tokens.unwrap_or(64),
        micro_batch: micro_batch.unwrap_or(2),
        transport: transport.unwrap_or_else(|| "tcp".to_string()),
    })
}

fn parse_toml_string_value(line: &str, key: &str) -> Option<String> {
    let mut parts = line.splitn(2, '=');
    let lhs = parts.next()?.trim();
    if lhs != key {
        return None;
    }
    let rhs = parts.next()?.trim();
    if !rhs.starts_with('"') {
        return None;
    }
    let value = rhs.trim_matches('"').trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_toml_u32_value(line: &str, key: &str) -> Option<u32> {
    let mut parts = line.splitn(2, '=');
    let lhs = parts.next()?.trim();
    if lhs != key {
        return None;
    }
    let rhs = parts.next()?.trim();
    rhs.parse::<u32>().ok()
}

fn sanitize_node_id(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn sanitize_transport(value: &str) -> String {
    let candidate = value.trim().to_ascii_lowercase();
    match candidate.as_str() {
        "tcp" | "inmem" | "ibverbs" | "ucx" => candidate,
        _ => "tcp".to_string(),
    }
}

fn sanitize_profile_name(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "studio-profile".to_string();
    }

    let cleaned = trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    if cleaned.is_empty() {
        "studio-profile".to_string()
    } else {
        cleaned
    }
}
