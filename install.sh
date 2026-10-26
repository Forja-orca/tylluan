#!/usr/bin/env bash
set -euo pipefail

REPO="Forja-orca/tylluan"
BIN_DIR="${HOME}/.tylluan/bin"
DATA_DIR="${HOME}/.tylluan"
CONFIG_FILE="${DATA_DIR}/config.toml"

say() { printf "\033[1;32m%s\033[0m\n" "$*" >&2; }
info() { printf "\033[1;34m%s\033[0m\n" "$*" >&2; }
err() { printf "\033[1;31m%s\033[0m\n" "$*" >&2; exit 1; }

NO_START=0
for arg in "$@"; do
  case "$arg" in
    --no-start) NO_START=1 ;;
    *) err "Unknown option: $arg. Usage: install.sh [--no-start]" ;;
  esac
done

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
      x86_64)        err "macOS Intel (x86_64) is not supported by the published releases yet. Use an Apple Silicon Mac or build from source: docs/getting-started/QUICKSTART.md" ;;
      *)             err "Unsupported macOS architecture: $ARCH. Tylluan releases support Apple Silicon only." ;;
    esac
    ;;
  *) err "Unsupported OS: $OS. Tylluan supports Linux, macOS, and Windows." ;;
esac

info "Tylluan Installer"
info "Detected: ${OS} (${TARGET})"
say ""

ARCHIVE="tylluan-${TARGET}.tar.gz"

is_github_url() {
  # Compare the HOST only: a glob over the whole URL lets '*' cross a '/'
  # (https://evil.example/x.github.com/f would match) and leak GITHUB_TOKEN.
  case "$1" in https://*) ;; *) return 1 ;; esac
  _host="${1#https://}"
  _host="${_host%%/*}"
  case "$_host" in
    *@*|*:*) return 1 ;;
    github.com|*.github.com|*.githubusercontent.com) return 0 ;;
    *) return 1 ;;
  esac
}

curl_github() {
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    curl -H "Authorization: Bearer ${GITHUB_TOKEN}" "$@"
  else
    curl "$@"
  fi
}

LATEST=""
if [ -n "${TYLLUAN_DOWNLOAD_URL:-}" ]; then
  URL="${TYLLUAN_DOWNLOAD_URL}"
  say "Using explicit download URL: ${URL}"
  say "Fetching Tylluan (${TARGET})..."
elif [ -n "${TYLLUAN_VERSION:-}" ]; then
  LATEST="${TYLLUAN_VERSION#v}"
  URL="https://github.com/${REPO}/releases/download/v${LATEST}/${ARCHIVE}"
  say "Using explicit version: v${LATEST} (${TARGET})..."
  say "Downloading Tylluan v${LATEST} (${TARGET})..."
else
  say "Detecting latest release..."
  # Resolve latest release tag without api.github.com to avoid anonymous API rate limits (T1128).
  # Following the HEAD redirect of /releases/latest yields Location: .../releases/tag/vX.Y.Z
  LATEST=$( (curl_github -sI "https://github.com/${REPO}/releases/latest" 2>/dev/null \
    | tr -d '\r' | grep -i '^location:' | head -1 | sed -E 's|.*/tag/v?||') || true )

  if [ -n "$LATEST" ]; then
    URL="https://github.com/${REPO}/releases/download/v${LATEST}/${ARCHIVE}"
    say "Downloading Tylluan v${LATEST} (${TARGET})..."
  else
    # Fallback to direct latest download redirect
    URL="https://github.com/${REPO}/releases/latest/download/${ARCHIVE}"
    say "Downloading Tylluan latest (${TARGET})..."
  fi
fi

mkdir -p "$BIN_DIR"
TMP_ARCHIVE=$(mktemp) || err "Failed to create temp file"
trap 'rm -f "$TMP_ARCHIVE"' EXIT
if [ -f "$URL" ]; then
  cp "$URL" "$TMP_ARCHIVE"
elif is_github_url "$URL"; then
  if ! curl_github -fsL "$URL" -o "$TMP_ARCHIVE"; then
    rm -f "$TMP_ARCHIVE"
    err "Download failed. Check your internet: $URL"
  fi
else
  if ! curl -fsL "$URL" -o "$TMP_ARCHIVE"; then
    rm -f "$TMP_ARCHIVE"
    err "Download failed. Check your internet: $URL"
  fi
fi
if ! tar tzf "$TMP_ARCHIVE" >/dev/null 2>&1; then
  rm -f "$TMP_ARCHIVE"
  err "Downloaded file is corrupted (not a valid archive). Try again."
fi
TMP_STAGE=$(mktemp -d) || { rm -f "$TMP_ARCHIVE"; err "Failed to create staging directory"; }
if ! tar xzf "$TMP_ARCHIVE" -C "$TMP_STAGE" --strip-components=1; then
  rm -rf "$TMP_STAGE" "$TMP_ARCHIVE"
  err "Extraction failed"
fi
CHECKSUM_FILE="$TMP_STAGE/checksums.txt"
if [ ! -f "$CHECKSUM_FILE" ]; then
  rm -rf "$TMP_STAGE" "$TMP_ARCHIVE"
  err "Release archive has no checksums.txt; refusing to install unverified binaries."
fi
if command -v sha256sum >/dev/null 2>&1; then
  if ! (cd "$TMP_STAGE" && sha256sum -c checksums.txt); then
    rm -rf "$TMP_STAGE" "$TMP_ARCHIVE"
    err "SHA-256 verification failed; refusing to install."
  fi
elif command -v shasum >/dev/null 2>&1; then
  if ! (cd "$TMP_STAGE" && shasum -a 256 -c checksums.txt); then
    rm -rf "$TMP_STAGE" "$TMP_ARCHIVE"
    err "SHA-256 verification failed; refusing to install."
  fi
else
  rm -rf "$TMP_STAGE" "$TMP_ARCHIVE"
  err "No sha256sum/shasum available; cannot verify the release checksum."
fi
cp -R "$TMP_STAGE"/. "$BIN_DIR"/
rm -rf "$TMP_STAGE"
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
KERNEL_DATA_DIR="${TYLLUAN_DATA_DIR:-${DATA_DIR}/data}"
PID_FILE="${KERNEL_DATA_DIR}/tylluan-nexus.pid"
OLD_PID=""
if [ -f "$PID_FILE" ]; then
  OLD_PID=$(cat "$PID_FILE" 2>/dev/null || true)
fi
# `install --profile portable` writes tylluan.toml AND boots the kernel
# itself (chdir's to the config dir first so the kernel finds it). If a
# config already exists it refuses without --force, so boot directly --
# from the config dir, otherwise the kernel would not discover the file.
if [ "$NO_START" -eq 1 ]; then
  if [ -f "${DATA_DIR}/tylluan.toml" ]; then
    say "Existing tylluan.toml found - keeping it. Kernel not started (--no-start)."
  else
    say "Installing portable profile (writes tylluan.toml, no start)..."
    "${BIN_DIR}/tylluan" install --profile portable --no-start
  fi
else
  if [ -f "${DATA_DIR}/tylluan.toml" ]; then
    say "Existing tylluan.toml found - keeping it. Starting kernel..."
    (cd "${DATA_DIR}" && "${BIN_DIR}/tylluan" start) &
  else
    say "Installing portable profile (writes tylluan.toml + starts kernel)..."
    "${BIN_DIR}/tylluan" install --profile portable
  fi

  say "Waiting for kernel to be ready..."
  KERNEL_PID=""
  for i in $(seq 1 30); do
    if [ -f "$PID_FILE" ]; then
      KERNEL_PID=$(cat "$PID_FILE" 2>/dev/null || true)
      if [ -n "$KERNEL_PID" ] && kill -0 "$KERNEL_PID" 2>/dev/null; then
        break
      fi
    fi
    if [ "$i" -eq 30 ]; then
      err "Kernel did not write a live PID file at ${PID_FILE}. Check logs at ${DATA_DIR}/logs/"
    fi
    sleep 1
  done
  if [ -n "$OLD_PID" ] && [ "$KERNEL_PID" = "$OLD_PID" ]; then
    err "PID file still points to the previous kernel (PID ${OLD_PID}); refusing to report success."
  fi
  for i in $(seq 1 30); do
    if curl -s "http://127.0.0.1:47004/health" >/dev/null 2>&1; then
      if ! kill -0 "$KERNEL_PID" 2>/dev/null; then
        err "Port 47004 answers but PID ${KERNEL_PID} is not alive: another kernel is occupying it."
      fi
      say "Tylluan is running at http://127.0.0.1:47004 (PID ${KERNEL_PID})"
      break
    fi
    if [ "$i" -eq 30 ]; then
      err "Kernel did not start within 30 seconds. Check logs at ${DATA_DIR}/logs/"
    fi
    printf "."
    sleep 1
  done
fi
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

# ── Python guilds (optional — the 45 Python tool plugins) ──────────────
# Detection only: NEVER pip-install into the user's system Python from an
# installer (PEP 668 blocks it on Debian/Ubuntu/Fedora and it mutates the
# user's environment without consent). Print the exact command instead.
say "Python guilds (45 tools, optional):"
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
  REQ_DIR="${BIN_DIR}/guilds"
  if [ ! -f "${REQ_DIR}/requirements.txt" ]; then
    REQ_DIR="guilds"
  fi
  info "  $("$GUILD_PY" --version 2>&1) found, guild deps missing. Install them with:"
  info "    $GUILD_PY -m pip install -r ${REQ_DIR}/requirements.txt"
fi
say ""

if [ -n "$LATEST" ]; then
  say "Tylluan v${LATEST} installed to ${BIN_DIR}/"
else
  say "Tylluan installed to ${BIN_DIR}/"
fi
