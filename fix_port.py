with open('crates/ghost-link/src/main.rs') as f:
    content = f.read()

# Change serve default port from 8000 to 8003
content = content.replace(
    '.unwrap_or(8000)',
    '.unwrap_or(8003)'
)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Changed serve default port to 8003')
