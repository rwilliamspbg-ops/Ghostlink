//! Custom inference backend plugin system.
//!
//! Ghostlink dispatches generation to one of two built-in backends (native
//! llama.cpp, Ollama) via the `InferenceBackend` enum in `main.rs`. This
//! module adds a third path — a registry of [`InferenceBackendPlugin`] trait
//! objects — so a custom backend (a remote OpenAI-compatible server, a
//! bespoke in-process engine, etc.) can be plugged in by implementing one
//! trait and registering an instance, without adding a match arm to (or
//! otherwise modifying) the existing Native/Ollama dispatch logic.
//!
//! A request selects a plugin by setting `inference_backend` in
//! `ghostlink.toml`/settings to the plugin's registered name; the OpenAI-
//! compatible REST handlers (`/v1/chat/completions`, `/v1/completions`)
//! check the registry for that name *before* falling through to the
//! existing Native/Ollama match, so built-in behavior is unchanged when no
//! plugin matches.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

/// Generation parameters passed to a plugin — deliberately a plain struct
/// (not the richer internal `GenerationParams` used by the GUI dispatch
/// path) so implementing the trait doesn't require depending on Ghostlink's
/// internal session/tool-calling machinery.
#[derive(Debug, Clone)]
pub struct PluginGenerationRequest {
    pub model: String,
    pub prompt: String,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: usize,
    pub penalty: f32,
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PluginGenerationResult {
    pub text: String,
    /// Best-effort output token count; `0` when the backend doesn't report
    /// one (metrics recording just treats that as "unknown", same as other
    /// backends already do for partial failures).
    pub tokens: u32,
}

/// Implemented by a custom inference backend. Object-safe (via `async-trait`)
/// so instances can be stored as `Arc<dyn InferenceBackendPlugin>` in the
/// registry.
#[async_trait]
pub trait InferenceBackendPlugin: Send + Sync {
    /// The name a request selects this plugin with (`inference_backend` in
    /// settings/config). Matched case-insensitively by the registry.
    fn name(&self) -> &str;

    async fn generate(
        &self,
        request: PluginGenerationRequest,
    ) -> Result<PluginGenerationResult, String>;
}

/// Registry of custom backend plugins, keyed by (lowercased) name.
///
/// Cheap to clone — the map lives behind an `Arc<RwLock<_>>` internally —
/// so it can be pulled out of `BackendState` under its own lock scope
/// without holding `BackendState`'s mutex for the duration of a generate()
/// call.
#[derive(Clone, Default)]
pub struct BackendPluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Arc<dyn InferenceBackendPlugin>>>>,
}

impl std::fmt::Debug for BackendPluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendPluginRegistry")
            .field("names", &self.names())
            .finish()
    }
}

impl BackendPluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a registry from the process environment, registering the
    /// built-in `OpenAiCompatPlugin` when `GHOSTLINK_OPENAI_COMPAT_BASE_URL`
    /// is set — an easy way to point Ghostlink at any OpenAI-compatible
    /// server (vLLM, LM Studio, a hosted API, ...) as a "custom" backend
    /// without writing any Rust.
    pub fn from_env() -> Self {
        let registry = Self::new();
        if let Ok(base_url) = std::env::var("GHOSTLINK_OPENAI_COMPAT_BASE_URL") {
            if !base_url.trim().is_empty() {
                let name = std::env::var("GHOSTLINK_OPENAI_COMPAT_NAME")
                    .ok()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| "openai_compat".to_string());
                let api_key = std::env::var("GHOSTLINK_OPENAI_COMPAT_API_KEY").ok();
                registry.register(Arc::new(OpenAiCompatPlugin::new(name, base_url, api_key)));
            }
        }
        registry
    }

    pub fn register(&self, plugin: Arc<dyn InferenceBackendPlugin>) {
        let key = plugin.name().trim().to_ascii_lowercase();
        self.plugins
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(key, plugin);
    }

    /// Looks up a plugin by name, case-insensitively.
    pub fn get(&self, name: &str) -> Option<Arc<dyn InferenceBackendPlugin>> {
        self.plugins
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(&name.trim().to_ascii_lowercase())
            .cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.plugins
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .keys()
            .cloned()
            .collect()
    }
}

/// Reference plugin implementation: forwards generation to any OpenAI-
/// compatible `/v1/completions` endpoint. Doubles as documentation-by-example
/// for implementing [`InferenceBackendPlugin`].
pub struct OpenAiCompatPlugin {
    name: String,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatPlugin {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl InferenceBackendPlugin for OpenAiCompatPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn generate(
        &self,
        request: PluginGenerationRequest,
    ) -> Result<PluginGenerationResult, String> {
        let payload = serde_json::json!({
            "model": request.model,
            "prompt": request.prompt,
            "temperature": request.temperature,
            "top_p": request.top_p,
            "top_k": request.top_k,
            "frequency_penalty": request.penalty,
            "max_tokens": request.max_tokens,
        });

        let mut req = self
            .client
            .post(format!("{}/v1/completions", self.base_url))
            .json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await.map_err(|err| {
            format!(
                "openai-compat backend '{}' request failed: {err}",
                self.name
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(format!(
                "openai-compat backend '{}' returned HTTP {status}: {body}",
                self.name
            ));
        }

        let body: serde_json::Value = resp.json().await.map_err(|err| {
            format!(
                "openai-compat backend '{}' returned invalid JSON: {err}",
                self.name
            )
        })?;

        parse_completion_response(&body).ok_or_else(|| {
            format!(
                "openai-compat backend '{}' response missing choices[0].text",
                self.name
            )
        })
    }
}

/// Extracted for unit testing without a live HTTP server: parses an
/// OpenAI-style `/v1/completions` response body.
fn parse_completion_response(body: &serde_json::Value) -> Option<PluginGenerationResult> {
    let text = body
        .get("choices")?
        .get(0)?
        .get("text")?
        .as_str()?
        .to_string();
    let tokens = body
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    Some(PluginGenerationResult { text, tokens })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoPlugin;

    #[async_trait]
    impl InferenceBackendPlugin for EchoPlugin {
        fn name(&self) -> &str {
            "echo"
        }

        async fn generate(
            &self,
            request: PluginGenerationRequest,
        ) -> Result<PluginGenerationResult, String> {
            Ok(PluginGenerationResult {
                text: format!("echo: {}", request.prompt),
                tokens: request.prompt.split_whitespace().count() as u32,
            })
        }
    }

    fn sample_request() -> PluginGenerationRequest {
        PluginGenerationRequest {
            model: "test-model".to_string(),
            prompt: "hello world".to_string(),
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            penalty: 1.1,
            max_tokens: 64,
        }
    }

    #[tokio::test]
    async fn register_and_dispatch_by_name() {
        let registry = BackendPluginRegistry::new();
        registry.register(Arc::new(EchoPlugin));

        let plugin = registry.get("echo").expect("registered plugin");
        let result = plugin.generate(sample_request()).await.expect("generate");
        assert_eq!(result.text, "echo: hello world");
        assert_eq!(result.tokens, 2);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let registry = BackendPluginRegistry::new();
        registry.register(Arc::new(EchoPlugin));

        assert!(registry.get("ECHO").is_some());
        assert!(registry.get("Echo").is_some());
        assert!(registry.get("unregistered").is_none());
    }

    #[test]
    fn names_lists_all_registered_plugins() {
        let registry = BackendPluginRegistry::new();
        registry.register(Arc::new(EchoPlugin));
        assert_eq!(registry.names(), vec!["echo".to_string()]);
    }

    #[test]
    fn from_env_registers_openai_compat_plugin_when_base_url_set() {
        // SAFETY: test-only env var mutation, scoped to this single test via
        // a unique var name so it can't race other tests in the same binary.
        std::env::set_var(
            "GHOSTLINK_OPENAI_COMPAT_BASE_URL",
            "http://127.0.0.1:9/does-not-need-to-be-reachable",
        );
        let registry = BackendPluginRegistry::from_env();
        assert!(registry.get("openai_compat").is_some());
        std::env::remove_var("GHOSTLINK_OPENAI_COMPAT_BASE_URL");
    }

    #[test]
    fn parses_valid_completion_response() {
        let body = serde_json::json!({
            "choices": [{ "text": "hello" }],
            "usage": { "completion_tokens": 3 }
        });
        let result = parse_completion_response(&body).expect("parse");
        assert_eq!(result.text, "hello");
        assert_eq!(result.tokens, 3);
    }

    #[test]
    fn rejects_response_missing_choices() {
        let body = serde_json::json!({ "usage": { "completion_tokens": 3 } });
        assert!(parse_completion_response(&body).is_none());
    }
}
