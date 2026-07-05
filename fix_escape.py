with open('crates/ghost-link/src/main.rs') as f:
    lines = f.readlines()

# Find and fix the HTTP request string
output = []
for i, line in enumerate(lines):
    if 'POST /api/generate HTTP/1.1' in line and '\r' in line and '\\r' not in line:
        # This line has literal \r\n, need to escape them properly
        # Just replace the whole let request = ... block with a simple format string
        output.append('                let request = format!("POST /api/generate HTTP/1.1\\r\\nHost: 127.0.0.1:11434\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\n\\r\\n{}", ollama_payload.len(), ollama_payload);\n')
        # Skip the next 3-4 lines which are part of the multiline format
        continue
    elif i > 0 and 'Host: 127.0.0.1:11434' in output[-1] if output else False:
        continue
    elif i > 0 and 'Content-Type: application/json' in line:
        continue
    elif i > 0 and 'Content-Length: {}' in line:
        continue
    else:
        output.append(line)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.writelines(output)

print('Fixed HTTP request format')
