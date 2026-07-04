with open('crates/ghost-link/src/main.rs') as f:
    lines = f.readlines()

# Find and fix the HTTP request string construction
output = []
for i, line in enumerate(lines):
    if 'POST /api/chat/completions HTTP/1.1' in line:
        # This is the problematic line - use raw string or fix escaping
        # Replace with simpler version that doesn't need raw strings
        output.append('                    let request = format!(\n')
        output.append('                        "POST /api/chat/completions HTTP/1.1\\r\\n"\n')
        output.append('                        "Host: 127.0.0.1:8090\\r\\n"\n')
        output.append('                        "Content-Type: application/json\\r\\n"\n')
        output.append('                        "Content-Length: {}\\r\\n"\n')
        output.append('                        "Connection: close\\r\\n"\n')
        output.append('                        "\\r\\n{}",\n')
        output.append('                        payload.len(),\n')
        output.append('                        payload\n')
        output.append('                    );\n')
        # Skip the next 8 lines (the old format! call)
        skip_until = i + 8
        while i < skip_until:
            i += 1
            if i < len(lines):
                continue
    else:
        output.append(line)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.writelines(output)

print('Fixed HTTP request string escaping')
