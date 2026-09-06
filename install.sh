#!/bin/sh
# Tack installer — downloads the latest release binary for your platform.
#
#   curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
#
# Options (environment variables):
#   TACK_INSTALL_DIR   target directory (default: $HOME/.local/bin)
#   TACK_VERSION       release tag to install, e.g. v0.1.0-beta.6 (default: newest)
#
# Installs the single `tack` binary (server + CLI, web UI embedded). No Docker,
# no database server — run `tack` and open http://localhost:3210.
#
# Asset names are produced by .github/workflows/release.yml as
# `tack-<tag>-<platform>.tar.gz` (e.g. tack-v0.1.0-beta.6-linux-x86_64.tar.gz).

set -eu

REPO="yielab/tack"
INSTALL_DIR="${TACK_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${TACK_VERSION:-}"
API="https://api.github.com/repos/$REPO/releases"

err() { printf 'tack-install: %s\n' "$1" >&2; exit 1; }

# ── Detect platform (must match the release workflow's matrix labels) ──────────
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux)  os_tag="linux" ;;
  Darwin) os_tag="macos" ;;
  *) err "unsupported OS '$os'. Build from source: https://github.com/$REPO" ;;
esac
case "$arch" in
  x86_64|amd64)  arch_tag="x86_64" ;;
  arm64|aarch64) arch_tag="aarch64" ;;
  *) err "unsupported architecture '$arch'. Build from source: https://github.com/$REPO" ;;
esac
platform="${os_tag}-${arch_tag}"

# Only these are published as prebuilt binaries (see release.yml matrix).
case "$platform" in
  linux-x86_64|macos-aarch64|macos-x86_64) ;;
  *) err "no prebuilt binary for '$platform' yet. Build from source: https://github.com/$REPO" ;;
esac

# ── Fetch tool ────────────────────────────────────────────────────────────────
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1"; }
  download() { curl -fSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO- "$1"; }
  download() { wget -qO "$2" "$1"; }
else
  err "need curl or wget to download"
fi

# ── Resolve the download URL from the GitHub API ──────────────────────────────
# Newest releases first; this includes pre-releases (the beta tags), which
# /releases/latest deliberately excludes.
suffix="-${platform}.tar.gz"
echo "Looking up the newest tack release for ${platform}…"
urls="$(fetch "$API?per_page=20" | grep -o '"browser_download_url": *"[^"]*"' | sed 's/.*"\(https[^"]*\)"/\1/')" \
  || err "could not query the releases API"

# Every release also publishes a `tack-runner-<tag>-<platform>.tar.gz` archive,
# ending in this same platform suffix, and the releases API lists it before
# the `tack-<tag>-<platform>.tar.gz` one this installer actually wants. A bare
# suffix match would pick it first, download it, then fail below — that
# archive holds a binary named `tack-runner`, never one named `tack`.
# Excluded explicitly; this script installs the board binary only.
if [ -n "$VERSION" ]; then
  url="$(printf '%s\n' "$urls" | grep "/download/$VERSION/" | grep -- "$suffix" | grep -v '/tack-runner-' | head -n 1 || true)"
  [ -n "$url" ] || err "no asset for $VERSION on $platform"
else
  url="$(printf '%s\n' "$urls" | grep -- "$suffix" | grep -v '/tack-runner-' | head -n 1 || true)"
  [ -n "$url" ] || err "no published $platform asset found"
fi

asset="$(basename "$url")"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

printf 'Downloading %s …\n' "$asset"
download "$url" "$tmp/$asset" || err "download failed: $url"

# ── Extract & install ─────────────────────────────────────────────────────────
tar xzf "$tmp/$asset" -C "$tmp" || err "extract failed"
bin="$(find "$tmp" -name tack -type f | head -n 1)"
[ -n "$bin" ] || err "no 'tack' binary found in archive"

mkdir -p "$INSTALL_DIR"
install -m 0755 "$bin" "$INSTALL_DIR/tack" 2>/dev/null || {
  cp "$bin" "$INSTALL_DIR/tack" && chmod 0755 "$INSTALL_DIR/tack"
}

printf '\nInstalled tack to %s/tack\n' "$INSTALL_DIR"
# SC2016: the literal $PATH below is intentional — it's instructional text for the user.
# shellcheck disable=SC2016
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) printf 'Note: %s is not on your PATH. Add it:\n  export PATH="%s:$PATH"\n' "$INSTALL_DIR" "$INSTALL_DIR" ;;
esac
printf 'Run "tack" and open http://localhost:3210\n'
