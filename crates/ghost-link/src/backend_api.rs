// Phase 2: Backend API Handlers
// These handlers provide REST endpoints for GPU/CPU backend management and switching

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::backend_config::ConfigManager;
use crate::backend_registry::ComputeBackend;
use crate::{active_backend_registry, active_runtime_switcher};

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
    let registry = active_backend_registry();
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
    let registry = active_backend_registry();
    let switcher = active_runtime_switcher();
    let config_manager = ConfigManager::new("ghostlink.toml");
    let previous_backend = registry.current_backend();

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

    // Switch to the backend with request draining and env updates
    match switcher.switch_backend(&registry, backend.clone()).await {
        Ok(result) => {
            if let Err(err) = config_manager.save_preferred_backend(backend.clone()) {
                let _ = switcher.rollback_backend(&registry, previous_backend).await;
                let response = Json(serde_json::json!({
                    "status": "error",
                    "backend": payload.backend,
                    "message": format!("switched backend but failed to persist preference: {}", err),
                    "restart_required": false
                }));
                return (StatusCode::INTERNAL_SERVER_ERROR, response).into_response();
            }

            let response = SwitchBackendResponse {
                status: "success".to_string(),
                backend: backend.as_str().to_string(),
                message: result.message,
                restart_required: result.restart_required,
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
    let registry = active_backend_registry();

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
    use axum::body::to_bytes;
    use axum::extract::Path;
    use axum::Json;
    use serde_json::Value;

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

    #[tokio::test]
    async fn test_list_backends_endpoint() {
        let response = handle_list_backends().await;
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("available").and_then(|v| v.as_array()).is_some());
        assert!(json.get("current").and_then(|v| v.as_str()).is_some());
        assert!(json
            .get("available")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| { entry.get("name").and_then(|v| v.as_str()) == Some("cpu") }));
    }

    #[tokio::test]
    async fn test_switch_backend_endpoint_accepts_cpu() {
        let response = handle_switch_backend(Json(SwitchBackendRequest {
            backend: "cpu".to_string(),
        }))
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.get("status").and_then(|v| v.as_str()), Some("success"));
        assert_eq!(json.get("backend").and_then(|v| v.as_str()), Some("cpu"));
    }

    #[tokio::test]
    async fn test_backend_status_endpoint_for_cpu() {
        let response = handle_backend_status(Path("cpu".to_string())).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.get("name").and_then(|v| v.as_str()), Some("cpu"));
        assert_eq!(json.get("health").and_then(|v| v.as_str()), Some("healthy"));
        assert!(json.get("status").and_then(|v| v.as_str()).is_some());
    }
}
