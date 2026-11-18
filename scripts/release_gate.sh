#!/usr/bin/env bash
# D210 (auditoría externa, gate de release aceptado por José): los tres gates que
# bloquean publicar un tarball roto.
#
#   1. `guilds/requirements-hashes.txt` existe en la RAÍZ CANÓNICA del tarball
#      (y `guilds/` no está solo dentro de `bin/`).
#   2. `guilds/` trae los guilds core y el requirements.txt.
#   3. Un guild REAL arranca con el intérprete de esta instalación (venv creado
#      desde el fichero con hashes) y completa el handshake MCP; falla ante
#      `ModuleNotFoundError` o crash-loop.
#   4. El `.tylluan-token` que crea el binario queda en modo **0600** (D206).
#
# Se extrae en un HOME temporal: nada toca el HOME del runner.
#
# Uso:
#   bash scripts/release_gate.sh --tarball tylluan-x86_64-unknown-linux-gnu.tar.gz
#   bash scripts/release_gate.sh --tarball … --python /ruta/python --reuse-python
#   bash scripts/release_gate.sh --tarball … --skip-token-gate
#
# Salidas: `::error::` (línea de GitHub Actions) y código != 0 si algún gate falla.
set -uo pipefail

TARBALL=""
PY="python3"
REUSE_PYTHON=0
SKIP_VENV_DEPS=0
SKIP_TOKEN_GATE=0
TOKEN_TIMEOUT=60

while [ "$#" -gt 0 ]; do
  case "$1" in
    --tarball) TARBALL="${2:?--tarball necesita ruta}"; shift ;;
    --python) PY="${2:?--python necesita comando}"; shift ;;
    --reuse-python) REUSE_PYTHON=1 ;;
    --skip-venv-deps) SKIP_VENV_DEPS=1 ;;
    --skip-token-gate) SKIP_TOKEN_GATE=1 ;;
    --token-timeout) TOKEN_TIMEOUT="${2:?}"; shift ;;
    -h|--help) sed -n '2,22p' "$0"; exit 0 ;;
    *) echo "::error::opción desconocida: $1"; exit 2 ;;
  esac
  shift
done

falla() { echo "::error::$*"; exit 1; }

[ -n "$TARBALL" ] || falla "falta --tarball"
[ -f "$TARBALL" ] || falla "no existe el tarball: $TARBALL"

WORK="$(mktemp -d)"
PID_KERNEL=""
limpieza() {
  if [ -n "$PID_KERNEL" ] && kill -0 "$PID_KERNEL" 2>/dev/null; then
    kill "$PID_KERNEL" 2>/dev/null || true
    sleep 1
    kill -9 "$PID_KERNEL" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap limpieza EXIT

ROOT="$WORK/root"
HOMETMP="$WORK/home"
mkdir -p "$ROOT" "$HOMETMP"

echo "== extrayendo $TARBALL en un HOME temporal =="
tar -xzf "$TARBALL" -C "$ROOT" || falla "el tarball no se puede extraer"
export HOME="$HOMETMP"

# ───────────────────────── gate 1: hashes en la raíz canónica ────────────────
echo "== gate 1: guilds/requirements-hashes.txt en la raíz del tarball =="
if [ -d "$ROOT/bin/guilds" ] && [ ! -d "$ROOT/guilds" ]; then
  falla "guilds/ está en bin/guilds y NO en la raíz canónica (el instalador no lo encuentra)"
fi
[ -f "$ROOT/guilds/requirements-hashes.txt" ] \
  || falla "falta guilds/requirements-hashes.txt en la raíz del tarball (el instalador se niega a instalar dependencias)"

# ───────────────────────── gate 2: guilds core presentes ─────────────────────
echo "== gate 2: guilds/ con requirements.txt y los core =="
for f in guilds/requirements.txt guilds/core/coloquio.py guilds/core/code_graph.py guilds/builders/plugins/bash.py; do
  [ -f "$ROOT/$f" ] || falla "falta $f en el tarball"
done
n_guilds="$(find "$ROOT/guilds" -name '*.py' -type f | wc -l | tr -d ' ')"
echo "::notice::guilds/ presente con $n_guilds ficheros .py"

# ───────────────────────── gate 3: un guild real arranca ─────────────────────
echo "== gate 3: un guild real arranca y contesta el handshake =="
if [ "$REUSE_PYTHON" -eq 1 ]; then
  VENVPY="$PY"
else
  VENV="$HOMETMP/.tylluan/.venv"
  mkdir -p "$HOMETMP/.tylluan"
  "$PY" -m venv "$VENV" || falla "no se pudo crear el venv con $PY"
  if [ "$(uname -s)" = "MINGW"* ] || [ "$(uname -s)" = "MSYS"* ]; then
    VENVPY="$VENV/Scripts/python.exe"
  else
    VENVPY="$VENV/bin/python"
  fi
  if [ "$SKIP_VENV_DEPS" -eq 0 ]; then
    "$VENVPY" -m pip install -q --disable-pip-version-check --require-hashes \
      -r "$ROOT/guilds/requirements-hashes.txt" \
      || falla "pip install --require-hashes falló (dependencias de los guilds)"
  fi
fi
[ -x "$VENVPY" ] || falla "no hay intérprete ejecutable en $VENVPY"

# El guild se lanza EXACTAMENTE como lo lanza el kernel (guild_process.rs:807-823):
# `python -m guilds.core.code_graph`, cwd = raíz del tarball, y PYTHONPATH apuntando a
# esa raíz. Lanzarlo como `python guilds/core/code_graph.py` rompe `from guilds.core import
# utils` por el root de import (medido: ModuleNotFoundError: No module named 'guilds').
MODULE="guilds.core.code_graph"
[ -f "$ROOT/guilds/core/code_graph.py" ] || falla "no existe guilds/core/code_graph.py para el arranque de prueba"

SALIDA="$WORK/guild.out"
( cd "$ROOT" && printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"release-gate","version":"1"}}}' \
    | PYTHONPATH="$ROOT" PYTHONUNBUFFERED=1 TYLLUAN_ROOT="$ROOT" \
      timeout 60 "$VENVPY" -m "$MODULE" >"$SALIDA" 2>&1 ) || true

# Orden de las comprobaciones, a propósito: el handshake manda. Un guild puede AVISAR
# de una dependencia OPCIONAL (medido: code_graph.py avisa de `tree_sitter` y contesta
# igual) y eso NO puede tumbar la release. Solo si NO hubo handshake se clasifica el fallo.
if grep -q '"result"' "$SALIDA"; then
  if grep -qi 'No module named' "$SALIDA"; then
    echo "::notice::el guild avisó de una dependencia opcional ausente, pero contestó el handshake:"
    grep -i 'No module named' "$SALIDA" | head -3 || true
  fi
  echo "::notice::guild real ($MODULE) arrancó y contestó el handshake"
else
  echo "----- salida del guild (sin handshake) -----"; tail -20 "$SALIDA"
  if grep -qi "No module named 'guilds'" "$SALIDA"; then
    falla "el guild no arranca: 'guilds' no es importable (root de import, no dependencias)"
  elif grep -qi 'ModuleNotFoundError\|No module named' "$SALIDA"; then
    falla "el guild no arrancó: dependencia del intérprete que falta (ModuleNotFoundError)"
  else
    falla "el guild no completó el handshake MCP (sin \"result\" en 60 s)"
  fi
fi

# ───────────────────────── gate 4: el token queda en 0600 ────────────────────
if [ "$SKIP_TOKEN_GATE" -eq 1 ]; then
  echo "== gate 4: OMITIDO (--skip-token-gate) =="
else
  echo "== gate 4: el .tylluan-token se crea en 0600 =="
  case "$(uname -s)" in
    MINGW*|MSYS*) echo "::notice::gate 4 omitido: en Windows no hay modos POSIX (la ACL manda)"; TOKEN_GATE=skip ;;
    *) TOKEN_GATE=run ;;
  esac
  if [ "$TOKEN_GATE" = run ]; then
    BIN="$ROOT/tylluan-nexus"
    [ -x "$BIN" ] || falla "no hay tylluan-nexus ejecutable en el tarball"
    CFG="$WORK/tylluan.toml"
    sed -e 's/^[[:space:]]*dev_mode[[:space:]]*=.*/dev_mode = false/' \
        -e "s|^[[:space:]]*port[[:space:]]*=.*|port = 47991|" \
        "$ROOT/tylluan.example.toml" > "$CFG" 2>/dev/null \
      || printf '[nexus]\nhost = "127.0.0.1"\nport = 47991\ndev_mode = false\n' > "$CFG"
    ( cd "$WORK" && "$BIN" --config "$CFG" >"$WORK/kernel.log" 2>&1 ) &
    PID_KERNEL=$!
    TOKEN="$WORK/.tylluan-token"
    n=0
    while [ ! -f "$TOKEN" ] && [ "$n" -lt "$TOKEN_TIMEOUT" ]; do sleep 1; n=$((n+1)); done
    [ -f "$TOKEN" ] || { tail -20 "$WORK/kernel.log" || true; falla "el binario no creó .tylluan-token en ${TOKEN_TIMEOUT}s"; }
    MODO="$(stat -c '%a' "$TOKEN" 2>/dev/null || stat -f '%Lp' "$TOKEN")"
    echo "::notice::.tylluan-token creado con modo $MODO"
    [ "$MODO" = "600" ] || falla "el .tylluan-token quedó en modo $MODO (debe ser 600: D206)"
  fi
fi

echo "== GATE DE RELEASE OK: los tres gates de la auditoría pasan =="
