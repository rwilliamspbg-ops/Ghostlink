#!/usr/bin/env bash
# Ghostlink Studio - Full Stack Launcher (Linux/macOS)

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

BACKEND_HOST="${GHOSTLINK_HOST:-127.0.0.1}"
BACKEND_PORT="${GHOSTLINK_PORT:-8003}"
GUI_PORT="${GUI_PORT:-5173}"
OLLAMA_URL="${OLLAMA_BASE_URL:-http://127.0.0.1:11434}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_PID=""
GUI_PID=""
OLLAMA_PID=""

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
    [[ -n "$BACKEND_PID" ]] && kill "$BACKEND_PID" 2>/dev/null || true
    [[ -n "$GUI_PID" ]] && kill "$GUI_PID" 2>/dev/null || true
    [[ -n "$OLLAMA_PID" ]] && kill "$OLLAMA_PID" 2>/dev/null || true
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

    if ! command -v curl &> /dev/null; then
        log_error "curl is required for health checks"
        exit 1
    fi
    
    log_success "Dependencies verified"
}

# Build backend
build_backend() {
    log_info "Building backend (Ghostlink API)..."
    cd "$PROJECT_ROOT"
    cargo build -p ghost-link
    log_success "Backend built"
}

wait_for_http() {
    local url="$1"
    local label="$2"
    local timeout_s="${3:-40}"
    local start
    start=$(date +%s)
    while true; do
        if curl -sf "$url" >/dev/null; then
            log_success "$label is healthy"
            return 0
        fi
        if (( $(date +%s) - start >= timeout_s )); then
            log_error "$label failed health check at $url"
            return 1
        fi
        sleep 1
    done
}

start_ollama_if_needed() {
    if curl -sf "$OLLAMA_URL/api/tags" >/dev/null; then
        log_success "Ollama already running at $OLLAMA_URL"
        return
    fi

    if ! command -v ollama &> /dev/null; then
        log_warn "Ollama is not installed; backend will run with fallback responses"
        return
    fi

    log_info "Starting Ollama service..."
    ollama serve >/tmp/ghostlink-ollama.log 2>&1 &
    OLLAMA_PID=$!
    wait_for_http "$OLLAMA_URL/api/tags" "Ollama" 30 || log_warn "Ollama did not report healthy in time"
}

start_services() {
    start_ollama_if_needed

    log_info "Starting Ghostlink API Backend..."
    cd "$PROJECT_ROOT"
    cargo run -p ghost-link -- serve "$BACKEND_HOST" "$BACKEND_PORT" >/tmp/ghostlink-backend.log 2>&1 &
    BACKEND_PID=$!
    wait_for_http "http://$BACKEND_HOST:$BACKEND_PORT/health" "Backend" 60

    log_info "Starting Ghostlink Studio GUI..."
    cd "$PROJECT_ROOT/ghostlink_gui_modern"
    if [ ! -d "node_modules" ]; then
        log_info "Installing GUI dependencies..."
        npm install --legacy-peer-deps
    fi

    npm run dev -- --host 127.0.0.1 --port "$GUI_PORT" >/tmp/ghostlink-frontend.log 2>&1 &
    GUI_PID=$!
    wait_for_http "http://127.0.0.1:$GUI_PORT" "Frontend" 60
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
    echo -e "${BLUE}Ollama:${NC}"
    echo -e "  URL: ${GREEN}${OLLAMA_URL}${NC}"
    if curl -sf "$OLLAMA_URL/api/tags" >/dev/null; then
        echo -e "  Status: ${GREEN}✓ Reachable${NC}"
    else
        echo -e "  Status: ${YELLOW}⚠ Unreachable (fallback mode)${NC}"
    fi
    if [[ -n "$OLLAMA_PID" ]]; then
        echo -e "  PID: $OLLAMA_PID"
    fi
    echo ""
    echo -e "${BLUE}GUI Frontend:${NC}"
    echo -e "  URL: ${GREEN}http://127.0.0.1:${GUI_PORT}${NC}"
    echo -e "  Status: ${GREEN}✓ Running${NC}"
    echo -e "  PID: $GUI_PID"
    echo ""
    echo -e "${BLUE}Test Commands:${NC}"
    echo -e "  Health:      ${GREEN}curl http://${BACKEND_HOST}:${BACKEND_PORT}/health${NC}"
    echo -e "  Models:      ${GREEN}curl http://${BACKEND_HOST}:${BACKEND_PORT}/api/models${NC}"
    echo -e "  Metrics:     ${GREEN}curl http://${BACKEND_HOST}:${BACKEND_PORT}/api/metrics${NC}"
    echo ""
    echo -e "${YELLOW}Ctrl+C to stop all services${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo ""

    # Keep script running
    wait
}

# Main execution
main() {
    log_info "Starting Ghostlink Studio full stack launch..."
    echo ""
    
    check_dependencies
    build_backend
    start_services

    if command -v xdg-open &> /dev/null; then
        xdg-open "http://127.0.0.1:${GUI_PORT}" >/dev/null 2>&1 || true
    elif command -v open &> /dev/null; then
        open "http://127.0.0.1:${GUI_PORT}" >/dev/null 2>&1 || true
    fi

    print_summary
}

main "$@"
