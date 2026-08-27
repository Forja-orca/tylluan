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
# Usage: scripts/check_test_count.sh [--fix]
# Exit 0 if README.md matches reality, exit 1 (with a diff) otherwise.
# --fix: rewrite README.md's count in place instead of just reporting the
#        mismatch. Added 2026-07-26 after this exact check failed 4 times in
#        one afternoon during the "vivir Tylluan" dogfooding week -- fast
#        parallel commits kept outrunning the manual README edit. Doesn't
#        touch CI (which stays read-only, as a safety net); this just saves
#        the human/agent from typing the same one-line edit by hand again.

set -euo pipefail
cd "$(dirname "$0")/.."

sum_passed() {
    # Sums every "N passed" from `cargo test` output across however many
    # test binaries got run (lib + each tests/*.rs integration file).
    grep -oE '[0-9]+ passed' | awk '{sum += $1} END {print sum+0}'
}

CARGO_CMD="cargo"
if ! command -v cargo >/dev/null 2>&1; then
    if command -v cargo.exe >/dev/null 2>&1; then
        CARGO_CMD="cargo.exe"
    elif command -v rustup >/dev/null 2>&1; then
        CARGO_CMD="rustup run stable cargo"
    fi
fi

run_and_count() {
    # Run cargo test, show its output live (visible to the user/CI log),
    # and return the "N passed" count on stdout (captured by the caller).
    #
    # The original `cargo test ... 2>&1 | tee /dev/stderr | sum_passed`
    # broke on Windows/Git Bash: /dev/stderr is a Linux-only symlink, not
    # a real device there. A first attempted fix (`2>&1 >&2 | sum_passed`,
    # no tee at all) was verified WRONG before being committed here: that
    # redirection order sends both stdout and stderr into the SAME pipe as
    # sum_passed, so cargo test's live output vanishes entirely into
    # grep -oE (which drops every non-matching line) -- nothing is visible
    # on the terminal or in a CI log while tests run, only the final sum.
    # Confirmed with a two-line reproduction before writing this comment.
    #
    # Fix: `tee` to a real temp FILE instead of /dev/stderr. mktemp is
    # portable to Git Bash/MSYS2, so this keeps tee's actual job (live
    # visibility + capture) working everywhere.
    #
    # tee's own passthrough copy must go to stderr (>&2), not this
    # function's stdout -- callers capture this function via $(...), and
    # stdout is the only stream $(...) captures. Without the >&2 here, the
    # full raw log leaks into that capture instead of just the final count
    # (caught live: kernel_count ended up holding the whole test log,
    # crashing the `$((kernel_count + ...))` arithmetic downstream).
    local out
    out=$(mktemp)
    $CARGO_CMD test "$@" 2>&1 | tee "$out" >&2
    sum_passed < "$out"
    rm -f "$out"
}

echo "Running tylluan-kernel lib tests..."
kernel_count=$(run_and_count -p tylluan-kernel --lib)

echo "Running tylluan-link lib tests..."
link_count=$(run_and_count -p tylluan-link --lib)

echo "Running tylluan-fsrs lib tests..."
fsrs_count=$(run_and_count -p tylluan-fsrs --lib)

real_total=$((kernel_count + link_count + fsrs_count))

echo ""
echo "Real counts: kernel=$kernel_count link=$link_count fsrs=$fsrs_count total=$real_total"

# README.md's claim looks like: "402 tests across Rust kernel --lib, tylluan-link, and tylluan-fsrs ..."
claimed_total=$(grep -oE '^[0-9]+ tests across Rust kernel' README.md | grep -oE '^[0-9]+' | head -1)

if [ -z "$claimed_total" ]; then
    echo "❌ Could not find the test-count line in README.md (expected a line matching"
    echo "   '<N> tests across Rust kernel --lib, tylluan-link, and tylluan-fsrs')."
    echo "   Did the wording change? Update this script's grep pattern to match."
    exit 1
fi

echo "README.md claims: $claimed_total"
echo ""

if [ "$real_total" -ne "$claimed_total" ]; then
    if [ "${1:-}" = "--fix" ]; then
        sed -i -E "s/^[0-9]+ tests across Rust kernel/${real_total} tests across Rust kernel/" README.md
        echo "✅ Fixed: README.md now says ${real_total} tests (was ${claimed_total})."
        echo "   Remember to check STATUS.md's Commit/test-count line too -- not covered by this flag."
        exit 0
    fi
    echo "❌ MISMATCH: README.md says $claimed_total tests, but $real_total actually pass."
    echo ""
    echo "Fix: update the test-count line in README.md to $real_total, e.g.:"
    echo "  $real_total tests across Rust kernel (lib), tylluan-link, and tylluan-fsrs — all green."
    echo "  Or just run: scripts/check_test_count.sh --fix"
    echo ""
    echo "Also check STATUS.md's Commit/test-count line while you're there -- it has the"
    echo "same kind of drift risk."
    exit 1
fi

echo "✅ README.md's test count matches reality ($real_total)."
