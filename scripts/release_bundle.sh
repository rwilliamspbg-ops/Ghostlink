#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/artifacts/release}"
SIGN_MODE="${2:-signed}"
if [[ "${SIGN_MODE}" != "signed" && "${SIGN_MODE}" != "unsigned" ]]; then
  echo "usage: scripts/release_bundle.sh [out_dir] [signed|unsigned]" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
cd "$ROOT_DIR"

NODE_BIN="$(command -v node || true)"
NPM_BIN="$(command -v npm || true)"
if [[ -z "$NODE_BIN" || -z "$NPM_BIN" ]]; then
  echo "node and npm are required to build the GUI release artifacts" >&2
  exit 1
fi

echo "[release] build ghost-link release binary"
cargo build --release -p ghost-link

BIN_PATH="$ROOT_DIR/target/release/ghost-link"
if [[ ! -x "$BIN_PATH" ]]; then
  echo "Release binary missing: $BIN_PATH" >&2
  exit 1
fi

cp "$BIN_PATH" "$OUT_DIR/"

echo "[release] build ghostlink_gui_modern frontend"
pushd "$ROOT_DIR/ghostlink_gui_modern" >/dev/null
if [[ ! -d node_modules ]]; then
  "$NPM_BIN" ci
fi
"$NPM_BIN" run build
popd >/dev/null

mkdir -p "$OUT_DIR/gui"
cp -R "$ROOT_DIR/ghostlink_gui_modern/dist" "$OUT_DIR/gui/"

pushd "$OUT_DIR" >/dev/null
find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS

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
  gpg --batch --yes --pinentry-mode loopback --passphrase "$GPG_PASSPHRASE" --armor --detach-sign SHA256SUMS
else
  gpg --batch --yes --armor --detach-sign SHA256SUMS
fi
popd >/dev/null

echo "[release] bundle generated at $OUT_DIR"
