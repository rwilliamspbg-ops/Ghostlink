#!/bin/bash

# Ghostlink Studio - Auto-Launch with Modern GUI (Linux/macOS)
# This script starts the backend and automatically opens the modern web GUI

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
GUI_PORT=3000
GUI_URL="http://localhost:$GUI_PORT"

echo -e "${BLUE}[INFO]${NC} Starting Ghostlink Studio components..."
echo -e "${BLUE}[INFO]${NC} Backend URL: $BACKEND_URL"
echo -e "${BLUE}[INFO]${NC} GUI URL: $GUI_URL"
echo ""

# Check if GUI directory exists
if [ ! -d "ghostlink_gui_modern" ]; then
    echo -e "${RED}ERROR: ghostlink_gui_modern directory not found${NC}"
    exit 1
fi

# Navigate to GUI directory
cd ghostlink_gui_modern

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
npm run dev
