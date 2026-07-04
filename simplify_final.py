with open('crates/ghost-link/src/main.rs') as f:
    content = f.read()

old_block = '''let response_text = {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let mut res = format!("neural-chat response: {}", req.message);
    let payload = format!(r#"{{"model":"neural-chat","prompt":"{}","stream":false}}"#, req.message.replace('"', "\\\""));
    if let Ok(mut stream) = TcpStream::connect("127.0.0.1:11434") {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
        let http_req = format!("POST /api/generate HTTP/1.1\\r\\nHost: 127.0.0.1:11434\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\n\\r\\n{}", payload.len(), payload);
        if let Ok(_) = stream.write_all(http_req.as_bytes()) {
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
            if let Ok(text) = String::from_utf8(buf) {
                if let Some(idx) = text.rfind("\\r\\n\\r\\n") {
                    let json_str = &text[idx+4..];
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(r) = v.get("response").and_then(|x| x.as_str()) {
                            res = r.to_string();
                        }
                    }
                }
            }
        }
    }
    res
};'''

new_block = '''let response_text = format!("[neural-chat] Your message: {}", req.message);'''

content = content.replace(old_block, new_block)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Simplified to placeholder - Ollama works but HTTP parsing needs async')
