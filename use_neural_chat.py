with open('crates/ghost-link/src/main.rs') as f:
    content = f.read()

# Replace the placeholder with actual Ollama call
old = 'let response_text = format!("Acknowledging: {}. Waiting for Ollama...", req.message);'
new = '''let response_text = {
            use std::io::{Read, Write};
            use std::net::TcpStream;
            use std::time::Duration;
            
            let mut result = format!("neural-chat: {}", req.message);
            
            // Call Ollama /api/generate endpoint with neural-chat model
            let ollama_payload = serde_json::json!({
                "model": "neural-chat",
                "prompt": req.message,
                "system": req.system_prompt.clone().unwrap_or_else(|| "You are a helpful AI assistant.".to_string()),
                "stream": false,
                "temperature": req.temperature.unwrap_or(0.7)
            }).to_string();
            
            if let Ok(mut stream) = TcpStream::connect("127.0.0.1:11434") {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                
                let request = format!(
                    "POST /api/generate HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    ollama_payload.len(),
                    ollama_payload
                );
                
                if stream.write_all(request.as_bytes()).is_ok() {
                    let mut buf = Vec::new();
                    let _ = stream.read_to_end(&mut buf);
                    
                    if let Ok(response_str) = String::from_utf8(buf) {
                        if let Some(idx) = response_str.find("\r\n\r\n") {
                            let json_part = &response_str[idx + 4..];
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_part) {
                                if let Some(response) = json.get("response").and_then(|r| r.as_str()) {
                                    result = response.to_string();
                                }
                            }
                        }
                    }
                }
            }
            
            result
        };'''

content = content.replace(old, new)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Updated to call Ollama neural-chat model')
