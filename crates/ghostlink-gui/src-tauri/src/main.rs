#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;

#[derive(Serialize)]
struct StudioStatus {
    app: &'static str,
    phase: &'static str,
    status: &'static str,
}

#[tauri::command]
fn studio_status() -> StudioStatus {
    StudioStatus {
        app: "Ghostlink Studio",
        phase: "Sprint 1",
        status: "scaffold-ready",
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![studio_status])
        .run(tauri::generate_context!())
        .expect("error while running Ghostlink Studio");
}
