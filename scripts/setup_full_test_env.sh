#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VENV_PATH="${ROOT_DIR}/.venv"
PYTHON_BIN="${PYTHON_BIN:-python3}"

log() {
  printf '[setup] %s\n' "$*"
}

warn() {
  printf '[setup][warn] %s\n' "$*" >&2
}

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required but not found in PATH" >&2
  exit 1
fi

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  echo "${PYTHON_BIN} is required but not found in PATH" >&2
  exit 1
fi

if [[ ! -d "$VENV_PATH" ]]; then
  log "Creating virtual environment at ${VENV_PATH}"
  "$PYTHON_BIN" -m venv "$VENV_PATH"
fi

VENV_PYTHON="${VENV_PATH}/bin/python"

log "Upgrading pip/setuptools/wheel"
"$VENV_PYTHON" -m pip install --upgrade pip setuptools wheel

log "Installing Mohawk GUI Python dependencies"
"$VENV_PYTHON" -m pip install -r "${ROOT_DIR}/third_party/mohawk_gui/requirements-runtime.txt"

if [[ "$(uname -s)" == "Linux" ]]; then
  if [[ (! -e /usr/lib/x86_64-linux-gnu/libGL.so.1 && ! -e /usr/lib64/libGL.so.1 && ! -e /usr/lib/libGL.so.1) || (! -e /usr/lib/x86_64-linux-gnu/libxkbcommon.so.0 && ! -e /usr/lib64/libxkbcommon.so.0 && ! -e /usr/lib/libxkbcommon.so.0) ]]; then
    warn "Required GUI system libraries missing; attempting OS package install (libgl1, libxkbcommon0)"
    if command -v sudo >/dev/null 2>&1; then
      if sudo -n true >/dev/null 2>&1; then
        sudo apt-get update
        sudo apt-get install -y libgl1 libxkbcommon0
      else
        warn "sudo requires a password or is unavailable in this session"
        warn "Run manually: sudo apt-get update && sudo apt-get install -y libgl1 libxkbcommon0"
      fi
    else
      warn "sudo not available"
      warn "Run manually as root: apt-get update && apt-get install -y libgl1 libxkbcommon0"
    fi
  fi
fi

log "Environment readiness check"
cd "$ROOT_DIR"
if ! GHOSTLINK_PYTHON="$VENV_PYTHON" cargo run --release -p ghost-link -- gui-check --strict; then
  warn "GUI readiness check failed"
  if [[ "${GHOSTLINK_SETUP_ALLOW_DEGRADED:-0}" == "1" ]]; then
    warn "Continuing in degraded mode because GHOSTLINK_SETUP_ALLOW_DEGRADED=1"
  else
    warn "Set GHOSTLINK_SETUP_ALLOW_DEGRADED=1 to continue with a degraded setup"
    exit 1
  fi
fi

log "Setup complete"
