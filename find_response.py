with open('crates/ghost-link/src/main.rs') as f:
    lines = f.readlines()

# Find the handle_gui_chat function and look for the response construction
in_fn = False
for i, line in enumerate(lines):
    if 'async fn handle_gui_chat' in line:
        in_fn = True
        start = i
    
    if in_fn and i > start + 5:
        if 'json!({' in line or 'serde_json::json' in line:
            print(f"Found response construction at line {i+1}")
            # Print from here to end of function
            for j in range(i, min(len(lines), i+40)):
                print(f"{j+1:5}: {lines[j]}", end='')
            break
        
        if i > start + 150:  # Safety limit
            print("Reached end of function without finding response")
            break
