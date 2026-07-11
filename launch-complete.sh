#!/usr/bin/env bash
# Ghostlink Studio - Native Llama Server Full Stack Launcher (Linux/macOS)

set -euo pipefail

BACKEND_HOST="${GHOSTLINK_API_HOST:-127.0.0.1}"
BACKEND_PORT="${GHOSTLINK_API_PORT:-8003}"
GUI_PORT="${GUI_PORT:-5173}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STACK_PID=""
GUI_PID=""

log() { printf '[launch-complete] %s\n' "$*"; }

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf '[launch-complete] missing required command: %s\n' "$1" >&2
        exit 1
    fi
}

wait_for_http() {
    local url="$1"
    local label="$2"
    local timeout_s="${3:-60}"
    local start
    start=$(date +%s)
    while true; do
        if curl -sf "$url" >/dev/null 2>&1; then
            log "$label is healthy at $url"
            return 0
        fi
        if (( $(date +%s) - start >= timeout_s )); then
            printf '[launch-complete] %s failed health check: %s\n' "$label" "$url" >&2
            return 1
        fi
        sleep 1
    done
}

cleanup() {
    log "stopping services"
    [[ -n "$GUI_PID" ]] && kill "$GUI_PID" 2>/dev/null || true
    [[ -n "$STACK_PID" ]] && kill "$STACK_PID" 2>/dev/null || true
    wait 2>/dev/null || true
}

trap cleanup EXIT INT TERM

main() {
    require_cmd bash
    require_cmd cargo
    require_cmd node
    require_cmd curl

    cd "$PROJECT_ROOT"

    log "starting native inference stack (llama-server + ghostlink api)"
    bash scripts/run_native_llama_server_stack.sh >/tmp/ghostlink-native-stack.log 2>&1 &
    STACK_PID=$!
    wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/health" "Backend" 80

    log "starting GUI"
    cd "$PROJECT_ROOT/ghostlink_gui_modern"
    if [[ ! -d node_modules ]]; then
        npm install --legacy-peer-deps
    fi
    npm run dev -- --host 127.0.0.1 --port "$GUI_PORT" >/tmp/ghostlink-frontend.log 2>&1 &
    GUI_PID=$!
    wait_for_http "http://127.0.0.1:${GUI_PORT}" "Frontend" 80

    log "stack ready"
    log "backend: http://${BACKEND_HOST}:${BACKEND_PORT}"
    log "frontend: http://127.0.0.1:${GUI_PORT}"
    log "native inference: llama-server via scripts/run_native_llama_server_stack.sh"
    log "logs: /tmp/ghostlink-native-stack.log and /tmp/ghostlink-frontend.log"

    wait
}

main "$@"
