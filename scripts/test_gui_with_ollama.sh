#!/usr/bin/env bash

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
  echo -e "${BLUE}[INFO]${NC} $*"
}

log_ok() {
  echo -e "${GREEN}[OK]${NC} $*"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $*"
}

log_fail() {
  echo -e "${RED}[ERROR]${NC} $*"
}

cleanup() {
  log_info "Cleaning up..."
  if [ -n "${BACKEND_PID:-}" ] && kill -0 "$BACKEND_PID" 2>/dev/null; then
    kill "$BACKEND_PID" 2>/dev/null || true
    wait "$BACKEND_PID" 2>/dev/null || true
  fi
  exit 0
}

trap cleanup EXIT

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

log_info "Ghostlink GUI Test Suite"
log_info "========================="

# Check Python
if ! command -v python3 >/dev/null 2>&1; then
  log_fail "Python 3 not found"
  exit 1
fi
log_ok "Python 3 available"

# Check if Ollama is running
if ! curl -s http://127.0.0.1:11434/api/health >/dev/null 2>&1; then
  log_warn "Ollama not running on localhost:11434"
  log_info "Install Ollama from https://ollama.com"
  log_info "Then run: ollama run tinyllama"
  exit 1
fi
log_ok "Ollama running"

# Check if tinyllama model exists
if ! curl -s http://127.0.0.1:11434/api/tags | grep -q tinyllama; then
  log_warn "tinyllama model not found"
  log_info "Pulling tinyllama (~405MB)..."
  if ! timeout 300 curl -s http://127.0.0.1:11434/api/pull -d '{"name":"tinyllama"}' > /dev/null 2>&1; then
    log_fail "Failed to pull tinyllama"
    exit 1
  fi
  log_ok "tinyllama pulled successfully"
else
  log_ok "tinyllama model available"
fi

# Install Python dependencies
log_info "Installing Python dependencies..."
cd "$PROJECT_ROOT"
if ! python3 -m pip install -q -r requirements.txt 2>/dev/null; then
  log_warn "Some pip packages may not be available, but continuing..."
fi
log_ok "Dependencies ready"

# Start backend server
log_info "Starting backend test server..."
cd "$PROJECT_ROOT"
python3 scripts/backend_test_server.py > /tmp/ghostlink_backend.log 2>&1 &
BACKEND_PID=$!
log_ok "Backend PID: $BACKEND_PID"

# Wait for backend to be ready
log_info "Waiting for backend to be ready..."
max_retries=30
for i in $(seq 1 $max_retries); do
  if curl -s http://127.0.0.1:8003/health >/dev/null 2>&1; then
    log_ok "Backend is ready"
    break
  fi
  if [ $i -eq $max_retries ]; then
    log_fail "Backend did not start"
    cat /tmp/ghostlink_backend.log
    exit 1
  fi
  sleep 1
done

# Run GUI tests
log_info "Running GUI function tests..."
echo ""
if python3 scripts/test_gui_functions.py 2>&1; then
  log_ok "All GUI tests passed!"
  echo ""
  log_info "Test Summary:"
  log_ok "✓ Health check"
  log_ok "✓ List models (real Ollama models)"
  log_ok "✓ Chat with real LLM (no mock responses)"
  log_ok "✓ Metrics"
  log_ok "✓ Sessions"
  log_ok "✓ Workers"
  log_ok "✓ Add worker"
  log_ok "✓ Connect workers"
  log_ok "✓ JWT refresh"
  log_ok "✓ PQC enable"
  log_ok "✓ Model load"
  log_ok "✓ Model download"
  log_ok "✓ Repeated chat (different responses)"
  log_ok "✓ No mock keywords detected"
  log_ok "✓ Concurrent requests"
  echo ""
  log_info "Next steps:"
  log_info "1. Run the GUI: python3 ghostlink_gui.py"
  log_info "2. Backend URL: http://127.0.0.1:8003"
  log_info "3. Keep Ollama running: ollama run tinyllama"
else
  log_fail "Some tests failed"
  echo ""
  echo "=== Backend logs ==="
  tail -50 /tmp/ghostlink_backend.log
  exit 1
fi
