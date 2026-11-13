#!/usr/bin/env bash
# scripts/check_dashboard_dist_fresh.sh — Gate anti-regresión del bundle dashboard/dist (T4028).
#
# Origen (2026-11-13, incidente en producción): build_prod.sh copió un dashboard/dist viejo
# sellándolo con el commit HEAD en .built_from sin verificar que el bundle compilado correspondiera
# al árbol real de código de dashboard/ + packages/.
#
# Comprobaciones (todas obligatorias, exit 0 solo si pasan las 3):
#   1. Estructura mínima: existen <dist>/index.html y el directorio <dist>/assets/.
#   2. Árbol fuente (.src_tree): <dist>/.src_tree existe y coincide exactamente con
#      "$(git rev-parse HEAD:dashboard):$(git rev-parse HEAD:packages)".
#   3. Literal del código en el bundle: un literal presente en dashboard/src/ (por defecto
#      "header-health-status" de dashboard/src/App.tsx) aparece compilado dentro de <dist>/assets/.
#
# Uso:
#   bash scripts/check_dashboard_dist_fresh.sh [--repo-root R] [--dist-dir D] [--literal L]
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR=""
LITERAL="header-health-status"

while [ $# -gt 0 ]; do
    case "$1" in
        --repo-root) REPO_ROOT="${2:-}"; shift 2 ;;
        --dist-dir)  DIST_DIR="${2:-}"; shift 2 ;;
        --literal)   LITERAL="${2:-}"; shift 2 ;;
        -h|--help)
            sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "FALLO uso: argumento desconocido '$1' (usa --help)" >&2
            exit 2
            ;;
    esac
done

[ -n "$DIST_DIR" ] || DIST_DIR="$REPO_ROOT/dashboard/dist"

REPO_G="$REPO_ROOT"
if command -v cygpath >/dev/null 2>&1; then
    REPO_G="$(cygpath -m "$REPO_ROOT")"
fi

FAILS=0

# ─── 1. Estructura mínima del dist ────────────────────────────────────────────
if [ ! -f "$DIST_DIR/index.html" ] || [ ! -d "$DIST_DIR/assets" ]; then
    printf 'FALLO dist       no existe %s/index.html o %s/assets\n' "$DIST_DIR" "$DIST_DIR"
    exit 1
fi
printf 'OK    dist       %s/index.html y assets/ presentes\n' "$DIST_DIR"

# ─── 2. .src_tree == git rev-parse HEAD:dashboard:HEAD:packages ──────────────
DASH_TREE="$(git -C "$REPO_G" rev-parse HEAD:dashboard 2>/dev/null | tr -d '\r\n')"
PKG_TREE="$(git -C "$REPO_G" rev-parse HEAD:packages 2>/dev/null | tr -d '\r\n')"

if [ -z "$DASH_TREE" ] || [ -z "$PKG_TREE" ]; then
    printf 'FALLO git        no se pudo resolver HEAD:dashboard o HEAD:packages en %s\n' "$REPO_G"
    exit 1
fi

WANT="${DASH_TREE}:${PKG_TREE}"
GOT=""
if [ -f "$DIST_DIR/.src_tree" ]; then
    GOT="$(tr -d '\r\n' < "$DIST_DIR/.src_tree" 2>/dev/null || true)"
fi

if [ -z "$GOT" ] || [ "$GOT" != "$WANT" ]; then
    printf 'FALLO .src_tree  dist declara "%s" != HEAD "%s"\n' "${GOT:-<ausente>}" "$WANT"
    FAILS=$((FAILS + 1))
else
    printf 'OK    .src_tree  %s == HEAD:dashboard:HEAD:packages\n' "$GOT"
fi

# ─── 3. Literal del código fuente presente en dashboard/src y en dist/assets ─
if [ -z "$LITERAL" ]; then
    printf 'FALLO literal    el literal a comprobar no puede estar vacío\n'
    exit 2
fi

if ! grep -rqF -- "$LITERAL" "$REPO_ROOT/dashboard/src" 2>/dev/null; then
    printf 'FALLO literal    "%s" no existe en %s/dashboard/src (comprobación vacua rechazada)\n' "$LITERAL" "$REPO_ROOT"
    exit 1
fi

if grep -rqF -- "$LITERAL" "$DIST_DIR/assets" 2>/dev/null; then
    printf 'OK    literal    "%s" verificado en dashboard/src y en %s/assets\n' "$LITERAL" "$DIST_DIR"
else
    printf 'FALLO literal    "%s" (presente en dashboard/src) NO aparece en %s/assets (dist viejo o corrupto)\n' "$LITERAL" "$DIST_DIR"
    FAILS=$((FAILS + 1))
fi

[ "$FAILS" -eq 0 ] && exit 0 || exit 1
