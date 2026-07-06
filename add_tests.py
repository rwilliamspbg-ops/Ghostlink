path = 'crates/ghost-link/src/main.rs'
with open(path, 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if '        #[test]' in line and 'fn test_detect_missing_python_modules' in lines[lines.index(line)+1]:
        continue
    if '        fn test_detect_missing_python_modules() {' in line:
        continue
    # skip the rest of the added function...
    # Better yet, just re-read and replace carefully.
    new_lines.append(line)

# Let's just fix it manually with a better script
