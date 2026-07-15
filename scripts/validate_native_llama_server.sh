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
LLAMA_NGL="${LLAMA_NGL:--1}"

log() {
  printf '[native-validate] %s\n' "$*"
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
  printf '[native-validate] %s did not become ready: %s\n' "$label" "$url" >&2
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

mkdir -p "$MODEL_DIR"

if [[ ! -f "$MODEL_PATH" ]]; then
  log "downloading model to $MODEL_PATH"
  curl -L --fail -o "$MODEL_PATH" "$MODEL_URL"
fi

if [[ ! -x "$LLAMA_CPP_DIR/build/bin/llama-server" ]]; then
  log "llama-server not found. run scripts/run_native_llama_server_stack.sh first"
  exit 1
fi

LLAMA_SERVER_BIN="$LLAMA_CPP_DIR/build/bin/llama-server"

log "starting llama-server (ngl=$LLAMA_NGL)"
"$LLAMA_SERVER_BIN" -m "$MODEL_PATH" --host "$LLAMA_HOST" --port "$LLAMA_PORT" -ngl "$LLAMA_NGL" >/tmp/ghostlink_llama_server.log 2>&1 &
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

REQ='{"message":"Give one short sentence.","stream":false}'
RESP="$(curl -fsS -X POST "http://${API_HOST}:${API_PORT}/api/inference/chat" -H 'content-type: application/json' -d "$REQ")"

python3 - <<'PY' "$RESP"
import json
import sys
obj = json.loads(sys.argv[1])
real = obj.get('real_inference')
backend = obj.get('inference_backend')
response = str(obj.get('response', ''))
print(f"real_inference={real}")
print(f"inference_backend={backend}")
print(f"response={response[:220]}")
if real is not True or backend != 'native':
    raise SystemExit(1)
PY

log "validation passed"
