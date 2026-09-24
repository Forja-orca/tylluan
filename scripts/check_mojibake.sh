#!/usr/bin/env bash
# scripts/check_mojibake.sh -- mechanical gate against mid-file UTF-8 mojibake.
#
# Why this exists (2026-09-24): distinct from the byte-0 BOM bug class
# (check_bom.sh) -- this is UTF-8 text that got double-encoded somewhere in
# the write path (typically PowerShell Set-Content or a similar tool reading
# UTF-8 bytes as Latin-1/cp1252 and re-saving), turning real characters like
# emoji and em-dashes into multi-byte garbage. Deep's ADR-015 Fase 2 commit (517b77d, caught and
# repaired by Claude Code before push) corrupted ~70 lines of config.rs this
# way -- the 4th recurrence of this general encoding-corruption pattern in
# this project (after 3 waves of the byte-0 BOM bug), the 2nd caused by Deep
# specifically. Deep's own root-cause fix afterward: never write files via a
# shell tool that re-encodes, always via an edit tool or explicit UTF-8
# bytes -- this gate is the mechanical backstop for that discipline, so a
# recurrence is caught before push instead of trusted on report alone.
#
# What it does:
#   check_mojibake.sh          list tracked text files containing common
#                              double-encoded UTF-8 byte sequences; exit 1 if
#                              any found (blocking gate in verify.sh --docs),
#                              0 if clean, 2 on internal error.
#
# No --fix mode: unlike the BOM gate (byte-exact 3-byte strip), mojibake
# repair requires knowing the ORIGINAL intended character per occurrence --
# blindly reversing the encoding is not always lossless (see Deep's
# ADR-015 Fase 2 repair: reconstructed by hand from the parent commit, not
# mechanically un-mojibake'd). This gate only detects; a human or agent
# fixes by restoring from a clean prior revision or retyping the characters.
#
# Scope notes:
#   - Binary files, .git/, target/, node_modules/, dist/, build/ excluded.
#   - Detection is a fixed set of the most common mojibake byte sequences
#     seen in this project's real incidents -- not an
#     exhaustive encoding-corruption detector. False negatives on encodings
#     this project hasn't hit yet are possible; false positives are the
#     bigger risk this gate guards against, so the pattern list stays
#     conservative (real garbage sequences only, not any Ã/Â occurrence).

set -uo pipefail
cd "$(dirname "$0")/.."

# Fixed set of real mojibake byte sequences seen in this project's incidents.
# Each is UTF-8 bytes that decode as visible mojibake when the source was
# actually valid UTF-8 misread as Latin-1/cp1252 and re-saved as UTF-8.
# ANSI-C \xNN escapes (not literal mojibake characters) so this file's own
# source text doesn't trip the gate it implements -- see incident this gate
# itself hit on first run, fixed the same day it was written.
PATTERN=$'\xc3\x83\xc2\xa2\xc3\xa2\xe2\x80\x9a\xc2\xac|\xc3\x83\xc2\xa2\xc3\x85\xc2\xa1|\xc3\x83\xc2\xb0\xc3\x85\xc2\xb8|\xc3\x83\xc2\xa2\xc3\x85\x22|\xc3\x83\xc2\xa2\xc3\x85\xe2\x80\x99|\xc3\x83\xc2\xaf\xc3\x82\xc2\xb8\xc3\x82|\xc3\x83\xc2\xa2\xc3\xa2\xe2\x82\xac\xc5\xbe|\xc3\x83\xc2\xa2\xc3\xa2\xe2\x82\xac\xc5\xa1'

if ! command -v git >/dev/null 2>&1; then
    echo "check_mojibake.sh: git not found" >&2
    exit 2
fi

mapfile -t files < <(git ls-files -- \
    ':!:*.png' ':!:*.jpg' ':!:*.jpeg' ':!:*.gif' ':!:*.ico' ':!:*.woff*' \
    ':!:*.ttf' ':!:*.otf' ':!:*.wasm' ':!:*.db' ':!:*.sqlite*' ':!:target/*' \
    ':!:node_modules/*' ':!:dist/*' ':!:build/*' ':!:*.lock')

offenders=()
for f in "${files[@]}"; do
    [ -f "$f" ] || continue
    if LC_ALL=C grep -qE "$PATTERN" "$f" 2>/dev/null; then
        offenders+=("$f")
    fi
done

if [ "${#offenders[@]}" -eq 0 ]; then
    echo "✅ no mid-file mojibake in tracked files"
    exit 0
fi

echo "❌ ${#offenders[@]} tracked file(s) contain mid-file mojibake (double-encoded UTF-8):"
for f in "${offenders[@]}"; do
    echo "  $f"
    LC_ALL=C grep -nE "$PATTERN" "$f" 2>/dev/null | head -3 | sed 's/^/    /'
done
echo
echo "   Fix: restore the affected lines from a clean prior revision (git show <good-commit>:<path>)"
echo "   or retype the corrupted characters by hand -- do not blindly reverse-decode, the original"
echo "   character isn't always recoverable that way. Never write files via a shell tool that can"
echo "   re-encode (e.g. PowerShell Set-Content without -Encoding utf8) -- use the edit tool instead."
exit 1
