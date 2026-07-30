//! Aggregates every configured MCP server behind one registry held in `BackendState`.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;

use super::client::{McpServerHandle, McpToolSchema, ToolCallOutcome};
use super::config::McpConfigManager;

pub struct McpRegistry {
    config_manager: McpConfigManager,
    servers: RwLock<HashMap<String, McpServerHandle>>,
}

impl std::fmt::Debug for McpRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpRegistry").finish_non_exhaustive()
    }
}

/// One row for the GUI's MCP server list — every server *configured* in
/// `mcp_servers.toml`, not just the ones currently connected, so a user can see
/// (and enable) a disabled server from the UI.
#[derive(Debug, Clone, Serialize)]
pub struct McpServerStatus {
    pub name: String,
    pub slot: String,
    pub enabled: bool,
    pub connected: bool,
    pub tool_count: usize,
    pub requires_confirmation: bool,
}

impl McpRegistry {
    pub fn new(config_manager: McpConfigManager) -> Self {
        Self {
            config_manager,
            servers: RwLock::new(HashMap::new()),
        }
    }

    /// Connects every `enabled` server from `mcp_servers.toml`. A server that fails
    /// to spawn or initialize is logged and skipped — one misconfigured server must
    /// never prevent chat (or the rest of the registry) from working.
    pub async fn connect_enabled(&self) {
        let configs = match self.config_manager.load() {
            Ok(configs) => configs,
            Err(err) => {
                tracing::warn!("mcp: failed to load mcp_servers.toml: {err}");
                return;
            }
        };

        for config in configs.into_iter().filter(|c| c.enabled) {
            match McpServerHandle::connect(&config).await {
                Ok(handle) => {
                    tracing::info!(
                        "mcp: connected server '{}' ({} tools)",
                        config.name,
                        handle.tools().len()
                    );
                    self.servers
                        .write()
                        .await
                        .insert(config.name.clone(), handle);
                }
                Err(err) => {
                    tracing::warn!("mcp: failed to connect server '{}': {err}", config.name);
                }
            }
        }
    }

    /// Finds the server bound to a legacy chat "tool slot" (web_search,
    /// calculator, ...). Standalone servers (sequential-thinking, Docker gateway)
    /// have an empty slot and are addressed directly by name instead — see
    /// `tool_schemas_for_server`.
    pub async fn server_for_slot(&self, slot: &str) -> Option<String> {
        self.servers
            .read()
            .await
            .values()
            .find(|handle| handle.slot() == slot)
            .map(|handle| handle.name().to_string())
    }

    pub async fn tool_schemas_for_server(&self, server_name: &str) -> Vec<McpToolSchema> {
        self.servers
            .read()
            .await
            .get(server_name)
            .map(|handle| handle.tools().to_vec())
            .unwrap_or_default()
    }

    /// Every server configured in `mcp_servers.toml`, merged with live
    /// connection status — used to render the GUI's MCP server list.
    pub async fn list_all_servers(&self) -> Result<Vec<McpServerStatus>, String> {
        let configs = self.config_manager.load()?;
        let connected = self.servers.read().await;

        Ok(configs
            .into_iter()
            .map(|config| {
                let live = connected.get(&config.name);
                McpServerStatus {
                    name: config.name,
                    slot: config.slot,
                    enabled: config.enabled,
                    connected: live.is_some(),
                    tool_count: live.map(|handle| handle.tools().len()).unwrap_or(0),
                    requires_confirmation: config.requires_confirmation,
                }
            })
            .collect())
    }

    /// Toggles a server's `enabled` flag in `mcp_servers.toml` and takes effect
    /// immediately: connects it now if enabling, or tears it down now if
    /// disabling — so the GUI doesn't need a Ghostlink restart to see the change.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), String> {
        self.config_manager.set_enabled(name, enabled)?;

        if enabled {
            let configs = self.config_manager.load()?;
            let config = configs
                .into_iter()
                .find(|c| c.name == name)
                .ok_or_else(|| format!("no MCP server named '{name}'"))?;
            let handle = McpServerHandle::connect(&config).await?;
            tracing::info!(
                "mcp: connected server '{}' ({} tools)",
                name,
                handle.tools().len()
            );
            self.servers.write().await.insert(name.to_string(), handle);
        } else if let Some(handle) = self.servers.write().await.remove(name) {
            handle.shutdown().await;
            tracing::info!("mcp: disconnected server '{name}'");
        }

        Ok(())
    }

    /// Reads a configured env var for one server from `mcp_servers.toml`
    /// (e.g. `rag`'s `OLLAMA_URL`), resolving `${VAR}` indirection the same
    /// way `connect_enabled` does. Needed because those env vars are only
    /// ever applied to the *spawned child process* — this (main) process
    /// never inherits them, so out-of-band checks (e.g. "is rag's Ollama
    /// actually reachable") can't just read `std::env::var` directly.
    pub fn env_var_for(&self, server_name: &str, key: &str) -> Option<String> {
        let configs = self.config_manager.load().ok()?;
        let config = configs.into_iter().find(|c| c.name == server_name)?;
        match config.transport {
            super::config::McpTransport::Stdio { env, .. } => {
                env.get(key).map(|v| super::config::resolve_env_value(v))
            }
            super::config::McpTransport::Http { .. } => None,
        }
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        args: Value,
    ) -> Option<ToolCallOutcome> {
        let servers = self.servers.read().await;
        let handle = servers.get(server_name)?;
        Some(handle.call_tool(tool_name, args).await)
    }

    pub async fn requires_confirmation(&self, server_name: &str) -> bool {
        self.servers
            .read()
            .await
            .get(server_name)
            .map(|handle| handle.requires_confirmation())
            .unwrap_or(false)
    }

    pub async fn shutdown_all(&self) {
        let mut servers = self.servers.write().await;
        for (_, handle) in servers.drain() {
            handle.shutdown().await;
        }
    }
}
