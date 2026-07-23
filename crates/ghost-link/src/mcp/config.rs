//! Configuration for MCP (Model Context Protocol) servers Ghostlink connects to.
//!
//! Mirrors `backend_config::ConfigManager`'s load-as-`toml::Value`-then-extract pattern
//! (rather than the simpler one-shot `FileConfig`) since the GUI needs to toggle
//! `enabled` per server and persist that back to disk.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum McpTransport {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// Values written as `"${VAR_NAME}"` are resolved from the host process
        /// environment at connect time — never stored here as literal secrets.
        #[serde(default)]
        env: HashMap<String, String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
}

fn default_true() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    /// Legacy chat "tool slot" this server backs (web_search, calculator, ...),
    /// or empty for standalone additions (sequential-thinking, Docker gateway, ...).
    #[serde(default)]
    pub slot: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub transport: McpTransport,
    /// Risky tools (terminal, code_execution) require an explicit user approval
    /// round-trip before the tool-calling loop executes them.
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct McpServersFile {
    #[serde(default, rename = "mcp_servers")]
    servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone)]
pub struct McpConfigManager {
    config_path: PathBuf,
}

impl McpConfigManager {
    pub fn new(config_path: impl AsRef<Path>) -> Self {
        Self {
            config_path: config_path.as_ref().to_path_buf(),
        }
    }

    /// `mcp_servers.toml` is gitignored (the GUI writes per-install enable/disable
    /// toggles back to it — same pattern as `ghostlink.toml`/`ghostlink.example.toml`).
    /// On a fresh checkout it won't exist yet; copy the checked-in
    /// `mcp_servers.example.toml` template so real servers connect out of the box
    /// instead of silently connecting zero tools until someone copies it by hand.
    fn bootstrap_from_example(&self) {
        let example_path = self.config_path.with_file_name("mcp_servers.example.toml");
        if example_path.exists() {
            if let Err(err) = std::fs::copy(&example_path, &self.config_path) {
                tracing::warn!(
                    "mcp: failed to bootstrap {} from {}: {err}",
                    self.config_path.display(),
                    example_path.display()
                );
            }
        }
    }

    pub fn load(&self) -> Result<Vec<McpServerConfig>, String> {
        if !self.config_path.exists() {
            self.bootstrap_from_example();
        }
        if !self.config_path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&self.config_path)
            .map_err(|err| format!("failed to read {}: {err}", self.config_path.display()))?;
        let file: McpServersFile = toml::from_str(&content)
            .map_err(|err| format!("failed to parse {}: {err}", self.config_path.display()))?;

        for server in &file.servers {
            validate_server(server)?;
        }

        Ok(file.servers)
    }

    pub fn save(&self, servers: &[McpServerConfig]) -> Result<(), String> {
        for server in servers {
            validate_server(server)?;
        }

        let file = McpServersFile {
            servers: servers.to_vec(),
        };
        let output = toml::to_string_pretty(&file)
            .map_err(|err| format!("failed to serialize mcp_servers.toml: {err}"))?;
        std::fs::write(&self.config_path, output)
            .map_err(|err| format!("failed to write {}: {err}", self.config_path.display()))?;
        Ok(())
    }

    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), String> {
        let mut servers = self.load()?;
        let server = servers
            .iter_mut()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("no MCP server named '{name}'"))?;
        server.enabled = enabled;
        self.save(&servers)
    }
}

/// Rejects a config whose `env` map contains a literal-looking secret instead of an
/// `${VAR_NAME}` reference — secrets must come from the host environment, never from
/// `mcp_servers.toml` itself.
fn validate_server(server: &McpServerConfig) -> Result<(), String> {
    if let McpTransport::Stdio { env, .. } = &server.transport {
        for (key, value) in env {
            if looks_like_literal_secret(value) {
                return Err(format!(
                    "mcp server '{}': env var '{}' looks like a literal secret; use \"${{{}}}\" to reference a host environment variable instead",
                    server.name, key, key
                ));
            }
        }
    }
    Ok(())
}

fn looks_like_literal_secret(value: &str) -> bool {
    if value.starts_with("${") && value.ends_with('}') {
        return false;
    }
    // Ordinary config values (URLs, model names, paths) are exempt — this
    // heuristic only targets bare, contiguous, key-shaped tokens.
    if value.starts_with("http://")
        || value.starts_with("https://")
        || value.contains(['/', ':', ' ', '\\'])
    {
        return false;
    }
    value.len() >= 20
        && value.chars().any(|c| c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_alphabetic())
}

/// Resolves `${VAR_NAME}` to the host process environment; returns the value
/// unchanged otherwise (used for non-secret values).
pub fn resolve_env_value(raw: &str) -> String {
    if let Some(var_name) = raw.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        std::env::var(var_name).unwrap_or_default()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path() -> PathBuf {
        let suffix = format!(
            "ghostlink-mcp-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(suffix)
    }

    #[test]
    fn missing_file_yields_empty_list() {
        let manager = McpConfigManager::new(temp_config_path());
        assert!(manager.load().unwrap().is_empty());
    }

    #[test]
    fn missing_config_bootstraps_from_sibling_example() {
        let dir = std::env::temp_dir().join(format!(
            "ghostlink-mcp-bootstrap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let example_path = dir.join("mcp_servers.example.toml");
        std::fs::write(
            &example_path,
            r#"[[mcp_servers]]
name = "filesystem"
slot = "file_operations"
enabled = true
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "./mcp_workspace"]
"#,
        )
        .unwrap();

        let config_path = dir.join("mcp_servers.toml");
        assert!(!config_path.exists());

        let manager = McpConfigManager::new(&config_path);
        let servers = manager.load().unwrap();

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "filesystem");
        assert!(
            config_path.exists(),
            "load() should have copied the example into place"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_load_round_trip() {
        let path = temp_config_path();
        let manager = McpConfigManager::new(&path);
        let servers = vec![McpServerConfig {
            name: "filesystem".to_string(),
            slot: "file_operations".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
            },
            requires_confirmation: false,
            timeout_secs: 30,
        }];

        manager.save(&servers).unwrap();
        let loaded = manager.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "filesystem");
        assert!(loaded[0].enabled);

        manager.set_enabled("filesystem", false).unwrap();
        let loaded = manager.load().unwrap();
        assert!(!loaded[0].enabled);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn literal_secret_is_rejected() {
        let path = temp_config_path();
        let manager = McpConfigManager::new(&path);
        let servers = vec![McpServerConfig {
            name: "brave-search".to_string(),
            slot: "web_search".to_string(),
            enabled: false,
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![],
                env: HashMap::from([(
                    "BRAVE_API_KEY".to_string(),
                    "BSAxxxxxxxxxxxxxxxxxxxxxxxxxxxxx1234".to_string(),
                )]),
            },
            requires_confirmation: false,
            timeout_secs: 30,
        }];

        assert!(manager.save(&servers).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn env_reference_is_accepted() {
        let path = temp_config_path();
        let manager = McpConfigManager::new(&path);
        let servers = vec![McpServerConfig {
            name: "brave-search".to_string(),
            slot: "web_search".to_string(),
            enabled: false,
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![],
                env: HashMap::from([("BRAVE_API_KEY".to_string(), "${BRAVE_API_KEY}".to_string())]),
            },
            requires_confirmation: false,
            timeout_secs: 30,
        }];

        assert!(manager.save(&servers).is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn url_shaped_value_is_not_flagged_as_a_secret() {
        // Regression test: an env value like an Ollama base URL was previously
        // (wrongly) rejected by the literal-secret heuristic just for being long
        // and containing both letters and digits.
        let path = temp_config_path();
        let manager = McpConfigManager::new(&path);
        let servers = vec![McpServerConfig {
            name: "vision".to_string(),
            slot: "".to_string(),
            enabled: false,
            transport: McpTransport::Stdio {
                command: "mcp-vision".to_string(),
                args: vec![],
                env: HashMap::from([(
                    "OLLAMA_URL".to_string(),
                    "http://127.0.0.1:11434".to_string(),
                )]),
            },
            requires_confirmation: false,
            timeout_secs: 30,
        }];

        assert!(manager.save(&servers).is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn resolve_env_value_reads_process_env() {
        std::env::set_var("GHOSTLINK_MCP_TEST_VAR", "resolved-value");
        assert_eq!(
            resolve_env_value("${GHOSTLINK_MCP_TEST_VAR}"),
            "resolved-value"
        );
        assert_eq!(resolve_env_value("literal"), "literal");
        std::env::remove_var("GHOSTLINK_MCP_TEST_VAR");
    }
}
