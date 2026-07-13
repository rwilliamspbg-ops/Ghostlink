# Ghostlink Launch Splash Screen Guide

## Overview

The launch splash screen provides a polished, animated startup experience for Ghostlink Studio. It shows:

- **Branded ASCII art banner** - Eye-catching Ghostlink logo
- **System information** - Node.js, npm versions
- **Component detection** - Checks for Ollama, Backend, GUI
- **Animated progress bars** - Shows startup progress for each service
- **Service endpoints** - Quick reference for all URLs
- **Quick start instructions** - How to use the platform

## Launch Scripts

### Linux/macOS
```bash
bash launch-complete.sh
```

Sequence:
1. Shows splash screen with ASCII art
2. Checks system components
3. Animates startup of each service
4. Displays service endpoints
5. Opens browser automatically
6. Shows quick start guide

### Windows
```bash
launch-complete.bat
```

Same sequence optimized for Windows command prompt:
- Cleaner output formatting
- Uses Windows-compatible progress indicators
- Handles paths with spaces
- Opens browser via `start` command

## What the Splash Screen Shows

### System Information Section
```
System Information:
  OS: Linux
  Node.js: v18.17.0
  npm: 9.8.1
```

### Component Check Section
```
Checking Components:
  Ollama: ✓ Installed
  Backend: ✓ Found
  GUI: ✓ Found
```

### Service Startup Section
Shows animated progress for each service:
- **Ollama** - Model inference engine
- **Backend** - API server (http://127.0.0.1:8003)
- **GUI** - Web interface (http://localhost:3000)

### Service Endpoints
```
Services Ready:
  Ollama     → http://localhost:11434
  Backend    → http://127.0.0.1:8003
  Frontend   → http://localhost:3000
```

## Progress Indicators

### Linux/macOS (Unicode)
```
  [==============------] 33%  Loading...
  [============================] 66%  Ready!
  [====================================] 100% Online!
```

### Windows (ASCII)
```
  [==========----------] 25% Starting...
  [====================] 50% Checking health...
  [============================] 75% Pulling model...
  [====================================] 100% Ready!
```

## Quick Start Instructions

After splash screen loads, you'll see:
```
Quick Start:
  1. Go to Models tab and select a model
  2. Switch to Chat tab
  3. Type a message and send
  4. Watch real model inference in action!
```

## Customization

### Modify Splash Screen

Edit `launch-splash.sh` or `launch-splash.bat` to customize:

**Colors (Bash):**
- `${CYAN}` - Service names
- `${GREEN}` - Success indicators
- `${YELLOW}` - Warnings
- `${MAGENTA}` - Borders
- `${WHITE}` - Main text

**ASCII Art:**
Replace the banner (lines 20-26) with your own

**Progress Bar Width:**
Change `width=40` in `progress_bar()` function

### Timing Adjustments

**Sleep durations** (in seconds):
- `sleep 1` - Quick transitions
- `sleep 2` - Slower, more visible progress
- `sleep 3` - Initial wait for services

Edit these values if your system is slower/faster.

## Troubleshooting

### Splash Screen Not Showing

**Linux/macOS:**
```bash
# Make script executable
chmod +x launch-splash.sh
chmod +x launch-complete.sh

# Run directly
bash launch-splash.sh
```

**Windows:**
```batch
REM Ensure .bat files are in project root
dir launch-splash.bat
```

### Colors Not Displaying (Bash)

If colors appear as gibberish:
- Terminal doesn't support ANSI colors
- Try: `export TERM=xterm-256color`
- Or upgrade terminal (iTerm2, modern Windows Terminal)

### Progress Bars Look Broken

- Unicode support issue on Windows
- Splash screen auto-detects and uses ASCII
- Should display correctly without modification

### Stuck on Ollama Startup

- First pull of Mistral (~2GB) takes time
- Check internet connection
- Monitor: `tail -f /tmp/ollama.log`

## What Happens After Splash Screen

1. **Splash screen exits** (after ~3 seconds of animations)
2. **Services start in background**
3. **Browser opens** to http://localhost:3000
4. **Frontend loads** (takes ~5 seconds)
5. **You're ready to chat!**

## Service Status After Launch

### All Services Running
```
✓ All services initialized successfully!
```

### Ollama Not Installed
```
! Ollama not installed
  Backend will fall back to mock responses without real inference
```

### Backend Binary Missing
```
! Backend binary not found - GUI will connect to http://127.0.0.1:8003
```

In all cases, the system continues - it just won't have real model inference without Ollama.

## Environment Variables

Control startup behavior:

```bash
# Use custom Ollama URL
export OLLAMA_BASE_URL=http://ollama-server:11434
bash launch-complete.sh

# Use custom backend port
export BACKEND_PORT=9000
bash launch-complete.sh
```

## Integration with CI/CD

### Skip Splash Screen
```bash
# Direct call to services (no splash)
./ghostlink serve 0.0.0.0 8003 &
ollama serve &
cd ghostlink_gui_modern && npm run dev
```

### Capture Startup Logs
```bash
bash launch-complete.sh > startup.log 2>&1
```

## Tips & Tricks

### Faster Development Workflow
```bash
# Terminal 1: Start with splash (handles all setup)
bash launch-complete.sh

# Browser opens automatically, ready to go!
```

### Multi-Window Monitoring
The script shows all three service URLs - open in separate tabs:
- http://localhost:3000 - Frontend
- http://127.0.0.1:8003 - Backend
- http://localhost:11434 - Ollama

### Checking Individual Services
```bash
# After launch, in another terminal:
curl http://localhost:11434/api/tags     # Ollama models
curl http://127.0.0.1:8003/health       # Backend health
```

---

**Splash screen makes startup professional and user-friendly while hiding complexity.** 🚀
