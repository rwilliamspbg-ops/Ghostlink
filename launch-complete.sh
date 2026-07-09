#!/bin/bash
#
# Ghostlink Studio - Complete Launch Script
# Starts all services: Backend API, GUI Frontend, and Runtime Detection
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
BACKEND_HOST="${GHOSTLINK_HOST:-127.0.0.1}"
BACKEND_PORT="${GHOSTLINK_PORT:-8003}"
GUI_PORT="${GUI_PORT:-5173}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Logging
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Cleanup on exit
cleanup() {
    log_warn "Shutting down services..."
    kill $BACKEND_PID 2>/dev/null || true
    kill $GUI_PID 2>/dev/null || true
    wait 2>/dev/null || true
    log_info "All services stopped"
}

trap cleanup EXIT INT TERM

# Check dependencies
check_dependencies() {
    log_info "Checking dependencies..."
    
    if ! command -v cargo &> /dev/null; then
        log_error "Rust/Cargo not found. Install from https://rustup.rs/"
        exit 1
    fi
    
    if ! command -v node &> /dev/null; then
        log_error "Node.js not found. Install from https://nodejs.org/"
        exit 1
    fi
    
    log_success "Dependencies verified"
}

# Build backend
build_backend() {
    log_info "Building backend (Ghostlink API)..."
    cd "$PROJECT_ROOT/crates/ghost-link"
    cargo build --release
    log_success "Backend built"
    cd "$PROJECT_ROOT"
}

# Start backend
start_backend() {
    log_info "Starting Ghostlink API Backend..."
    cd "$PROJECT_ROOT/crates/ghost-link"
    cargo run --release -- serve "$BACKEND_HOST" "$BACKEND_PORT" &
    BACKEND_PID=$!
    log_success "Backend started (PID: $BACKEND_PID)"
    sleep 3
}

# Start GUI
start_gui() {
    log_info "Starting Ghostlink Studio GUI..."
    cd "$PROJECT_ROOT/ghostlink_gui_modern"
    
    # Install dependencies if needed
    if [ ! -d "node_modules" ]; then
        log_info "Installing GUI dependencies..."
        npm install
    fi
    
    npm run dev &
    GUI_PID=$!
    log_success "GUI started (PID: $GUI_PID)"
    sleep 3
}

# Print startup summary
print_summary() {
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  Ghostlink Studio - Complete Launch${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Backend API:${NC}"
    echo -e "  URL: ${GREEN}http://${BACKEND_HOST}:${BACKEND_PORT}${NC}"
    echo -e "  Status: ${GREEN}✓ Running${NC}"
    echo -e "  PID: $BACKEND_PID"
    echo ""
    echo -e "${BLUE}Runtime Detection Endpoints:${NC}"
    echo -e "  Detect:      ${GREEN}GET /api/runtime/detect${NC}"
    echo -e "  Models:      ${GREEN}GET /api/runtime/models?runtime=cpu${NC}"
    echo -e "  Recommend:   ${GREEN}GET /api/runtime/recommend?memory_gb=16${NC}"
    echo ""
    echo -e "${BLUE}GUI Frontend:${NC}"
    echo -e "  URL: ${GREEN}http://localhost:${GUI_PORT}${NC}"
    echo -e "  Status: ${GREEN}✓ Running${NC}"
    echo -e "  PID: $GUI_PID"
    echo ""
    echo -e "${BLUE}Test Commands:${NC}"
    echo -e "  Runtime:     ${GREEN}curl http://${BACKEND_HOST}:${BACKEND_PORT}/api/runtime/detect${NC}"
    echo -e "  Models:      ${GREEN}curl 'http://${BACKEND_HOST}:${BACKEND_PORT}/api/runtime/models?runtime=cpu'${NC}"
    echo -e "  Recommend:   ${GREEN}curl 'http://${BACKEND_HOST}:${BACKEND_PORT}/api/runtime/recommend?memory_gb=8'${NC}"
    echo ""
    echo -e "${YELLOW}Ctrl+C to stop all services${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo ""
}

# Main execution
main() {
    log_info "Starting Ghostlink Studio complete launch..."
    echo ""
    
    check_dependencies
    build_backend
    start_backend
    start_gui
    print_summary
    
    # Keep script running
    wait
}

main "$@"
