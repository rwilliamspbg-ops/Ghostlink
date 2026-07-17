// Phase 2: Backend API Handlers
// These handlers provide REST endpoints for GPU/CPU backend management and switching

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::backend_registry::{BackendRegistry, ComputeBackend};

/// Response for available backends query
#[derive(Debug, Serialize, Deserialize)]
pub struct BackendListResponse {
    pub available: Vec<BackendInfoResponse>,
    pub current: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackendInfoResponse {
    pub name: String,
    pub device_name: String,
    pub vram_gb: Option<f32>,
    pub compute_capability: String,
    pub driver_version: String,
    pub status: String, // "active" or "ready"
}

/// Request to switch backends
#[derive(Debug, Deserialize)]
pub struct SwitchBackendRequest {
    pub backend: String,
}

/// Response for backend switch operation
#[derive(Debug, Serialize)]
pub struct SwitchBackendResponse {
    pub status: String,
    pub backend: String,
    pub message: String,
    pub restart_required: bool,
}

/// Response for backend status
#[derive(Debug, Serialize)]
pub struct BackendStatusResponse {
    pub name: String,
    pub device_name: String,
    pub vram_gb: Option<f32>,
    pub status: String,
    pub health: String,
    pub utilization: Option<f32>,
    pub temperature: Option<f32>,
}

/// API Handler: GET /api/backends - List all available backends
pub async fn handle_list_backends() -> Response {
    let registry = BackendRegistry::discover();
    let backends = registry.available_backends();
    let current = registry.current_backend();

    let available: Vec<BackendInfoResponse> = backends
        .iter()
        .map(|info| {
            let current_name = current.as_str();
            let is_active = info.backend.as_str() == current_name;

            BackendInfoResponse {
                name: info.backend.as_str().to_string(),
                device_name: info.device_name.clone(),
                vram_gb: info.vram_gb,
                compute_capability: info.compute_capability.clone(),
                driver_version: info.driver_version.clone(),
                status: if is_active { "active" } else { "ready" }.to_string(),
            }
        })
        .collect();

    let response = BackendListResponse {
        available,
        current: current.as_str().to_string(),
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// API Handler: POST /api/backends/switch - Switch to a different backend
pub async fn handle_switch_backend(Json(payload): Json<SwitchBackendRequest>) -> Response {
    let registry = BackendRegistry::discover();

    // Parse the backend name
    let backend = match ComputeBackend::from_str(&payload.backend) {
        Some(b) => b,
        None => {
            let response = Json(serde_json::json!({
                "status": "error",
                "backend": payload.backend,
                "message": format!("Unknown backend: {}", payload.backend),
                "restart_required": false
            }));
            return (StatusCode::BAD_REQUEST, response).into_response();
        }
    };

    // Check if backend is available
    if registry.get_backend(&backend).is_none() {
        let response = Json(serde_json::json!({
            "status": "error",
            "backend": payload.backend,
            "message": format!("Backend {} is not available", payload.backend),
            "restart_required": false
        }));
        return (StatusCode::NOT_FOUND, response).into_response();
    }

    // Switch to the backend
    match registry.switch_backend(backend.clone()) {
        Ok(_) => {
            let response = SwitchBackendResponse {
                status: "success".to_string(),
                backend: backend.as_str().to_string(),
                message: format!("Switched to {} backend", backend.as_str()),
                restart_required: false, // TODO: Set based on actual need
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(err) => {
            let response = Json(serde_json::json!({
                "status": "error",
                "backend": payload.backend,
                "message": err,
                "restart_required": false
            }));
            (StatusCode::INTERNAL_SERVER_ERROR, response).into_response()
        }
    }
}

/// API Handler: GET /api/backends/:name/status - Get backend status
pub async fn handle_backend_status(Path(name): Path<String>) -> Response {
    let registry = BackendRegistry::discover();

    // Parse the backend name
    let backend = match ComputeBackend::from_str(&name) {
        Some(b) => b,
        None => {
            let response = Json(serde_json::json!({
                "error": format!("Unknown backend: {}", name)
            }));
            return (StatusCode::BAD_REQUEST, response).into_response();
        }
    };

    // Get backend status
    match registry.get_status(&backend) {
        Some(status) => {
            let response = BackendStatusResponse {
                name: backend.as_str().to_string(),
                device_name: status.device_name,
                vram_gb: status.vram_gb,
                status: status.status,
                health: status.health,
                utilization: status.utilization,
                temperature: status.temperature,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        None => {
            let response = Json(serde_json::json!({
                "error": format!("Backend {} status unavailable", name)
            }));
            (StatusCode::NOT_FOUND, response).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_list_response_serialization() {
        let response = BackendListResponse {
            available: vec![BackendInfoResponse {
                name: "rocm".to_string(),
                device_name: "AMD Radeon 860M".to_string(),
                vram_gb: Some(14.2),
                compute_capability: "gfx906".to_string(),
                driver_version: "ROCm 6.1".to_string(),
                status: "active".to_string(),
            }],
            current: "rocm".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("rocm"));
        assert!(json.contains("14.2"));
    }

    #[test]
    fn test_switch_backend_request_deserialization() {
        let json = r#"{"backend": "cpu"}"#;
        let req: SwitchBackendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.backend, "cpu");
    }

    #[test]
    fn test_backend_status_response_serialization() {
        let response = BackendStatusResponse {
            name: "rocm".to_string(),
            device_name: "AMD Radeon 860M".to_string(),
            vram_gb: Some(14.2),
            status: "active".to_string(),
            health: "healthy".to_string(),
            utilization: Some(25.5),
            temperature: Some(45.0),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("rocm"));
        assert!(json.contains("healthy"));
        assert!(json.contains("25.5"));
    }
}
