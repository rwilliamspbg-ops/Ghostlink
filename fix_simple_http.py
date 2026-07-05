with open('crates/ghost-link/src/main.rs') as f:
    content = f.read()

# Find the problematic section and replace with a simpler version
old_section = '''        // Call Open WebUI API for real LLM responses
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

new_section = '''        // Call Open WebUI API for real LLM responses
        let system_prompt = req.system_prompt.unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
        let response_text = {
            let mut response = format!("[{}] {}", current_model, req.message);
            
            // Try Open WebUI API endpoint
            let payload = serde_json::json!({
                "model": current_model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": req.message}
                ],
                "temperature": req.temperature.unwrap_or(0.7),
                "max_tokens": req.max_tokens.unwrap_or(256),
                "stream": false
            });
            
            // Use blocking HTTP call
            if let Ok(client) = (|| {
                use std::io::{Read, Write};
                use std::net::TcpStream;
                use std::time::Duration;
                
                let mut stream = TcpStream::connect("127.0.0.1:8090")?;
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                stream.set_write_timeout(Some(Duration::from_secs(5)))?;
                
                let body = payload.to_string();
                let request = format!(
                    "POST /api/chat/completions HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                
                stream.write_all(request.as_bytes())?;
                let mut buf = String::new();
                stream.read_to_string(&mut buf)?;
                
                // Parse response
                if let Some(idx) = buf.find("\r\n\r\n") {
                    let json_str = &buf[idx + 4..];
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(msg) = json.get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|o| o.get("message"))
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str()) {
                            response = msg.to_string();
                        }
                    }
                }
                Ok::<(), std::io::Error>(())
            })() {
                // Success path
            }
            
            response
        };'''

content = content.replace(old_section, new_section)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Simplified Open WebUI integration')
