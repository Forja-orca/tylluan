#!/usr/bin/env bash
# scripts/check_bom.sh -- mechanical gate against UTF-8 BOM infection.
#
# Why this exists (2026-09-15): the BOM bug class broke this project four
# separate times -- a0a614a, 8c7d0bf (TOML configs breaking Tauri CI),
# ad96253 (check_head_sync.sh shebang), 336ed13 (check_docs_reality.sh
# shebang). Every fix cured only the files that failed that day; the scan
# at 2e257d8 found 130 tracked files still carrying a BOM, including
# executable sh scripts (the shebang becomes EF BB BF followed by '#!' --
# exec dies with "bad interpreter"), doc files that byte-0 gates parse
# (STATUS.md, README.md), and ~60 Rust sources.
#
# What it does:
#   check_bom.sh          list tracked files starting with the UTF-8 BOM
#                         (EF BB BF); exit 1 if any found (blocking gate in
#                         verify.sh --docs), 0 if clean, 2 on internal error.
#   check_bom.sh --fix    strip the 3-byte BOM from every offender
#                         (byte-exact: bytes 4..end untouched, no mode
#                         changes -- git file modes are metadata, content
#                         edits cannot alter them).
#
# Scope notes:
#   - *.ps1 files are EXCLUDED: PowerShell 5.1 reads BOM-less UTF-8 as
#     ANSI, so with non-ASCII content a BOM there is CORRECT (install.ps1
#     has one such line). Revisit only if the encoding story changes.
#   - UTF-16 files (FF FE / FE FF) are out of scope: different encodings,
#     different BOMs; scan at 2e257d8 found zero tracked.
#   - Untracked files are out of scope: the gate checks what git ships.
#
# Performance: the detection loop is pure bash builtins (one 3-byte `read
# -N` per file, zero subprocess spawns) -- a full 900+ file scan runs in
# about a second even on Windows, where per-file spawns cost ~100ms and
# made naive implementations time out.

set -uo pipefail
cd "$(dirname "$0")/.."

FIX=0
case "${1:-}" in
    "") ;;
    --fix) FIX=1 ;;
    *) echo "usage: check_bom.sh [--fix]"; exit 2 ;;
esac

BOM=$'\xef\xbb\xbf'
bad=0
fixed=0
errors=0

while IFS= read -r -d '' f; do
    case "$f" in
        *.ps1) continue ;;   # PS 5.1 legitimately needs BOM (see header)
    esac
    [ -f "$f" ] || continue
    sig=''
    LC_ALL=C read -r -N 3 sig < "$f" 2>/dev/null || true
    if [ "$sig" = "$BOM" ]; then
        if [ "$FIX" = "1" ]; then
            if tail -c +4 "$f" > "$f.bom-tmp" 2>/dev/null && mv "$f.bom-tmp" "$f"; then
                echo "stripped: $f"
                fixed=$((fixed + 1))
            else
                echo "error stripping: $f"
                rm -f "$f.bom-tmp"
                errors=$((errors + 1))
            fi
        else
            echo "BOM: $f"
            bad=$((bad + 1))
        fi
    fi
done < <(git ls-files -z)

if [ "$FIX" = "1" ]; then
    if [ "$errors" -gt 0 ]; then
        echo "❌ $errors file(s) failed to strip."
        exit 1
    fi
    echo "✅ stripped $fixed file(s). Re-run without --fix to verify."
    exit 0
fi

if [ "$bad" -gt 0 ]; then
    echo "❌ $bad tracked file(s) start with a UTF-8 BOM. Shebang scripts"
    echo "   break with 'bad interpreter', byte-0 parsers misread, and CI"
    echo "   burns a round-trip on every recurrence."
    echo "   Fix: scripts/check_bom.sh --fix   (removes exactly 3 bytes/file)"
    exit 1
fi
echo "✅ no UTF-8 BOM in tracked files"
exit 0
