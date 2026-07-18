//! Ollama inference client for real model execution
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::error::Error;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
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
    pub license: String,
    pub modelfile: String,
    pub parameters: String,
    pub template: String,
    #[serde(default)]
    pub details: Option<ModelDetails>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelRequest {
    pub name: String,
    pub modelfile: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub stream: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyModelRequest {
    pub source: String,
    pub destination: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteModelRequest {
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embedding: Vec<f32>,
}

#[allow(dead_code)]
pub type OllamaStream = Pin<Box<dyn Stream<Item = Result<String, Box<dyn Error + Send + Sync>>> + Send>>;

impl OllamaClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
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
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "num_predict": max_tokens,
        });

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&payload)
            .send()
            .await?;

        let data: OllamaResponse = resp.json().await?;
        Ok(data.response)
    }

    /// Generate text using Ollama (streaming)
    #[allow(dead_code, clippy::too_many_arguments)]
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
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "num_predict": max_tokens,
        });

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&payload)
            .send()
            .await?;

        let stream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(
            Self::stream_response(resp).await?
        ));

        Ok(stream)
    }

    #[allow(dead_code)]
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
    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: Option<f32>,
        top_p: Option<f32>,
        top_k: Option<usize>,
        repeat_penalty: Option<f32>,
        max_tokens: Option<usize>,
    ) -> Result<ChatResponse, Box<dyn Error>> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: Some(false),
            temperature,
            top_p,
            top_k,
            repeat_penalty,
            num_predict: max_tokens,
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        let data: ChatResponse = resp.json().await?;
        Ok(data)
    }

    /// Chat with Ollama (streaming)
    #[allow(dead_code, clippy::too_many_arguments)]
    pub async fn chat_stream(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: Option<f32>,
        top_p: Option<f32>,
        top_k: Option<usize>,
        repeat_penalty: Option<f32>,
        max_tokens: Option<usize>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponse, Box<dyn Error + Send + Sync>>> + Send>>, Box<dyn Error>> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: Some(true),
            temperature,
            top_p,
            top_k,
            repeat_penalty,
            num_predict: max_tokens,
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        let stream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(
            Self::stream_chat_response(resp).await?
        ));

        Ok(stream)
    }

    #[allow(dead_code)]
    async fn stream_chat_response(
        resp: reqwest::Response,
    ) -> Result<mpsc::Receiver<Result<ChatResponse, Box<dyn Error + Send + Sync>>>, Box<dyn Error>> {
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

        match resp.text().await {
            Ok(text) => Ok(text),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Pull a model from Ollama registry with progress streaming
    pub async fn pull_model_stream(
        &self,
        model_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<PullProgressResponse, Box<dyn Error + Send + Sync>>> + Send>>, Box<dyn Error>> {
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

        let stream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(
            Self::stream_pull_progress(resp).await?
        ));

        Ok(stream)
    }

    async fn stream_pull_progress(
        resp: reqwest::Response,
    ) -> Result<mpsc::Receiver<Result<PullProgressResponse, Box<dyn Error + Send + Sync>>>, Box<dyn Error>> {
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

        let data: ModelInfoResponse = resp.json().await?;
        Ok(data)
    }

    /// Create a model from a Modelfile
    pub async fn create_model(&self, name: &str, modelfile: &str) -> Result<String, Box<dyn Error>> {
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

        match resp.text().await {
            Ok(text) => Ok(text),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Copy a model
    pub async fn copy_model(&self, source: &str, destination: &str) -> Result<String, Box<dyn Error>> {
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
}