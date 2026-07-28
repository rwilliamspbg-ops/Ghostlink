#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/artifacts/release}"
SIGN_MODE="${2:-signed}"
# Optional: distinguishes output filenames when building for multiple OSes in
# one release (e.g. "ubuntu-latest"). Left empty, filenames are unsuffixed —
# matches this script's original single-platform behavior.
PLATFORM_SUFFIX="${3:-}"
if [[ "${SIGN_MODE}" != "signed" && "${SIGN_MODE}" != "unsigned" ]]; then
  echo "usage: scripts/release_bundle.sh [out_dir] [signed|unsigned] [platform_suffix]" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
cd "$ROOT_DIR"

echo "[release] build ghost-link release binary"
cargo build --release -p ghost-link

SRC_BIN="$ROOT_DIR/target/release/ghost-link"
SRC_EXT=""
if [[ ! -f "$SRC_BIN" ]]; then
  # Windows produces a .exe; try that before giving up.
  if [[ -f "$ROOT_DIR/target/release/ghost-link.exe" ]]; then
    SRC_BIN="$ROOT_DIR/target/release/ghost-link.exe"
    SRC_EXT=".exe"
  else
    echo "Release binary missing: $SRC_BIN" >&2
    exit 1
  fi
fi

if [[ -n "$PLATFORM_SUFFIX" ]]; then
  BIN_NAME="ghost-link-${PLATFORM_SUFFIX}${SRC_EXT}"
  SUMS_NAME="SHA256SUMS-${PLATFORM_SUFFIX}"
else
  BIN_NAME="ghost-link${SRC_EXT}"
  SUMS_NAME="SHA256SUMS"
fi

cp "$SRC_BIN" "$OUT_DIR/$BIN_NAME"

pushd "$OUT_DIR" >/dev/null
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$BIN_NAME" > "$SUMS_NAME"
else
  # macOS has no sha256sum by default; shasum -a 256 is the equivalent.
  shasum -a 256 "$BIN_NAME" > "$SUMS_NAME"
fi

if [[ "$SIGN_MODE" == "unsigned" ]]; then
  echo "[release] unsigned mode selected; skipping GPG signature"
  popd >/dev/null
  echo "[release] bundle generated at $OUT_DIR"
  exit 0
fi

if ! command -v gpg >/dev/null 2>&1; then
  echo "gpg is required for signed release bundles (or use 'unsigned' mode)" >&2
  exit 1
fi
if ! gpg --list-secret-keys >/dev/null 2>&1; then
  echo "no gpg secret key found for release signing (or use 'unsigned' mode)" >&2
  exit 1
fi

if [[ -n "${GPG_PASSPHRASE:-}" ]]; then
  gpg --batch --yes --pinentry-mode loopback --passphrase "$GPG_PASSPHRASE" --armor --detach-sign "$SUMS_NAME"
else
  gpg --batch --yes --armor --detach-sign "$SUMS_NAME"
fi
popd >/dev/null

echo "[release] bundle generated at $OUT_DIR"
