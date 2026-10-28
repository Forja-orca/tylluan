#!/usr/bin/env bash
# check_read_routes.sh -- gate de las rutas de lectura agenticas (2026-10-28, pedido de Jose).
#
# Invariantes que vigila:
#   1. Todos los ficheros agenticos de la lista llevan el MISMO bloque entre los
#      marcadores RUTAS-DE-LECTURA:BEGIN/END (REVIEW.md es la fuente de verdad).
#   2. En ese bloque, REVIEW.md ocupa el puesto 3 de la tabla.
#   3. Cada ruta entre backticks del bloque existe en el repo.
#   4. CLAUDE.md importa REVIEW.md con una linea "@REVIEW.md" (REVIEW.md solo lo
#      lee el servicio Code Review de GitHub; /code-review local lee CLAUDE.md).
# Solo lectura. Salida 0 = todo bien.
set -u
cd "$(dirname "$0")/.." || exit 2

FILES="REVIEW.md AGENTS.md CLAUDE.md .agents/AGENTS.md STATUS.md docs/internal/WORK_PROTOCOL.md CONTRIBUTING.md"
BEGIN='RUTAS-DE-LECTURA:BEGIN'
END='RUTAS-DE-LECTURA:END'
fail=0

block() { tr -d '\r' < "$1" | awk -v b="$BEGIN" -v e="$END" 'index($0,b){f=1} f{print} index($0,e){f=0}'; }

ref="$(block REVIEW.md)"
if [ -z "$ref" ]; then echo "❌ REVIEW.md no tiene el bloque $BEGIN"; exit 1; fi

for f in $FILES; do
    if [ ! -f "$f" ]; then echo "❌ falta $f"; fail=1; continue; fi
    if [ "$(block "$f")" != "$ref" ]; then
        echo "❌ $f: el bloque de rutas de lectura difiere del de REVIEW.md"; fail=1
    fi
done

# 2) REVIEW.md en el puesto 3
third="$(printf '%s\n' "$ref" | grep -E '^\| *3 *\|' | head -1)"
case "$third" in
    *'`REVIEW.md`'*) ;;
    *) echo "❌ REVIEW.md no esta en el puesto 3 de la tabla"; fail=1 ;;
esac

# 3) rutas citadas existen
for p in $(printf '%s\n' "$ref" | grep -oE '`[A-Za-z0-9_./-]+\.(md|toml)`' | tr -d '`' | sort -u); do
    [ -e "$p" ] || { echo "❌ la ruta citada no existe: $p"; fail=1; }
done

# 4) CLAUDE.md importa REVIEW.md
if ! tr -d '\r' < CLAUDE.md | grep -qx '@REVIEW.md'; then
    echo "❌ CLAUDE.md no tiene la linea @REVIEW.md"; fail=1
fi

if [ "$fail" = 0 ]; then echo "✅ rutas de lectura coherentes en $(echo $FILES | wc -w) ficheros; REVIEW.md es el puesto 3"; fi
exit $fail
