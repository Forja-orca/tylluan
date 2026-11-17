#!/usr/bin/env bash
set -euo pipefail

REPO="Forja-orca/tylluan"
TYLLUAN_HOME="${HOME}/.tylluan"
BIN_DIR="${TYLLUAN_HOME}/bin"
DATA_DIR="${TYLLUAN_HOME}"
CONFIG_FILE="${DATA_DIR}/config.toml"
# D192: registro de instancias con nombre (el mismo que lee `tylluan --instance`).
REGISTRY_FILE="${TYLLUAN_INSTANCES_FILE:-${TYLLUAN_HOME}/instances.toml}"

say() { printf "\033[1;32m%s\033[0m\n" "$*" >&2; }
info() { printf "\033[1;34m%s\033[0m\n" "$*" >&2; }
warn() { printf "\033[1;31m%s\033[0m\n" "$*" >&2; }
err() { printf "\033[1;31m%s\033[0m\n" "$*" >&2; exit 1; }

NO_START=0
SKIP_GUILD_DEPS=0
ALLOW_UNHASHED_DEPS=0
INSTANCE="default"
UPGRADE=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-start) NO_START=1 ;;
    # P1 installer: skip creating ${TYLLUAN_HOME}/.venv and installing the Python
    # guild dependencies there (installed by default; the operator's system
    # Python is never touched).
    --skip-guild-deps) SKIP_GUILD_DEPS=1 ;;
    # P1 (TL T3907): installing guild deps WITHOUT the hash lock is opt-in.
    # Default is fail-closed: no requirements-hashes.txt means no pip run.
    --allow-unhashed-deps) ALLOW_UNHASHED_DEPS=1 ;;
    # D192: instancia con nombre en vez de `default` (ver bloque de instancias).
    --instance) [ "$#" -ge 2 ] || err "--instance necesita un nombre"; INSTANCE="$2"; shift ;;
    --instance=*) INSTANCE="${1#--instance=}" ;;
    # D192: reinstalar encima de una instancia YA declarada es explicito.
    --upgrade) UPGRADE=1 ;;
    *) err "Unknown option: $1. Usage: install.sh [--no-start] [--skip-guild-deps] [--allow-unhashed-deps] [--instance <nombre>] [--upgrade]" ;;
  esac
  shift
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
    # D192: mirar solo `$PATH` no basta. La sesión NO tiene la línea hasta que se
    # hace `source`, así que una segunda instalación la volvía a escribir: con
    # instancias, instalar N veces dejaba N `export PATH` idénticos en el perfil
    # (medido: D192_prueba_path_shell.log). Se mira también el perfil.
    if grep -qF "PATH=\"\$PATH:${BIN_DIR}\"" "$SHELL_PROFILE" 2>/dev/null; then
      info "PATH ya declarado en ${SHELL_PROFILE} — no lo repito."
    else
      echo "export PATH=\"\$PATH:${BIN_DIR}\"" >> "$SHELL_PROFILE"
      say "Added ${BIN_DIR} to PATH in ${SHELL_PROFILE}"
      say "   → Run: source ${SHELL_PROFILE}"
    fi
  else
    info "Add ${BIN_DIR} to your PATH manually, or run:"
    info "   export PATH=\"\$PATH:${BIN_DIR}\""
  fi
fi

say ""
# ── Instancias con nombre (D192) ───────────────────────────────────────
# `default` es la de siempre (~/.tylluan). Cualquier otro nombre vive en
# ~/.tylluan/instances/<nombre>, con su propio DATA_DIR y su propio puerto, y
# queda declarado en ${REGISTRY_FILE}. El registro se escribe en modo
# append-only: instalar una instancia NUNCA reescribe los bloques existentes, y
# en particular nunca toca el de `default` (ni sus datos ni su config).
#
# D194/T4437: el puerto por defecto NO se escribe aqui — se pregunta al CLI, que
# es el dueno del valor (crates/tylluan-cli/src/instances.rs, DEFAULT_PORT). Este
# script solo lo copia al registro y usa $PORT en todo lo demas (health, PID,
# `start`). Asi el numero vive en un solo sitio.
# `tail -n 1`: si el CLI escribe un aviso por stdout antes del numero, quedarse con
# TODO el flujo pegaba los digitos del aviso al puerto y salia un "puerto" de seis
# cifras (medido: el instalador lo escribia en el registro sin quejarse). Ademas se
# valida el rango, que es lo que corta ese caso.
CLI="${BIN_DIR}/tylluan"
# `|| true` dentro del grupo: con `set -o pipefail`, un CLI que no conoce el flag
# sale con rc=2 y, al ser una asignacion, tumbaba el instalador entero (medido:
# rc=2 sin ningun mensaje). El fallback existe justo para ese caso.
DEFAULT_PORT_RAW="$({ "$CLI" --print-default-port 2>/dev/null || true; } | tail -n 1)"
DEFAULT_INSTANCE_PORT="$(printf '%s' "$DEFAULT_PORT_RAW" | tr -dc '0-9')"

# D195 (1): el Install Smoke instala con el CLI **publicado**, que puede ser
# anterior a las instancias. Se le pregunta; si no sabe, se instala en modo
# clasico (default, sin registro nuevo y sin --port), que es lo que entiende.
if [ -n "$DEFAULT_INSTANCE_PORT" ] \
  && [ "$DEFAULT_INSTANCE_PORT" -ge 1 ] \
  && [ "$DEFAULT_INSTANCE_PORT" -le 65535 ]; then
  CLI_HAS_INSTANCES=1
else
  CLI_HAS_INSTANCES=0
  # FALLBACK (2): valor historico, SOLO para un CLI publicado anterior a D194
  # (v0.17.2 y anteriores), que no conoce `--print-default-port`. Con un CLI que
  # sabe contestar, este numero no se usa nunca.
  DEFAULT_INSTANCE_PORT=47004
fi

case "$INSTANCE" in
  *[!A-Za-z0-9_-]*|"") err "Nombre de instancia invalido: '${INSTANCE}' (usa letras, numeros, '-' o '_')" ;;
esac

# D195: con un CLI que no conoce instancias, pedir una con nombre es un error
# claro y temprano (no un `start --instance` que reventaria al final).
if [ "$CLI_HAS_INSTANCES" -eq 0 ] && [ "$INSTANCE" != "default" ]; then
  err "El binario instalado (${CLI}) no soporta instancias con nombre: no conoce '--instance'. Instala sin --instance, o usa un release posterior."
fi

mkdir -p "${TYLLUAN_HOME}"
touch "${REGISTRY_FILE}"

# Un TOML valido puede no acabar en salto de linea; sin esto el bloque nuevo se
# pegaria a la ultima linea del anterior.
ensure_trailing_newline() {
  if [ -s "$REGISTRY_FILE" ] && [ -n "$(tail -c 1 "$REGISTRY_FILE" 2>/dev/null || true)" ]; then
    printf '\n' >> "$REGISTRY_FILE"
  fi
}

instance_block() {
  awk -v name="$1" '
    $0 ~ "^\\[instances\\." name "\\]$" { f = 1; next }
    /^\[/ { f = 0 }
    f { print }
  ' "$REGISTRY_FILE"
}

instance_declared() { [ -n "$(instance_block "$1")" ]; }

instance_port() {
  instance_block "$1" \
    | sed -n 's/^[[:space:]]*port[[:space:]]*=[[:space:]]*\([0-9][0-9]*\).*/\1/p' \
    | head -1
}

# Sin `[instances.default]`, un registro que solo declare otra instancia hace que
# `tylluan` (sin --instance) falle cerrado. Se deja SIEMPRE declarado.
if ! instance_declared default; then
  ensure_trailing_newline
  printf '\n[instances.default]\nport = %s\n' "$DEFAULT_INSTANCE_PORT" >> "$REGISTRY_FILE"
  info "Declarada [instances.default] port = ${DEFAULT_INSTANCE_PORT} en ${REGISTRY_FILE}"
fi

port_free() {
  if (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; then
    return 1
  fi
  return 0
}

next_free_port() {
  _p=47005
  while [ "$_p" -lt 47100 ]; do
    if ! grep -qE "^[[:space:]]*port[[:space:]]*=[[:space:]]*${_p}([[:space:]]|$)" "$REGISTRY_FILE" \
      && port_free "$_p"; then
      printf '%s\n' "$_p"
      return 0
    fi
    _p=$((_p + 1))
  done
  return 1
}

if [ "$INSTANCE" = "default" ]; then
  DATA_DIR="${TYLLUAN_HOME}"
  PORT="$(instance_port default)"
  PORT="${PORT:-$DEFAULT_INSTANCE_PORT}"
  info "Instancia: default (${DATA_DIR}, puerto ${PORT})"
else
  DATA_DIR="${TYLLUAN_HOME}/instances/${INSTANCE}"
  if instance_declared "$INSTANCE"; then
    if [ "$UPGRADE" -eq 0 ]; then
      err "La instancia '${INSTANCE}' ya existe en ${REGISTRY_FILE}. Reinstalar encima es explicito: repite con --upgrade."
    fi
    PORT="$(instance_port "$INSTANCE")"
    [ -n "$PORT" ] || err "La instancia '${INSTANCE}' esta declarada sin puerto en ${REGISTRY_FILE}."
    info "Instancia: ${INSTANCE} (--upgrade) -> ${DATA_DIR}, puerto ${PORT}"
  else
    if [ "$UPGRADE" -eq 1 ]; then
      err "No existe la instancia '${INSTANCE}': --upgrade solo reinstala una ya declarada."
    fi
    PORT="$(next_free_port)" || err "No hay puerto libre a partir de 47005."
    ensure_trailing_newline
    printf '\n[instances.%s]\nport = %s\ndata_dir = "%s"\n' \
      "$INSTANCE" "$PORT" "${DATA_DIR}/data" >> "$REGISTRY_FILE"
    say "Instancia '${INSTANCE}' declarada en ${REGISTRY_FILE} (puerto ${PORT})."
  fi
fi
say ""

# D195 (1): `--port` solo existe en CLIs nuevos y para `default` no hace falta
# (su puerto ya es el que escribe el config). Se pasa unicamente si hay instancia
# con nombre Y el CLI lo anuncia en `install --help`.
INSTALL_PORT_ARG=""
if [ "$INSTANCE" != "default" ]; then
  if "$CLI" install --help 2>&1 | grep -q -- '--port'; then
    INSTALL_PORT_ARG="--port ${PORT}"
  elif [ "$CLI_HAS_INSTANCES" -eq 1 ]; then
    err "Este binario no acepta 'install --port': no puedo fijar el puerto de la instancia '${INSTANCE}'."
  fi
fi

KERNEL_DATA_DIR="${TYLLUAN_DATA_DIR:-${DATA_DIR}/data}"
PID_FILE="${KERNEL_DATA_DIR}/tylluan-nexus.pid"
OLD_PID=""
if [ -f "$PID_FILE" ]; then
  # D195/R1 (Hermes): el PID file puede llevar una segunda linea `instance=…`;
# leerlo entero metia "PID\ninstance=x" en la comparacion y `kill -0` fallaba con
# el kernel vivo.
OLD_PID=$(head -1 "$PID_FILE" 2>/dev/null || true)
fi
# ── Python guilds (the 45 Python tool plugins) ─────────────────────────
# Policy: NEVER pip-install into the user's system Python from an installer
# (PEP 668 blocks it on Debian/Ubuntu/Fedora and it mutates the user's
# environment without consent). The operator's Python only *creates* a venv;
# the dependencies go into ${TYLLUAN_HOME}/.venv, which the kernel already
# prefers over the system interpreter (registry/guild_process.rs, find_python()).
#
# D192: es **una sola** .venv para todas las instancias, a propósito.
# `find_python()` busca `.venv` primero en la cadena del **ejecutable**
# (guild_process.rs:2391) y `~/.tylluan/.venv` es el segundo eslabón de
# `~/.tylluan/bin/tylluan-nexus`: si existiera, ganaría siempre a la de la
# instancia. Crear una .venv por instancia sería disco y tiempo tirados, y el
# `.venv` que usa el kernel sería otro (el del default) sin decirlo.
say "Python guilds (45 tools):"
GUILD_REQ="${BIN_DIR}/guilds"
if [ ! -f "${GUILD_REQ}/requirements.txt" ]; then
  GUILD_REQ="guilds"
fi
VENV_DIR="${TYLLUAN_HOME}/.venv"
VENV_PY="${VENV_DIR}/bin/python"
REQ_HASHES="${GUILD_REQ}/requirements-hashes.txt"
REQ_PLAIN="${GUILD_REQ}/requirements.txt"

guild_deps_hint() {
  if [ -f "${REQ_HASHES}" ]; then
    info "  Install them later with:"
    info "    ${VENV_PY} -m pip install --require-hashes -r ${REQ_HASHES}"
  else
    warn "  SIN VERIFICACION DE HASHES: este release no trae requirements-hashes.txt."
    warn "  No hay un comando verificado que darte: vuelve a descargar el release"
    warn "  (deberia incluir guilds/requirements-hashes.txt) o revisa la cadena de suministro."
  fi
}

if [ "$SKIP_GUILD_DEPS" -eq 1 ]; then
  info "  Skipped (--skip-guild-deps)."
  guild_deps_hint
elif [ -x "${VENV_PY}" ] && "${VENV_PY}" -c 'import mcp, fastmcp, psutil' >/dev/null 2>&1; then
  say "  guild deps OK in ${VENV_DIR}"
else
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
    [ -x "${VENV_PY}" ] && guild_deps_hint || true
  elif ! "$GUILD_PY" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 12) else 1)' >/dev/null 2>&1; then
    info "  $("$GUILD_PY" --version 2>&1) found — guilds need Python 3.12+."
    info "  Upgrade: https://www.python.org/downloads/"
  else
    if [ ! -x "${VENV_PY}" ]; then
      info "  Creating ${VENV_DIR} with $("$GUILD_PY" --version 2>&1)..."
      "$GUILD_PY" -m venv "${VENV_DIR}" || true
    fi
    if [ ! -x "${VENV_PY}" ]; then
      info "  Could not create ${VENV_DIR}."
      info "  Create it yourself with: ${GUILD_PY} -m venv ${VENV_DIR}"
      guild_deps_hint
    else
      info "  Installing guild dependencies into ${VENV_DIR} (one time, this can take a few minutes)..."
      if [ -f "${REQ_HASHES}" ]; then
        "${VENV_PY}" -m pip install --require-hashes --disable-pip-version-check -r "${REQ_HASHES}" || true
      elif [ "$ALLOW_UNHASHED_DEPS" -eq 1 ]; then
        warn "  AVISO: SIN VERIFICACION DE HASHES (el release no trae requirements-hashes.txt)."
        warn "  Sigues con --allow-unhashed-deps: se instalan rangos sin comprobar."
        "${VENV_PY}" -m pip install --disable-pip-version-check -r "${REQ_PLAIN}" || true
      else
        warn "  SIN VERIFICACION DE HASHES: el release no trae requirements-hashes.txt."
        warn "  NO instalo dependencias Python sin hashes (falla cerrado)."
        warn "  Si aceptas el riesgo, repite con --allow-unhashed-deps."
      fi
      if "${VENV_PY}" -c 'import mcp, fastmcp, psutil' >/dev/null 2>&1; then
        say "  guild deps OK in ${VENV_DIR}"
      else
        info "  Guild dependencies are NOT installed (pip did not finish)."
        guild_deps_hint
      fi
    fi
  fi
fi
say ""
# `install --profile portable` writes tylluan.toml AND boots the kernel
# itself (chdir's to the config dir first so the kernel finds it). If a
# config already exists it refuses without --force, so boot directly --
# from the config dir, otherwise the kernel would not discover the file.
if [ "$NO_START" -eq 1 ]; then
  if [ -f "${DATA_DIR}/tylluan.toml" ]; then
    say "Existing tylluan.toml found - keeping it. Kernel not started (--no-start)."
  else
    say "Installing portable profile (writes tylluan.toml, no start)..."
    # --dir: el CLI resolvia HOME (antes que nada) y podia escribir el
    # tylluan.toml en un home distinto del DATA_DIR de este instalador, dejando
    # huerfano el .venv que creamos aqui. Fijarlo mantiene todo junto.
    # --port (D192): el config generado tiene que declarar el puerto de ESTA
    # instancia; si no, el kernel arrancaria en el puerto de `default`.
    "${BIN_DIR}/tylluan" install --profile portable --no-start --dir "${DATA_DIR}" ${INSTALL_PORT_ARG}
  fi
else
  if [ -f "${DATA_DIR}/tylluan.toml" ]; then
    say "Existing tylluan.toml found - keeping it. Starting kernel..."
    if [ "$INSTANCE" = "default" ]; then
      (cd "${DATA_DIR}" && "${BIN_DIR}/tylluan" start) &
    else
      (cd "${DATA_DIR}" && "${BIN_DIR}/tylluan" start --instance "${INSTANCE}") &
    fi
  else
    say "Installing portable profile (writes tylluan.toml + starts kernel)..."
    if [ "$INSTANCE" = "default" ]; then
      "${BIN_DIR}/tylluan" install --profile portable --dir "${DATA_DIR}" ${INSTALL_PORT_ARG}
    else
      # En modo instancia el config se escribe sin arrancar y el arranque va por
      # `start --instance`: es el unico camino que le pasa al kernel su
      # TYLLUAN_DATA_DIR (si no, el kernel usaria ./data-port-<puerto>).
      "${BIN_DIR}/tylluan" install --profile portable --no-start --dir "${DATA_DIR}" ${INSTALL_PORT_ARG}
      (cd "${DATA_DIR}" && "${BIN_DIR}/tylluan" start --instance "${INSTANCE}") &
    fi
  fi

  say "Waiting for kernel to be ready..."
  KERNEL_PID=""
  for i in $(seq 1 30); do
    if [ -f "$PID_FILE" ]; then
      KERNEL_PID=$(head -1 "$PID_FILE" 2>/dev/null || true)  # D195/R1
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
    if curl -s "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
      if ! kill -0 "$KERNEL_PID" 2>/dev/null; then
        err "Port ${PORT} answers but PID ${KERNEL_PID} is not alive: another kernel is occupying it."
      fi
      say "Tylluan is running at http://127.0.0.1:${PORT} (PID ${KERNEL_PID})"
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
printf '      "tylluan": { "type": "sse",\n'
printf '        "url": "http://127.0.0.1:%s/sse" }\n' "$PORT"
echo '    }'
echo '  }'
say ""
say "  Claude Code:"
say "    /mcp add tylluan sse http://127.0.0.1:${PORT}/sse"
say ""
say "  Cursor:"
say "    Add MCP server: http://127.0.0.1:${PORT}/sse"
say ""
say "  curl (verify):"
say "    curl http://127.0.0.1:${PORT}/health"
say ""
say "Getting started:"
say "  tylluan start    Start the Tylluan kernel"
say "  tylluan status   Check if kernel is running"
say "  tylluan doctor   Run diagnostic checks"

say ""
say "For better retrieval (BGE-M3):"
info "  tylluan download-models"
say ""


if [ -n "$LATEST" ]; then
  say "Tylluan v${LATEST} installed to ${BIN_DIR}/"
else
  say "Tylluan installed to ${BIN_DIR}/"
fi
