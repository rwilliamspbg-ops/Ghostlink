#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
PYTHON_CMD="python3"
if [[ -x ".venv/bin/python" ]]; then
    PYTHON_CMD=".venv/bin/python"
fi
exec "$PYTHON_CMD" scripts/launch_studio.py "$@"
