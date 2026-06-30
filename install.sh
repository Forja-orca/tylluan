#!/usr/bin/env bash
set -euo pipefail

REPO="Forja-orca/tylluan"
BIN_DIR="${HOME}/.tylluan/bin"

say() { printf "\033[1;32m%s\033[0m\n" "$*" >&2; }
err() { printf "\033[1;31m%s\033[0m\n" "$*" >&2; exit 1; }

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$OS" in
  linux)
    case "$ARCH" in
      aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
      x86_64)        TARGET="x86_64-unknown-linux-gnu" ;;
      *)             err "unsupported Linux arch: $ARCH" ;;
    esac
    ;;
  darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      x86_64)        TARGET="x86_64-apple-darwin" ;;
      *)             err "unsupported macOS arch: $ARCH" ;;
    esac
    ;;
  *) err "unsupported OS: $OS" ;;
esac

say "📡 Detecting latest release..."
LATEST=$(curl -fsL "https://api.github.com/repos/${REPO}/releases/latest" \
  | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p')
[ -n "$LATEST" ] || err "could not detect latest version"

ARCHIVE="tylluan-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${LATEST}/${ARCHIVE}"

say "📦 Downloading Tylluan v${LATEST} (${TARGET})..."
mkdir -p "$BIN_DIR"
curl -fsL "$URL" | tar xzf - -C "$BIN_DIR" --strip-components=1

chmod +x "$BIN_DIR"/tylluan-nexus "$BIN_DIR"/tylluan-cli 2>/dev/null || true

if ! echo ":$PATH:" | grep -qF ":$BIN_DIR:"; then
  SHELL_PROFILE=""
  case "${SHELL:-}" in
    */zsh) SHELL_PROFILE="${ZDOTDIR:-$HOME}/.zshrc" ;;
    */bash) SHELL_PROFILE="$HOME/.bashrc" ;;
  esac
  if [ -n "$SHELL_PROFILE" ]; then
    echo "export PATH=\"\$PATH:${BIN_DIR}\"" >> "$SHELL_PROFILE"
    say "🔧 Added \${BIN_DIR} to PATH (${SHELL_PROFILE})"
  else
    say "⚠️  Add \${BIN_DIR} to your PATH manually"
  fi
  say "   → Open a NEW terminal, or run: source ${SHELL_PROFILE}"
fi

say ""
say "✅ Tylluan v${LATEST} installed to ${BIN_DIR}/"
say ""
say "   ┌─────────────────────────────────────────────┐"
say "   │  tylluan-cli start    # Start the kernel    │"
say "   │  curl -s 127.0.0.1:3030/health  # Verify   │"
say "   └─────────────────────────────────────────────┘"
say ""
say "   📄 Auth token (auto-generated on first boot):"
say "       .tylluan-token     (in working directory)"
say ""
say "   🔗 Connect your MCP client:"
say '       { "mcpServers": { "tylluan": { "type": "sse", "url": "http://127.0.0.1:3030/sse" } } }'
