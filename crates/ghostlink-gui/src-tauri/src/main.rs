#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
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
            run_doctor,
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

#[tauri::command]
fn run_doctor(strict: bool) -> Result<CommandResult, String> {
    run_ghostlink_command(if strict {
        vec!["doctor", "--strict"]
    } else {
        vec!["doctor"]
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
