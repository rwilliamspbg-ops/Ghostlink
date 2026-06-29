#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/artifacts/release}"

mkdir -p "$OUT_DIR"
cd "$ROOT_DIR"

echo "[release] build ghost-link release binary"
cargo build --release -p ghost-link

BIN_PATH="$ROOT_DIR/target/release/ghost-link"
if [[ ! -x "$BIN_PATH" ]]; then
  echo "Release binary missing: $BIN_PATH" >&2
  exit 1
fi

cp "$BIN_PATH" "$OUT_DIR/"

pushd "$OUT_DIR" >/dev/null
sha256sum ghost-link > SHA256SUMS

# Optional signature when GPG is configured on runner.
if command -v gpg >/dev/null 2>&1 && gpg --list-secret-keys >/dev/null 2>&1; then
  gpg --batch --yes --armor --detach-sign SHA256SUMS
fi
popd >/dev/null

echo "[release] bundle generated at $OUT_DIR"
