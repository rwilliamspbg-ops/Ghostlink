//! Model-driven tool-calling: schema injection into the prompt, ReAct-style parsing
//! of the model's tool-call intent, and the observation text fed back on the next
//! turn. Works with any local model (no reliance on a model-specific tool-calling
//! chat template) since Ghostlink runs arbitrary GGUF/Ollama models.

use serde_json::Value;

use super::client::McpToolSchema;

/// Hard cap on tool round-trips per user turn, so a model that keeps requesting
/// tools (or misunderstands the marker) can't loop forever.
pub const MAX_TOOL_ITERATIONS: usize = 3;

const MARKER: &str = "TOOL_CALL:";

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToolCall {
    pub tool: String,
    pub args: Value,
}

/// Builds the instructions block describing every enabled tool, meant to be
/// prefixed onto the system/user prompt actually sent to the model. Returns an
/// empty string when there are no tools to offer (so callers can skip injection
/// entirely rather than sending a pointless empty header).
pub fn build_tool_instructions(tools: &[McpToolSchema]) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let mut block = String::from(
        "You have access to tools. To call one, reply with ONLY a single line of the \
         EXACT form below (not a function-call-looking expression, not any other syntax):\n\
         TOOL_CALL: {\"tool\": \"<tool_name>\", \"args\": { ... }}\n\
         Nothing else on that line — no other text before or after it.\n\n\
         Example:\n\
         User: What is 9 times 6?\n\
         Assistant: TOOL_CALL: {\"tool\": \"calculate\", \"args\": {\"expression\": \"9 * 6\"}}\n\
         (the tool result is then given to you as \"Observation: ...\")\n\
         Assistant: 9 times 6 is 54.\n\n\
         Rules:\n\
         - As soon as you receive an Observation, write your final answer in plain text \
         using that result. Do not call the same tool again for the same question.\n\
         - If no tool is needed, just answer normally in plain text.\n\n\
         Available tools:\n",
    );

    for tool in tools {
        let description = if tool.description.is_empty() {
            "(no description)"
        } else {
            tool.description.as_str()
        };
        block.push_str(&format!(
            "- {} — {}\n  input schema: {}\n",
            tool.name, description, tool.input_schema
        ));
    }

    block
}

/// Parses a `TOOL_CALL: {...}` marker out of the model's generated text. Tracks
/// brace depth (rather than trying to `serde_json::from_str` the whole remainder)
/// so trailing commentary after the JSON object doesn't break parsing.
pub fn extract_tool_call(text: &str) -> Option<ParsedToolCall> {
    let marker_pos = text.find(MARKER)?;
    let after_marker = &text[marker_pos + MARKER.len()..];
    let json_start_rel = after_marker.find('{')?;
    let json_region = &after_marker[json_start_rel..];

    let mut depth = 0i32;
    let mut end = None;
    for (i, ch) in json_region.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + ch.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let json_str = &json_region[..end];

    let parsed: Value = serde_json::from_str(json_str).ok()?;
    let tool = parsed.get("tool")?.as_str()?.to_string();
    let args = parsed.get("args").cloned().unwrap_or(Value::Null);
    Some(ParsedToolCall { tool, args })
}

/// Hard cap (in characters) on a single tool result folded back into the prompt.
/// A `fetch` call pulling a whole webpage (nav, footer, ads, unrelated content) can
/// otherwise blow the entire context budget in one shot regardless of how large
/// `--ctx-size` is configured — this bounds the damage a single observation can do.
const MAX_OBSERVATION_CHARS: usize = 4000;

/// Truncates `text` to `MAX_OBSERVATION_CHARS`, appending a marker noting how much
/// was cut so the model (and anyone reading the transcript) knows content is missing
/// rather than silently seeing a shortened result as if it were complete.
fn truncate_observation(text: &str) -> std::borrow::Cow<'_, str> {
    let total_chars = text.chars().count();
    if total_chars <= MAX_OBSERVATION_CHARS {
        return std::borrow::Cow::Borrowed(text);
    }
    let kept: String = text.chars().take(MAX_OBSERVATION_CHARS).collect();
    let omitted = total_chars - MAX_OBSERVATION_CHARS;
    std::borrow::Cow::Owned(format!(
        "{kept}... [truncated, {omitted} more characters omitted]"
    ))
}

/// Formats a tool result as an "Observation" turn appended to the running prompt
/// before asking the model to continue.
pub fn format_observation(tool: &str, result_json: &Value) -> String {
    let rendered = result_json.to_string();
    let observation = truncate_observation(&rendered);
    format!("\nObservation ({tool}): {observation}\n")
}

/// Formats a denial (confirmation gate rejected by the user) as an observation.
pub fn format_denial(tool: &str) -> String {
    format!("\nObservation ({tool}): the user denied permission to run this tool.\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_marker_returns_none() {
        assert_eq!(extract_tool_call("just a normal answer"), None);
    }

    #[test]
    fn parses_simple_call() {
        let text = r#"TOOL_CALL: {"tool": "calculate", "args": {"expression": "2+2"}}"#;
        let call = extract_tool_call(text).unwrap();
        assert_eq!(call.tool, "calculate");
        assert_eq!(call.args, serde_json::json!({"expression": "2+2"}));
    }

    #[test]
    fn tolerates_leading_and_trailing_commentary() {
        let text = "Sure, let me check that.\nTOOL_CALL: {\"tool\": \"read_text_file\", \"args\": {\"path\": \"a.txt\"}}\nI'll wait for the result.";
        let call = extract_tool_call(text).unwrap();
        assert_eq!(call.tool, "read_text_file");
        assert_eq!(call.args, serde_json::json!({"path": "a.txt"}));
    }

    #[test]
    fn handles_nested_braces_in_args() {
        let text = r#"TOOL_CALL: {"tool": "api_call", "args": {"body": {"nested": {"a": 1}}}}"#;
        let call = extract_tool_call(text).unwrap();
        assert_eq!(call.tool, "api_call");
        assert_eq!(call.args, serde_json::json!({"body": {"nested": {"a": 1}}}));
    }

    #[test]
    fn missing_tool_field_returns_none() {
        let text = r#"TOOL_CALL: {"args": {}}"#;
        assert_eq!(extract_tool_call(text), None);
    }

    #[test]
    fn no_args_defaults_to_null() {
        let text = r#"TOOL_CALL: {"tool": "list_allowed_directories"}"#;
        let call = extract_tool_call(text).unwrap();
        assert_eq!(call.tool, "list_allowed_directories");
        assert_eq!(call.args, Value::Null);
    }

    #[test]
    fn instructions_block_is_empty_for_no_tools() {
        assert_eq!(build_tool_instructions(&[]), "");
    }

    #[test]
    fn format_observation_passes_through_small_results_untouched() {
        let result = serde_json::json!({"content": "hello world"});
        let observation = format_observation("fetch", &result);
        assert_eq!(
            observation,
            "\nObservation (fetch): {\"content\":\"hello world\"}\n"
        );
    }

    #[test]
    fn format_observation_truncates_oversized_results() {
        let huge_text = "x".repeat(10_000);
        let result = serde_json::json!({"content": huge_text});
        let observation = format_observation("fetch", &result);
        assert!(
            observation.len() < 5_000,
            "expected truncation, got {} chars",
            observation.len()
        );
        assert!(observation.contains("truncated"));
        assert!(observation.contains("more characters omitted"));
    }

    #[test]
    fn format_observation_truncation_is_utf8_safe() {
        // Multi-byte chars near the truncation boundary shouldn't panic or split
        // a character in half.
        let huge_text = "é".repeat(5_000);
        let result = serde_json::json!({"content": huge_text});
        let observation = format_observation("fetch", &result);
        assert!(observation.contains("truncated"));
    }

    #[test]
    fn instructions_block_lists_each_tool() {
        let tools = vec![McpToolSchema {
            server: "calculator".to_string(),
            name: "calculate".to_string(),
            description: "Evaluate a math expression".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }];
        let block = build_tool_instructions(&tools);
        assert!(block.contains("TOOL_CALL:"));
        assert!(block.contains("calculate"));
        assert!(block.contains("Evaluate a math expression"));
    }
}
