#!/usr/bin/env bash
set -euo pipefail

BACKEND_HOST="${GHOSTLINK_API_HOST:-127.0.0.1}"
BACKEND_PORT="${GHOSTLINK_API_PORT:-8003}"
GUI_PORT="${GUI_PORT:-5173}"
LLAMA_PORT="${GHOSTLINK_LLAMA_SERVER_PORT:-8080}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LLAMA_NGL="${LLAMA_NGL:--1}"
MODEL_DIR="${PROJECT_ROOT}/models"
MODEL_FILE="${MODEL_DIR}/stories15M-q4_0.gguf"
MODEL_URL="https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

LLAMA_PID=""
API_PID=""
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
    [[ -n "$API_PID" ]] && kill "$API_PID" 2>/dev/null || true
    [[ -n "$LLAMA_PID" ]] && kill "$LLAMA_PID" 2>/dev/null || true
    wait 2>/dev/null || true
}

trap cleanup EXIT INT TERM

main() {
    require_cmd bash
    require_cmd cargo
    require_cmd node
    require_cmd curl

    cd "$PROJECT_ROOT"

    mkdir -p "$MODEL_DIR"

    if [[ ! -f "$MODEL_FILE" ]]; then
        log "downloading model to $MODEL_FILE"
        curl -L --fail -o "$MODEL_FILE" "$MODEL_URL" || {
            log "model download failed"
            exit 1
        }
    fi

    local LLAMA_SERVER_BIN=""
    for candidate in \
        "third_party/llama.cpp/build/bin/llama-server" \
        "third_party/llama.cpp/build/bin/Release/llama-server.exe" \
        "third_party/llama.cpp/build/bin/llama-server.exe"; do
        if [[ -f "$PROJECT_ROOT/$candidate" ]]; then
            LLAMA_SERVER_BIN="$PROJECT_ROOT/$candidate"
            break
        fi
    done

    local NATIVE_ENGINE="simulated"

    if [[ -n "$LLAMA_SERVER_BIN" ]]; then
        log "starting llama-server on port $LLAMA_PORT (ngl=$LLAMA_NGL)"
        "$LLAMA_SERVER_BIN" \
            -m "$MODEL_FILE" \
            --host 127.0.0.1 --port "$LLAMA_PORT" \
            -ngl "$LLAMA_NGL" \
            >/tmp/ghostlink_llama_server.log 2>&1 &
        LLAMA_PID=$!
        wait_for_http "http://127.0.0.1:${LLAMA_PORT}/health" "llama-server" 60
        NATIVE_ENGINE="llama_server"
    else
        log "llama-server binary not found, using simulated native mode"
        log "run launch.sh first to build llama-server"
    fi

    log "starting Ghostlink API on port $BACKEND_PORT"
    GHOSTLINK_INFERENCE_BACKEND=native \
    GHOSTLINK_NATIVE_ENGINE="$NATIVE_ENGINE" \
    GHOSTLINK_LLAMA_SERVER_URL="http://127.0.0.1:${LLAMA_PORT}/completion" \
    cargo run -p ghost-link -- serve "$BACKEND_HOST" "$BACKEND_PORT" \
        >/tmp/ghostlink_api.log 2>&1 &
    API_PID=$!
    wait_for_http "http://${BACKEND_HOST}:${BACKEND_PORT}/health" "Backend" 80

    log "starting GUI on port $GUI_PORT"
    cd "$PROJECT_ROOT/ghostlink_gui_modern"
    if [[ ! -d node_modules ]]; then
        npm install --legacy-peer-deps
    fi
    npm run dev -- --host 127.0.0.1 --port "$GUI_PORT" >/tmp/ghostlink_frontend.log 2>&1 &
    GUI_PID=$!
    cd "$PROJECT_ROOT"
    wait_for_http "http://127.0.0.1:${GUI_PORT}" "Frontend" 80

    log "stack ready"
    log "backend: http://${BACKEND_HOST}:${BACKEND_PORT}"
    log "frontend: http://127.0.0.1:${GUI_PORT}"
    log "native inference: http://127.0.0.1:${LLAMA_PORT} (llama-server)"
    log "logs: /tmp/ghostlink_api.log and /tmp/ghostlink_frontend.log"

    wait
}

main "$@"
