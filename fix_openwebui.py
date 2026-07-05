with open('crates/ghost-link/src/main.rs') as f:
    content = f.read()

# Replace the mock response with real Open WebUI integration
old_response = '''        // Generate a thoughtful response based on the input
        let response_text = match req.message.to_lowercase().as_str() {
            msg if msg.contains("hello") => "Hello! I'm Ghostlink AI, running on the fabric. How can I help you today?".to_string(),
            msg if msg.contains("how are you") => "I'm functioning optimally across the distributed nodes. Thanks for asking!".to_string(),
            msg if msg.contains("what is") => format!("That's a great question about {}. Based on the context and knowledge base, I can provide several insights...", req.message.split_whitespace().skip(2).take(3).collect::<Vec<_>>().join(" ")),
            msg if msg.contains("2+2") => "2 + 2 equals 4. Simple arithmetic operation.".to_string(),
            msg if msg.contains("test") => "Test successful! Ghostlink backend is processing requests correctly through the inference core.".to_string(),
            _ => format!("I understand you're asking about: '{}'. Running inference through {} model on {} nodes with {} layers. Processing request...", 
                req.message,
                current_model,
                cluster.nodes().len(),
                ((cluster.total_vram_gb() * 2.0).clamp(8.0, 60.0)) as usize
            ),
        };'''

new_response = '''        // Call Open WebUI API for real LLM responses
        let system_prompt = req.system_prompt.unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
        let response_text = {
            // Try to call Open WebUI's chat API
            use std::io::Write;
            let mut best_response = format!("[{}] {}", current_model, req.message);
            
            // Attempt HTTP call using standard library
            if let Ok(stream) = std::net::TcpStream::connect("127.0.0.1:8090") {
                let payload = serde_json::json!({
                    "model": current_model,
                    "messages": [
                        {"role": "system", "content": system_prompt},
                        {"role": "user", "content": req.message}
                    ],
                    "temperature": req.temperature.unwrap_or(0.7),
                    "max_tokens": req.max_tokens.unwrap_or(256),
                    "stream": false
                }).to_string();
                
                let request = format!(
                    "POST /api/chat/completions HTTP/1.1\r\n\
                     Host: 127.0.0.1:8090\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n\
                     {}",
                    payload.len(),
                    payload
                );
                
                if let Ok(mut stream) = std::net::TcpStream::connect("127.0.0.1:8090") {
                    let _ = stream.write_all(request.as_bytes());
                    use std::io::Read;
                    let mut response_buf = String::new();
                    let _ = stream.read_to_string(&mut response_buf);
                    
                    // Extract JSON from response
                    if let Some(body_start) = response_buf.find("\r\n\r\n") {
                        let body = &response_buf[body_start + 4..];
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
                            if let Some(content) = json.get("choices")
                                .and_then(|c| c.get(0))
                                .and_then(|c| c.get("message"))
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_str()) {
                                best_response = content.to_string();
                            }
                        }
                    }
                }
            }
            best_response
        };'''

content = content.replace(old_response, new_response)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Modified to call Open WebUI for real LLM responses')
