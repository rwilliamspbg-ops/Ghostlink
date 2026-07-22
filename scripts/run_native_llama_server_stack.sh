#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
THIRD_PARTY_DIR="${ROOT_DIR}/third_party"
LLAMA_CPP_DIR="${THIRD_PARTY_DIR}/llama.cpp"
MODEL_DIR="${MODEL_DIR:-/tmp/ghostlink-models}"
MODEL_PATH="${GHOSTLINK_MODEL_PATH:-${MODEL_DIR}/stories15M-q4_0.gguf}"
MODEL_URL="${GHOSTLINK_MODEL_URL:-https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf}"
LLAMA_HOST="${GHOSTLINK_LLAMA_SERVER_HOST:-127.0.0.1}"
LLAMA_PORT="${GHOSTLINK_LLAMA_SERVER_PORT:-8080}"
API_HOST="${GHOSTLINK_API_HOST:-127.0.0.1}"
API_PORT="${GHOSTLINK_API_PORT:-8003}"
# GPU layers: 0 = CPU only, -1 = all layers on GPU, or specific number
LLAMA_NGL="${LLAMA_NGL:--1}"
# Perf defaults: Flash Attention + larger prompt batches (override via GHOSTLINK_LLAMA_SERVER_ARGS)
LLAMA_PERF_ARGS="${GHOSTLINK_LLAMA_SERVER_ARGS:--fa on -b 2048 -ub 512}"

log() {
  printf '[native-stack] %s\n' "$*"
}

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    printf '[native-stack] missing required command: %s\n' "$cmd" >&2
    exit 1
  fi
}

wait_http() {
  local url="$1"
  local label="$2"
  local attempts="${3:-60}"
  local i
  for i in $(seq 1 "$attempts"); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      log "$label is ready at $url"
      return 0
    fi
    sleep 1
  done
  printf '[native-stack] %s did not become ready: %s\n' "$label" "$url" >&2
  return 1
}

cleanup() {
  if [[ -n "${API_PID:-}" ]] && kill -0 "$API_PID" >/dev/null 2>&1; then
    kill "$API_PID" >/dev/null 2>&1 || true
    wait "$API_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${LLAMA_PID:-}" ]] && kill -0 "$LLAMA_PID" >/dev/null 2>&1; then
    kill "$LLAMA_PID" >/dev/null 2>&1 || true
    wait "$LLAMA_PID" >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT INT TERM

require_cmd cargo
require_cmd git
require_cmd cmake
require_cmd curl

mkdir -p "$MODEL_DIR"

if [[ ! -f "$MODEL_PATH" ]]; then
  log "downloading model to $MODEL_PATH"
  curl -L --fail -o "$MODEL_PATH" "$MODEL_URL"
fi

if [[ ! -d "$LLAMA_CPP_DIR" ]]; then
  log "cloning llama.cpp"
  mkdir -p "$THIRD_PARTY_DIR"
  git clone https://github.com/ggml-org/llama.cpp.git "$LLAMA_CPP_DIR"
fi

if [[ ! -x "$LLAMA_CPP_DIR/build/bin/llama-server" ]]; then
  log "building llama-server"
  cmake -S "$LLAMA_CPP_DIR" -B "$LLAMA_CPP_DIR/build" -DCMAKE_BUILD_TYPE=Release
  local BUILD_JOBS="${CMAKE_BUILD_PARALLEL_LEVEL:-2}"
  cmake --build "$LLAMA_CPP_DIR/build" -j "$BUILD_JOBS" --target llama-server
fi

LLAMA_SERVER_BIN="$LLAMA_CPP_DIR/build/bin/llama-server"

log "starting llama-server (ngl=$LLAMA_NGL, perf=$LLAMA_PERF_ARGS)"
# shellcheck disable=SC2086
"$LLAMA_SERVER_BIN" -m "$MODEL_PATH" --host "$LLAMA_HOST" --port "$LLAMA_PORT" -ngl "$LLAMA_NGL" $LLAMA_PERF_ARGS >/tmp/ghostlink_llama_server.log 2>&1 &
LLAMA_PID=$!

wait_http "http://${LLAMA_HOST}:${LLAMA_PORT}/health" "llama-server"

log "starting Ghostlink API"
(
  cd "$ROOT_DIR"
  GHOSTLINK_INFERENCE_BACKEND=native \
  GHOSTLINK_NATIVE_ENGINE=llama_server \
  GHOSTLINK_LLAMA_SERVER_URL="http://${LLAMA_HOST}:${LLAMA_PORT}/completion" \
  cargo run -p ghost-link -- serve "$API_HOST" "$API_PORT"
) >/tmp/ghostlink_native_api.log 2>&1 &
API_PID=$!

wait_http "http://${API_HOST}:${API_PORT}/health" "Ghostlink API"

log "stack is live"
log "llama-server: http://${LLAMA_HOST}:${LLAMA_PORT}"
log "ghostlink api: http://${API_HOST}:${API_PORT}"
log "logs: /tmp/ghostlink_llama_server.log and /tmp/ghostlink_native_api.log"
log "press Ctrl+C to stop"

wait "$API_PID"
