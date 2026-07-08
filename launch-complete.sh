#!/bin/bash
set -e

# Ghostlink - Complete Auto-Launch Script (Linux/macOS)
# Starts Ollama, backend and modern GUI automatically

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Show splash screen first
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

if [ -f "./launch-splash.sh" ]; then
    bash ./launch-splash.sh
fi

echo ""
echo "================================================================================"
echo -e "${GREEN}[STARTING SERVICES - PLEASE WAIT]${NC}"
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
echo -e "${GREEN}[INIT SERVICES]${NC}"
echo ""

# Start Ollama if available
OLLAMA_PID=""
if command -v ollama &> /dev/null; then
    # Check if Ollama is already running
    if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
        echo -e "${BLUE}[1]${NC} Starting Ollama..."
        cd "$SCRIPT_DIR"
        ollama serve > /tmp/ollama.log 2>&1 &
        OLLAMA_PID=$!
        echo -e "${GREEN}[✓]${NC} Ollama started (PID: $OLLAMA_PID)"
        echo -e "${BLUE}    Log:${NC} /tmp/ollama.log"
        sleep 3
        
        # Pull a default model if none exist
        MODELS=$(curl -s http://localhost:11434/api/tags 2>/dev/null | grep -o '"name":' | wc -l)
        if [ $MODELS -eq 0 ]; then
            echo -e "${BLUE}[INFO]${NC} Pulling mistral model (first run, ~2GB)..."
            ollama pull mistral > /tmp/ollama-pull.log 2>&1 &
            PULL_PID=$!
        fi
    else
        echo -e "${GREEN}[✓]${NC} Ollama already running on http://localhost:11434"
    fi
else
    echo -e "${YELLOW}[!]${NC} Ollama not installed"
    echo -e "${YELLOW}    Install: curl -fsSL https://ollama.ai/install.sh | sh${NC}"
    echo -e "${YELLOW}    Backend will use mock responses without real inference${NC}"
fi

# Start backend if binary exists
BACKEND_PID=""
if [ $HAS_BACKEND -eq 1 ]; then
    cd "$SCRIPT_DIR"
    BACKEND_NUM=$((${OLLAMA_PID:+2} + 1))
    echo -e "${BLUE}[$BACKEND_NUM]${NC} Starting backend..."
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
GUI_NUM=$((${BACKEND_PID:+2} + ${OLLAMA_PID:+1} + 1))
echo -e "${BLUE}[$GUI_NUM]${NC} Starting GUI..."
echo -e "${GREEN}[✓]${NC} Dev server starting"

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
echo -e "${GREEN}[SERVICES ONLINE]${NC}"
echo ""
if [ -n "$OLLAMA_PID" ] || command -v ollama &> /dev/null; then
    echo -e "  Ollama:   ${CYAN}http://localhost:11434${NC}"
fi
if [ $HAS_BACKEND -eq 1 ]; then
    echo -e "  Backend:  ${CYAN}http://127.0.0.1:8003${NC}"
fi
echo -e "  GUI:      ${CYAN}http://localhost:3000${NC}"
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop all services${NC}"
echo "================================================================================"
echo ""

# Start GUI (foreground)
echo "Starting development server..."
echo ""
npm run dev -- --host 0.0.0.0

# Cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}[SHUTTING DOWN...]${NC}"
    [ -n "$BACKEND_PID" ] && kill $BACKEND_PID 2>/dev/null || true
    [ -n "$OLLAMA_PID" ] && kill $OLLAMA_PID 2>/dev/null || true
    [ -n "$PULL_PID" ] && kill $PULL_PID 2>/dev/null || true
    echo -e "${GREEN}[SHUTDOWN COMPLETE]${NC}"
}

trap cleanup SIGINT SIGTERM EXIT
