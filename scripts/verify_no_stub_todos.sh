#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT_DIR/crates/ghost-link/src"
PATTERN='TODO: Implement actual'

if [[ ! -d "$TARGET_DIR" ]]; then
  echo "Target source directory not found: $TARGET_DIR" >&2
  exit 1
fi

matches="$(grep -R -n -- "$PATTERN" "$TARGET_DIR" || true)"
if [[ -n "$matches" ]]; then
  echo "Found unresolved stub TODO markers in $TARGET_DIR:" >&2
  echo "$matches" >&2
  exit 1
fi

echo "No unresolved stub TODO markers found in $TARGET_DIR."
