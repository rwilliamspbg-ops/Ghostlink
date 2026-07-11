//! Native inference adapter for Ghost-Link.
//!
//! This is a launch-focused adapter that provides a stable native execution
//! interface while the full transformer runtime is being integrated.

use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct NativeGeneration {
    pub text: String,
    pub real_inference: bool,
}

#[derive(Debug, Clone)]
pub struct NativeEngineClient;

impl NativeEngineClient {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(
        &self,
        model: &str,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<NativeGeneration, String> {
        if model.trim().is_empty() {
            return Err("model cannot be empty".to_string());
        }

        let cleaned_prompt = prompt.trim();
        if cleaned_prompt.is_empty() {
            return Ok(NativeGeneration {
                text: format!(
                    "Native backend is ready for model '{}'. Provide a non-empty prompt for generation.",
                    model
                ),
                real_inference: false,
            });
        }

        let max_tokens = max_tokens.clamp(16, 4096);

        match std::env::var("GHOSTLINK_NATIVE_ENGINE")
            .unwrap_or_else(|_| "simulated".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "llama_server" | "llama-server" => {
                let text = self.generate_with_llama_server(cleaned_prompt, max_tokens)?;
                Ok(NativeGeneration {
                    text,
                    real_inference: true,
                })
            }
            "llama_cpp" | "llama.cpp" | "llama" => {
                let text = self.generate_with_llama_cpp(cleaned_prompt, max_tokens)?;
                Ok(NativeGeneration {
                    text,
                    real_inference: true,
                })
            }
            _ => self.generate_simulated(model, cleaned_prompt, max_tokens),
        }
    }

    fn generate_simulated(
        &self,
        model: &str,
        cleaned_prompt: &str,
        max_tokens: usize,
    ) -> Result<NativeGeneration, String> {
        let preview = cleaned_prompt
            .split_whitespace()
            .take(20)
            .collect::<Vec<_>>()
            .join(" ");

        Ok(NativeGeneration {
            text: format!(
                "[native:{}] generated response with token budget {}. Prompt preview: {}",
                model, max_tokens, preview
            ),
            real_inference: false,
        })
    }

    fn generate_with_llama_cpp(
        &self,
        cleaned_prompt: &str,
        max_tokens: usize,
    ) -> Result<String, String> {
        let bin = std::env::var("GHOSTLINK_LLAMA_CLI_BIN")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "llama-cli".to_string());
        let model_path = std::env::var("GHOSTLINK_MODEL_PATH")
            .map_err(|_| "GHOSTLINK_MODEL_PATH is required for llama_cpp mode".to_string())?;

        let output = Command::new(&bin)
            .arg("-m")
            .arg(model_path)
            .arg("-p")
            .arg(cleaned_prompt)
            .arg("-n")
            .arg(max_tokens.to_string())
            .arg("-no-cnv")
            .arg("-st")
            .arg("--no-display-prompt")
            .output()
            .map_err(|err| format!("failed to execute '{}': {}", bin, err))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("llama_cpp execution failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut response = extract_generation_text(&stdout, &stderr, cleaned_prompt);
        if response.trim().is_empty() {
            let raw = if stdout.trim().is_empty() {
                stderr.trim()
            } else {
                stdout.trim()
            };
            if !raw.is_empty() {
                response = raw.to_string();
            }
        }
        if response.is_empty() {
            return Err("llama_cpp returned empty output".to_string());
        }

        Ok(response)
    }

    fn generate_with_llama_server(
        &self,
        cleaned_prompt: &str,
        max_tokens: usize,
    ) -> Result<String, String> {
        let url = std::env::var("GHOSTLINK_LLAMA_SERVER_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:8080/completion".to_string());

        let timeout_secs = std::env::var("GHOSTLINK_LLAMA_SERVER_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .clamp(5, 300);

        let payload = serde_json::json!({
            "prompt": cleaned_prompt,
            "n_predict": max_tokens,
            "temperature": 0.7,
            "stream": false
        })
        .to_string();

        let timeout = Duration::from_secs(timeout_secs).as_secs().to_string();
        let output = Command::new("curl")
            .arg("--silent")
            .arg("--show-error")
            .arg("--fail")
            .arg("--max-time")
            .arg(timeout)
            .arg("-H")
            .arg("content-type: application/json")
            .arg("-d")
            .arg(payload)
            .arg(url)
            .output()
            .map_err(|err| format!("failed to execute curl for llama-server: {}", err))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("llama_server request failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
            .map_err(|err| format!("invalid llama_server JSON response: {}", err))?;

        if let Some(content) = parsed.get("content").and_then(|v| v.as_str()) {
            let text = content.trim();
            if !text.is_empty() {
                return Ok(text.to_string());
            }
        }

        if let Some(text) = parsed
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|c| c.get("text").or_else(|| c.get("message").and_then(|m| m.get("content"))))
            .and_then(|v| v.as_str())
        {
            let text = text.trim();
            if !text.is_empty() {
                return Ok(text.to_string());
            }
        }

        Err("llama_server returned empty content".to_string())
    }
}

fn extract_generation_text(stdout: &str, stderr: &str, prompt: &str) -> String {
    let candidate = if stdout.trim().is_empty() { stderr } else { stdout };

    let mut kept = Vec::new();
    for raw in candidate.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("Loading model")
            || line.starts_with("build")
            || line.starts_with("model")
            || line.starts_with("ftype")
            || line.starts_with("modalities")
            || line.starts_with("available commands")
            || line.starts_with("/exit")
            || line.starts_with("/regen")
            || line.starts_with("/clear")
            || line.starts_with("/read")
            || line.starts_with("/glob")
            || line.starts_with("[ Prompt:")
            || line.starts_with("Exiting")
            || line.contains('█')
        {
            continue;
        }

        if let Some(rest) = line.strip_prefix('>') {
            let prompt_line = rest.trim();
            if prompt_line.eq_ignore_ascii_case(prompt) {
                continue;
            }
            if prompt_line.is_empty() {
                continue;
            }
            kept.push(prompt_line.to_string());
            continue;
        }

        kept.push(line.to_string());
    }

    kept.join(" ")
}

#[cfg(test)]
mod tests {
    use super::NativeEngineClient;

    #[test]
    fn native_engine_generates_preview() {
        let engine = NativeEngineClient::new();
        let out = engine
            .generate("ghostlink-30b-v1", "summarize distributed runtime scheduling", 128)
            .expect("native generation should succeed");
        assert!(out.text.contains("[native:ghostlink-30b-v1]"));
        assert!(out.text.contains("token budget 128"));
        assert!(!out.real_inference);
    }

    #[test]
    fn llama_mode_requires_model_path() {
        let engine = NativeEngineClient::new();
        std::env::set_var("GHOSTLINK_NATIVE_ENGINE", "llama_cpp");
        std::env::remove_var("GHOSTLINK_MODEL_PATH");
        let err = engine
            .generate("ghostlink-30b-v1", "hello", 32)
            .expect_err("llama mode without model path should fail");
        assert!(err.contains("GHOSTLINK_MODEL_PATH"));
        std::env::remove_var("GHOSTLINK_NATIVE_ENGINE");
    }
}
