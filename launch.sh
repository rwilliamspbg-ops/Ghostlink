#!/bin/bash

# Ghostlink - Unified launcher script (Linux/macOS)
# Automatically starts backend and GUI

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo ""
echo "========================================"
echo "  GHOSTLINK STUDIO"
echo "  Unified Launcher"
echo "========================================"
echo ""

# Check if backend is available
if [ ! -f "ghostlink" ] && [ ! -f "ghostlink.exe" ]; then
    echo -e "${YELLOW}[WARN]${NC} Ghostlink backend binary not found"
    echo -e "${BLUE}[INFO]${NC} GUI will connect to: http://127.0.0.1:8003"
    BACKEND_ONLY=0
else
    BACKEND_ONLY=1
fi

# Start GUI
cd ghostlink_gui_modern
echo -e "${GREEN}[✓]${NC} Starting Modern GUI..."
bash launch-gui.sh
