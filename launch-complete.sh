#!/usr/bin/env bash
# Compatibility wrapper — full stack lives in launch.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$ROOT/launch.sh" "$@"
