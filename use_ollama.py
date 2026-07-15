with open('crates/ghost-link/src/main.rs') as f:
    content = f.read()

# Replace Open WebUI call with Ollama API call
old = '''        // Call Open WebUI's v1 API (compatible with OpenAI format)
        let response_text = {
            let system_prompt = req.system_prompt.clone().unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
            let msg_clone = req.message.clone();
            let temp = req.temperature.unwrap_or(0.7);
            let max_tok = req.max_tokens.unwrap_or(256);
            
            // Call Open WebUI on port 8090
            let chat_req = serde_json::json!({
                "model": "mistral",
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": msg_clone}
                ],
                "temperature": temp,
                "max_tokens": max_tok,
                "stream": false
            });
            
            match (|| {
                use std::io::{Read, Write};
                use std::net::TcpStream;
                use std::time::Duration;
                
                let mut stream = TcpStream::connect("127.0.0.1:8090")?;
                stream.set_read_timeout(Some(Duration::from_secs(30)))?;
                
                let body = chat_req.to_string();
                let request = format!(
                    "POST /api/v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                
                stream.write_all(request.as_bytes())?;
                let mut buf = String::new();
                stream.read_to_string(&mut buf)?;
                
                // Extract JSON from HTTP response
                if let Some(idx) = buf.find("\r\n\r\n") {
                    let json_str = &buf[idx + 4..];
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                        // Try OpenAI format first
                        if let Some(content) = json.get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|o| o.get("message"))
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str()) {
                            return Ok(content.to_string());
                        }
                    }
                }
                Err(std::io::Error::new(std::io::ErrorKind::Other, "No response"))
            })() {
                Ok(response) => response,
                Err(e) => format!("Open WebUI connection error: {}", e),
            }
        };'''

new = '''        // Call Ollama API for real LLM inference (no authentication required)
        let response_text = {
            let system_prompt = req.system_prompt.clone().unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
            let msg_clone = req.message.clone();
            let temp = req.temperature.unwrap_or(0.7);
            
            // Call Ollama's generate endpoint on port 11434
            let ollama_req = serde_json::json!({
                "model": "mistral",
                "prompt": msg_clone,
                "system": system_prompt,
                "stream": false,
                "temperature": temp
            });
            
            match (|| {
                use std::io::{Read, Write};
                use std::net::TcpStream;
                use std::time::Duration;
                
                let mut stream = TcpStream::connect("127.0.0.1:11434")?;
                stream.set_read_timeout(Some(Duration::from_secs(60)))?;
                stream.set_write_timeout(Some(Duration::from_secs(10)))?;
                
                let body = ollama_req.to_string();
                let request = format!(
                    "POST /api/generate HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                
                stream.write_all(request.as_bytes())?;
                let mut buf = String::new();
                stream.read_to_string(&mut buf)?;
                
                // Extract JSON from HTTP response
                if let Some(idx) = buf.find("\r\n\r\n") {
                    let json_str = &buf[idx + 4..];
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(response) = json.get("response").and_then(|r| r.as_str()) {
                            return Ok(response.to_string());
                        }
                    }
                }
                Err(std::io::Error::new(std::io::ErrorKind::Other, "No response"))
            })() {
                Ok(response) => response,
                Err(e) => format!("Ollama error: {}. Is Ollama running on port 11434?", e),
            }
        };'''

content = content.replace(old, new)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Modified to call Ollama API (port 11434) for real LLM responses')
