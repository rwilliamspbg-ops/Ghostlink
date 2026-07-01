#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-127.0.0.1}"
PORT="${2:-8003}"
BACKEND_URL="http://${HOST}:${PORT}"
BACKEND_LOG="${ROOT_DIR}/tmp/gui-real-backend.log"

mkdir -p "${ROOT_DIR}/tmp"

cd "${ROOT_DIR}"

echo "[INFO] Building ghost-link binary"
cargo build -p ghost-link >/dev/null

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
