//! Ollama inference client for real model execution
#![allow(dead_code)]
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::error::Error;
use std::io;
use std::pin::Pin;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct OllamaClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaResponse {
    pub response: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub digest: String,
    pub modified_at: String,
    #[serde(default)]
    pub details: Option<ModelDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDetails {
    pub format: String,
    pub family: String,
    pub families: Option<Vec<String>>,
    pub parameter_size: String,
    pub quantization_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelsResponse {
    pub models: Vec<OllamaModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Only ever populated by Ollama on an incoming response (when the model
    /// natively supports tool-calling and decides to call one) — never set on
    /// an outgoing message we build ourselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaOptions>,
    /// OpenAI-style function-tool definitions:
    /// `{"type": "function", "function": {"name", "description", "parameters"}}`.
    /// Only sent to models that declare tool-calling support in their template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
}

/// Sampling parameters accepted by Ollama's `/api/generate` and `/api/chat`
/// endpoints. Ollama only reads these from a nested `options` object in the
/// request body -- top-level fields with the same names are silently
/// ignored by the server, so this must not be flattened.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: ChatMessage,
    pub done: bool,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub load_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,
    #[serde(default)]
    pub prompt_eval_duration: Option<u64>,
    #[serde(default)]
    pub eval_count: Option<u32>,
    #[serde(default)]
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullProgressResponse {
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfoResponse {
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub modelfile: String,
    /// Older Ollama versions returned a string; newer builds often omit it.
    #[serde(default)]
    pub parameters: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub details: Option<ModelDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelRequest {
    pub name: String,
    pub modelfile: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyModelRequest {
    pub source: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteModelRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embedding: Vec<f32>,
}

pub type OllamaStream =
    Pin<Box<dyn Stream<Item = Result<String, Box<dyn Error + Send + Sync>>> + Send>>;

impl OllamaClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    fn matches_model_name(candidate: &str, requested: &str) -> bool {
        candidate == requested || candidate.eq_ignore_ascii_case(requested)
    }

    /// Check if Ollama is reachable
    pub async fn health(&self) -> Result<bool, Box<dyn Error>> {
        match self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
        {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// List available models in Ollama
    pub async fn list_models(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;

        let data: OllamaModelsResponse = resp.json().await?;
        let models = data.models.iter().map(|m| m.name.clone()).collect();
        Ok(models)
    }

    /// List available models with full details
    pub async fn list_models_detailed(&self) -> Result<Vec<OllamaModel>, Box<dyn Error>> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;

        let data: OllamaModelsResponse = resp.json().await?;
        Ok(data.models)
    }

    /// Generate text using Ollama (non-streaming)
    #[allow(clippy::too_many_arguments)]
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        temperature: f32,
        top_p: f32,
        top_k: usize,
        repeat_penalty: f32,
        max_tokens: usize,
    ) -> Result<String, Box<dyn Error>> {
        let payload = json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": temperature.clamp(0.0, 2.0),
                "top_p": top_p.clamp(0.0, 1.0),
                "top_k": top_k.clamp(1, 200),
                "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
                "num_predict": max_tokens,
            }
        });

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama generate failed: HTTP {status} - {body}"
            ))));
        }

        let data: OllamaResponse = resp.json().await?;
        Ok(data.response)
    }

    /// Generate text using Ollama (streaming)
    #[allow(clippy::too_many_arguments)]
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        temperature: f32,
        top_p: f32,
        top_k: usize,
        repeat_penalty: f32,
        max_tokens: usize,
    ) -> Result<OllamaStream, Box<dyn Error>> {
        let payload = json!({
            "model": model,
            "prompt": prompt,
            "stream": true,
            "options": {
                "temperature": temperature.clamp(0.0, 2.0),
                "top_p": top_p.clamp(0.0, 1.0),
                "top_k": top_k.clamp(1, 200),
                "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
                "num_predict": max_tokens,
            }
        });

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&payload)
            .send()
            .await?;

        // Without this check, an HTTP error response (model not found, bad
        // request, etc.) flows into `stream_response`, which only forwards
        // lines that parse as `{"response": ..., "done": ...}`. An error
        // body like `{"error": "..."}` matches neither shape, so it was
        // silently dropped and the caller got back an empty-but-Ok stream
        // instead of any indication the request failed.
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama generate failed: HTTP {status} - {body}"
            ))));
        }

        let stream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(
            Self::stream_response(resp).await?,
        ));

        Ok(stream)
    }

    async fn stream_response(
        resp: reqwest::Response,
    ) -> Result<mpsc::Receiver<Result<String, Box<dyn Error + Send + Sync>>>, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel(100);

        let body = resp.bytes().await?;

        tokio::spawn(async move {
            let text = String::from_utf8_lossy(&body);
            for line in text.lines() {
                if let Some(json_str) = line.strip_prefix("data: ") {
                    if let Ok(data) = serde_json::from_str::<Value>(json_str) {
                        if let Some(response) = data.get("response").and_then(|v| v.as_str()) {
                            let _ = tx.send(Ok(response.to_string())).await;
                        }
                        if data.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                            return;
                        }
                    }
                } else if let Ok(data) = serde_json::from_str::<Value>(line) {
                    if let Some(response) = data.get("response").and_then(|v| v.as_str()) {
                        let _ = tx.send(Ok(response.to_string())).await;
                    }
                    if data.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Chat with Ollama (non-streaming)
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: Option<f32>,
        top_p: Option<f32>,
        top_k: Option<usize>,
        repeat_penalty: Option<f32>,
        max_tokens: Option<usize>,
        tools: Option<Vec<Value>>,
    ) -> Result<ChatResponse, Box<dyn Error>> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: Some(false),
            options: Some(OllamaOptions {
                temperature,
                top_p,
                top_k,
                repeat_penalty,
                num_predict: max_tokens,
            }),
            tools,
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama chat failed: HTTP {status} - {body}"
            ))));
        }

        let data: ChatResponse = resp.json().await?;
        Ok(data)
    }

    /// Chat with Ollama (streaming)
    #[allow(clippy::too_many_arguments)]
    pub async fn chat_stream(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: Option<f32>,
        top_p: Option<f32>,
        top_k: Option<usize>,
        repeat_penalty: Option<f32>,
        max_tokens: Option<usize>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ChatResponse, Box<dyn Error + Send + Sync>>> + Send>>,
        Box<dyn Error>,
    > {
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: Some(true),
            options: Some(OllamaOptions {
                temperature,
                top_p,
                top_k,
                repeat_penalty,
                num_predict: max_tokens,
            }),
            tools: None,
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        // Same class of bug as `generate_stream`: an HTTP error body doesn't
        // deserialize as `ChatResponse` (which requires `model`/`created_at`/
        // `message`/`done`), so without this check it was silently dropped
        // by `stream_chat_response` and the caller saw an empty Ok stream.
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama chat failed: HTTP {status} - {body}"
            ))));
        }

        let stream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(
            Self::stream_chat_response(resp).await?,
        ));

        Ok(stream)
    }

    async fn stream_chat_response(
        resp: reqwest::Response,
    ) -> Result<mpsc::Receiver<Result<ChatResponse, Box<dyn Error + Send + Sync>>>, Box<dyn Error>>
    {
        let (tx, rx) = mpsc::channel(100);

        let body = resp.bytes().await?;

        tokio::spawn(async move {
            let text = String::from_utf8_lossy(&body);
            for line in text.lines() {
                if let Some(json_str) = line.strip_prefix("data: ") {
                    if let Ok(data) = serde_json::from_str::<ChatResponse>(json_str) {
                        let done = data.done;
                        let _ = tx.send(Ok(data)).await;
                        if done {
                            return;
                        }
                    }
                } else if let Ok(data) = serde_json::from_str::<ChatResponse>(line) {
                    let done = data.done;
                    let _ = tx.send(Ok(data)).await;
                    if done {
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Pull a model from Ollama registry (non-streaming)
    pub async fn pull_model(&self, model_name: &str) -> Result<String, Box<dyn Error>> {
        let payload = json!({
            "name": model_name,
            "stream": false,
        });

        let resp = self
            .client
            .post(format!("{}/api/pull", self.base_url))
            .json(&payload)
            .send()
            .await?;

        // Previously this returned `Ok(text)` for any response whose body
        // could be read as text, including 4xx/5xx error responses — a
        // failed pull (unknown model, registry unreachable, etc.) was
        // reported to the caller as a successful result.
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama pull failed: HTTP {status} - {body}"
            ))));
        }

        match resp.text().await {
            Ok(text) => Ok(text),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Pull a model from Ollama registry with progress streaming
    pub async fn pull_model_stream(
        &self,
        model_name: &str,
    ) -> Result<
        Pin<
            Box<
                dyn Stream<Item = Result<PullProgressResponse, Box<dyn Error + Send + Sync>>>
                    + Send,
            >,
        >,
        Box<dyn Error>,
    > {
        let payload = json!({
            "name": model_name,
            "stream": true,
        });

        let resp = self
            .client
            .post(format!("{}/api/pull", self.base_url))
            .json(&payload)
            .send()
            .await?;

        // Same class of bug as `generate_stream`/`chat_stream`: an error body
        // (e.g. unknown model name) doesn't parse as `PullProgressResponse`
        // (requires a `status` field), so it was silently dropped and the
        // caller saw an empty Ok stream instead of the pull failure.
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama pull failed: HTTP {status} - {body}"
            ))));
        }

        let stream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(
            Self::stream_pull_progress(resp).await?,
        ));

        Ok(stream)
    }

    async fn stream_pull_progress(
        resp: reqwest::Response,
    ) -> Result<
        mpsc::Receiver<Result<PullProgressResponse, Box<dyn Error + Send + Sync>>>,
        Box<dyn Error>,
    > {
        let (tx, rx) = mpsc::channel(100);

        let body = resp.bytes().await?;

        tokio::spawn(async move {
            let text = String::from_utf8_lossy(&body);
            for line in text.lines() {
                if let Ok(data) = serde_json::from_str::<PullProgressResponse>(line) {
                    let status = data.status.clone();
                    let _ = tx.send(Ok(data)).await;
                    if status == "success" || status == "error" {
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Show model information
    pub async fn show_model(&self, model_name: &str) -> Result<ModelInfoResponse, Box<dyn Error>> {
        let payload = json!({
            "name": model_name,
        });

        let resp = self
            .client
            .post(format!("{}/api/show", self.base_url))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama show failed: HTTP {status} - {body}"
            ))));
        }

        // Tolerate schema drift across Ollama versions (missing parameters/template, etc.).
        let body = resp.text().await?;
        match serde_json::from_str::<ModelInfoResponse>(&body) {
            Ok(data) => Ok(data),
            Err(primary) => {
                // Minimal success: model exists if /api/show returned JSON object.
                if serde_json::from_str::<Value>(&body).is_ok() {
                    Ok(ModelInfoResponse {
                        license: String::new(),
                        modelfile: String::new(),
                        parameters: String::new(),
                        template: String::new(),
                        details: None,
                    })
                } else {
                    Err(Box::new(io::Error::other(format!(
                        "Ollama show decode failed: {primary}; body={}",
                        body.chars().take(200).collect::<String>()
                    ))))
                }
            }
        }
    }

    /// Unload a model by setting keep_alive to zero.
    pub async fn unload_model(&self, model_name: &str) -> Result<String, Box<dyn Error>> {
        // Propagate failures from `list_running` instead of treating them as
        // "nothing is running": if Ollama is unreachable or errors here, we
        // genuinely don't know the model's state, and silently reporting
        // "unload skipped" as success would hide a real connectivity failure
        // from the caller.
        let running_models = self.list_running().await?;
        if !running_models
            .iter()
            .any(|model| Self::matches_model_name(&model.name, model_name))
        {
            return Ok(format!(
                "model '{model_name}' is not running; unload skipped"
            ));
        }

        let payload = json!({
            "model": model_name,
            "prompt": "",
            "stream": false,
            "keep_alive": 0,
        });

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama unload failed: HTTP {status} - {body}"
            ))));
        }

        let body = resp.text().await.unwrap_or_default();
        Ok(body)
    }

    /// Create a model from a Modelfile
    pub async fn create_model(
        &self,
        name: &str,
        modelfile: &str,
    ) -> Result<String, Box<dyn Error>> {
        let payload = json!({
            "name": name,
            "modelfile": modelfile,
            "stream": false,
        });

        let resp = self
            .client
            .post(format!("{}/api/create", self.base_url))
            .json(&payload)
            .send()
            .await?;

        // See `pull_model`: an error response must not be reported as Ok.
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama create failed: HTTP {status} - {body}"
            ))));
        }

        match resp.text().await {
            Ok(text) => Ok(text),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Copy a model
    pub async fn copy_model(
        &self,
        source: &str,
        destination: &str,
    ) -> Result<String, Box<dyn Error>> {
        let payload = json!({
            "source": source,
            "destination": destination,
        });

        let resp = self
            .client
            .post(format!("{}/api/copy", self.base_url))
            .json(&payload)
            .send()
            .await?;

        // See `pull_model`: an error response must not be reported as Ok.
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama copy failed: HTTP {status} - {body}"
            ))));
        }

        match resp.text().await {
            Ok(text) => Ok(text),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Delete a model
    pub async fn delete_model(&self, name: &str) -> Result<String, Box<dyn Error>> {
        let payload = json!({
            "name": name,
        });

        let resp = self
            .client
            .delete(format!("{}/api/delete", self.base_url))
            .json(&payload)
            .send()
            .await?;

        // See `pull_model`: an error response must not be reported as Ok.
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(Box::new(io::Error::other(format!(
                "Ollama delete failed: HTTP {status} - {body}"
            ))));
        }

        match resp.text().await {
            Ok(text) => Ok(text),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// List running models
    pub async fn list_running(&self) -> Result<Vec<OllamaModel>, Box<dyn Error>> {
        let resp = self
            .client
            .get(format!("{}/api/ps", self.base_url))
            .send()
            .await?;

        let data: OllamaModelsResponse = resp.json().await?;
        Ok(data.models)
    }

    /// Generate embeddings
    pub async fn embeddings(&self, model: &str, prompt: &str) -> Result<Vec<f32>, Box<dyn Error>> {
        let payload = json!({
            "model": model,
            "prompt": prompt,
        });

        let resp = self
            .client
            .post(format!("{}/api/embeddings", self.base_url))
            .json(&payload)
            .send()
            .await?;

        let data: EmbeddingResponse = resp.json().await?;
        Ok(data.embedding)
    }

    /// Get Ollama version
    pub async fn version(&self) -> Result<String, Box<dyn Error>> {
        let resp = self
            .client
            .get(format!("{}/api/version", self.base_url))
            .send()
            .await?;

        let data: Value = resp.json().await?;
        Ok(data["version"].as_str().unwrap_or("unknown").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ollama_client_creation() {
        let client = OllamaClient::new("http://localhost:11434".to_string());
        assert_eq!(client.base_url, "http://localhost:11434");
    }

    #[test]
    fn chat_request_nests_sampling_params_under_options() {
        fn assert_json_number_close(value: &Value, expected: f64, label: &str) {
            let actual = value
                .as_f64()
                .unwrap_or_else(|| panic!("{label} must be a JSON number"));
            let delta = (actual - expected).abs();
            assert!(
                delta <= 1e-6,
                "{label} expected {expected} but was {actual} (|delta|={delta})"
            );
        }

        // Regression: Ollama's /api/chat and /api/generate silently ignore
        // temperature/top_p/top_k/repeat_penalty/num_predict at the top
        // level of the request body -- they're only honored inside a
        // nested "options" object. If these ever end up top-level again,
        // every sampling setting from the GUI/API is silently dropped.
        let request = ChatRequest {
            model: "llama3".to_string(),
            messages: vec![],
            stream: Some(false),
            options: Some(OllamaOptions {
                temperature: Some(0.5),
                top_p: Some(0.8),
                top_k: Some(30),
                repeat_penalty: Some(1.2),
                num_predict: Some(256),
            }),
            tools: None,
        };
        let value = serde_json::to_value(&request).expect("serialize ChatRequest");
        assert!(
            value.get("temperature").is_none(),
            "temperature must not be top-level"
        );
        assert!(value.get("top_p").is_none(), "top_p must not be top-level");
        let options = value
            .get("options")
            .expect("options object must be present");
        assert_json_number_close(&options["temperature"], 0.5, "temperature");
        assert_json_number_close(&options["top_p"], 0.8, "top_p");
        assert_eq!(options["top_k"], 30);
        assert_json_number_close(&options["repeat_penalty"], 1.2, "repeat_penalty");
        assert_eq!(options["num_predict"], 256);
    }
}
