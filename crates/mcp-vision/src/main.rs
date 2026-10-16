//! Minimal stdio MCP server exposing one `analyze_image` tool backed by a local
//! Ollama vision model (llava/moondream/...) — the real backend for Ghostlink
//! chat's "vision" addition. Stays local-model-first: no cloud vision API calls.

use base64::Engine;
use rmcp::{
    handler::server::wrapper::Parameters, schemars, tool, tool_router, transport::stdio, ServiceExt,
};

/// Read an image file and return its base64-encoded contents.
/// Extracted for unit testing.
pub fn encode_image_to_base64(image_path: &str) -> Result<String, String> {
    let bytes = std::fs::read(image_path)
        .map_err(|err| format!("error: failed to read image \'{image_path}\': {err}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Build the Ollama API URL from the base URL.
pub fn build_ollama_url(ollama_url: &str) -> String {
    format!("{}/api/generate", ollama_url.trim_end_matches('/'))
}

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
        let encoded = match encode_image_to_base64(&image_path) {
            Ok(enc) => enc,
            Err(err) => return err,
        };

        let payload = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "images": [encoded],
            "stream": false,
        });

        let url = build_ollama_url(&self.ollama_url);
        match self.client.post(&url).json(&payload).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(body) => body
                    .get("response")
                    .and_then(|v| v.as_str())
                    .unwrap_or("error: no \'response\' field in Ollama\'s reply")
                    .to_string(),
                Err(err) => format!("error: invalid JSON from Ollama: {err}"),
            },
            Err(err) => format!(
                "error: failed to reach Ollama at {url}: {err} (is `ollama serve` running, and has \'{}\' been pulled?)",
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

#[cfg(test)]
mod tests {
    use super::{build_ollama_url, encode_image_to_base64};
    use base64::Engine;
    use std::io::Write;

    #[test]
    fn build_ollama_url_appends_api_generate() {
        assert_eq!(
            build_ollama_url("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434/api/generate"
        );
    }

    #[test]
    fn build_ollama_url_trims_trailing_slash() {
        assert_eq!(
            build_ollama_url("http://localhost:11434/"),
            "http://localhost:11434/api/generate"
        );
    }

    #[test]
    fn encode_image_to_base64_success() {
        // Create a temporary file with known contents
        let tmp_path = std::env::temp_dir().join("mcp_vision_test_image.png");
        let mut file = std::fs::File::create(&tmp_path).unwrap();
        // Write a minimal PNG header bytes
        let png_bytes: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        file.write_all(&png_bytes).unwrap();
        drop(file);

        let result = encode_image_to_base64(tmp_path.to_str().unwrap());
        assert!(result.is_ok());

        let encoded = result.unwrap();
        // Verify it's valid base64
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        assert_eq!(decoded, png_bytes);

        // Clean up
        std::fs::remove_file(&tmp_path).ok();
    }

    #[test]
    fn encode_image_to_base64_file_not_found() {
        let result = encode_image_to_base64("/nonexistent/path/to/image.png");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("failed to read image"),
            "Unexpected error: {err}"
        );
    }

    #[test]
    fn encode_image_to_base64_empty_file() {
        let tmp_path = std::env::temp_dir().join("mcp_vision_test_empty.png");
        std::fs::File::create(&tmp_path).unwrap();

        let result = encode_image_to_base64(tmp_path.to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ""); // Empty file → empty base64

        std::fs::remove_file(&tmp_path).ok();
    }
}
