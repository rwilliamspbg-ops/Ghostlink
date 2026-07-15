//! Ollama inference client for real model execution
use reqwest::Client;
use serde_json::json;
use std::error::Error;

#[derive(Clone, Debug)]
pub struct OllamaClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OllamaResponse {
    pub response: String,
    pub done: bool,
}

impl OllamaClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    /// Check if Ollama is reachable
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub async fn list_models(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;

        let data: serde_json::Value = resp.json().await?;
        let models = data["models"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
            .collect();

        Ok(models)
    }

    /// Generate text using Ollama
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

    /// Pull a model from Ollama registry
    #[allow(dead_code)]
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
