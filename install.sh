#!/usr/bin/env bash
set -euo pipefail

REPO="Forja-orca/tylluan"
BIN_DIR="${HOME}/.tylluan/bin"

say() { printf "\033[1;32m%s\033[0m\n" "$*" >&2; }
err() { printf "\033[1;31m%s\033[0m\n" "$*" >&2; exit 1; }

# --- detect platform ---
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$OS" in
  linux)  TARGET="x86_64-unknown-linux-gnu"    ;;
  darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      x86_64)        TARGET="aarch64-apple-darwin" ;; # prefer arm64 via Rosetta
      *)             err "unsupported macOS arch: $ARCH" ;;
    esac
    ;;
  *) err "unsupported OS: $OS" ;;
esac

# --- fetch latest release ---
say "📡 Detecting latest release..."
LATEST=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | head -1 | sed 's/.*"v\([^"]*\)".*/\1/')
[ -n "$LATEST" ] || err "could not detect latest version"

ARCHIVE="tylluan-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${LATEST}/${ARCHIVE}"

say "📦 Downloading Tylluan v${LATEST} (${TARGET})..."
mkdir -p "$BIN_DIR"
curl -sL "$URL" | tar xz -C "$BIN_DIR" --strip-components=1

# --- make binaries executable ---
chmod +x "$BIN_DIR"/tylluan-nexus "$BIN_DIR"/tylluan-cli 2>/dev/null || true

# --- PATH setup ---
if ! echo "$PATH" | grep -q "$BIN_DIR"; then
  SHELL_PROFILE=""
  case "${SHELL:-}" in
    */zsh) SHELL_PROFILE="${ZDOTDIR:-$HOME}/.zshrc" ;;
    */bash) SHELL_PROFILE="$HOME/.bashrc" ;;
  esac
  if [ -n "$SHELL_PROFILE" ]; then
    echo "export PATH=\"\$PATH:${BIN_DIR}\"" >> "$SHELL_PROFILE"
    say "🔧 Added ${BIN_DIR} to PATH in ${SHELL_PROFILE}"
  else
    say "⚠️  Add ${BIN_DIR} to your PATH manually"
  fi
fi

say "✅ Tylluan v${LATEST} installed!"
say "   Binaries: ${BIN_DIR}/"
say ""
say "   Run:  tylluan-cli start"
say "   Then: curl http://127.0.0.1:3030/health"
