#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WORKSPACE_LICENSE="$(grep -E '^license\s*=\s*"' "$ROOT_DIR/Cargo.toml" | head -n1 | sed -E 's/.*"([^"]+)".*/\1/')"

if [[ -z "$WORKSPACE_LICENSE" ]]; then
  echo "failed to read workspace license from Cargo.toml" >&2
  exit 1
fi

EXIT_CODE=0
while IFS= read -r cargo_file; do
  crate_license="$(grep -E '^license\s*=\s*"' "$cargo_file" | head -n1 | sed -E 's/.*"([^"]+)".*/\1/')"
  if [[ -z "$crate_license" ]]; then
    echo "missing license field: $cargo_file" >&2
    EXIT_CODE=1
    continue
  fi
  if [[ "$crate_license" != "$WORKSPACE_LICENSE" ]]; then
    echo "license mismatch in $cargo_file: expected '$WORKSPACE_LICENSE' got '$crate_license'" >&2
    EXIT_CODE=1
  fi
done < <(find "$ROOT_DIR/crates" -name Cargo.toml | sort)

exit "$EXIT_CODE"
