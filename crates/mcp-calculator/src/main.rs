//! Minimal stdio MCP server exposing one `calculate` tool, backed by `evalexpr`'s
//! sandboxed expression evaluator (no shell, no arbitrary code execution) — this is
//! the real backend for Ghostlink chat's `calculator` tool slot, replacing the old
//! hardcoded "42 (Calculated via Rust built-in tool)" stub.

use rmcp::{
    handler::server::wrapper::Parameters, schemars, tool, tool_router, transport::stdio, ServiceExt,
};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CalculateRequest {
    #[schemars(description = "A math expression to evaluate, e.g. \"17 * 23\" or \"(4 + 5) / 3\"")]
    expression: String,
}

#[derive(Debug, Clone)]
struct Calculator;

#[tool_router(server_handler)]
impl Calculator {
    #[tool(description = "Evaluate a math expression and return the numeric result")]
    fn calculate(
        &self,
        Parameters(CalculateRequest { expression }): Parameters<CalculateRequest>,
    ) -> String {
        match evalexpr::eval(&expression) {
            Ok(value) => value.to_string(),
            Err(err) => format!("error: {err}"),
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

    let service = Calculator.serve(stdio()).await.inspect_err(|err| {
        tracing::error!("mcp-calculator serving error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}
