#!/usr/bin/env bash
set -euo pipefail

REPO="Forja-orca/tylluan"
BIN_DIR="${HOME}/.tylluan/bin"
DATA_DIR="${HOME}/.tylluan"
CONFIG_FILE="${DATA_DIR}/config.toml"

say() { printf "\033[1;32m%s\033[0m\n" "$*" >&2; }
info() { printf "\033[1;34m%s\033[0m\n" "$*" >&2; }
err() { printf "\033[1;31m%s\033[0m\n" "$*" >&2; exit 1; }

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$OS" in
  linux)
    case "$ARCH" in
      aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
      x86_64)        TARGET="x86_64-unknown-linux-gnu" ;;
      *)             err "Unsupported Linux architecture: $ARCH. Tylluan supports x86_64 and aarch64." ;;
    esac
    ;;
  darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      x86_64)        TARGET="x86_64-apple-darwin" ;;
      *)             err "Unsupported macOS architecture: $ARCH. Tylluan supports Apple Silicon and Intel." ;;
    esac
    ;;
  *) err "Unsupported OS: $OS. Tylluan supports Linux, macOS, and Windows." ;;
esac

info "Tylluan Installer — v0.12.0"
info "Detected: ${OS} (${TARGET})"
say ""

say "Detecting latest release..."
LATEST=$(curl -fsL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | cut -d'"' -f4 | sed 's/^v//')
[ -n "$LATEST" ] || err "Could not detect latest version from GitHub. Check your internet connection."

ARCHIVE="tylluan-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${LATEST}/${ARCHIVE}"

say "Downloading Tylluan v${LATEST} (${TARGET})..."
mkdir -p "$BIN_DIR"
TMP_ARCHIVE=$(mktemp) || err "Failed to create temp file"
trap 'rm -f "$TMP_ARCHIVE"' EXIT
if ! curl -fsL "$URL" -o "$TMP_ARCHIVE"; then
  rm -f "$TMP_ARCHIVE"
  err "Download failed. Check your internet: $URL"
fi
if ! tar tzf "$TMP_ARCHIVE" >/dev/null 2>&1; then
  rm -f "$TMP_ARCHIVE"
  err "Downloaded file is corrupted (not a valid archive). Try again."
fi
tar xzf "$TMP_ARCHIVE" -C "$BIN_DIR" --strip-components=1
rm -f "$TMP_ARCHIVE"
trap - EXIT

chmod +x "$BIN_DIR"/tylluan-nexus "$BIN_DIR"/tylluan-cli 2>/dev/null || true
# Backward compat: symlink tylluan-cli -> tylluan if only one exists
if [ ! -f "$BIN_DIR/tylluan" ] && [ -f "$BIN_DIR/tylluan-cli" ]; then
  ln -sf "$BIN_DIR/tylluan-cli" "$BIN_DIR/tylluan"
fi

if ! echo ":$PATH:" | grep -qF ":$BIN_DIR:"; then
  SHELL_PROFILE=""
  case "${SHELL:-}" in
    */zsh) SHELL_PROFILE="${ZDOTDIR:-$HOME}/.zshrc" ;;
    */bash) SHELL_PROFILE="$HOME/.bashrc" ;;
  esac
  if [ -n "$SHELL_PROFILE" ]; then
    echo "export PATH=\"\$PATH:${BIN_DIR}\"" >> "$SHELL_PROFILE"
    say "Added ${BIN_DIR} to PATH in ${SHELL_PROFILE}"
    say "   → Run: source ${SHELL_PROFILE}"
  else
    info "Add ${BIN_DIR} to your PATH manually, or run:"
    info "   export PATH=\"\$PATH:${BIN_DIR}\""
  fi
fi

say ""
# `install --profile portable` writes tylluan.toml AND boots the kernel
# itself (chdir's to the config dir first so the kernel finds it). If a
# config already exists it refuses without --force, so boot directly --
# from the config dir, otherwise the kernel would not discover the file.
if [ -f "${DATA_DIR}/tylluan.toml" ]; then
  say "Existing tylluan.toml found — keeping it. Starting kernel..."
  (cd "${DATA_DIR}" && "${BIN_DIR}/tylluan" start) &
else
  say "Installing portable profile (writes tylluan.toml + starts kernel)..."
  "${BIN_DIR}/tylluan" install --profile portable
fi

say "Waiting for kernel to be ready..."
for i in $(seq 1 30); do
  if curl -s "http://127.0.0.1:47004/health" >/dev/null 2>&1; then
    say "Tylluan is running at http://127.0.0.1:47004"
    break
  fi
  if [ "$i" -eq 30 ]; then
    err "Kernel did not start within 30 seconds. Check logs at ${DATA_DIR}/logs/"
  fi
  printf "."
  sleep 1
done
say ""

say "Connect your MCP client:"
say ""
say "  Claude Desktop (~/.claude/claude_desktop_config.json):"
echo '  {'
echo '    "mcpServers": {'
echo '      "tylluan": { "type": "sse",'
echo '        "url": "http://127.0.0.1:47004/sse" }'
echo '    }'
echo '  }'
say ""
say "  Claude Code:"
say '    /mcp add tylluan sse http://127.0.0.1:47004/sse'
say ""
say "  Cursor:"
say "    Add MCP server: http://127.0.0.1:47004/sse"
say ""
say "  curl (verify):"
say "    curl http://127.0.0.1:47004/health"
say ""
say "Getting started:"
say "  tylluan start    Start the Tylluan kernel"
say "  tylluan status   Check if kernel is running"
say "  tylluan doctor   Run diagnostic checks"

say ""
say "For better retrieval (BGE-M3):"
info "  tylluan download-models"
say ""

# ── Python guilds (optional — the 46 Python tool plugins) ──────────────
# Detection only: NEVER pip-install into the user's system Python from an
# installer (PEP 668 blocks it on Debian/Ubuntu/Fedora and it mutates the
# user's environment without consent). Print the exact command instead.
say "Python guilds (46 tools, optional):"
GUILD_PY=""
for c in python3 python; do
  if command -v "$c" >/dev/null 2>&1; then
    GUILD_PY="$c"
    break
  fi
done
if [ -z "$GUILD_PY" ]; then
  info "  Python 3.12+ not found — Python guilds won't run (kernel + MCP memory unaffected)."
  info "  Install Python 3.12+: https://www.python.org/downloads/"
elif ! "$GUILD_PY" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 12) else 1)' >/dev/null 2>&1; then
  info "  $("$GUILD_PY" --version 2>&1) found — guilds need Python 3.12+."
  info "  Upgrade: https://www.python.org/downloads/"
elif "$GUILD_PY" -c 'import mcp, fastmcp, psutil' >/dev/null 2>&1; then
  say "  $("$GUILD_PY" --version 2>&1) + guild deps OK"
else
  REQ_DIR="https://raw.githubusercontent.com/Forja-orca/tylluan/main/guilds"
  if [ -f guilds/requirements.txt ]; then
    REQ_DIR="guilds"
  fi
  info "  $("$GUILD_PY" --version 2>&1) found, guild deps missing. Install them with:"
  info "    $GUILD_PY -m pip install -r ${REQ_DIR}/requirements.txt"
fi
say ""

say "Tylluan v${LATEST} installed to ${BIN_DIR}/"
