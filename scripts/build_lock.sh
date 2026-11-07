#!/usr/bin/env bash
# scripts/build_lock.sh — build-lock (cerrojo) WITH OWNER + audit trail.
# Origin (TL T2673, T-HER-NEW-1 point 1): the lock D:/tylluan-loops/tl_build.lock
# kept vanishing or getting misattributed (T2688 blamed @hermes for a lock whose
# tree was clean; the TL found it had no owner.txt at all). The old protocol was
# bare mkdir/rmdir by each agent's ad-hoc scripts — whoever rmdir'd blindly could
# delete a lock they did not own.
#
# Contract:
#   take <who>     mkdir the lock; write <lock>/owner ("<who> <pid> <ISO>");
#                  append "TAKE" to <lock>.log. Contention -> prints current
#                  owner+since, exit 1. With --wait N: retry every 20 s up to
#                  N seconds before giving up.
#   release <who>  reads owner; rmdir ONLY if the owner name matches <who>;
#                  otherwise REFUSES (exit 3) and leaves the lock intact.
#                  Appends "RELEASE" to the log.
#   status         free | taken (owner, since, log tail).
#
# A stale lock (holder crashed) is released with: force <who-new> --reason "..."
# which logs FORCE-TAKE with the previous owner preserved in the audit line.
#
# PARE window (T2977, 2026-11-06): while D:/tylluan-loops/PARE.flag exists, `take`
# and `force` are REFUSED with exit 5. The flag is created and removed by the TL;
# the point is that no agent's automation can start a build during the production
# restart window even if it wakes up on a schedule. `release` and `status` always
# work (freeing a lock must never be blocked).
set -uo pipefail
LOCK="${TYLLUAN_BUILD_LOCK:-D:/tylluan-loops/tl_build.lock}"
LOGF="${LOCK}.log"
PAREF="${TYLLUAN_PARE_FLAG:-D:/tylluan-loops/PARE.flag}"
now() { date -u +%Y-%m-%dT%H:%M:%SZ; }
log() { printf '%s %s\n' "$(now)" "$*" >> "$LOGF"; }
# T2977: el PARE manda sobre cualquier automatizacion. Exit 5 = ventana de PARE.
# El dueno del cerrojo. Convencion: `owner`. Compatibilidad: los cerrojos tomados a
# mano con `owner.txt` (paso de verdad el 2026-11-06 a las 19:33) tambien se leen, para
# que su dueno real pueda soltarlos y para que `status` no los llame "legacy" a ciegas.
# Si existen los dos, gana `owner` y se avisa por stderr.
owner_of() {
    if [ -f "$LOCK/owner" ]; then
        if [ -f "$LOCK/owner.txt" ]; then
            echo "AVISO: el cerrojo tiene 'owner' y 'owner.txt'; se usa 'owner'" >&2
        fi
        cat "$LOCK/owner" 2>/dev/null
        return 0
    fi
    if [ -f "$LOCK/owner.txt" ]; then
        cat "$LOCK/owner.txt" 2>/dev/null
        return 0
    fi
    return 1
}
pare_active() { [ -e "$PAREF" ]; }
refuse_if_pare() {
    if pare_active; then
        echo "PARE ACTIVE: existe $PAREF — no se toma el cerrojo." >&2
        echo "  Ventana de reinicio: no se compila. Protocolo:" >&2
        echo "  D:/tylluan-loops/PROTOCOLO_PARE_2026-11-06.md" >&2
        echo "  El fichero lo crea y lo retira el TL; NO lo borres tu." >&2
        exit 5
    fi
}

cmd="${1:-status}"; who="${2:-}"
if [ "$cmd" != "status" ] && [ -z "$who" ]; then
    echo "usage: build_lock.sh {take|release|status|force} <who> [--wait N] [--reason '...']" >&2
    echo "  exit 5 = ventana de PARE activa ($PAREF): no se toma el cerrojo" >&2
    exit 2
fi

case "$cmd" in
  take)
    refuse_if_pare
    wait_s=0
    if [ "${3:-}" = "--wait" ]; then wait_s="${4:-0}"; fi
    deadline=$(( $(date +%s) + wait_s ))
    while :; do
        if mkdir "$LOCK" 2>/dev/null; then
            printf '%s %s %s\n' "$who" "$$" "$(now)" > "$LOCK/owner"
            log "TAKE $who pid=$$"
            echo "lock taken by $who ($$) $(now)"
            exit 0
        fi
        own="$(owner_of || echo 'owner-unknown (legacy bare lock)')"
        if [ "$(date +%s)" -ge "$deadline" ]; then
            echo "LOCKED: taken by [$own]"
            exit 1
        fi
        sleep 20
    done
    ;;
  release)
    [ -d "$LOCK" ] || { echo "lock was already free"; exit 0; }
    own="$(owner_of 2>/dev/null | awk '{print $1}' || true)"
    if [ "$own" != "$who" ]; then
        echo "REFUSED: lock owner is [$own], not [$who] — not touching it." >&2
        exit 3
    fi
    log "RELEASE $who pid=$$ (was owner: $(owner_of 2>/dev/null))"
    rm -f "$LOCK/owner" "$LOCK/owner.txt"
    rmdir "$LOCK" 2>/dev/null || { echo "WARN: rmdir failed (dir not empty?)"; exit 4; }
    echo "lock released by $who"
    ;;
  force)
    refuse_if_pare
    reason="unspecified"; if [ "${3:-}" = "--reason" ]; then reason="${4:-}"; fi
    own="$(owner_of 2>/dev/null || echo 'none/legacy')"
    log "FORCE-TAKE $who pid=$$ prev_owner=[$own] reason=$reason"
    mkdir -p "$LOCK"; printf '%s %s %s\n' "$who" "$$" "$(now)" > "$LOCK/owner"
    echo "lock force-taken by $who (prev: $own)"
    ;;
  status)
    if pare_active; then echo "PARE: ACTIVE ($PAREF) — take/force rechazados"; fi
    if [ -d "$LOCK" ]; then
        echo "taken: $(owner_of 2>/dev/null || echo 'no owner file (legacy)')"
        # `|| true`: sin log, `status` tiene que seguir saliendo con 0 (un preflight
        # que mire el codigo de salida no puede leer "fallo" solo porque falta el log).
        [ -f "$LOGF" ] && tail -5 "$LOGF" || true
    else
        echo "free"
    fi
    ;;
  *) echo "unknown command: $cmd" >&2; exit 2 ;;
esac
