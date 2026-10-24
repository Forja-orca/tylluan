#!/usr/bin/env bash
# T-DSH-60 — Comprobación posterior al reinicio del kernel (SOLO LECTURA).
#
# Hace GET /health y verifica, una línea por comprobación con OK/FALLO:
#   1. commit   : el `commit` de /health == `git rev-parse --short origin/main`
#   2. status   : pasa de `starting` a `ok` dentro de --timeout segundos
#   3. repo_map : el campo `repo_map` de /health (o components.repo_map.status) llega a `ok`
# Sale 0 si las tres están OK, 1 si alguna falla.
#
# Puerto: --port N, o [nexus].port de tylluan.toml (por defecto la raíz del repo;
# --config para indicar otro fichero). NO hay ningún puerto fijado en el script.
#
# Uso:
#   bash scripts/verify_restart.sh [--port N] [--timeout SEG] [--interval SEG]
#                                  [--host H] [--config ruta/tylluan.toml]
#   Variables equivalentes: VERIFY_RESTART_TIMEOUT, VERIFY_RESTART_INTERVAL
#
# No arranca ni para kernels y no escribe nada: solo curl (lectura).
set -uo pipefail

HOST="127.0.0.1"
PORT=""
TIMEOUT="${VERIFY_RESTART_TIMEOUT:-300}"
INTERVAL="${VERIFY_RESTART_INTERVAL:-2}"
CONFIG=""

usage() {
    sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --port)     PORT="${2:-}"; shift 2 ;;
        --timeout)  TIMEOUT="${2:-}"; shift 2 ;;
        --interval) INTERVAL="${2:-}"; shift 2 ;;
        --host)     HOST="${2:-}"; shift 2 ;;
        --config)   CONFIG="${2:-}"; shift 2 ;;
        -h|--help)  usage; exit 0 ;;
        *) echo "argumento desconocido: $1 (usa --help)" >&2; exit 2 ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ─── Puerto: --port o [nexus].port del tylluan.toml ─────────────────────────
if [ -z "$CONFIG" ]; then
    if [ -f "$REPO_ROOT/tylluan.toml" ]; then
        CONFIG="$REPO_ROOT/tylluan.toml"
    elif [ -f "./tylluan.toml" ]; then
        CONFIG="./tylluan.toml"
    fi
fi

extract_port() { # $1 = config; BOM/CRLF robustos (ficheros editados en Windows)
    local clean
    clean="$(sed '1s/^\xEF\xBB\xBF//' "$1" | tr -d '\r')"
    printf '%s\n' "$clean" | awk '
        /^[[:space:]]*\[nexus\][[:space:]]*$/ { in_nexus = 1; next }
        /^[[:space:]]*\[/                    { in_nexus = 0 }
        in_nexus && $1 == "port"             { gsub(/[^0-9]/, "", $3); print $3; exit }
    '
}

if [ -z "$PORT" ] && [ -n "$CONFIG" ]; then
    PORT="$(extract_port "$CONFIG")"
fi
if [ -z "$PORT" ]; then
    echo "FALLO puerto: no hay --port ni [nexus].port en ${CONFIG:-tylluan.toml}" >&2
    exit 1
fi

BASE_URL="http://$HOST:$PORT"
EXPECTED_COMMIT="$(git -C "$REPO_ROOT" rev-parse --short origin/main 2>/dev/null || true)"
[ -n "$EXPECTED_COMMIT" ] || EXPECTED_COMMIT="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || true)"

json_field() { # $1 = json, $2 = ruta punteada simple
    local json="$1" path="$2"
    if command -v python >/dev/null 2>&1; then
        printf '%s' "$json" | python -c '
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    print("")
    sys.exit(0)
out = data
for key in sys.argv[1].split("."):
    out = out.get(key, "") if isinstance(out, dict) else ""
print(out if isinstance(out, str) else ("" if out is None else out))
' "$path" 2>/dev/null
    elif command -v python3 >/dev/null 2>&1; then
        printf '%s' "$json" | python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    print("")
    sys.exit(0)
out = data
for key in sys.argv[1].split("."):
    out = out.get(key, "") if isinstance(out, dict) else ""
print(out if isinstance(out, str) else ("" if out is None else out))
' "$path" 2>/dev/null
    else
        # Fallback sin Python: suficiente para status/commit y repo_map plano.
        case "$path" in
            commit) printf '%s' "$json" | tr -d '\r\n' | grep -o '"commit"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' ;;
            status) printf '%s' "$json" | tr -d '\r\n' | grep -o '"status"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' ;;
            components.repo_map.status)
                    printf '%s' "$json" | tr -d '\r\n' | grep -o '"repo_map"[^}]*}' | grep -o '"status"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/' ;;
            *) echo "" ;;
        esac
    fi
}

START_TS=$(date +%s)
SEEN_WARMING=""
SEEN_STARTING=""
STARTING_AT=""
FINAL_JSON=""
OK_AT=""
REPO_OK_AT=""

while :; do
    now=$(date +%s)
    elapsed=$(( now - START_TS ))
    if [ "$elapsed" -ge "$TIMEOUT" ]; then
        break
    fi
    body="$(curl -sS --max-time 5 "$BASE_URL/health" 2>/dev/null || true)"
    if [ -z "$body" ]; then
        printf '... esperando a que %s responda (%ss)\n' "$BASE_URL" "$elapsed" >&2
        sleep "$INTERVAL"
        continue
    fi
    FINAL_JSON="$body"
    status="$(json_field "$body" status)"
    repo_status="$(json_field "$body" repo_map)"
    [ -n "$repo_status" ] || repo_status="$(json_field "$body" components.repo_map.status)"
    case "$status" in
        warming_up) [ -n "$SEEN_WARMING" ] || SEEN_WARMING="$elapsed" ;;
        starting)   [ -n "$SEEN_STARTING" ] || STARTING_AT="$elapsed"; SEEN_STARTING=1 ;;
        ok)         [ -n "$OK_AT" ] || OK_AT="$elapsed" ;;
    esac
    if [ -n "$OK_AT" ] && [ "$repo_status" = "ok" ]; then
        [ -n "$REPO_OK_AT" ] || REPO_OK_AT="$elapsed"
        break
    fi
    printf '... status=%s repo_map=%s (%ss)\n' "${status:-?}" "${repo_status:-?}" "$elapsed" >&2
    sleep "$INTERVAL"
done

TOTAL_ELAPSED=$(( $(date +%s) - START_TS ))
FAILS=0
COMMIT_SEEN="$(json_field "$FINAL_JSON" commit)"

# ─── 1. commit ───────────────────────────────────────────────────────────────
if [ -z "$EXPECTED_COMMIT" ]; then
    printf 'FALLO commit    no pude leer origin/main (¿repo sin fetch?) — /health.commit=%s\n' "${COMMIT_SEEN:-?}"
    FAILS=$((FAILS + 1))
elif [ -n "$COMMIT_SEEN" ] && [ "${COMMIT_SEEN#"$EXPECTED_COMMIT"}" != "$COMMIT_SEEN" ]; then
    printf 'OK    commit    /health.commit=%s == origin/main %s\n' "$COMMIT_SEEN" "$EXPECTED_COMMIT"
else
    printf 'FALLO commit    /health.commit=%s != origin/main %s\n' "${COMMIT_SEEN:-?}" "$EXPECTED_COMMIT"
    FAILS=$((FAILS + 1))
fi

# ─── 2. status starting -> ok ────────────────────────────────────────────────
if [ -n "$OK_AT" ]; then
    if [ -n "$SEEN_STARTING" ]; then
        printf 'OK    status    starting@%ss -> ok@%ss (timeout %ss)\n' "$STARTING_AT" "$OK_AT" "$TIMEOUT"
    else
        printf 'OK    status    ok@%ss (no se observó starting: el kernel ya estaba ok)\n' "$OK_AT"
    fi
else
    printf 'FALLO status    no llegó a ok en %ss (último: %s)\n' "$TIMEOUT" "${FINAL_JSON:+$(json_field "$FINAL_JSON" status)}"
    FAILS=$((FAILS + 1))
fi

# ─── 3. repo_map ok ──────────────────────────────────────────────────────────
REPO_FINAL="$(json_field "$FINAL_JSON" repo_map)"
[ -n "$REPO_FINAL" ] || REPO_FINAL="$(json_field "$FINAL_JSON" components.repo_map.status)"
if [ "$REPO_FINAL" = "ok" ]; then
    printf 'OK    repo_map  repo_map=ok@%ss (total %ss)\n' "${REPO_OK_AT:-$TOTAL_ELAPSED}" "$TOTAL_ELAPSED"
else
    printf 'FALLO repo_map  repo_map=%s tras %ss\n' "${REPO_FINAL:-?}" "$TOTAL_ELAPSED"
    FAILS=$((FAILS + 1))
fi

[ "$FAILS" -eq 0 ] && exit 0 || exit 1
