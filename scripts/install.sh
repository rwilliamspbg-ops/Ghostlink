#!/bin/sh
# Ghostlink one-line installer (Linux / macOS)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/rwilliamspbg-ops/Ghostlink/main/scripts/install.sh | sh
#   VERSION=v1.16.1 sh install.sh          # install a specific release instead of latest
#   GHOSTLINK_INSTALL_DIR=/opt/bin sh install.sh   # install somewhere other than ~/.local/bin
#
# This downloads the prebuilt `ghost-link` binary from this repo's GitHub
# Releases (published by .github/workflows/release-artifacts.yml), verifies
# it against the release's published SHA256SUMS file, and installs it to a
# user-writable directory. No sudo, no package manager.
#
# What this does NOT install: the Go `control-plane` gateway or the built
# React GUI. Only the `ghost-link` binary itself (plus SHA256SUMS and an
# SBOM) is published as a release asset today -- see scripts/release_bundle.sh
# and .github/workflows/release-artifacts.yml if you want to check. For the
# full browser GUI, clone the repo and run `docker compose up` or
# `./launch.sh` (see the README's Quick Start section instead).
#
# Honesty notes, read before filing a bug:
#   - "Linux" here really means "the binary was built and tested on the
#     ubuntu-latest GitHub Actions runner." There is no distro-specific
#     testing or guarantee beyond that (e.g. it may not run on musl-based
#     distros like Alpine).
#   - Only x86_64/amd64 binaries are published as of this writing -- there is
#     no separate arm64 build for any OS (check the `os:` matrix in
#     .github/workflows/release-artifacts.yml if that has since changed).
#     This script refuses to install on non-x86_64 hosts rather than
#     silently handing you a binary that won't run.
#
# Written in POSIX sh on purpose (no bashisms) so `curl | sh` works
# regardless of which shell `sh` happens to be on the target machine.

set -eu

REPO="rwilliamspbg-ops/Ghostlink"
GITHUB="https://github.com/${REPO}"
API="https://api.github.com/repos/${REPO}"

VERSION="${VERSION:-latest}"
INSTALL_DIR="${GHOSTLINK_INSTALL_DIR:-${HOME}/.local/bin}"

info() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command '$1' not found on PATH"
}

need_cmd curl
need_cmd uname
need_cmd chmod
need_cmd mktemp
need_cmd awk

# --- OS detection ----------------------------------------------------------
# Maps 1:1 to the `os:` matrix in .github/workflows/release-artifacts.yml,
# which is also the literal suffix baked into each release asset's filename
# by scripts/release_bundle.sh (BIN_NAME="ghost-link-${PLATFORM_SUFFIX}...").
os_name="$(uname -s)"
case "$os_name" in
  Linux)
    asset_os="ubuntu-latest"
    ;;
  Darwin)
    asset_os="macos-latest"
    ;;
  *)
    die "unsupported OS '${os_name}'. Ghostlink release binaries are only published for Linux (built on ubuntu-latest) and macOS (built on macos-latest). On Windows, use scripts/install.ps1 instead, or build from source -- see the README's Quick Start."
    ;;
esac

# --- Arch detection ----------------------------------------------------------
# No arm64/aarch64 build exists for any OS today (single-arch matrix, no
# cross-compilation step) -- warn clearly and refuse rather than install a
# binary that will not execute.
arch_name="$(uname -m)"
case "$arch_name" in
  x86_64 | amd64) : ;;
  *)
    die "unsupported architecture '${arch_name}'. Ghostlink release binaries are x86_64/amd64-only as of this writing -- there is no arm64/aarch64 build for Linux, macOS, or Windows (check .github/workflows/release-artifacts.yml's build matrix in case this has changed since). Refusing to install a binary that won't run on this machine. On Apple Silicon you may be able to run the x86_64 binary under Rosetta 2 if you have it installed, or build from source: ${GITHUB}#quick-start"
    ;;
esac

bin_asset="ghost-link-${asset_os}"
sums_asset="SHA256SUMS-${asset_os}"

# --- Resolve version ---------------------------------------------------------
if [ "$VERSION" = "latest" ]; then
  info "Looking up the latest Ghostlink release..."
  release_json="$(curl -fsSL "${API}/releases/latest")" \
    || die "failed to query ${API}/releases/latest -- check your network connection"
  tag="$(printf '%s' "$release_json" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//; s/".*//')"
  [ -n "$tag" ] || die "could not determine the latest release tag from the GitHub API response"
else
  tag="$VERSION"
fi
info "Installing Ghostlink ${tag} (${asset_os}, ${arch_name})"

download_base="${GITHUB}/releases/download/${tag}"
bin_url="${download_base}/${bin_asset}"
sums_url="${download_base}/${sums_asset}"

# --- Download ------------------------------------------------------------
tmp_dir="$(mktemp -d)"
cleanup() { rm -rf "$tmp_dir"; }
trap cleanup EXIT INT TERM

tmp_bin="${tmp_dir}/${bin_asset}"
tmp_sums="${tmp_dir}/${sums_asset}"

info "Downloading ${bin_asset}..."
curl -fsSL -o "$tmp_bin" "$bin_url" \
  || die "failed to download ${bin_url} -- does release ${tag} exist and include a ${asset_os} build?"

info "Downloading ${sums_asset} for verification..."
curl -fsSL -o "$tmp_sums" "$sums_url" \
  || die "failed to download ${sums_url}"

# --- Verify checksum -------------------------------------------------------
# SHA256SUMS-* files are the direct output of `sha256sum`/`shasum -a 256`
# (see scripts/release_bundle.sh) and list every file in the release bundle,
# not just the binary -- lines look like "<hash>  ./ghost-link-ubuntu-latest"
# (text mode) or "<hash> *./ghost-link-ubuntu-latest" (binary mode, seen on
# the windows-latest runner's sha256sum). Match on the last field with any
# leading '*' stripped so either format is handled.
expected_hash="$(awk -v want="./${bin_asset}" '
  { name = $NF; sub(/^\*/, "", name); if (name == want) print $1 }
' "$tmp_sums" | head -1)"

[ -n "$expected_hash" ] \
  || die "could not find a checksum entry for ${bin_asset} in ${sums_asset} -- refusing to install an unverified binary"

if command -v sha256sum >/dev/null 2>&1; then
  actual_hash="$(sha256sum "$tmp_bin" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  actual_hash="$(shasum -a 256 "$tmp_bin" | awk '{print $1}')"
else
  die "neither sha256sum nor shasum is available -- cannot verify the download's integrity, refusing to install"
fi

if [ "$expected_hash" != "$actual_hash" ]; then
  die "checksum mismatch for ${bin_asset}: expected ${expected_hash}, got ${actual_hash}. The download may be corrupted or tampered with -- not installing."
fi
info "Checksum verified (sha256:${actual_hash})"

chmod +x "$tmp_bin"

# --- Install ---------------------------------------------------------------
mkdir -p "$INSTALL_DIR" 2>/dev/null \
  || die "could not create install directory '${INSTALL_DIR}'. Set GHOSTLINK_INSTALL_DIR to a writable directory and retry."
[ -w "$INSTALL_DIR" ] \
  || die "install directory '${INSTALL_DIR}' is not writable. Set GHOSTLINK_INSTALL_DIR to a writable directory and retry (this script never uses sudo)."

dest="${INSTALL_DIR}/ghost-link"
mv "$tmp_bin" "$dest"

info ""
info "Ghostlink ${tag} installed to ${dest}"
info ""

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    warn "${INSTALL_DIR} is not on your PATH."
    info "Add it, e.g.:"
    info "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    info "and add that line to your shell profile (~/.bashrc, ~/.zshrc, ~/.profile, etc.) to persist it."
    info ""
    ;;
esac

info "Next steps:"
info "  ${dest} --help"
info "  ${dest} doctor --strict         # sanity-check your setup"
info "  ${dest} serve 127.0.0.1 8003    # start the OpenAI-compatible API server"
info ""
info "This installs the ghost-link binary only (CLI + OpenAI-compatible API"
info "server) -- it does not include the Go control-plane gateway or the"
info "React GUI, which aren't published as standalone release assets. For"
info "the full browser GUI, clone the repo and run 'docker compose up' or"
info "'./launch.sh': ${GITHUB}#quick-start"
info ""
info "Models load from a 'models/' directory relative to wherever you run"
info "ghost-link from. More: ${GITHUB}/blob/main/docs/QUICKSTART.md"
