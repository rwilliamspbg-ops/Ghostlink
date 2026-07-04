#!/usr/bin/env bash
# Ghostlink Studio - Cross-platform Launch Script

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# Detect OS and use appropriate launcher
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" || "$OSTYPE" == "win32" ]]; then
    # Windows - use Python launcher
    exec python3 scripts/launch_studio.py "$@"
elif [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS - use Python launcher
    exec python3 scripts/launch_studio.py "$@"
else
    # Linux - use Python launcher
    exec python3 scripts/launch_studio.py "$@"
fi
