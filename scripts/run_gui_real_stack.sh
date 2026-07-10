#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-127.0.0.1}"
PORT="${2:-8003}"
BACKEND_URL="http://${HOST}:${PORT}"
BACKEND_LOG="${ROOT_DIR}/tmp/gui-real-backend.log"
CHECK_ONLY="${GHOSTLINK_CHECK_ONLY:-0}"

if [[ "${3:-}" == "--check" ]]; then
  CHECK_ONLY="1"
fi

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] Missing required command: ${cmd}" >&2
    exit 1
  fi
}

require_cmd cargo
require_cmd curl

mkdir -p "${ROOT_DIR}/tmp"

cd "${ROOT_DIR}"

echo "[INFO] Building ghost-link binary"
cargo build -p ghost-link >/dev/null

if [[ "$CHECK_ONLY" == "1" ]]; then
  echo "[OK] Preflight completed (check-only mode)"
  exit 0
fi

echo "[INFO] Starting real backend on ${BACKEND_URL}"
cargo run -p ghost-link -- serve "${HOST}" "${PORT}" >"${BACKEND_LOG}" 2>&1 &
BACKEND_PID=$!

cleanup() {
  if kill -0 "${BACKEND_PID}" >/dev/null 2>&1; then
    kill "${BACKEND_PID}" >/dev/null 2>&1 || true
    wait "${BACKEND_PID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "[INFO] Waiting for backend health endpoint"
for _ in $(seq 1 40); do
  if curl -fsS "${BACKEND_URL}/health" >/dev/null 2>&1; then
    echo "[OK] Backend is healthy"
    break
  fi
  sleep 0.25
done

if ! curl -fsS "${BACKEND_URL}/health" >/dev/null 2>&1; then
  echo "[ERROR] Backend failed to become healthy. See ${BACKEND_LOG}"
  exit 1
fi

echo "[INFO] Launching GUI against ${BACKEND_URL}"
# Use explicit backend URL and disable auto-backend so this path always tests the real server process.
cargo run -p ghost-link -- gui --backend-url "${BACKEND_URL}" --no-auto-backend
