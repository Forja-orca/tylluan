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
set -uo pipefail
LOCK="${TYLLUAN_BUILD_LOCK:-D:/tylluan-loops/tl_build.lock}"
LOGF="${LOCK}.log"
now() { date -u +%Y-%m-%dT%H:%M:%SZ; }
log() { printf '%s %s\n' "$(now)" "$*" >> "$LOGF"; }

cmd="${1:-status}"; who="${2:-}"
if [ "$cmd" != "status" ] && [ -z "$who" ]; then
    echo "usage: build_lock.sh {take|release|status|force} <who> [--wait N] [--reason '...']" >&2; exit 2
fi

case "$cmd" in
  take)
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
        own="$(cat "$LOCK/owner" 2>/dev/null || echo 'owner-unknown (legacy bare lock)')"
        if [ "$(date +%s)" -ge "$deadline" ]; then
            echo "LOCKED: taken by [$own]"
            exit 1
        fi
        sleep 20
    done
    ;;
  release)
    [ -d "$LOCK" ] || { echo "lock was already free"; exit 0; }
    own="$(awk '{print $1}' "$LOCK/owner" 2>/dev/null || true)"
    if [ "$own" != "$who" ]; then
        echo "REFUSED: lock owner is [$own], not [$who] — not touching it." >&2
        exit 3
    fi
    log "RELEASE $who pid=$$ (was owner: $(cat "$LOCK/owner" 2>/dev/null))"
    rm -f "$LOCK/owner"
    rmdir "$LOCK" 2>/dev/null || { echo "WARN: rmdir failed (dir not empty?)"; exit 4; }
    echo "lock released by $who"
    ;;
  force)
    reason="unspecified"; if [ "${3:-}" = "--reason" ]; then reason="${4:-}"; fi
    own="$(cat "$LOCK/owner" 2>/dev/null || echo 'none/legacy')"
    log "FORCE-TAKE $who pid=$$ prev_owner=[$own] reason=$reason"
    mkdir -p "$LOCK"; printf '%s %s %s\n' "$who" "$$" "$(now)" > "$LOCK/owner"
    echo "lock force-taken by $who (prev: $own)"
    ;;
  status)
    if [ -d "$LOCK" ]; then
        echo "taken: $(cat "$LOCK/owner" 2>/dev/null || echo 'no owner file (legacy)')"
        [ -f "$LOGF" ] && tail -5 "$LOGF"
    else
        echo "free"
    fi
    ;;
  *) echo "unknown command: $cmd" >&2; exit 2 ;;
esac
