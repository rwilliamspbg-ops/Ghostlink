#!/usr/bin/env bash
# Download pre-built llama-server binary to cache (avoids OOM builds)
# Usage: ./scripts/download-llama-server.sh [version]
#   version defaults to latest release

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CACHE_DIR="$PROJECT_ROOT/bin"
VERSION="${1:-latest}"

detect_asset() {
  local arch os
  arch="$(uname -m)"
  os="$(uname -s)"

  case "$os" in
    Linux)  os="ubuntu" ;;   # llama.cpp publishes generic linux builds as ubuntu
    Darwin) os="macos" ;;
    *)      echo "unsupported os: $os"; exit 1 ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="arm64" ;;
    *)          echo "unsupported arch: $arch"; exit 1 ;;
  esac

  echo "llama-server-${os}-${arch}.tar.xz"
}

ASSET="$(detect_asset)"
echo "Target: $ASSET"

if [ "$VERSION" = "latest" ]; then
  RELEASE_URL="https://api.github.com/repos/ggml-org/llama.cpp/releases/latest"
  TAG="$(curl -sL "$RELEASE_URL" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": "//;s/".*//')"
  echo "Latest release: $TAG"
else
  TAG="$VERSION"
fi

DOWNLOAD_URL="https://github.com/ggml-org/llama.cpp/releases/download/${TAG}/${ASSET}"

mkdir -p "$CACHE_DIR"
echo "Downloading llama-server ${TAG}..."
curl -L --progress-bar "$DOWNLOAD_URL" -o "$CACHE_DIR/$ASSET"
echo "Extracting..."
tar -xf "$CACHE_DIR/$ASSET" -C "$CACHE_DIR"
chmod +x "$CACHE_DIR/llama-server" 2>/dev/null || true
echo "Pre-built llama-server cached at $CACHE_DIR/llama-server"
echo "Set GHOSTLINK_SKIP_BUILD=1 to skip source builds."
