#!/usr/bin/env bash
# Reports absolute filesystem paths hardcoded in Tylluan's own source --
# the class of bug that would silently break the portability initiative
# (2026-08-29, Regla 13 of verification-discipline: the gate exists before
# the feature it protects, not after someone ships something that only
# works from E:\tylluan on Jose's machine).
#
# Portability means: copy the whole install directory anywhere, on any of
# the 3 supported platforms, and it must still work. An absolute path baked
# into source code (a Windows drive letter, a Unix home dir, this repo's
# own checkout path) is exactly the kind of thing that works by accident on
# the machine it was written on and breaks silently the moment the folder
# moves -- which is the entire point of this initiative.
#
# Heuristic, not type-aware analysis (same trade-off as check_dead_config.sh):
#   - False negatives expected: a path built by string concatenation at
#     runtime (`format!("{}/data", base)`) is invisible to this grep and
#     is exactly the CORRECT pattern -- nothing to catch there.
#   - False positives to actively avoid: comments, docs, CLAUDE.md-style
#     example commands for a human to type, and test fixtures that
#     deliberately assert against a known temp path are not bugs. This is
#     why matches are printed with file:line for a human/agent to triage
#     (Regla 6), not silently trusted.
#
# Usage: scripts/no-absolute-paths.sh [--strict]
# Default: report-only, always exits 0.
# --strict: exit 1 if any un-allowlisted match is found (promote once the
#           baseline is triaged and clean -- same lifecycle as
#           check_dead_config.sh's --strict flag).

set -uo pipefail
cd "$(dirname "$0")/.."

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

# Source trees that ship to users and must be relocatable. Deliberately
# excludes docs/*.md, README.md, CLAUDE.md, STATUS.md (example commands for
# a human are not runtime paths) and scripts/ (developer tooling run from
# a fixed checkout, not part of the portable bundle).
TARGETS=(
    "crates/tylluan-kernel/src"
    "crates/tylluan-link/src"
    "crates/tylluan-cli/src"
    "guilds"
    "tylluan.example.toml"
)

# Patterns: Windows drive-letter absolute paths, Unix home/root dirs, and
# this project's own known checkout paths (would match on Jose's or any
# contributor's machine equally -- the point isn't "not E:\tylluan
# specifically", it's "not ANY machine-specific absolute path").
PATTERN='[A-Za-z]:[\\/](Users|tylluan)|/home/[A-Za-z0-9_.-]+|/Users/[A-Za-z0-9_.-]+|/root/[A-Za-z0-9_.-]+'

# Lines allowlisted as false positives, triaged by a human/agent -- add
# "file:line: reason" here when a real match is confirmed non-portable-risk
# (e.g. a test fixture that legitimately asserts against its own temp dir).
# Format matches check_dead_config.sh's exception convention: documented
# reason, not a bare path, so the NEXT false positive of the same shape
# doesn't need its own new entry.
ALLOWLIST_PATTERN='ALLOW-ABSOLUTE-PATH'

echo "── no-absolute-paths: scanning for machine-specific paths in portable source ──"

FOUND=0
for target in "${TARGETS[@]}"; do
    [ -e "$target" ] || continue
    while IFS= read -r match; do
        [ -z "$match" ] && continue
        file="${match%%:*}"
        # Skip if the matched line itself carries the allowlist marker.
        line_content="${match#*:*:}"
        if echo "$line_content" | grep -q "$ALLOWLIST_PATTERN"; then
            continue
        fi
        echo "$match"
        FOUND=1
    done < <(grep -rnE "$PATTERN" "$target" \
        --include="*.rs" --include="*.py" --include="*.toml" 2>/dev/null)
done

echo
if [ "$FOUND" -eq 0 ]; then
    echo "✅ No hardcoded machine-specific absolute paths found in portable source."
    exit 0
else
    echo "⚠️  Found path(s) above. Triage each: real portability risk, or false"
    echo "   positive (mark the line with a trailing '# ALLOW-ABSOLUTE-PATH: <why>'"
    echo "   comment, or '// ALLOW-ABSOLUTE-PATH: <why>' in Rust)."
    if [ "$STRICT" -eq 1 ]; then
        exit 1
    fi
    exit 0
fi
