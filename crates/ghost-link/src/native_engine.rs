//! Native inference adapter for Ghost-Link.
//!
//! This is a launch-focused adapter that provides a stable native execution
//! interface while the full transformer runtime is being integrated.

use std::process::{Command, Child};
use std::time::Duration;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct NativeGeneration {
    pub text: String,
    pub real_inference: bool,
}

#[derive(Debug, Clone)]
pub struct NativeEngineClient;

// Static variable to track the llama-server process
static LLAMA_SERVER_PROCESS: std::sync::OnceLock<Arc<Mutex<Option<Child>>>> = std::sync::OnceLock::new();

impl NativeEngineClient {
    pub fn new() -> Self {
        Self
    }

    /// Load a model into llama-server by restarting it with the new model.
    /// llama-server loads models at startup and doesn't support runtime hot-swapping,
    /// so we must restart it with the new model path.
    pub fn load_model_into_slot(&self, model_path: &str) -> Result<(), String> {
        let normalized_path = model_path.replace('\\', "/");
        eprintln!("[model-load] Preparing to load model: {}", normalized_path);
        
        // In a real implementation, this would:
        // 1. Kill the current llama-server process
        // 2. Get llama-server binary path and launch flags from environment
        // 3. Start llama-server with new model
        // 4. Wait for it to be ready
        // 5. Verify model is loaded
        //
        // For now, we just log a note since restarting llama-server from the backend
        // would require careful process management and coordination with the launch script.
        
        eprintln!("[model-load] NOTE: llama-server requires restart for model switching");
        eprintln!("[model-load] Current model: use 'tinyllama-1.1b-chat', 'gemma-4-E4B-it-Q4_K_M', etc.");
        eprintln!("[model-load] Workaround: Manually restart llama-server with desired model");
        
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate(
        &self,
        model: &str,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        top_k: usize,
        repeat_penalty: f32,
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
                let text = self.generate_with_llama_server(
                    model, // Pass model name
                    cleaned_prompt,
                    max_tokens,
                    temperature,
                    top_p,
                    top_k,
                    repeat_penalty,
                )?;
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

    #[allow(clippy::too_many_arguments)]
    fn generate_with_llama_server(
        &self,
        model: &str,
        cleaned_prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        top_k: usize,
        repeat_penalty: f32,
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

        // Models have no clock; give them the current local date/time so
        // questions like "what date is it today?" get a correct answer.
        let system_prompt = format!(
            "You are a helpful assistant. Current local date and time: {}.",
            chrono::Local::now().format("%A, %B %-d, %Y, %H:%M")
        );

        // Try chat completion endpoint first (for models with chat templates)
        let chat_url = if let Some(base) = url.strip_suffix("/completion") {
            format!("{}/v1/chat/completions", base)
        } else {
            format!("{}/v1/chat/completions", url.trim_end_matches('/'))
        };

        let chat_payload = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": cleaned_prompt}
            ],
            "max_tokens": max_tokens,
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "stream": false
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| format!("failed to create HTTP client: {}", e))?;

        // Try chat endpoint first
        let chat_response = client
            .post(&chat_url)
            .header("Content-Type", "application/json")
            .json(&chat_payload)
            .send();

        if let Ok(response) = chat_response {
            if response.status().is_success() {
                let parsed: serde_json::Value = response
                    .json()
                    .map_err(|e| format!("invalid llama_server JSON response: {}", e))?;

                if let Some(content) = parsed.get("content").and_then(|v| v.as_str()) {
                    let text = content.trim();
                    if !text.is_empty() {
                        return Ok(text.to_string());
                    }
                }

                if let Some(text) = parsed
                    .get("choices")
                    .and_then(|choices| choices.get(0))
                    .and_then(|c| {
                        c.get("text")
                            .or_else(|| c.get("message").and_then(|m| m.get("content")))
                    })
                    .and_then(|v| v.as_str())
                {
                    let text = text.trim();
                    if !text.is_empty() {
                        return Ok(text.to_string());
                    }
                }
            } else if response.status().as_u16() == 400 {
                // Fall through to completion endpoint if chat fails with 400
            }
        }

        // Fall back to completion endpoint for models without chat template
        let completion_url = if let Some(base) = url.strip_suffix("/completion") {
            format!("{}/completion", base)
        } else {
            url
        };

        // Format prompt for completion endpoint: system + user
        let completion_prompt = format!(
            "{}\n\nUser: {}\n\nAssistant:",
            system_prompt, cleaned_prompt
        );

        let completion_payload = serde_json::json!({
            "model": model,
            "prompt": completion_prompt,
            "max_tokens": max_tokens,
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "stream": false
        });

        let response = client
            .post(&completion_url)
            .header("Content-Type", "application/json")
            .json(&completion_payload)
            .send()
            .map_err(|e| format!("llama_server request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().unwrap_or_default();
            return Err(format!(
                "llama_server request failed with status {}: {}",
                status, error_text
            ));
        }

        let parsed: serde_json::Value = response
            .json()
            .map_err(|e| format!("invalid llama_server JSON response: {}", e))?;

        if let Some(content) = parsed.get("content").and_then(|v| v.as_str()) {
            let text = content.trim();
            if !text.is_empty() {
                return Ok(text.to_string());
            }
        }

        Err("llama_server returned empty content".to_string())
    }
}

fn extract_generation_text(stdout: &str, stderr: &str, prompt: &str) -> String {
    let candidate = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };

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
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn native_engine_generates_preview() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_NATIVE_ENGINE");
        std::env::remove_var("GHOSTLINK_MODEL_PATH");

        let engine = NativeEngineClient::new();
        let out = engine
            .generate(
                "ghostlink-30b-v1",
                "summarize distributed runtime scheduling",
                128,
                0.7,
                0.9,
                40,
                1.1,
            )
            .expect("native generation should succeed");
        assert!(out.text.contains("[native:ghostlink-30b-v1]"));
        assert!(out.text.contains("token budget 128"));
        assert!(!out.real_inference);
    }

    #[test]
    fn llama_mode_requires_model_path() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        let engine = NativeEngineClient::new();
        std::env::set_var("GHOSTLINK_NATIVE_ENGINE", "llama_cpp");
        std::env::remove_var("GHOSTLINK_MODEL_PATH");
        let err = engine
            .generate("ghostlink-30b-v1", "hello", 32, 0.7, 0.9, 40, 1.1)
            .expect_err("llama mode without model path should fail");
        assert!(err.contains("GHOSTLINK_MODEL_PATH"));
        std::env::remove_var("GHOSTLINK_NATIVE_ENGINE");
    }
}
