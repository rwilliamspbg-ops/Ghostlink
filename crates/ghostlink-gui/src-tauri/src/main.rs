#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
            load_ghostlink_config,
            save_ghostlink_config,
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
        summary: format!(
            "{} checks passed, {} checks need attention",
            passed, warn
        ),
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

    let local = parse_probe_to_node(command.stdout.as_str())
        .unwrap_or_else(|| fallback_node(node_id.as_str()));

    // Lightweight preview peer node to visualize placement/health at cluster scale.
    let peer = ClusterNodeCard {
        id: format!("{}-peer", local.id),
        acceleration: "GPU".to_string(),
        workers: local.workers.max(2),
        system_memory_gb: (local.system_memory_gb + 8.0).max(16.0),
        gpu_vram_gb: local.gpu_vram_gb.max(16.0),
        health: if local.gpu_vram_gb > 0.0 {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
    };

    let nodes = vec![local, peer];
    let healthy = nodes.iter().filter(|node| node.health == "healthy").count();
    let degraded = nodes.len().saturating_sub(healthy);

    Ok(ClusterPreview {
        summary: format!(
            "{} nodes total ({} healthy, {} degraded)",
            nodes.len(), healthy, degraded
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
    let raw = fs::read_to_string(&output_path)
        .map_err(|err| format!("doctor report missing at {}: {}", output_path.display(), err))?;
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
    run_ghostlink_command(vec!["flow", "studio-local", "studio-remote", "32", "32", "64", "2", "tcp"])
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
    let backend = if distributed {
        "distributed-flow"
    } else {
        "single-node"
    };

    let concise_prompt = prompt.trim();
    if concise_prompt.is_empty() {
        return Err("prompt cannot be empty".to_string());
    }

    let style_hint = if temperature >= 0.9 {
        "creative"
    } else if temperature >= 0.6 {
        "balanced"
    } else {
        "deterministic"
    };

    let response = format!(
        "Model {} ({}) suggests: '{}' -> plan a {} response under {} tokens. This Studio preview uses Ghost-Link orchestration signals and will be replaced with live streaming generation in the next integration slice.",
        model,
        backend,
        concise_prompt,
        style_hint,
        max_tokens
    );

    let trace = format!(
        "backend={} temperature={:.2} max_tokens={} distributed={} prompt_len={}",
        backend,
        temperature,
        max_tokens,
        distributed,
        concise_prompt.len()
    );

    Ok(ChatResult {
        backend: backend.to_string(),
        model,
        response,
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
