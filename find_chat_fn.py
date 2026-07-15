with open('crates/ghost-link/src/main.rs') as f:
    lines = f.readlines()

# Find the handle_gui_chat function
for i, line in enumerate(lines):
    if 'async fn handle_gui_chat' in line:
        print(f"Found at line {i+1}")
        # Print surrounding context
        for j in range(i, min(len(lines), i+100)):
            print(f"{j+1:5}: {lines[j]}", end='')
            if j > i and lines[j].strip() and not lines[j].startswith(' ' * 4) and 'async fn' in lines[j]:
                break
        break
