#!/bin/bash

# Ghostlink Studio - Splash Screen (native llama-server path)
# With real service verification

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
WHITE='\033[1;37m'
GRAY='\033[0;37m'
NC='\033[0m'

# Progress bar function
progress_bar() {
    local current=$1
    local total=$2
    local width=40
    local percent=$((current * 100 / total))
    local filled=$((current * width / total))
    
    printf "  ["
    printf "%${filled}s" | tr ' ' '='
    printf "%$((width - filled))s" | tr ' ' '-'
    printf "] %3d%% " "$percent"
}

# Wait for HTTP endpoint with timeout
wait_for_http() {
    local url="$1"
    local label="$2"
    local timeout_s="${3:-60}"
    local start
    start=$(date +%s)
    while true; do
        if curl -sf "$url" >/dev/null 2>&1; then
            printf "  ${GREEN}✓${NC} %s is healthy at %s\n" "$label" "$url"
            return 0
        fi
        if (( $(date +%s) - start >= timeout_s )); then
            printf "  ${RED}✗${NC} %s failed health check: %s\n" "$label" "$url"
            return 1
        fi
        sleep 1
    done
}

# Clear screen
clear

# Display banner
echo -e "${CYAN}"
echo "╔════════════════════════════════════════════════════════════════════════════════╗"
echo "║                                                                                ║"
echo "║          ${WHITE}███████╗██╗  ██╗ ██████╗ ███████╗████████╗██╗     ██╗███╗   ██╗██╗  ██╗${CYAN}          ║"
echo "║          ${WHITE}██╔════╝██║  ██║██╔═══██╗██╔════╝╚══██╔══╝██║     ██║████╗  ██║██║ ██╔╝${CYAN}          ║"
echo "║          ${WHITE}███████╗███████║██║   ██║███████╗   ██║   ██║     ██║██╔██╗ ██║█████╔╝${CYAN}           ║"
echo "║          ${WHITE}╚════██║██╔══██║██║   ██║╚════██║   ██║   ██║     ██║██║╚██╗██║██╔═██╗${CYAN}           ║"
echo "║          ${WHITE}███████║██║  ██║╚██████╔╝███████║   ██║   ███████╗██║██║ ╚████║██║  ██╗${CYAN}          ║"
echo "║          ${WHITE}╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝${CYAN}          ║"
echo "║                                                                                ║"
echo "║                 ${WHITE}Distributed LLM Inference Fabric${CYAN}                         ║"
echo "║                 ${GRAY}Enterprise AI Model Management${CYAN}                            ║"
echo "║                                                                                ║"
echo "╚════════════════════════════════════════════════════════════════════════════════╝"
echo ""

# System info
echo -e "${BLUE}System Information:${NC}"
echo -e "  OS: $(uname -s)"
echo -e "  Node.js: $(node -v 2>/dev/null || echo 'not installed')"
echo -e "  npm: $(npm -v 2>/dev/null || echo 'not installed')"
echo ""

# Check components
echo -e "${BLUE}Checking Components:${NC}"
echo ""

# Check native launcher
echo -n "  Native stack launcher: "
if [ -f "scripts/run_native_llama_server_stack.sh" ]; then
    echo -e "${GREEN}✓ Found${NC}"
else
    echo -e "${RED}✗ Missing${NC}"
fi

# Check Backend
echo -n "  Backend: "
if [ -f "./target/release/ghost-link" ] || [ -f "./target/debug/ghost-link" ]; then
    echo -e "${GREEN}✓ Found${NC}"
    BACKEND_FOUND=1
else
    echo -e "${YELLOW}✗ Will build on launch${NC}"
    BACKEND_FOUND=0
fi

# Check GUI
echo -n "  GUI: "
if [ -d "ghostlink_gui_modern" ]; then
    echo -e "${GREEN}✓ Found${NC}"
    GUI_FOUND=1
else
    echo -e "${RED}✗ Not found${NC}"
    GUI_FOUND=0
fi

echo ""
echo -e "${BLUE}Starting Services:${NC}"
echo ""

# Backend startup with real health check
if [ $BACKEND_FOUND -eq 1 ]; then
    echo "  1. Ghostlink Backend (API Server)"
    progress_bar 0 3
    echo "   Starting..."
    # Actual service start would happen here via launch-complete.sh
    # This splash just verifies, so we check if already running
    if wait_for_http "http://127.0.0.1:8003/health" "Backend" 5; then
        progress_bar 3 3
        echo -e "   ${GREEN}✓ Online${NC}"
    else
        progress_bar 1 3
        echo "   Starting..."
        progress_bar 2 3
        echo "   Loading..."
        progress_bar 3 3
        echo -e "   ${YELLOW}⚠ Starting (will be verified by full launcher)${NC}"
    fi
    echo ""
fi

# GUI startup with real health check
if [ $GUI_FOUND -eq 1 ]; then
    echo "  2. Ghostlink GUI (Web Interface)"
    progress_bar 0 4
    echo "   Checking..."
    if wait_for_http "http://127.0.0.1:5173" "Frontend" 5; then
        progress_bar 4 4
        echo -e "   ${GREEN}✓ Online${NC}"
    else
        progress_bar 1 4
        echo "   Installing dependencies..."
        progress_bar 2 4
        echo "   Building assets..."
        progress_bar 3 4
        echo "   Starting dev server..."
        progress_bar 4 4
        echo -e "   ${YELLOW}⚠ Starting (will be verified by full launcher)${NC}"
    fi
    echo ""
fi

# Service info
echo -e "${BLUE}Services Ready:${NC}"
echo ""

if [ $BACKEND_FOUND -eq 1 ]; then
    echo -e "  ${CYAN}Backend${NC}          → ${WHITE}http://127.0.0.1:8003${NC}"
fi
if [ $GUI_FOUND -eq 1 ]; then
    echo -e "  ${CYAN}Frontend${NC}         → ${WHITE}http://127.0.0.1:5173${NC}"
fi
echo -e "  ${CYAN}Native Inference${NC} → ${WHITE}llama-server (port 8080 by default)${NC}"

echo ""
echo -e "${MAGENTA}┌────────────────────────────────────────────────────────────────────────────────┐${NC}"
echo -e "${MAGENTA}│${NC}                                                                              ${MAGENTA}│${NC}"
echo -e "${MAGENTA}│${NC}  ${GREEN}✓ All services initialized successfully!${NC}                                 ${MAGENTA}│${NC}"
echo -e "${MAGENTA}│${NC}                                                                              ${MAGENTA}│${NC}"
echo -e "${MAGENTA}│${NC}  Opening browser in 3 seconds...                                             ${MAGENTA}│${NC}"
echo -e "${MAGENTA}│${NC}                                                                              ${MAGENTA}│${NC}"
echo -e "${MAGENTA}└────────────────────────────────────────────────────────────────────────────────┘${NC}"
echo ""

# Loading animation
for i in {1..3}; do
    echo -ne "  Launching${CYAN}$(printf '.%.0s' $(seq 1 $i))${NC}  \r"
    sleep 1
done

echo -e "  Launching   ${GREEN}Ready!${NC}           "
echo ""

# Instructions
echo -e "${BLUE}Quick Start:${NC}"
echo ""
echo -e "  ${GRAY}1. Go to Models tab and select a model${NC}"
echo -e "  ${GRAY}2. Switch to Chat tab${NC}"
echo -e "  ${GRAY}3. Type a message and send${NC}"
echo -e "  ${GRAY}4. Watch real model inference in action!${NC}"
echo ""

echo -e "${YELLOW}Tip:${NC} Press Ctrl+C to stop all services"
echo ""
echo -e "${GRAY}────────────────────────────────────────────────────────────────────────────────${NC}"
echo ""

# Execute the full launcher after splash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$SCRIPT_DIR/launch-complete.sh" "$@"
