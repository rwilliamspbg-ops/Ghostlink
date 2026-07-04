with open('crates/ghost-link/src/main.rs') as f:
    lines = f.readlines()

# Find the route registration for /api/inference/chat
for i, line in enumerate(lines):
    if '.post("/api/inference/chat"' in line or '.route("/api/inference/chat"' in line:
        print(f"Found at line {i+1}")
        # Print surrounding context
        for j in range(max(0, i-5), min(len(lines), i+50)):
            print(f"{j+1:5}: {lines[j]}", end='')
        break
