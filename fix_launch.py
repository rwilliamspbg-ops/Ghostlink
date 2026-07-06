import sys
import os

file_path = "scripts/launch_studio.py"

with open(file_path, 'r') as f:
    lines = f.readlines()

# We'll process line by line and build new lines.
new_lines = []
i = 0
while i < len(lines):
    line = lines[i]
    # Change the default for requested_gui_mode from 'tauri' to 'tkinter'
    if line.strip().startswith("requested_gui_mode = os.getenv('GHOSTLINK_STUDIO_GUI', 'tauri')"):
        new_lines.append("    requested_gui_mode = os.getenv('GHOSTLINK_STUDIO_GUI', 'tkinter').strip().lower()\n")
        i += 1
        continue
    # Change the fallback from 'tauri' to 'tkinter' in the check for valid values
    if line.strip().startswith("if requested_gui_mode not in {'tauri', 'tkinton'}:"):
        # We need to change the next line as well
        new_lines.append(line)
        i += 1
        # The next line should be the indented block
        if i < len(lines) and lines[i].strip().startswith("requested_gui_mode = 'tauri'"):
            new_lines.append("        requested_gui_mode = 'tkinter'\n")
            i += 1
            continue
        else:
            # If the next line is not as expected, just add the current line and move on
            new_lines.append(lines[i])
            i += 1
            continue
    # Remove the failure condition block and replace with fallback logic
    if line.strip().startswith("if not no_gui and requested_gui_mode == 'tauri' and not tauri_ready:"):
        # Skip this line and the next 5 lines (the fail block)
        i += 1  # skip the if line
        # Skip until we are past the fail block
        while i < len(lines) and not (lines[i].strip() == "" and i+1 < len(lines) and lines[i+1].strip().startswith("# 1. Build Backend")):
            i += 1
        # Now we are at the line before "# 1. Build Backend", so we will add that line in the next iteration
        # But we need to insert our fallback logic before that.
        # We'll insert the fallback logic here and then let the loop continue to add the "# 1. Build Backend" line.
        new_lines.append("    # If Tauri is not ready and we are not in no_gui mode, fallback to tkinter\n")
        new_lines.append("    if not no_gui and requested_gui_mode == 'tauri' and not tauri_ready:\n")
        new_lines.append("        effective_gui_mode = 'tkinter'\n")
        new_lines.append("        log(\"[WARN] Tauri GUI prerequisites missing (cargo tauri and/or npm), falling back to tkinter GUI.\")\n")
        new_lines.append("    else:\n")
        new_lines.append("        effective_gui_mode = requested_gui_mode\n")
        # Now we continue without incrementing i, so the next line (the "# 1. Build Backend") will be processed in the next iteration.
        continue
    # In the check_only block, we want to replace the logging section for GUI mode
    if line.strip().startswith("if no_gui:"):
        # We will replace from this line to the end of the else block (which is after the else: log line)
        # We'll insert our new block and then skip until we are past the existing block.
        new_lines.append("    if no_gui:\n")
        new_lines.append("        effective_gui_mode = 'headless'\n")
        new_lines.append("    else:\n")
        new_lines.append("        if requested_gui_mode == 'tauri' and not tauri_ready:\n")
        new_lines.append("            effective_gui_mode = 'tkinter'\n")
        new_lines.append("            log(\"  [WARN] Tauri GUI prerequisites missing (cargo tauri and/or nm).\")\n")
        new_lines.append("            log(\"         Falling back to tkinter GUI.\")\n")
        new_lines.append("        else:\n")
        new_lines.append("            effective_gui_mode = requested_gui_mode\n")
        new_lines.append("    if no_gui:\n")
        new_lines.append("        log(\"  [OK] GUI launch mode: headless (--no-gui)\")\n")
        new_lines.append("    elif requested_gui_mode == 'tauri' and not tauri_ready:\n")
        new_lines.append("        log(\"  [WARN] Tauri GUI prerequisites missing (cargo tauri and/or nm).\")\n")
        new_lines.append("        log(\"         Falling back to tkinter GUI.\")\n")
        new_lines.append("    else:\n")
        new_lines.append("        log(f\"  [OK] GUI launch mode: {effective_gui_mode}\")\n")
        # Now skip the existing lines until we are past the existing block.
        # We'll skip until we see the line "log(\"Preflight completed successfully\")"
        i += 1
        while i < len(lines) and not lines[i].strip().startswith("log(\"Preflight completed successfully\")"):
            i += 1
        # Now we are at the line that we want to keep (the "log(\"Preflight completed successfully\")" line)
        # We will add it in the next iteration.
        continue
    # If none of the above, just add the line
    new_lines.append(line)
    i += 1

# Write the new lines back to the file
with open(file_path, 'w') as f:
    f.writelines(new_lines)

print("File updated successfully.")
