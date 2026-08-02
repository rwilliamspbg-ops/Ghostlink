//! A single connected MCP server session, built on the `rmcp` SDK.
//!
//! Two transports are implemented: stdio (child-process) — Docker MCP Toolkit and
//! other stdio-spawned servers use it identically, since `docker mcp gateway run` is
//! just another command — and streamable HTTP/SSE, for remote MCP servers reachable
//! over a URL.

use std::collections::HashMap;
use std::time::Duration;

use http::{HeaderName, HeaderValue};
use mcp_reqwest::Client as McpHttpClient;
use rmcp::{
    model::CallToolRequestParams,
    service::{RoleClient, RunningService, ServiceExt},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
        TokioChildProcess,
    },
};
use serde_json::Value;
use tokio::process::Command;

use super::config::{resolve_env_value, McpServerConfig, McpTransport};

/// Builds the actual child-process command for a configured `command`/`args` pair.
///
/// On Windows, `CreateProcess` (which `tokio::process::Command` calls directly) does
/// not resolve `PATHEXT`/shim scripts the way a shell does — `npx`/`npm`/`docker` are
/// often `.cmd` shims, and spawning "npx" bare fails with "program not found" even
/// though `npx` works fine when typed into a terminal. Routing through `cmd /C`
/// reproduces the same resolution a user's shell performs, for any command.
#[cfg(windows)]
fn build_command(command: &str, args: &[String]) -> Command {
    // `cmd.exe` treats an unquoted "/" in the program-name position as a switch
    // separator (it tries to run "target" from "target/release/foo.exe" and
    // fails with "'target' is not recognized..."), even though the same path
    // works fine as an *argument* to another program. Backslashes are the only
    // separator `cmd /C` reliably accepts there.
    let normalized_command = command.replace('/', "\\");
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(normalized_command).args(args);
    cmd
}

#[cfg(not(windows))]
fn build_command(command: &str, args: &[String]) -> Command {
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd
}

/// Converts a configured `headers` map into the `HeaderName`/`HeaderValue` pairs
/// `rmcp`'s HTTP transport expects, surfacing any invalid header as a plain string
/// (config-driven values aren't guaranteed to be valid HTTP header syntax).
fn parse_header_map(
    headers: &HashMap<String, String>,
) -> Result<HashMap<HeaderName, HeaderValue>, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let header_name = HeaderName::try_from(name.as_str())
                .map_err(|err| format!("header name '{name}': {err}"))?;
            let header_value = HeaderValue::try_from(resolve_env_value(value))
                .map_err(|err| format!("header value for '{name}': {err}"))?;
            Ok((header_name, header_value))
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct McpToolSchema {
    pub server: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct ToolCallOutcome {
    // Callers already know which server/tool they invoked (they pass both in
    // as call_tool()'s own arguments), so these two are currently write-only
    // - kept on the struct because they're the natural place to carry that
    // context if a future caller needs to fan a batch of outcomes out without
    // holding onto the original request.
    #[allow(dead_code)]
    pub server: String,
    #[allow(dead_code)]
    pub tool: String,
    pub result: Value,
    pub success: bool,
    pub error: Option<String>,
}

pub struct McpServerHandle {
    name: String,
    slot: String,
    requires_confirmation: bool,
    timeout: Duration,
    session: RunningService<RoleClient, ()>,
    tools: Vec<McpToolSchema>,
    /// PID of the directly-spawned child (e.g. `cmd.exe` on Windows, or the server
    /// binary itself elsewhere). `rmcp`'s own Drop-time cleanup only kills this one
    /// process, which is not enough when it's a shim that spawned its own children
    /// (e.g. `cmd /C npx ...` → node.exe) — see `shutdown()`.
    //
    // `shutdown()` (and the process-tree kill it does) has no caller yet:
    // there's no graceful-shutdown hook anywhere in ghost-link's main() today
    // (Ctrl+C just drops the process), so spawned MCP servers are currently
    // orphaned on exit regardless of this field. Tracked as a follow-up
    // rather than bundled into this change.
    #[allow(dead_code)]
    child_pid: Option<u32>,
}

impl std::fmt::Debug for McpServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServerHandle")
            .field("name", &self.name)
            .field("slot", &self.slot)
            .field("tools", &self.tools.len())
            .finish()
    }
}

impl McpServerHandle {
    pub async fn connect(config: &McpServerConfig) -> Result<Self, String> {
        let (session, child_pid) = match &config.transport {
            McpTransport::Stdio { command, args, env } => {
                let mut cmd = build_command(command, args);
                for (key, value) in env {
                    cmd.env(key, resolve_env_value(value));
                }
                let transport = TokioChildProcess::new(cmd).map_err(|err| {
                    format!("failed to spawn MCP server '{}': {err}", config.name)
                })?;
                let child_pid = transport.id();

                let session = ().serve(transport).await.map_err(|err| {
                    format!("failed to initialize MCP server '{}': {err}", config.name)
                })?;
                (session, child_pid)
            }
            McpTransport::Http { url, headers } => {
                let custom_headers = parse_header_map(headers).map_err(|err| {
                    format!("MCP server '{}': invalid header ({err})", config.name)
                })?;
                let transport_config = StreamableHttpClientTransportConfig::with_uri(url.clone())
                    .custom_headers(custom_headers);
                let transport = StreamableHttpClientTransport::with_client(
                    McpHttpClient::new(),
                    transport_config,
                );

                let session = ().serve(transport).await.map_err(|err| {
                    format!("failed to initialize MCP server '{}': {err}", config.name)
                })?;
                (session, None)
            }
        };

        let tools_result = session
            .list_tools(Default::default())
            .await
            .map_err(|err| {
                format!(
                    "failed to list tools for MCP server '{}': {err}",
                    config.name
                )
            })?;

        let tools = tools_result
            .tools
            .into_iter()
            .map(|tool| McpToolSchema {
                server: config.name.clone(),
                name: tool.name.to_string(),
                description: tool.description.as_deref().unwrap_or_default().to_string(),
                input_schema: serde_json::to_value(&tool.input_schema).unwrap_or(Value::Null),
            })
            .collect();

        Ok(Self {
            name: config.name.clone(),
            slot: config.slot.clone(),
            requires_confirmation: config.requires_confirmation,
            timeout: Duration::from_secs(config.timeout_secs.max(1)),
            session,
            tools,
            child_pid,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn slot(&self) -> &str {
        &self.slot
    }

    pub fn requires_confirmation(&self) -> bool {
        self.requires_confirmation
    }

    pub fn tools(&self) -> &[McpToolSchema] {
        &self.tools
    }

    pub async fn call_tool(&self, tool_name: &str, args: Value) -> ToolCallOutcome {
        let request = match args.as_object() {
            Some(map) => {
                CallToolRequestParams::new(tool_name.to_string()).with_arguments(map.clone())
            }
            None => CallToolRequestParams::new(tool_name.to_string()),
        };

        match tokio::time::timeout(self.timeout, self.session.call_tool(request)).await {
            Ok(Ok(result)) => ToolCallOutcome {
                server: self.name.clone(),
                tool: tool_name.to_string(),
                result: serde_json::to_value(&result).unwrap_or(Value::Null),
                success: true,
                error: None,
            },
            Ok(Err(err)) => ToolCallOutcome {
                server: self.name.clone(),
                tool: tool_name.to_string(),
                result: Value::Null,
                success: false,
                error: Some(err.to_string()),
            },
            Err(_) => ToolCallOutcome {
                server: self.name.clone(),
                tool: tool_name.to_string(),
                result: Value::Null,
                success: false,
                error: Some(format!("tool call timed out after {:?}", self.timeout)),
            },
        }
    }

    #[allow(dead_code)]
    pub async fn shutdown(self) {
        let pid = self.child_pid;
        let name = self.name.clone();
        // Best-effort graceful shutdown first (lets the server flush/exit cleanly).
        let _ = tokio::time::timeout(Duration::from_secs(5), self.session.cancel()).await;

        // `rmcp`'s own cleanup only kills the directly-spawned process. On Windows
        // that's `cmd.exe` (see `build_command`), whose own children (e.g. the
        // node.exe processes an `npx`-launched server spawns) are not part of any
        // Windows job object and would otherwise be orphaned. Force-kill the whole
        // tree the same way `native_engine.rs` tears down llama-server.
        if let Some(pid) = pid {
            kill_process_tree(pid, &name).await;
        }
    }
}

#[allow(dead_code)]
#[cfg(windows)]
async fn kill_process_tree(pid: u32, server_name: &str) {
    let output = tokio::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output()
        .await;
    if let Err(err) = output {
        tracing::warn!("mcp: failed to taskkill server '{server_name}' (pid {pid}): {err}");
    }
}

#[allow(dead_code)]
#[cfg(not(windows))]
async fn kill_process_tree(pid: u32, server_name: &str) {
    let output = tokio::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output()
        .await;
    if let Err(err) = output {
        tracing::warn!("mcp: failed to kill server '{server_name}' (pid {pid}): {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header_map_resolves_env_references() {
        std::env::set_var("GHOSTLINK_MCP_CLIENT_TEST_HEADER", "resolved-value");
        let headers = HashMap::from([(
            "X-Api-Key".to_string(),
            "${GHOSTLINK_MCP_CLIENT_TEST_HEADER}".to_string(),
        )]);

        let parsed = parse_header_map(&headers).expect("valid header");
        let value = parsed
            .get(&HeaderName::try_from("X-Api-Key").unwrap())
            .expect("header present");
        assert_eq!(value, "resolved-value");
        std::env::remove_var("GHOSTLINK_MCP_CLIENT_TEST_HEADER");
    }

    #[test]
    fn parse_header_map_rejects_invalid_header_name() {
        let headers = HashMap::from([("Invalid Header Name".to_string(), "value".to_string())]);
        assert!(parse_header_map(&headers).is_err());
    }

    #[test]
    fn parse_header_map_passes_through_literal_values() {
        let headers = HashMap::from([("X-Plain".to_string(), "plain-value".to_string())]);
        let parsed = parse_header_map(&headers).expect("valid header");
        let value = parsed
            .get(&HeaderName::try_from("X-Plain").unwrap())
            .expect("header present");
        assert_eq!(value, "plain-value");
    }
}
