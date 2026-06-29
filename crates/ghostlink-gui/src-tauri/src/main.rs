#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

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
            list_model_presets,
            run_validation_tier,
            load_ghostlink_config,
            save_ghostlink_config,
            export_studio_profile,
            import_studio_profile,
            run_doctor,
            run_doctor_with_json,
            run_probe,
            run_flow_quick,
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
    config_content: String,
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

    let nodes = vec![parse_probe_to_node(command.stdout.as_str())
        .unwrap_or_else(|| fallback_node(node_id.as_str()))];
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
    config_content: String,
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
        config_content,
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
fn run_cluster_start(node_count: usize, base_port: u16) -> Result<CommandResult, String> {
    run_ghostlink_command(vec![
        "cluster-start",
        &node_count.to_string(),
        &base_port.to_string(),
    ])
}

#[tauri::command]
fn verify_hf_repo(repo: String, file: String) -> Result<ModelVerifyResult, String> {
    let root = repo_root();
    let python = preferred_python();
    let output = Command::new(&python)
        .arg("scripts/verify_hf_models.py")
        .arg("--repo")
        .arg(repo.as_str())
        .arg("--file")
        .arg(file.as_str())
        .current_dir(&root)
        .output()
        .map_err(|err| format!("failed to execute verify_hf_models.py: {}", err))?;

    Ok(ModelVerifyResult {
        repo,
        file,
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

#[tauri::command]
fn chat_infer(
    prompt: String,
    model: String,
    temperature: f32,
    max_tokens: u32,
    distributed: bool,
) -> Result<ChatResult, String> {
    let concise_prompt = prompt.trim();
    if concise_prompt.is_empty() {
        return Err("prompt cannot be empty".to_string());
    }

    let backend = if distributed {
        "distributed-flow"
    } else {
        "single-node-flow"
    };
    let transport = if distributed { "tcp" } else { "inmem" };
    let execution_tokens = max_tokens.clamp(16, 512);
    let execution_tokens_arg = execution_tokens.to_string();

    let command_result = run_ghostlink_command(vec![
        "flow",
        "studio-local",
        "studio-remote",
        "32",
        "32",
        execution_tokens_arg.as_str(),
        "1",
        transport,
    ])?;

    if !command_result.ok {
        let stderr = command_result.stderr.trim();
        let stdout = command_result.stdout.trim();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!(
            "live flow execution failed (exit code {:?}): {}",
            command_result.exit_code, detail
        ));
    }

    let throughput = extract_metric(command_result.stdout.as_str(), "Throughput:", "tokens/sec")
        .unwrap_or_else(|| "unknown".to_string());
    let avg_latency = extract_metric(command_result.stdout.as_str(), "Avg token latency:", "ms")
        .unwrap_or_else(|| "unknown".to_string());
    let p95_latency = extract_metric(command_result.stdout.as_str(), "P95:", "ms")
        .unwrap_or_else(|| "unknown".to_string());

    let response = format!(
        "Live runtime completed for prompt '{}' using model '{}'. Backend={} transport={} temp={:.2}. Metrics: throughput={} tokens/sec, avg_latency={} ms, p95={} ms.",
        concise_prompt,
        model,
        backend,
        transport,
        temperature,
        throughput,
        avg_latency,
        p95_latency
    );

    let trace = format!(
        "{}\n\n{}\n{}",
        command_result.command,
        format!(
            "prompt_len={} requested_max_tokens={} execution_tokens={} distributed={}",
            concise_prompt.len(),
            max_tokens,
            execution_tokens,
            distributed
        ),
        command_result.stdout.trim()
    );

    Ok(ChatResult {
        backend: backend.to_string(),
        model,
        response,
        trace,
    })
}

fn extract_metric(output: &str, prefix: &str, suffix: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| line.contains(prefix))?;
    let marker_idx = line.find(prefix)?;
    let raw = line[(marker_idx + prefix.len())..].trim();
    let value = if suffix.is_empty() {
        raw
    } else {
        raw.split(suffix).next()?.trim()
    };
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn run_ghostlink_command(args: Vec<&str>) -> Result<CommandResult, String> {
    let root = repo_root();
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

fn fallback_node(node_id: &str) -> ClusterNodeCard {
    ClusterNodeCard {
        id: node_id.to_string(),
        acceleration: "unknown".to_string(),
        workers: 1,
        system_memory_gb: 0.0,
        gpu_vram_gb: 0.0,
        health: "degraded".to_string(),
    }
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
            system_memory_gb = value.trim().split_whitespace().next()?.parse::<f32>().ok();
        } else if let Some(value) = trimmed.strip_prefix("GPU VRAM:") {
            gpu_vram_gb = value.trim().split_whitespace().next()?.parse::<f32>().ok();
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
