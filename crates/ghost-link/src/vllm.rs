use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::error::Error;

#[derive(Clone, Debug)]
pub struct VllmClient {
    base_url: String,
    api_key: Option<String>,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelCard>,
}

#[derive(Debug, Deserialize)]
struct ModelCard {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

impl VllmClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            base_url: normalize_base_url(&base_url),
            api_key: api_key.filter(|key| !key.trim().is_empty()),
            client: Client::new(),
        }
    }

    #[allow(dead_code)]
    pub async fn health(&self) -> Result<bool, Box<dyn Error>> {
        let health_url = format!("{}/health", self.base_url);
        let response = self.request(self.client.get(health_url)).send().await;
        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => {
                let fallback = self
                    .request(self.client.get(format!("{}/v1/models", self.base_url)))
                    .send()
                    .await?;
                Ok(fallback.status().is_success())
            }
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let resp = self
            .request(self.client.get(format!("{}/v1/models", self.base_url)))
            .send()
            .await?;
        let resp = resp.error_for_status()?;
        let payload: ModelListResponse = resp.json().await?;
        Ok(payload.data.into_iter().map(|model| model.id).collect())
    }

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
            "messages": [{ "role": "user", "content": prompt }],
            "stream": false,
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repetition_penalty": repeat_penalty.clamp(0.0, 2.0),
            "max_tokens": max_tokens,
        });

        let resp = self
            .request(
                self.client
                    .post(format!("{}/v1/chat/completions", self.base_url)),
            )
            .json(&payload)
            .send()
            .await?;
        let resp = resp.error_for_status()?;
        let payload: ChatCompletionResponse = resp.json().await?;
        Ok(payload
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .unwrap_or_default())
    }

    fn request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(api_key) = &self.api_key {
            let mut headers = HeaderMap::new();
            if let Ok(value) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
                headers.insert(AUTHORIZATION, value);
            }
            builder.headers(headers)
        } else {
            builder
        }
    }
}

fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_suffix("/v1") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_base_url;

    #[test]
    fn strips_trailing_v1_segment() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:8000/v1/"),
            "http://127.0.0.1:8000"
        );
        assert_eq!(
            normalize_base_url("http://127.0.0.1:8000"),
            "http://127.0.0.1:8000"
        );
    }
}
