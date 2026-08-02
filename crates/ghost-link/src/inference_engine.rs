use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceEngine {
    Ollama,
    Native,
    Vllm,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InferenceEngineCapabilities {
    pub streaming: bool,
    pub model_listing: bool,
    pub model_load: bool,
    pub model_unload: bool,
    pub structured_outputs: bool,
    pub tool_calls: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InferenceEngineDescriptor {
    pub name: String,
    pub label: String,
    pub status: String,
    pub default_base_url: Option<String>,
    pub capabilities: InferenceEngineCapabilities,
}

impl InferenceEngine {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "native" | "fabric" | "llama_server" | "llama-server" => Self::Native,
            "vllm" | "vllm-openai" => Self::Vllm,
            _ => Self::Ollama,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Native => "native",
            Self::Vllm => "vllm",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::Native => "Native",
            Self::Vllm => "vLLM",
        }
    }

    pub const fn default_base_url(self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("http://127.0.0.1:11434"),
            Self::Native => None,
            Self::Vllm => Some("http://127.0.0.1:8000"),
        }
    }

    pub const fn capabilities(self) -> InferenceEngineCapabilities {
        match self {
            Self::Ollama => InferenceEngineCapabilities {
                streaming: true,
                model_listing: true,
                model_load: true,
                model_unload: true,
                structured_outputs: false,
                tool_calls: false,
            },
            Self::Native => InferenceEngineCapabilities {
                streaming: true,
                model_listing: false,
                model_load: true,
                model_unload: true,
                structured_outputs: false,
                tool_calls: false,
            },
            Self::Vllm => InferenceEngineCapabilities {
                streaming: true,
                model_listing: true,
                model_load: true,
                model_unload: false,
                structured_outputs: true,
                tool_calls: true,
            },
        }
    }

    pub fn descriptor(self, active: Self) -> InferenceEngineDescriptor {
        InferenceEngineDescriptor {
            name: self.as_str().to_string(),
            label: self.label().to_string(),
            status: if self == active { "active" } else { "ready" }.to_string(),
            default_base_url: self.default_base_url().map(str::to_string),
            capabilities: self.capabilities(),
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Ollama, Self::Native, Self::Vllm]
    }
}

#[cfg(test)]
mod tests {
    use super::InferenceEngine;

    #[test]
    fn parses_supported_engine_aliases() {
        assert_eq!(InferenceEngine::parse("ollama"), InferenceEngine::Ollama);
        assert_eq!(InferenceEngine::parse("native"), InferenceEngine::Native);
        assert_eq!(
            InferenceEngine::parse("llama-server"),
            InferenceEngine::Native
        );
        assert_eq!(InferenceEngine::parse("vllm"), InferenceEngine::Vllm);
    }

    #[test]
    fn vllm_descriptor_exposes_capabilities() {
        let descriptor = InferenceEngine::Vllm.descriptor(InferenceEngine::Native);
        assert_eq!(descriptor.name, "vllm");
        assert_eq!(descriptor.status, "ready");
        assert!(descriptor.capabilities.structured_outputs);
        assert!(descriptor.capabilities.tool_calls);
    }
}
