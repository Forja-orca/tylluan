#!/usr/bin/env bash
# scripts/check_build_lock_pare.sh — la ventana de PARE manda sobre el cerrojo,
# y el cerrojo entiende a su dueno aunque lo haya tomado a mano (owner.txt).
#
# T2977 (2026-11-06): mientras exista D:/tylluan-loops/PARE.flag, `build_lock.sh take`
# y `force` tienen que RECHAZAR (exit 5). Si no, la automatización de cualquier agente
# puede compilar en mitad del reinicio de producción — que es justo lo que pasó hoy
# (el TL compiló sin ventana declarada) y lo que esta bandera existe para impedir.
#
# Se prueban las cuatro esquinas:
#   1) bandera puesta  -> take  = exit 5 y NO crea el cerrojo
#   2) bandera puesta  -> force = exit 5 (si no, la ventana se salta con force)
#   3) bandera puesta  -> release SIGUE funcionando (liberar nunca se bloquea)
#   4) bandera quitada -> take = exit 0 y release = exit 0 (no se rompe lo de siempre)
#   5) bandera puesta  -> status lo dice
#
# Uso: bash scripts/check_build_lock_pare.sh          (exit 0 = todo bien)
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/build_lock.sh"
[ -f "$SCRIPT" ] || { echo "FALLO: no encuentro $SCRIPT"; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
export TYLLUAN_BUILD_LOCK="$TMP/lock"
export TYLLUAN_PARE_FLAG="$TMP/PARE.flag"

fallos=0
ok()   { echo "  ok: $*"; }
mal()  { echo "  FALLO: $*"; fallos=$((fallos + 1)); }

echo "1) bandera puesta -> take rechaza (exit 5) y no crea cerrojo"
: > "$TYLLUAN_PARE_FLAG"
out="$(bash "$SCRIPT" take t1 2>&1)"; rc=$?
[ "$rc" -eq 5 ] && ok "exit 5" || mal "exit=$rc (esperado 5): $out"
[ ! -d "$TYLLUAN_BUILD_LOCK" ] && ok "no se creo el cerrojo" || mal "creo el cerrojo con PARE activo"
echo "$out" | grep -qi "PARE ACTIVE" && ok "mensaje claro" || mal "mensaje poco claro: $out"

echo "2) bandera puesta -> force rechaza (exit 5)"
out="$(bash "$SCRIPT" force t2 --reason "prueba" 2>&1)"; rc=$?
[ "$rc" -eq 5 ] && ok "exit 5" || mal "exit=$rc (esperado 5): $out"
[ ! -d "$TYLLUAN_BUILD_LOCK" ] && ok "no se creo el cerrojo" || mal "force creo el cerrojo con PARE activo"

echo "3) bandera puesta -> release NO se bloquea"
out="$(bash "$SCRIPT" release t1 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && ok "release sigue libre" || mal "release exit=$rc: $out"

echo "4) bandera quitada -> take y release funcionan como siempre"
rm -f "$TYLLUAN_PARE_FLAG"
out="$(bash "$SCRIPT" take t3 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && ok "take exit 0" || mal "take exit=$rc: $out"
[ -d "$TYLLUAN_BUILD_LOCK" ] && ok "cerrojo creado" || mal "no se creo el cerrojo"
[ -f "$TYLLUAN_BUILD_LOCK/owner" ] && ok "owner escrito" || mal "sin owner"
out="$(bash "$SCRIPT" release t3 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && ok "release exit 0" || mal "release exit=$rc: $out"
[ ! -d "$TYLLUAN_BUILD_LOCK" ] && ok "cerrojo liberado" || mal "el cerrojo siguio ahi"

echo "5) bandera puesta -> status lo anuncia"
: > "$TYLLUAN_PARE_FLAG"
out="$(bash "$SCRIPT" status 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && ok "status exit 0" || mal "status exit=$rc"
echo "$out" | grep -q "PARE: ACTIVE" && ok "status lo dice" || mal "status no menciona el PARE: $out"

rm -f "$TYLLUAN_PARE_FLAG"

# ── v2 (2026-11-07): los cerrojos ad-hoc con owner.txt ───────────────────────────
# Medido antes del arreglo: `status` decia "no owner file (legacy)" con exit 1 y
# `release <dueno real>` respondia "REFUSED: lock owner is []" con exit 3, o sea que
# su propio dueno no podia soltarlo. Paso de verdad el 2026-11-06 a las 19:33.

echo "6) cerrojo ad-hoc con owner.txt -> status ve al dueno y sale con 0"
# El log hay que borrarlo de verdad: los casos 1-5 ya escribieron en el, y si existe
# el `|| true` del status no se ejercita (mutacion que pasaba verde sin esto).
rm -f "$TYLLUAN_BUILD_LOCK.log"
rm -rf "$TYLLUAN_BUILD_LOCK"; mkdir -p "$TYLLUAN_BUILD_LOCK"
printf '%s %s %s\n' "dsh-adhoc" "4242" "2026-11-06T19:33:00Z" > "$TYLLUAN_BUILD_LOCK/owner.txt"
out="$(bash "$SCRIPT" status 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && ok "status exit 0 (sin fichero de log)" || mal "status exit=$rc con owner.txt"
echo "$out" | grep -q "dsh-adhoc" && ok "status muestra el dueno real" || mal "status no ve el dueno: $out"

echo "7) el dueno real puede soltarlo con release"
out="$(bash "$SCRIPT" release dsh-adhoc 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && ok "release exit 0" || mal "release exit=$rc: $out"
[ ! -d "$TYLLUAN_BUILD_LOCK" ] && ok "cerrojo liberado" || mal "sigue ahi: $(ls "$TYLLUAN_BUILD_LOCK")"

echo "8) con owner Y owner.txt gana owner (y se avisa por stderr)"
rm -rf "$TYLLUAN_BUILD_LOCK"; mkdir -p "$TYLLUAN_BUILD_LOCK"
printf '%s %s %s\n' "el-bueno" "1" "2026-11-07T00:00:00Z" > "$TYLLUAN_BUILD_LOCK/owner"
printf '%s %s %s\n' "el-viejo" "2" "2026-11-06T19:33:00Z" > "$TYLLUAN_BUILD_LOCK/owner.txt"
out="$(bash "$SCRIPT" status 2>&1)"
echo "$out" | grep -q "el-bueno" && ok "status usa owner" || mal "status no usa owner: $out"
out="$(bash "$SCRIPT" release el-viejo 2>&1)"; rc=$?
[ "$rc" -eq 3 ] && ok "release del viejo se niega (exit 3)" || mal "release el-viejo exit=$rc: $out"
out="$(bash "$SCRIPT" release el-bueno 2>&1)"; rc=$?
[ "$rc" -eq 0 ] && ok "release del bueno funciona" || mal "release el-bueno exit=$rc: $out"
[ ! -d "$TYLLUAN_BUILD_LOCK" ] && ok "y limpia los dos nombres" || mal "quedo algo: $(ls "$TYLLUAN_BUILD_LOCK")"

echo ""
if [ "$fallos" -eq 0 ]; then
    echo "OK: el cerrojo respeta la ventana de PARE y entiende los ad-hoc (8/8 casos)"
    exit 0
fi
echo "FALLO: $fallos caso(s) mal"
exit 1
