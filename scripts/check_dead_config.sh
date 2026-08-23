#!/usr/bin/env bash
# Reports config.rs struct fields that are declared but never read anywhere
# in src/ outside config.rs itself -- the exact pattern behind two real bugs
# found in the 2026-08-22 audit: `decay_half_life_hours` (decay.rs discarded
# it after the FSRS migration, but it stayed threaded through 7 files and a
# public toml key) and `[federation] auto_sync_interval_secs` (defined,
# defaulted, read nowhere -- the real loop used a different config section's
# field instead).
#
# This is a heuristic, name-based grep, not real type-aware dead-code
# analysis (that's what `cargo check`/clippy already do for genuinely unused
# *variables* -- this catches the different, sneakier case of a struct field
# that's read somewhere, just never anywhere that has any effect, e.g. only
# inside its own Default impl or its own serde derive). Two consequences of
# that heuristic nature, both deliberate:
#   - False negatives are expected and fine: a field named `enabled` will
#     match `.enabled` usages belonging to a DIFFERENT struct with the same
#     field name, hiding a real dead `enabled` field behind an unrelated
#     one. Accepted trade-off -- a real-analysis tool would need
#     rust-analyzer or a proc-macro, out of scope for a bash script.
#   - False positives are the failure mode to actively avoid, since this
#     runs in CI on every push: a field only ever matched via a re-export,
#     a macro, or serde's own (de)serialization internals could look
#     "unused" to grep and wrongly block an unrelated commit. This is why
#     this script is REPORT-ONLY by default (exit 0 even when it finds
#     suspects) -- promote it to a hard blocking gate only once its
#     suspect list has been triaged by a human/agent and is stable.
#
# Usage: scripts/check_dead_config.sh [--strict]
# Default: always exits 0, prints suspects as a report.
# --strict: exits 1 if any suspect is found (for a future promotion to a
#           blocking CI gate, once the baseline is clean).

set -euo pipefail
cd "$(dirname "$0")/.."

CONFIG_FILE="crates/tylluan-kernel/src/config.rs"
SRC_DIR="crates/tylluan-kernel/src"

mapfile -t fields < <(grep -oE '^\s*pub [a-z_][a-z0-9_]*:' "$CONFIG_FILE" | grep -oE '[a-z_][a-z0-9_]*' | grep -v '^pub$' | sort -u)

echo "Scanning ${#fields[@]} unique config field names declared in $CONFIG_FILE..."
echo ""

suspects=()

for field in "${fields[@]}"; do
    # Count usages as `.field_name` anywhere in src/ EXCEPT inside
    # config.rs's own struct/Default/serde declarations. grep -w on the
    # field name after a literal dot, across all source files, then
    # subtract matches that live inside config.rs itself.
    total_hits=$( (grep -rE "\.${field}\b" "$SRC_DIR" --include="*.rs" -l 2>/dev/null || true) | wc -l)
    outside_hits=$( (grep -rlE "\.${field}\b" "$SRC_DIR" --include="*.rs" 2>/dev/null || true) | (grep -v "^${CONFIG_FILE}$" || true) | wc -l)

    if [ "$outside_hits" -eq 0 ]; then
        suspects+=("$field")
        echo "⚠️  '$field' — declared in config.rs, but \".${field}\" never appears read anywhere outside config.rs (found in $total_hits file(s) total, all config.rs itself)."
    fi
done

echo ""
if [ "${#suspects[@]}" -eq 0 ]; then
    echo "✅ No dead-config suspects found."
    exit 0
fi

echo "Found ${#suspects[@]} suspect(s): ${suspects[*]}"
echo "Each needs a human/agent look -- some may be false positives (macro-only"
echo "access, a re-export, serde-internal use) rather than genuinely dead."
echo ""

if [ "${1:-}" = "--strict" ]; then
    exit 1
fi
exit 0
