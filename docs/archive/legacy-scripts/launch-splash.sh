#!/bin/bash

# Ghostlink Studio - Splash Screen (Linux/macOS)
# Now delegates to launch-complete.sh for native stack launch.

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
WHITE='\033[1;37m'
GRAY='\033[0;37m'
DIM='\033[2m'
NC='\033[0m'

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
echo "║          ${WHITE}███████║██║  ██║╚██████╔╝███████╗   ██║   ███████╗██║██║ ╚████║██║  ██╗${CYAN}          ║"
echo "║          ${WHITE}╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝${CYAN}          ║"
echo "║                                                                                ║"
echo "║                 ${WHITE}Distributed LLM Inference Fabric${CYAN}                         ║"
echo "║                 ${GRAY}Native mode - no Docker required${CYAN}                           ║"
echo "║                                                                                ║"
echo "╚════════════════════════════════════════════════════════════════════════════════╝"
echo ""

# System info
echo -e "${BLUE}System Information:${NC}"
echo -e "  OS: $(uname -s)"
echo -e "  Node.js: $(node -v 2>/dev/null || echo 'not installed')"
echo -e "  npm: $(npm -v 2>/dev/null || echo 'not installed')"
echo ""

# Execute the launch-complete script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$SCRIPT_DIR/launch-complete.sh" "$@"
