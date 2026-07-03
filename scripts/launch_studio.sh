#!/usr/bin/env bash
# Ghostlink Studio - One-Click Launch Script
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_PATH="${ROOT_DIR}/.venv"
CHECK_ONLY=0
GUI_ARGS=()

if [[ "${1:-}" == "--check" ]]; then
  CHECK_ONLY=1
else
  GUI_ARGS=("$@")
fi

log() {
  echo -e "\033[1;34m[Ghostlink]\033[0m $*"
}

fail() {
  echo -e "\033[1;31m[Ghostlink][error]\033[0m $*" >&2
  exit 1
}

ensure_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    fail "Missing required command: ${cmd}"
  fi
}

cd "$ROOT_DIR"

log "Starting Ghostlink Studio initialization..."

ensure_cmd cargo
ensure_cmd python3

# 1. Setup Environment
if [[ ! -d "$VENV_PATH" ]]; then
  log "Initializing Python virtual environment..."
  bash scripts/setup_full_test_env.sh
fi

# 2. Bootstrap Config
if [[ ! -f "ghostlink.toml" ]]; then
  log "Bootstrapping local configuration..."
  cp ghostlink.example.toml ghostlink.toml
fi

# 3. Build Core
log "Building high-performance core (release)..."
cargo build --release -p ghost-link

if [[ "$CHECK_ONLY" == "1" ]]; then
  log "Preflight completed successfully (check-only mode)."
  exit 0
fi

# 4. Launch Studio
log "Launching Ghostlink Studio..."
./target/release/ghost-link gui "${GUI_ARGS[@]}"
