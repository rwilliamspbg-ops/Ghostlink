//! Real MCP (Model Context Protocol) client support for Ghostlink chat.
//!
//! Replaces the old `ToolDispatcher` stub (canned strings) with genuine connections
//! to MCP servers spawned over stdio (or, for Docker MCP Toolkit / remote servers, HTTP).

mod client;
mod config;
mod registry;
pub mod toolcall;

pub use client::McpToolSchema;
pub use config::{McpConfigManager, McpServerConfig};
pub use registry::McpRegistry;

/// Default path for the MCP server registry file, alongside `ghostlink.toml`.
pub fn default_config_path() -> std::path::PathBuf {
    std::env::var("GHOSTLINK_MCP_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("mcp_servers.toml"))
}

/// End-to-end proof that the stdio transport really spawns a server, calls a real
/// tool, and gets real (non-canned) data back. Requires `npx`/Node on the host and
/// network access to fetch `@modelcontextprotocol/server-filesystem` on first run,
/// so it's `#[ignore]`d by default — run explicitly with `cargo test -- --ignored`.
#[cfg(test)]
mod integration_tests {
    use super::client::McpServerHandle;
    use super::config::{McpServerConfig, McpTransport};
    use std::collections::HashMap;

    #[tokio::test]
    #[ignore]
    async fn filesystem_server_lists_real_directory_contents() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/ghost-link should be two levels below the repo root");
        let workspace = repo_root
            .join("mcp_workspace")
            .canonicalize()
            .expect("repo root should contain mcp_workspace/");

        let config = McpServerConfig {
            name: "filesystem".to_string(),
            slot: "file_operations".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    workspace.display().to_string(),
                ],
                env: HashMap::new(),
            },
            requires_confirmation: false,
            timeout_secs: 60,
        };

        let handle = McpServerHandle::connect(&config)
            .await
            .expect("filesystem MCP server should connect over stdio");

        assert!(
            !handle.tools().is_empty(),
            "filesystem server should expose at least one tool"
        );

        let list_tool = handle
            .tools()
            .iter()
            .find(|t| t.name.contains("list_directory"))
            .expect("filesystem server should expose a list_directory-style tool");

        let outcome = handle
            .call_tool(
                &list_tool.name,
                serde_json::json!({ "path": workspace.display().to_string() }),
            )
            .await;

        assert!(outcome.success, "tool call failed: {:?}", outcome.error);
        let rendered = serde_json::to_string(&outcome.result).unwrap();
        assert!(
            rendered.contains("hello.txt"),
            "expected real directory listing to contain hello.txt, got: {rendered}"
        );

        handle.shutdown().await;
    }

    /// Proves the custom in-repo `mcp-calculator` server (built via
    /// `cargo build --release -p mcp-calculator`) computes a real result instead of
    /// the old hardcoded "42" stub.
    #[tokio::test]
    #[ignore]
    async fn calculator_server_evaluates_real_expression() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/ghost-link should be two levels below the repo root");
        let binary = repo_root
            .join("target")
            .join("release")
            .join(if cfg!(windows) {
                "mcp-calculator.exe"
            } else {
                "mcp-calculator"
            });
        assert!(
            binary.exists(),
            "run `cargo build --release -p mcp-calculator` first ({})",
            binary.display()
        );

        let config = McpServerConfig {
            name: "calculator".to_string(),
            slot: "calculator".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: binary.display().to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            requires_confirmation: false,
            timeout_secs: 30,
        };

        let handle = McpServerHandle::connect(&config)
            .await
            .expect("calculator MCP server should connect over stdio");

        let outcome = handle
            .call_tool("calculate", serde_json::json!({ "expression": "17 * 23" }))
            .await;

        assert!(outcome.success, "tool call failed: {:?}", outcome.error);
        let rendered = serde_json::to_string(&outcome.result).unwrap();
        assert!(
            rendered.contains("391"),
            "expected real evaluation of 17*23=391, got: {rendered}"
        );

        handle.shutdown().await;
    }

    /// Proves the official sequential-thinking reference server (a standalone
    /// addition with no legacy "slot", addressed by server name — see
    /// `McpRegistry::server_for_slot`/`tool_schemas_for_server`) connects and
    /// exposes its `sequentialthinking` tool for real.
    #[tokio::test]
    #[ignore]
    async fn sequential_thinking_server_exposes_real_tool() {
        let config = McpServerConfig {
            name: "sequential-thinking".to_string(),
            slot: "".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-sequential-thinking".to_string(),
                ],
                env: HashMap::new(),
            },
            requires_confirmation: false,
            timeout_secs: 60,
        };

        let handle = McpServerHandle::connect(&config)
            .await
            .expect("sequential-thinking MCP server should connect over stdio");

        assert!(
            !handle.tools().is_empty(),
            "sequential-thinking server should expose at least one tool"
        );
        assert!(handle.tools().iter().any(|t| t.name.contains("thinking")));

        let tool_name = handle.tools()[0].name.clone();
        let outcome = handle
            .call_tool(
                &tool_name,
                serde_json::json!({
                    "thought": "Step 1: verify the MCP pipe works end to end.",
                    "thoughtNumber": 1,
                    "totalThoughts": 1,
                    "nextThoughtNeeded": false
                }),
            )
            .await;

        assert!(outcome.success, "tool call failed: {:?}", outcome.error);

        handle.shutdown().await;
    }

    /// Proves the custom in-repo `mcp-vision` server (built via
    /// `cargo build --release -p mcp-vision`) connects, exposes `analyze_image`,
    /// and — since this environment has no vision model pulled/running — reports
    /// a real, specific connection failure rather than crashing or hanging.
    #[tokio::test]
    #[ignore]
    async fn vision_server_reports_real_ollama_error_when_unreachable() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/ghost-link should be two levels below the repo root");
        let binary = repo_root
            .join("target")
            .join("release")
            .join(if cfg!(windows) {
                "mcp-vision.exe"
            } else {
                "mcp-vision"
            });
        assert!(
            binary.exists(),
            "run `cargo build --release -p mcp-vision` first ({})",
            binary.display()
        );

        let config = McpServerConfig {
            name: "vision".to_string(),
            slot: "".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: binary.display().to_string(),
                args: vec![],
                env: HashMap::from([(
                    "OLLAMA_URL".to_string(),
                    "http://127.0.0.1:1".to_string(), // deliberately unreachable
                )]),
            },
            requires_confirmation: false,
            timeout_secs: 30,
        };

        let handle = McpServerHandle::connect(&config)
            .await
            .expect("vision MCP server should connect over stdio");

        assert!(handle.tools().iter().any(|t| t.name == "analyze_image"));

        let image_path = repo_root.join("mcp_workspace").join("hello.txt");
        let outcome = handle
            .call_tool(
                "analyze_image",
                serde_json::json!({ "image_path": image_path.display().to_string(), "prompt": "describe" }),
            )
            .await;

        assert!(outcome.success);
        let rendered = serde_json::to_string(&outcome.result).unwrap();
        assert!(
            rendered.contains("failed to reach Ollama"),
            "expected a real connection-failure message, got: {rendered}"
        );

        handle.shutdown().await;
    }

    /// Proves a Python-distributed (`uvx`-launched) MCP server also works through the
    /// same Windows `cmd /C` shim-resolution path used for `npx`-launched servers.
    #[tokio::test]
    #[ignore]
    async fn fetch_server_retrieves_real_content() {
        let config = McpServerConfig {
            name: "fetch".to_string(),
            slot: "api_call".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "uvx".to_string(),
                args: vec!["mcp-server-fetch".to_string()],
                env: HashMap::new(),
            },
            requires_confirmation: false,
            timeout_secs: 60,
        };

        let handle = McpServerHandle::connect(&config)
            .await
            .expect("fetch MCP server should connect over stdio");

        assert!(!handle.tools().is_empty());
        let fetch_tool = &handle.tools()[0];

        let outcome = handle
            .call_tool(
                &fetch_tool.name,
                serde_json::json!({ "url": "https://example.com" }),
            )
            .await;

        assert!(outcome.success, "tool call failed: {:?}", outcome.error);
        let rendered = serde_json::to_string(&outcome.result).unwrap();
        assert!(
            rendered.to_lowercase().contains("example domain"),
            "expected real fetched content from example.com, got: {rendered}"
        );

        handle.shutdown().await;
    }
}
