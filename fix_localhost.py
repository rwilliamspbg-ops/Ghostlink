import re

with open('crates/ghost-link/src/main.rs', 'r') as f:
    content = f.read()

# Find and fix the address string construction
# Replace localhost with 127.0.0.1 OR use DNS resolution
old_pattern = r'let addr_string = format!\("{}:{}", host, port\);'
new_pattern = '''let addr_string = if host == "localhost" || host == "localhost." {
        format!("127.0.0.1:{}", port)
    } else {
        format!("{}:{}", host, port)
    };'''

content = re.sub(old_pattern, new_pattern, content)

# Alternative: resolve localhost to 127.0.0.1 using std::net
# But the simpler fix above should work

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.write(content)

print('Fixed localhost -> 127.0.0.1 conversion')
