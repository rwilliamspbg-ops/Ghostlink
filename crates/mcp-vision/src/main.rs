//! Minimal stdio MCP server exposing one `analyze_image` tool backed by a local
//! Ollama vision model (llava/moondream/...) — the real backend for Ghostlink
//! chat's "vision" addition. Stays local-model-first: no cloud vision API calls.

use base64::Engine;
use rmcp::{
    handler::server::wrapper::Parameters, schemars, tool, tool_router, transport::stdio, ServiceExt,
};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AnalyzeImageRequest {
    #[schemars(description = "Path to a local image file (png/jpg/webp) to analyze")]
    image_path: String,
    #[schemars(
        description = "What to ask about the image, e.g. \"describe this image\" or \"read the text in this image\""
    )]
    prompt: String,
}

#[derive(Debug, Clone)]
struct Vision {
    ollama_url: String,
    model: String,
    client: reqwest::Client,
}

impl Vision {
    fn new() -> Self {
        Self {
            ollama_url: std::env::var("OLLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string()),
            model: std::env::var("OLLAMA_VISION_MODEL").unwrap_or_else(|_| "llava".to_string()),
            client: reqwest::Client::new(),
        }
    }
}

#[tool_router(server_handler)]
impl Vision {
    #[tool(
        description = "Analyze a local image with a vision-capable local model and answer a question about it"
    )]
    async fn analyze_image(
        &self,
        Parameters(AnalyzeImageRequest { image_path, prompt }): Parameters<AnalyzeImageRequest>,
    ) -> String {
        let bytes = match std::fs::read(&image_path) {
            Ok(b) => b,
            Err(err) => return format!("error: failed to read image '{image_path}': {err}"),
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let payload = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "images": [encoded],
            "stream": false,
        });

        let url = format!("{}/api/generate", self.ollama_url.trim_end_matches('/'));
        match self.client.post(&url).json(&payload).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(body) => body
                    .get("response")
                    .and_then(|v| v.as_str())
                    .unwrap_or("error: no 'response' field in Ollama's reply")
                    .to_string(),
                Err(err) => format!("error: invalid JSON from Ollama: {err}"),
            },
            Err(err) => format!(
                "error: failed to reach Ollama at {url}: {err} (is `ollama serve` running, and has `{}` been pulled?)",
                self.model
            ),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let service = Vision::new().serve(stdio()).await.inspect_err(|err| {
        tracing::error!("mcp-vision serving error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}
