#!/usr/bin/env bash
# scripts/check_build_inputs.sh — every include_str!/include_bytes! that leaves
# the crate directory must resolve to a file that (a) is tracked by git, (b) is
# not excluded from the Docker build context, and (c) for production (non-test)
# includes, is actually copied by the Dockerfile.
#
# Origin (TL T2673, T-HER-NEW-1): push 266's Docker Smoke went red because the
# Dockerfile never copied assets/ and include_str!("../../../../assets/
# routing_anchors_seed.jsonl") found nothing at build time. Nothing in the
# local pre-push caught it because verify.sh never builds the image — this gate
# closes that mechanism gap statically.
#
# Semantics:
#  - In-crate includes are skipped: COPY crates/ ./crates/ carries the whole
#    crate, and a missing in-crate file breaks compilation anyway.
#  - An include is TEST-ONLY when its source file lives under crates/*/tests/
#    (integration test targets) or the include sits right below a
#    #[cfg(test)] attribute (heuristic: attribute within 60 lines above).
#    Test-only includes need git tracking + dockerignore presence but NOT a
#    COPY line: the image build never compiles tests.
#  - dockerignore check uses a throwaway empty git repo with
#    core.excludesFile=<dockerignore> — gitignore and .dockerignore share the
#    last-match-wins glob syntax, and the empty repo isolates the result from
#    the real repo's own .gitignore. Documented approximation, not git's exact
#    Docker matcher; sufficient for the failure class this gate guards.
#
# Usage: check_build_inputs.sh [--repo-root P] [--dockerfile P] [--dockerignore P]
# Exit 0 = all inputs consistent; exit 1 with one red line per violation.
set -uo pipefail

REPO="${BASH_SOURCE[0]%/*}"; REPO="$(cd "$REPO/.." && pwd)"
DOCKERFILE=""
DOCKERIGNORE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --repo-root)    REPO="$2"; shift 2 ;;
        --dockerfile)   DOCKERFILE="$2"; shift 2 ;;
        --dockerignore) DOCKERIGNORE="$2"; shift 2 ;;
        *) echo "usage: $0 [--repo-root P] [--dockerfile P] [--dockerignore P]" >&2; exit 2 ;;
    esac
done
[ -n "$DOCKERFILE" ] || DOCKERFILE="$REPO/Dockerfile"
[ -n "$DOCKERIGNORE" ] || DOCKERIGNORE="$REPO/.dockerignore"
[ -f "$DOCKERFILE" ] || { echo "❌ Dockerfile not found: $DOCKERFILE" >&2; exit 2; }

CYG=""
command -v cygpath >/dev/null 2>&1 && CYG=1

# ── 1) harvest escapes: file:line:relpath ────────────────────────────────
HITS="$(grep -rn --include='*.rs' -oE 'include_(str|bytes)!\("[^"]+"\)' "$REPO/crates" | awk -F: '{p=$3; sub(/include_(str|bytes)!\("/,"",p); gsub(/[")]/,"",p); print $1":"$2":"p}')"
[ -n "$HITS" ] || { echo "check_build_inputs: no include_str!/include_bytes! found at all — scanner broken?"; exit 2; }

# ── 2) dockerignore matcher (isolated temp repo) ─────────────────────────
TMPR="$(mktemp -d)"
TMPR_G="$TMPR"
[ -n "$CYG" ] && TMPR_G="$(cygpath -m "$TMPR")"
git -C "$TMPR_G" init -q
di="$DOCKERIGNORE"
[ -n "$CYG" ] && di="$(cygpath -m "$di")"
ISIGNORED() { # $1 = repo-relative path
    printf '%s\n' "$1" | git -C "$TMPR_G" -c "core.excludesFile=$di" check-ignore --stdin -q >/dev/null 2>&1
}

# ── 3) Dockerfile COPY coverage ──────────────────────────────────────────
# host-context COPYs (skip --from= stage copies and multi-stage internals)
SRCS="$(grep -E '^[[:space:]]*COPY[[:space:]]' "$DOCKERFILE" | tr -d '\r' | grep -v -- '--from=' | sed 's/^[[:space:]]*COPY[[:space:]]*//;s/ --chown=[^ ]*//' | awk '{NF--; print}' | tr ' ' '\n')"
COVERED() { # $1 = repo-relative path ; covered if some COPY src equals it or is an ancestor dir
    local s
    while IFS= read -r s; do
        [ -z "$s" ] && continue
        s="${s#./}"; s="${s%/}"
        if [ "$s" = "$1" ] || [ "${1#"$s"/}" != "$1" ]; then return 0; fi
    done <<< "$SRCS"
    return 1
}

# ── 4) judge every escaping include ──────────────────────────────────────
VIOL=0; CHECKED=0
REPO_ABS="$(cd "$REPO" && pwd)"
# native git (and any native tool) needs forward-slash native paths on this host:
# /d/... MSYS paths make `git -C` fail SILENTLY (caught live: every file looked
# "NOT TRACKED"). cygpath -m gives D:/... which bash and git both resolve.
REPO_G="$REPO_ABS"
[ -n "$CYG" ] && REPO_G="$(cygpath -m "$REPO_ABS")"
while IFS= read -r hit; do
    src="${hit%%:*}"; rest="${hit#*:}"; line="${rest%%:*}"; rel="${rest#*:}"
    case "$rel" in ../*) ;; *) continue ;; esac   # only includes leaving their own dir
    crate_dir="${src%%/src/*}"; crate_dir="${crate_dir%%/tests/*}"
    abs_dir="$(cd "$(dirname "$src")" 2>/dev/null && pwd)" || continue
    resolved="$(cd "$(dirname "$src")" && realpath -ms -- "$rel" 2>/dev/null)" || resolved=""
    [ -n "$resolved" ] || { echo "❌ UNRESOLVABLE include: $src:$line -> $rel"; VIOL=1; continue; }
    case "$resolved" in
        "$crate_dir"/*) continue ;;              # stays inside the crate
        "$REPO_ABS"/*) : ;;                       # escapes crate but inside repo
        *) echo "❌ OUTSIDE-REPO include: $src:$line -> $resolved"; VIOL=1; continue ;;
    esac
    rp="${resolved#$REPO_ABS/}"
    CHECKED=$((CHECKED+1))
    # test-only classification
    testonly=0
    case "$src" in */crates/*/tests/*) testonly=1 ;; esac
    if [ "$testonly" -eq 0 ]; then
        lo=$(( line - 60 )); [ $lo -lt 1 ] && lo=1
        if sed -n "${lo},${line}p" "$src" | grep -q '#\[cfg(test)\]'; then testonly=1; fi
    fi
    # (a) tracked
    if ! git -C "$REPO_G" ls-files --error-unmatch -- "$rp" >/dev/null 2>&1; then
        echo "❌ NOT TRACKED by git: $rp  (included by $src:$line)"; VIOL=1
    fi
    # (b) dockerignore
    if ISIGNORED "$rp"; then
        echo "❌ EXCLUDED by .dockerignore: $rp  (included by $src:$line)"; VIOL=1
    fi
    # (c) Dockerfile COPY (production only)
    if [ "$testonly" -eq 0 ]; then
        if ! COVERED "$rp"; then
            echo "❌ NOT COPIED by Dockerfile: $rp  (production include at $src:$line)"; VIOL=1
        fi
    fi
done <<< "$HITS"

if [ "$VIOL" -eq 0 ]; then
    echo "✅ check_build_inputs: $CHECKED escaping include(s) consistent (tracked + docker context + Dockerfile COPY where production)."
    exit 0
fi
echo "❌ check_build_inputs: inconsistencies above — a Docker build or clean install will miss these files."
exit 1
