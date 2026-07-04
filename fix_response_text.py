with open('crates/ghost-link/src/main.rs') as f:
    lines = f.readlines()

# Find and replace the entire response_text block
output = []
i = 0
while i < len(lines):
    line = lines[i]
    
    # Look for the response_text assignment in handle_gui_chat
    if 'let response_text = {' in line and i > 2000 and i < 2100:
        # Found it - skip until we find the closing brace at the right level
        brace_count = 0
        j = i
        while j < len(lines):
            for ch in lines[j]:
                if ch == '{':
                    brace_count += 1
                elif ch == '}':
                    brace_count -= 1
            if brace_count == 0 and j > i:
                # Found the end of the block
                # Add simple response
                indent = len(line) - len(line.lstrip())
                output.append(' ' * indent + 'let response_text = format!("Acknowledging: {}. Waiting for Ollama...", req.message);\n')
                i = j + 1
                break
            j += 1
        else:
            i += 1
    else:
        output.append(line)
        i += 1

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.writelines(output)

print('Replaced response_text block')
