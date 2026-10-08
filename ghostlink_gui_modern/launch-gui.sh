#!/bin/bash

# Ghostlink Studio - GUI launcher (Linux/macOS)
# Starts the frontend only; backend must already be running.

set -e

# Colors for output
RED='[0;31m'
GREEN='[0;32m'
YELLOW='[1;33m'
BLUE='[0;34m'
NC='[0m' # No Color

# Print header
echo ""
echo "================================================================================"
echo "  GHOSTLINK STUDIO - Advanced AI Model Management"
echo "================================================================================"
echo ""

# Check if Node.js is installed
if ! command -v node &> /dev/null; then
    echo -e "${RED}ERROR: Node.js is not installed${NC}"
    echo "Please install Node.js 18+ from https://nodejs.org/"
    return 1 2>/dev/null || exit 1
fi

# Check Node.js version
NODE_VERSION=$(node -v | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 18 ]; then
    echo -e "${RED}ERROR: Node.js 18+ required, found version $(node -v)${NC}"
    return 1 2>/dev/null || exit 1
fi

# Get script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

# Default backend URL
BACKEND_HOST="${1:-127.0.0.1}"
BACKEND_PORT="${2:-8003}"
BACKEND_URL="http://$BACKEND_HOST:$BACKEND_PORT"
GUI_PORT="${GUI_PORT:-5173}"
GUI_URL="http://localhost:$GUI_PORT"
export GHOSTLINK_API_BASE="$BACKEND_URL"
export VITE_GHOSTLINK_API_BASE="$BACKEND_URL"
export GHOSTLINK_BACKEND_URL="$BACKEND_URL"
export VITE_GHOSTLINK_BACKEND_URL="$BACKEND_URL"

echo -e "${BLUE}[INFO]${NC} Starting Ghostlink Studio components..."
echo -e "${BLUE}[INFO]${NC} Backend URL: $BACKEND_URL"
echo -e "${BLUE}[INFO]${NC} GUI URL: $GUI_URL"
echo ""

# Check and install dependencies
if [ ! -d "node_modules" ]; then
    echo -e "${BLUE}[INFO]${NC} Installing GUI dependencies..."
    npm install --legacy-peer-deps
fi

echo -e "${BLUE}[INFO]${NC} Starting development server..."
echo ""
echo "================================================================================"
echo "  Server running at: $GUI_URL"
echo "  Backend connected to: $BACKEND_URL"
echo -e "  Press ${YELLOW}Ctrl+C${NC} to stop"
echo "================================================================================"
echo ""

# Start the dev server first
npm run dev -- --host 127.0.0.1 --port "$GUI_PORT" &
DEV_PID=$!

trap 'kill "$DEV_PID" 2>/dev/null || true' EXIT SIGINT SIGTERM

# Poll until http://127.0.0.1:$GUI_PORT responds (10s max)
echo -e "${BLUE}[INFO]${NC} Waiting for dev server at http://127.0.0.1:$GUI_PORT..."
SERVER_READY=0
for i in $(seq 1 20); do
    if curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$GUI_PORT" 2>/dev/null | grep -qE "^(200|304|404)"; then
        SERVER_READY=1
        break
    fi
    sleep 0.5
done

if [ "$SERVER_READY" -eq 1 ]; then
    echo -e "  ${GREEN}GUI dev server is ready! Opening browser at $GUI_URL${NC}"
else
    echo -e "  ${YELLOW}Opening browser at $GUI_URL...${NC}"
fi

# Open browser after server responds or timeout
if command -v xdg-open &> /dev/null; then
    xdg-open "$GUI_URL" &
elif command -v open &> /dev/null; then
    open "$GUI_URL" &
fi

wait "$DEV_PID"
