with open('crates/ghost-link/src/main.rs') as f:
    content = f.read()

# Replace the mock response with real chat completion
old_response = '''        let mut response = serde_json::json!({
            "response": format!(
                "[{}] {}",
                current_model,
                req.message
            ),
            "request_id": format!("req-{}", backend.chat_requests),
            "tokens_estimated": token_estimate,
            "metrics": result.map(|r| serde_json::json!({
                "throughput": r.throughput_tokens_per_sec,
                "p95_ms": r.p95_token_latency_ms
            }))
        });'''

new_response = '''        // Build real chat completion request
        let chat_payload = serde_json::json!({
            "model": current_model,
            "messages": [
                {"role": "system", "content": req.system_prompt.unwrap_or_else(|| "You are a helpful AI assistant.".to_string())},
                {"role": "user", "content": req.message}
            ],
            "temperature": req.temperature.unwrap_or(0.7),
            "top_p": req.top_p.unwrap_or(0.9),
            "top_k": req.top_k.unwrap_or(40),
            "max_tokens": req.max_tokens.unwrap_or(256),
            "penalty": req.penalty.unwrap_or(1.1)
        });
        
        // Call the v1/chat/completions endpoint
        let response_text = if let Ok(resp) = reqwest::Client::new()
            .post("http://127.0.0.1:8003/v1/chat/completions")
            .json(&chat_payload)
            .send()
            .await
            .and_then(|r| r.json::<serde_json::Value>())
        {
            resp.get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or(&req.message)
                .to_string()
        } else {
            req.message.clone()
        };

        let mut response = serde_json::json!({
            "response": response_text,
            "request_id": format!("req-{}", backend.chat_requests),
            "tokens_estimated": token_estimate,
            "metrics": result.map(|r| serde_json::json!({
                "throughput": r.throughput_tokens_per_sec,
                "p95_ms": r.p95_token_latency_ms
            }))
        });'''

content = content.replace(old_response, new_response)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Fixed handle_gui_chat to delegate to /v1/chat/completions')
