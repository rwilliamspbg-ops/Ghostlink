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
            load_ghostlink_config,
            save_ghostlink_config,
            run_doctor,
            run_doctor_with_json,
            run_probe,
            run_flow_quick,
            run_cluster_start
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
