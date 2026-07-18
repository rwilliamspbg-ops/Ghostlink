#!/bin/bash

# Ghostlink Studio - GUI launcher (Linux/macOS)
# Starts the frontend only; backend must already be running.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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
    exit 1
fi

# Check Node.js version
NODE_VERSION=$(node -v | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 18 ]; then
    echo -e "${RED}ERROR: Node.js 18+ required, found version $(node -v)${NC}"
    exit 1
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
echo -e "  ${GREEN}GUI will open automatically in your default browser${NC}"
echo "  Server running at: $GUI_URL"
echo "  Backend connected to: $BACKEND_URL"
echo -e "  Press ${YELLOW}Ctrl+C${NC} to stop"
echo "================================================================================"
echo ""

# Try to open in browser
if command -v xdg-open &> /dev/null; then
    # Linux
    xdg-open "$GUI_URL" &
elif command -v open &> /dev/null; then
    # macOS
    open "$GUI_URL" &
fi

# Start the dev server
npm run dev -- --host 127.0.0.1 --port "$GUI_PORT"
