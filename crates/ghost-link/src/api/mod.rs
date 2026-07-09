//! Ghostlink API Server Handlers
//!
//! This module contains all HTTP endpoint handlers for the Ghostlink backend.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::runtime::{RuntimeDetector, ModelRegistry};
use crate::cluster::ClusterState;

/// API State shared across handlers
#[derive(Clone)]
pub struct ApiState {
    pub cluster: Arc<ClusterState>,
}

impl Default for ApiState {
    fn default() -> Self {
        Self {
            cluster: Arc::new(ClusterState::new()),
        }
    }
}

/// Runtime detection endpoint
pub async fn detect_runtime(
    _state: State<ApiState>,
) -> Result<Json<RuntimeResponse>, StatusCode> {
    let runtimes = RuntimeDetector::detect();
    let primary = RuntimeDetector::detect_primary();
    
    let response = RuntimeResponse {
        available_runtimes: runtimes,
        primary_runtime: format!("{}", primary),
    };
    
    Ok(Json(response))
}

/// Models listing endpoint
pub async fn list_models(
    state: State<ApiState>,
    runtime: Option<String>,
) -> Result<Json<ModelResponse>, StatusCode> {
    let runtime_enum = match runtime.as_deref() {
        Some("cpu") => crate::runtime::Runtime::CPU,
        Some("cuda") => crate::runtime::Runtime::CUDA,
        Some("metal") => crate::runtime::Runtime::Metal,
        Some("rocm") => crate::runtime::Runtime::ROCm,
        Some("npu") => crate::runtime::Runtime::NPU,
        _ => crate::runtime::Runtime::CPU,
    };
    
    let models = ModelRegistry::models_for_runtime(runtime_enum);
    
    let response = ModelResponse {
        runtime: format!("{}", runtime_enum),
        model_count: models.len(),
        models,
        best_model: ModelRegistry::best_for_runtime(runtime_enum),
    };
    
    Ok(Json(response))
}

/// Recommendations endpoint
pub async fn recommend_models(
    state: State<ApiState>,
    memory_gb: f32,
) -> Result<Json<RecommendationResponse>, StatusCode> {
    let runtime = RuntimeDetector::detect_primary();
    let models = ModelRegistry::recommend_models(runtime, memory_gb);
    
    let response = RecommendationResponse {
        detected_runtime: format!("{}", runtime),
        available_memory_gb: memory_gb,
        recommended_models: models,
    };
    
    Ok(Json(response))
}

/// Chat completion endpoint (OpenAI-compatible)
pub async fn chat_completion(
    _state: State<ApiState>,
    json: Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, StatusCode> {
    // Connect to Ollama or use mock responses
    let model_name = json.model.clone();
    let messages = json.messages;
    
    // TODO: Implement actual Ollama connection
    // For now, return a mock response
    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs(),
        model: model_name,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: "Hello! This is a mock response. To enable real inference, connect to Ollama.".to_string(),
            },
            finish_reason: Some("stop".to_string()),
        }],
    };
    
    Ok(Json(response))
}

#[derive(Serialize)]
pub struct RuntimeResponse {
    pub available_runtimes: Vec<RuntimeInfo>,
    pub primary_runtime: String,
}

#[derive(Serialize)]
pub struct RuntimeInfo {
    pub runtime: String,
    pub available: bool,
    pub device_count: usize,
    pub memory_gb: f32,
}

#[derive(Serialize)]
pub struct ModelResponse {
    pub runtime: String,
    pub model_count: usize,
    pub models: Vec<ModelInfo>,
    pub best_model: Option<ModelInfo>,
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub parameters: String,
    pub size_gb: f32,
    pub memory_required_gb: f32,
    pub quality_tier: String,
    pub inference_speed: String,
}

#[derive(Serialize)]
pub struct RecommendationResponse {
    pub detected_runtime: String,
    pub available_memory_gb: f32,
    pub recommended_models: Vec<ModelInfo>,
}

#[derive(Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
}

#[derive(Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
}

#[derive(Serialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}
