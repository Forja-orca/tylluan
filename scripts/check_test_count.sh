#!/usr/bin/env bash
# Verifies that README.md's claimed test count matches the real, live count.
#
# Why this exists: three times in one session (2026-07-13), README.md's test
# count drifted out of sync with reality after milestones closed -- STATUS.md
# (the internal source of truth) got updated correctly each time, but the
# public-facing README lagged behind because nothing forced it to stay
# accurate. This script is that force: it runs the actual test suites,
# counts real passes, and fails CI if README.md's number doesn't match.
#
# Usage: scripts/check_test_count.sh
# Exit 0 if README.md matches reality, exit 1 (with a diff) otherwise.

set -euo pipefail
cd "$(dirname "$0")/.."

sum_passed() {
    # Sums every "N passed" from `cargo test` output across however many
    # test binaries got run (lib + each tests/*.rs integration file).
    grep -oE '[0-9]+ passed' | awk '{sum += $1} END {print sum+0}'
}

echo "Running tylluan-kernel test suite (lib + integration)..."
kernel_count=$(cargo test -p tylluan-kernel 2>&1 | tee /dev/stderr | sum_passed)

echo "Running tylluan-link test suite..."
link_count=$(cargo test -p tylluan-link --lib 2>&1 | tee /dev/stderr | sum_passed)

echo "Running tylluan-fsrs test suite..."
fsrs_count=$(cargo test -p tylluan-fsrs --lib 2>&1 | tee /dev/stderr | sum_passed)

real_total=$((kernel_count + link_count + fsrs_count))

echo ""
echo "Real counts: kernel=$kernel_count link=$link_count fsrs=$fsrs_count total=$real_total"

# README.md's claim looks like: "578 tests across Rust kernel (lib + integration), ..."
claimed_total=$(grep -oE '^[0-9]+ tests across Rust kernel' README.md | grep -oE '^[0-9]+' | head -1)

if [ -z "$claimed_total" ]; then
    echo "❌ Could not find the test-count line in README.md (expected a line matching"
    echo "   '<N> tests across Rust kernel (lib + integration), tylluan-link, and tylluan-fsrs')."
    echo "   Did the wording change? Update this script's grep pattern to match."
    exit 1
fi

echo "README.md claims: $claimed_total"
echo ""

if [ "$real_total" -ne "$claimed_total" ]; then
    echo "❌ MISMATCH: README.md says $claimed_total tests, but $real_total actually pass."
    echo ""
    echo "Fix: update the test-count line in README.md to $real_total, e.g.:"
    echo "  $real_total tests across Rust kernel (lib + integration), tylluan-link, and tylluan-fsrs — all green."
    echo ""
    echo "Also check STATUS.md's Commit/test-count line while you're there -- it has the"
    echo "same kind of claim and the same drift risk."
    exit 1
fi

echo "✅ README.md's test count matches reality ($real_total)."
