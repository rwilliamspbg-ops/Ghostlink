#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_FILE="$ROOT_DIR/ghostlink.toml"
EXAMPLE_CONFIG="$ROOT_DIR/ghostlink.example.toml"

log_info() {
  echo "[INFO] $*"
}

log_ok() {
  echo "[OK] $*"
}

log_warn() {
  echo "[WARN] $*"
}

log_fail() {
  echo "[ERROR] $*"
}

print_fix_and_retry() {
  local fix_cmd="$1"
  local retry_cmd="$2"
  echo "FIX: $fix_cmd"
  echo "RETRY: $retry_cmd"
}

require_cmd() {
  local name="$1"
  local install_hint="$2"
  if ! command -v "$name" >/dev/null 2>&1; then
    log_fail "Missing required command: $name"
    print_fix_and_retry "$install_hint" "bash scripts/quickstart.sh"
    exit 1
  fi
}

run_step() {
  local title="$1"
  shift
  log_info "$title"
  if ! "$@"; then
    log_fail "$title failed"
    return 1
  fi
  log_ok "$title"
}

show_next_steps() {
  cat <<EOF

Quickstart completed.

Next steps:
1) Edit local defaults: $CONFIG_FILE
2) Try local discovery cluster: cargo run -p ghost-link -- cluster-start 3 46000
3) Validate host health: python3 scripts/xdp_preflight_check.py --output ./tmp/xdp-preflight.json
4) Read operator rollout path: docs/DEPLOYMENT.md

Artifacts:
- Config file: $CONFIG_FILE
- Flow metrics (if enabled): ./tmp/flow-metrics.json
EOF
}

main() {
  cd "$ROOT_DIR"

  log_info "Starting Ghostlink quickstart in $ROOT_DIR"

  require_cmd "cargo" "Install Rust with: curl https://sh.rustup.rs -sSf | sh -s -- -y && . \"$HOME/.cargo/env\""
  require_cmd "python3" "Install Python 3.10+ using your OS package manager"

  if [[ ! -f "$EXAMPLE_CONFIG" ]]; then
    log_fail "Missing example config: $EXAMPLE_CONFIG"
    print_fix_and_retry "Restore repository files or re-clone the project" "bash scripts/quickstart.sh"
    exit 1
  fi

  if [[ ! -f "$CONFIG_FILE" ]]; then
    cp "$EXAMPLE_CONFIG" "$CONFIG_FILE"
    log_ok "Created local config from template: $CONFIG_FILE"
  else
    log_info "Using existing local config: $CONFIG_FILE"
  fi

  run_step "Building ghost-link binary (dev profile)" cargo build -p ghost-link

  log_info "Running quick smoke flow with local config"
  if ! cargo run -p ghost-link -- --config "$CONFIG_FILE" flow; then
    log_fail "Smoke flow failed"
    print_fix_and_retry "Inspect config and runtime diagnostics: cargo run -p ghost-link -- gui-check --strict" "cargo run -p ghost-link -- --config ./ghostlink.toml flow"
    echo "DETAILS: See docs/TROUBLESHOOTING.md for common quickstart failures"
    exit 1
  fi
  log_ok "Smoke flow completed"

  show_next_steps
}

main "$@"