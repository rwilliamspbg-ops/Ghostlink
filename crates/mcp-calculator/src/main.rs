//! Minimal stdio MCP server exposing one `calculate` tool, backed by `evalexpr`'s
//! sandboxed expression evaluator (no shell, no arbitrary code execution) — this is
//! the real backend for Ghostlink chat's `calculator` tool slot, replacing the old
//! hardcoded "42 (Calculated via Rust built-in tool)" stub.

use rmcp::{
    handler::server::wrapper::Parameters, schemars, tool, tool_router, transport::stdio, ServiceExt,
};

/// Evaluate a math expression and return the result as a string.
/// This function is extracted from the tool handler to enable unit testing.
pub fn evaluate_expression(expression: &str) -> String {
    match evalexpr::eval(expression) {
        Ok(value) => value.to_string(),
        Err(err) => format!("error: {err}"),
    }
}

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
        evaluate_expression(&expression)
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

#[cfg(test)]
mod tests {
    use super::evaluate_expression;

    #[test]
    fn simple_addition() {
        assert_eq!(evaluate_expression("1 + 2"), "3");
    }

    #[test]
    fn simple_subtraction() {
        assert_eq!(evaluate_expression("10 - 3"), "7");
    }

    #[test]
    fn simple_multiplication() {
        assert_eq!(evaluate_expression("17 * 23"), "391");
    }

    #[test]
    fn simple_division() {
        assert_eq!(evaluate_expression("(4 + 5) / 3"), "3");
    }

    #[test]
    fn division_with_remainder() {
        // evalexpr does integer division when both operands are integers
        let result = evaluate_expression("10 / 3");
        assert_eq!(result, "3");
    }

    #[test]
    fn operator_precedence() {
        assert_eq!(evaluate_expression("2 + 3 * 4"), "14");
    }

    #[test]
    fn parentheses_override_precedence() {
        assert_eq!(evaluate_expression("(2 + 3) * 4"), "20");
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(evaluate_expression("-5 + 3"), "-2");
    }

    #[test]
    fn zero_result() {
        assert_eq!(evaluate_expression("0 + 0"), "0");
    }

    #[test]
    fn large_number() {
        assert_eq!(evaluate_expression("1000000 * 1000000"), "1000000000000");
    }

    #[test]
    fn invalid_syntax_returns_error() {
        let result = evaluate_expression("1 +");
        assert!(
            result.starts_with("error:"),
            "Expected error, got: {result}"
        );
    }

    #[test]
    fn empty_expression_returns_unit() {
        // evalexpr returns () (unit) for empty string, not an error
        let result = evaluate_expression("");
        assert_eq!(result, "()");
    }

    #[test]
    fn division_by_zero_returns_error_or_infinity() {
        let result = evaluate_expression("1 / 0");
        // evalexpr may return inf or error depending on version
        assert!(
            result.starts_with("error:") || result.contains("inf") || result.contains("NaN"),
            "Expected error or infinity, got: {result}"
        );
    }

    #[test]
    fn whitespace_handling() {
        assert_eq!(evaluate_expression("  1  +  2  "), "3");
    }

    #[test]
    fn decimal_result() {
        let result = evaluate_expression("1.5 + 2.5");
        assert_eq!(result, "4");
    }
}
