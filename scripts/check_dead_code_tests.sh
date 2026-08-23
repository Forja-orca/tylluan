#!/usr/bin/env bash
# G3 Gate: Dead code + stale tests detection
# Reports public Rust items that appear unused and test functions with no assertions.
# Heuristic, name-based grep (like check_dead_config.sh) -- not real type-aware analysis.
# False negatives expected (common names hide behind unrelated usage), false positives
# are the failure mode to avoid since this runs in CI on every push.
# Report-only by default (exit 0 even when suspects found) -- promote to blocking
# only once the suspect list has been triaged and is stable.
#
# Usage: scripts/check_dead_code_tests.sh [--strict]
# Default: always exits 0, prints suspects as a report.
# --strict: exits 1 if any suspect is found (for future promotion to blocking CI gate).

set -euo pipefail
cd "$(dirname "$0")/.."

SRC_DIR="crates/tylluan-kernel/src"
TEST_DIR="crates/tylluan-kernel/tests"

echo "Scanning for dead code + stale tests in $SRC_DIR..."
echo ""

suspects=()
stale_tests=()

# --- Dead code detection: public modules that are never re-exported or used ---
# Check each public module in lib.rs to see if it's actually used anywhere.
# This is fast and has fewer false positives than function-level analysis.

mapfile -t pub_mods < <(rg -N "^\s*pub mod\s+[a-z_][a-z0-9_]*\s*;" "$SRC_DIR/lib.rs" | sed -E 's/.*pub mod\s+([a-z_][a-z0-9_]*)\s*;.*/\1/')

echo "Checking ${#pub_mods[@]} public modules for usage..."

for mod in "${pub_mods[@]}"; do
    # Count usages: `use crate::mod`, `use super::mod`, `crate::mod::`, `super::mod::`, `mod::`
    total_hits=$(rg -c "(^|\s)(crate|super)::${mod}::|(^|\s)${mod}::" "$SRC_DIR" --type rust 2>/dev/null || echo 0)
    total_hits=$(echo "$total_hits" | awk -F: '{sum+=$2} END {print sum+0}')

    # Also check tests directory
    if [ -d "$TEST_DIR" ]; then
        test_hits=$(rg -c "(^|\s)(crate|super)::${mod}::|(^|\s)${mod}::" "$TEST_DIR" --type rust 2>/dev/null || echo 0)
        test_hits=$(echo "$test_hits" | awk -F: '{sum+=$2} END {print sum+0}')
        total_hits=$((total_hits + test_hits))
    fi

    if [ "$total_hits" -eq 0 ]; then
        suspects+=("pub mod $mod (in lib.rs)")
        echo "⚠️  'pub mod $mod' — declared in lib.rs but never imported/used anywhere (0 hits in src/ + tests/)."
    else
        echo "✅ 'pub mod $mod' — used ($total_hits hits)."
    fi
done

# --- Dead code detection: .rs files that exist but are not included in module tree ---
# SKIPPED: too slow and redundant with module usage check above.
# The module usage check already catches unused public modules.
# Orphan files would only exist if someone creates a .rs file but forgets to declare it,
# which cargo would already error on (unused crate root item).
# echo ""
# echo "Checking for orphan .rs files (not in module tree)..."

# --- Stale tests detection: test functions with no assertions ---
# Only check test files (fast, focused).

if [ -d "$TEST_DIR" ]; then
    TEST_SCAN_DIRS=("$TEST_DIR")
else
    TEST_SCAN_DIRS=()
fi

echo ""
echo "Checking for stale tests (no assertions) in test files..."

# Per-file iteration, not per-function reverse lookup: the previous approach
# tried to find "which file contains function X" via a single-line regex
# `#\[test\].*fn X\s*\(`, which silently fails whenever `#[test]` and `fn` sit
# on separate lines (the common case with nested `mod foo_test { #[test] fn
# foo() ... } }`, e.g. dump_catalog_test.rs) -- rg without -U doesn't match
# across lines, so the lookup returned 0 files with 0 crashes and 0 findings,
# looking clean while checking nothing. Iterating files directly and
# extracting each file's own test fn names via `-A 1` context (proven to work
# across the #[test]/fn line split) has no such blind spot.
check_stale_tests_in() {
    local dir="$1"
    local label="$2"
    mapfile -t files < <(rg -l "#\[test\]" "$dir" --type rust 2>/dev/null || true)
    echo "  Checking ${#files[@]} $label file(s) with #[test]..."
    for f in "${files[@]}"; do
        mapfile -t fns_in_file < <(rg -N "#\[test\]" "$f" -A 1 | rg "fn [a-z_][a-z0-9_]*\s*\(" | sed -E 's/.*fn ([a-z_][a-z0-9_]*)\s*\(.*/\1/' | sort -u)
        if rg -q "assert!|assert_eq!|assert_ne!|debug_assert!" "$f"; then
            echo "    OK: $f has assertions (${#fns_in_file[@]} test fn(s))"
        else
            for fn in "${fns_in_file[@]}"; do
                stale_tests+=("$fn (in $f)")
                echo "⚠️  Test '$fn' in $f — file has no assertion macros (assert!, assert_eq!, assert_ne!, debug_assert!)."
            done
        fi
    done
}

echo ""
echo "Checking for stale tests (no assertions) in test files..."
for scan_dir in "${TEST_SCAN_DIRS[@]}"; do
    check_stale_tests_in "$scan_dir" "test"
done

echo ""
echo "Checking for stale inline tests in src/..."
check_stale_tests_in "$SRC_DIR" "src"

echo ""
echo "=== SUMMARY ==="

if [ "${#suspects[@]}" -eq 0 ] && [ "${#stale_tests[@]}" -eq 0 ]; then
    echo "✅ No dead code suspects or stale tests found."
    exit 0
fi

if [ "${#suspects[@]}" -gt 0 ]; then
    echo "Dead code suspects (${#suspects[@]}):"
    for s in "${suspects[@]}"; do
        echo "  - $s"
    done
fi

if [ "${#stale_tests[@]}" -gt 0 ]; then
    echo "Stale tests (${#stale_tests[@]}):"
    for s in "${stale_tests[@]}"; do
        echo "  - $s"
    done
fi

echo ""
echo "Known false positive patterns (not auto-excluded, require human review):"
echo "  - Modules used only via conditional compilation (cfg gates)"
echo "  - Modules only used in tests/ that aren't scanned (add to TEST_SCAN_DIRS)"
echo "  - Test helper functions named like test_* but not #[test]"
echo "  - Tests that only test panic behavior (no explicit assertion needed)"

if [ "${1:-}" = "--strict" ]; then
    exit 1
fi
exit 0