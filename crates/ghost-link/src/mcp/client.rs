//! A single connected MCP server session, built on the `rmcp` SDK.
//!
//! Only the stdio (child-process) transport is implemented today; Docker MCP Toolkit
//! and other stdio-spawned servers use it identically (`docker mcp gateway run` is
//! just another command). HTTP/SSE transport is deferred until a remote server is
//! actually needed.

use std::time::Duration;

use rmcp::{
    model::CallToolRequestParams,
    service::{RoleClient, RunningService, ServiceExt},
    transport::TokioChildProcess,
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

#[derive(Debug, Clone)]
pub struct McpToolSchema {
    pub server: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct ToolCallOutcome {
    pub server: String,
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
            McpTransport::Http { .. } => {
                return Err(format!(
                    "MCP server '{}': HTTP/SSE transport is not implemented yet",
                    config.name
                ));
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
