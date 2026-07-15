#!/usr/bin/env bash
# Ghostlink Studio - One-Click Launch Script
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_PATH="${ROOT_DIR}/.venv"

log() {
  echo -e "\033[1;34m[Ghostlink]\033[0m $*"
}

cd "$ROOT_DIR"

log "Starting Ghostlink Studio initialization..."

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

# 4. Launch Studio
log "Launching Ghostlink Studio..."
GHOSTLINK_PYTHON="${VENV_PATH}/bin/python" ./target/release/ghost-link gui
