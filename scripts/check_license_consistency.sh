#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WORKSPACE_LICENSE="$(grep -E '^license\s*=\s*"' "$ROOT_DIR/Cargo.toml" | head -n1 | sed -E 's/.*"([^"]+)".*/\1/')"

if [[ -z "$WORKSPACE_LICENSE" ]]; then
  echo "failed to read workspace license from Cargo.toml" >&2
  exit 1
fi

EXIT_CODE=0
# Use a temp file instead of process substitution for broader shell/CRLF portability.
TMP_LIST="$(mktemp)"
find "$ROOT_DIR/crates" -name Cargo.toml | sort >"$TMP_LIST"
while IFS= read -r cargo_file; do
  [[ -z "$cargo_file" ]] && continue
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
done <"$TMP_LIST"

# Each crate also ships a LICENSE *file*. Cargo auto-includes LICENSE* from the crate
# root, so the published tarball carries the licence text with no manifest field --
# but a copy can drift from the root, which is exactly the kind of silent divergence
# the field check above cannot see. Compare bytes, not names.
ROOT_LICENSE_FILE="$ROOT_DIR/LICENSE"
if [[ ! -f "$ROOT_LICENSE_FILE" ]]; then
  echo "missing root LICENSE at $ROOT_LICENSE_FILE" >&2
  EXIT_CODE=1
else
  # Only crates a publish workflow actually publishes need to ship the licence text.
  # The list is derived from publish-crates.yml rather than hardcoded, so adding a
  # crate to the publish workflow automatically brings it under this check -- and the
  # workspace members that are never published (the mcp-* crates, the tauri shell) are
  # not falsely required to carry a copy.
  PUBLISH_WORKFLOW="$ROOT_DIR/.github/workflows/publish-crates.yml"
  if [[ ! -f "$PUBLISH_WORKFLOW" ]]; then
    echo "cannot verify shipped licences: no $PUBLISH_WORKFLOW" >&2
    EXIT_CODE=1
  else
    while IFS= read -r crate; do
      [[ -z "$crate" ]] && continue
      crate_dir="$ROOT_DIR/crates/$crate"
      crate_license_file="$crate_dir/LICENSE"
      if [[ ! -f "$crate_license_file" ]]; then
        echo "missing LICENSE in $crate_dir: published crates must ship the licence text" >&2
        EXIT_CODE=1
        continue
      fi
      if ! cmp -s "$ROOT_LICENSE_FILE" "$crate_license_file"; then
        echo "LICENSE differs from the root copy: $crate_license_file" >&2
        EXIT_CODE=1
      fi
    done < <(grep -oE 'cargo publish -p [A-Za-z0-9_-]+' "$PUBLISH_WORKFLOW" \
               | awk '{print $NF}' | sort -u)
  fi
fi

rm -f "$TMP_LIST"

exit "$EXIT_CODE"
