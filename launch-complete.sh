#!/bin/bash
set -e

# Ghostlink - Complete Auto-Launch Script (Linux/macOS)
# Starts backend and modern GUI automatically

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

echo ""
echo "================================================================================"
echo -e "${CYAN}  GHOSTLINK STUDIO - Auto-Launch${NC}"
echo -e "${CYAN}  Backend + Modern GUI${NC}"
echo "================================================================================"
echo ""

# Check Node.js
if ! command -v node &> /dev/null; then
    echo -e "${RED}ERROR: Node.js not found${NC}"
    exit 1
fi

NODE_VERSION=$(node -v | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 18 ]; then
    echo -e "${RED}ERROR: Node.js 18+ required${NC}"
    exit 1
fi

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

echo -e "${BLUE}[INFO]${NC} Script directory: $SCRIPT_DIR"
echo ""

# Check if ghostlink backend binary exists
if [ -f "./ghostlink" ] || [ -f "./ghostlink-backend" ]; then
    echo -e "${GREEN}[✓]${NC} Backend binary found"
    HAS_BACKEND=1
else
    echo -e "${YELLOW}[!]${NC} Backend binary not found - GUI will connect to http://127.0.0.1:8003"
    HAS_BACKEND=0
fi

# Check GUI
if [ ! -d "ghostlink_gui_modern" ]; then
    echo -e "${RED}ERROR: ghostlink_gui_modern directory not found${NC}"
    exit 1
fi

cd ghostlink_gui_modern

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo -e "${BLUE}[INFO]${NC} Installing dependencies..."
    npm install --legacy-peer-deps > /dev/null 2>&1
fi

echo ""
echo "================================================================================"
echo -e "${GREEN}Starting Services:${NC}"
echo ""

# Start backend if binary exists
if [ $HAS_BACKEND -eq 1 ]; then
    cd "$SCRIPT_DIR"
    echo -e "${BLUE}[1]${NC} Starting backend..."
    if [ -f "./ghostlink" ]; then
        ./ghostlink serve 0.0.0.0 8003 > /tmp/ghostlink-backend.log 2>&1 &
    else
        ./ghostlink-backend > /tmp/ghostlink-backend.log 2>&1 &
    fi
    BACKEND_PID=$!
    echo -e "${GREEN}[✓]${NC} Backend started (PID: $BACKEND_PID)"
    echo -e "${BLUE}    Log:${NC} /tmp/ghostlink-backend.log"
    sleep 2
fi

# Start GUI
cd "$SCRIPT_DIR/ghostlink_gui_modern"
echo -e "${BLUE}[2]${NC} Starting GUI..."
echo -e "${GREEN}[✓]${NC} Dev server starting"

# Open browser
# Open browser (with delay)
(
    sleep 5
    if command -v xdg-open &> /dev/null; then
        xdg-open "http://localhost:3000" 2>/dev/null || true
    elif command -v open &> /dev/null; then
        open "http://localhost:3000" 2>/dev/null || true
    fi
) &

echo ""
echo "================================================================================"
echo -e "${GREEN}Services Ready:${NC}"
echo ""
if [ $HAS_BACKEND -eq 1 ]; then
    echo -e "  Backend:  ${CYAN}http://127.0.0.1:8003${NC}"
fi
echo -e "  GUI:      ${CYAN}http://localhost:3000${NC}"
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop all services${NC}"
echo "================================================================================"
echo ""

# Start GUI (foreground)
npm run dev -- --host 0.0.0.0

# Cleanup on exit
trap "kill $BACKEND_PID 2>/dev/null; exit" SIGINT SIGTERM
