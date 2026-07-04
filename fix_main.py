import re

with open('crates/ghost-link/src/main.rs', 'r') as f:
    lines = f.readlines()

# Find the line with the problematic addr parsing (around line 2227)
output = []
for i, line in enumerate(lines):
    # Before rt.block_on, add the addr parsing
    if 'let rt = tokio::runtime::Builder::new_multi_thread()' in line:
        output.append('    let addr_string = format!("{}:{}", host, port);\n')
        output.append('    let addr: SocketAddr = addr_string.parse()\n')
        output.append('        .map_err(|e: std::net::AddrParseError| anyhow::anyhow!("Invalid socket address {}: {}", addr_string, e))?;\n')
        output.append('\n')
        output.append(line)
    # Inside the block, skip the duplicate addr parsing
    elif 'let addr: SocketAddr = format!("{}:{}", host, port).parse().unwrap();' in line:
        output.append('        // addr already parsed above\n')
    else:
        output.append(line)

with open('crates/ghost-link/src/main.rs', 'w') as f:
    f.writelines(output)

print('Fixed main.rs - moved addr parsing before rt.block_on')
