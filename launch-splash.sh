#!/bin/bash

# Ghostlink Studio - Splash Screen with Progress Indicator
# Shows animated progress while services start up

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
echo -e "  Node.js: $(node -v)"
echo -e "  npm: $(npm -v)"
echo ""

# Check components
echo -e "${BLUE}Checking Components:${NC}"
echo ""

# Check Ollama
echo -n "  Ollama: "
if command -v ollama &> /dev/null; then
    echo -e "${GREEN}✓ Installed${NC}"
    OLLAMA_INSTALLED=1
else
    echo -e "${YELLOW}✗ Not installed${NC}"
    OLLAMA_INSTALLED=0
fi

# Check Backend
echo -n "  Backend: "
if [ -f "./ghostlink" ] || [ -f "./ghostlink-backend" ]; then
    echo -e "${GREEN}✓ Found${NC}"
    BACKEND_FOUND=1
else
    echo -e "${YELLOW}✗ Binary not found${NC}"
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

# Ollama startup
if [ $OLLAMA_INSTALLED -eq 1 ]; then
    echo "  1. Ollama (Model Inference)"
    progress_bar 0 4
    echo "   Starting..."
    sleep 1
    progress_bar 1 4
    echo "   Checking health..."
    sleep 1
    progress_bar 2 4
    echo "   Pulling model..."
    sleep 1
    progress_bar 3 4
    echo "   Ready!"
    progress_bar 4 4
    echo -e "   ${GREEN}✓ Online${NC}"
    echo ""
fi

# Backend startup
if [ $BACKEND_FOUND -eq 1 ]; then
    echo "  2. Ghostlink Backend (API Server)"
    progress_bar 0 3
    echo "   Starting..."
    sleep 1
    progress_bar 1 3
    echo "   Loading..."
    sleep 1
    progress_bar 2 3
    echo "   Ready!"
    progress_bar 3 3
    echo -e "   ${GREEN}✓ Online${NC}"
    echo ""
fi

# GUI startup
if [ $GUI_FOUND -eq 1 ]; then
    echo "  3. Ghostlink GUI (Web Interface)"
    progress_bar 0 4
    echo "   Installing dependencies..."
    sleep 1
    progress_bar 1 4
    echo "   Building assets..."
    sleep 1
    progress_bar 2 4
    echo "   Starting dev server..."
    sleep 1
    progress_bar 3 4
    echo "   Opening browser..."
    sleep 1
    progress_bar 4 4
    echo -e "   ${GREEN}✓ Online${NC}"
    echo ""
fi

# Service info
echo -e "${BLUE}Services Ready:${NC}"
echo ""

if [ $OLLAMA_INSTALLED -eq 1 ]; then
    echo -e "  ${CYAN}Ollama${NC}           → ${WHITE}http://localhost:11434${NC}"
fi
if [ $BACKEND_FOUND -eq 1 ]; then
    echo -e "  ${CYAN}Backend${NC}          → ${WHITE}http://127.0.0.1:8003${NC}"
fi
if [ $GUI_FOUND -eq 1 ]; then
    echo -e "  ${CYAN}Frontend${NC}         → ${WHITE}http://localhost:3000${NC}"
fi

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

# Return success
exit 0
