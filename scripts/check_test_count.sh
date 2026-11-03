#!/usr/bin/env bash
# Verifies that README.md's AND AGENTS.md's claimed test counts match the
# real, live count.
#
# Why this exists: three times in one session (2026-07-13), README.md's test
# count drifted out of sync with reality after milestones closed -- STATUS.md
# (the internal source of truth) got updated correctly each time, but the
# public-facing README lagged behind because nothing forced it to stay
# accurate. This script is that force: it gets the real per-suite pass
# counts and fails CI if README.md's number doesn't match.
#
# 2026-10-10 (José, checklist P1 "AGENTS.md cifras falsas sin gate"): the
# same gate now covers AGENTS.md component-wise (kernel/link/fsrs), because
# that file is auto-loaded by every agent on connect and had drifted to
# "790 en verde" while reality was 1079 -- the exact incident class its own
# header warns about, but with no CI line forcing it honest.
#
# 2026-11-03 (T2590 punto 6, TL order, hermes): REUSE mode. scripts/verify.sh
# already ran kernel --lib, link --lib --tests and fsrs this very run; before
# this change this gate RE-RAN all three (122 s measured, pure duplicate).
# Now verify.sh exports TYLLUAN_TMC_DIR with the tee'd per-gate logs and this
# script sums those instead (~5 s). Semantics are identical because only the
# "Running unittests src/lib.rs" blocks are counted (exactly what the old
# per-crate "--lib" invocations measured) -- integration-binary blocks in
# reused logs are provably ignored (the self-test plants a decoy block for
# this). No TYLLUAN_TMC_DIR (standalone run, CI job) -> falls back to running
# cargo itself, byte-for-byte the old behavior.
# SELF-TEST (TL demanded evidence the detector is not vacuous): before any
# real comparison, a throwaway tree with planted counts and a planted FAKE
# claim must exit 1, and the same tree with correct claims must exit 0.
# Mirrors check_async_guild_io.py's "runs its own fixture self-test first"
# pattern. A broken detector fails the push instead of passing silently.
#
# Usage: scripts/check_test_count.sh [--fix]
# Exit 0 if README.md and AGENTS.md match reality, exit 1 (with a diff)
# otherwise.
# --fix: rewrite the counts in README.md AND AGENTS.md in place instead of
#        just reporting the mismatch. Added 2026-07-26 after this exact
#        check failed 4 times in one afternoon during the "vivir Tylluan"
#        dogfooding week -- fast parallel commits kept outrunning the manual
#        README edit. Doesn't touch CI (which stays read-only, as a safety
#        net); this just saves the human/agent from typing the same one-line
#        edit by hand again. STATUS.md's canonical HEAD line stays manual by
#        design (it carries hash + date + context).

set -euo pipefail

# ── self-test FIRST on every normal run (async-guild-io pattern), ────────
# unless explicitly suppressed (inner wrapper sets the guard env).
# --self-test        : kept as a no-op-compatible alias (self-test runs anyway)
# --self-test-inner  : one comparison pass inside the planted tree (guard env set)
if [ "${TYLLUAN_TMC_NO_SELFTEST:-}" != "1" ]; then
    FIX=$(mktemp -d)
    mkdir -p "$FIX/logs"
    # Planted real counts: kernel=10 (lib only, decoy integration block
    # must NOT count), link=4 (lib) + decoy, fsrs=2 (lib) + decoy.
    printf '     Running unittests src\\lib.rs (target/debug/deps/tylluan_kernel-0000000000)\n\nrunning 10 tests\ntest result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\n     Running tests\\decoy_must_not_count.rs (target/debug/deps/decoy-1111)\n\nrunning 999 tests\ntest result: ok. 999 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n' > "$FIX/logs/kernel_lib.log"
    cat > "$FIX/logs/link_all.log" <<'EOF'
     Running unittests src/lib.rs (target/debug/deps/tylluan_link-2222)

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/integration.rs (target/debug/deps/integration-3333)

test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
EOF
    cat > "$FIX/logs/fsrs_all.log" <<'EOF'
     Running unittests src/lib.rs (target/debug/deps/tylluan_fsrs-4444)

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
EOF
    printf '%s\n' 'Some README' '402 tests across Rust kernel --lib, tylluan-link, and tylluan-fsrs — all green.' > "$FIX/README.md"
    printf '%s\n' '# AGENTS' '**Tests:** 10 lib tests (kernel) + 4 (tylluan-link) + 2 (tylluan-fsrs) = 16 en verde — x' > "$FIX/AGENTS.md"
    # Negative control: planted FAKE README claim (402 vs real 16) MUST be caught.
    if TYLLUAN_TMC_NO_SELFTEST=1 TYLLUAN_TMC_ROOT="$FIX" TYLLUAN_TMC_DIR="$FIX/logs" bash "$0" --self-test-inner >"$FIX/neg.out" 2>&1; then
        echo "❌ SELF-TEST FAILED: gate PASSED against a planted fake count (402 vs 16) -- detector is vacuous. Refusing to run as a real gate."
        cat "$FIX/neg.out"; rm -rf "$FIX"; exit 1
    fi
    if ! grep -qi 'MISMATCH' "$FIX/neg.out"; then
        echo "❌ SELF-TEST FAILED: gate exited non-zero but never said MISMATCH -- wrong failure mode."
        cat "$FIX/neg.out"; rm -rf "$FIX"; exit 1
    fi
    # Positive control: fix README to the real planted number -> MUST exit 0.
    printf '%s\n' 'Some README' '16 tests across Rust kernel --lib, tylluan-link, and tylluan-fsrs — all green.' > "$FIX/README.md"
    if ! TYLLUAN_TMC_NO_SELFTEST=1 TYLLUAN_TMC_ROOT="$FIX" TYLLUAN_TMC_DIR="$FIX/logs" bash "$0" --self-test-inner >"$FIX/pos.out" 2>&1; then
        echo "❌ SELF-TEST FAILED: gate rejected the CORRECT counts (16/10+4+2). Integration blocks may be leaking into the sum:"
        cat "$FIX/pos.out"; rm -rf "$FIX"; exit 1
    fi
    rm -rf "$FIX"
    echo "✅ check_test_count self-test: planted mismatch caught, correct counts accepted (detector proven, not vacuous)."
fi

REPO_ROOT="${TYLLUAN_TMC_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$REPO_ROOT"

sum_passed() {
    # Sums every "N passed" from `cargo test` output across however many
    # test binaries got run (lib + each tests/*.rs integration file).
    grep -oE '[0-9]+ passed' | awk '{sum += $1} END {print sum+0}'
}

lib_passed_from_log() {
    # REUSE mode: sum "N passed" ONLY from the 'Running unittests src/lib.rs'
    # block(s) of a tee'd verify.sh log -- i.e. exactly what the old
    # `cargo test -p X --lib` measured. Every other "Running ..." block
    # (integration binaries under tests/*.rs) flips the filter off again.
    # A suite that FAILED flips the whole gate to a hard fail (defensive:
    # verify.sh already failed those gates; never compare against garbage).
    if grep -q 'test result: FAILED' "$1" 2>/dev/null; then
        echo "FAILED-SUITE"
        return
    fi
    awk '
        /^ *(Running|Doc-tests)/ { inlib = (index($0, "unittests src") > 0); next }
        inlib && /test result:/ {
            if (match($0, /[0-9]+ passed/)) sum += substr($0, RSTART, RLENGTH) + 0
        }
        END { print sum+0 }
    ' "$1"
}

CARGO_CMD="cargo"
if ! command -v cargo >/dev/null 2>&1; then
    if command -v cargo.exe >/dev/null 2>&1; then
        CARGO_CMD="cargo.exe"
    elif command -v rustup >/dev/null 2>&1; then
        CARGO_CMD="rustup run 1.99.1 cargo"
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

kernel_count=""; link_count=""; fsrs_count=""
if [ -n "${TYLLUAN_TMC_DIR:-}" ] \
   && [ -f "$TYLLUAN_TMC_DIR/kernel_lib.log" ] \
   && [ -f "$TYLLUAN_TMC_DIR/link_all.log" ] \
   && [ -f "$TYLLUAN_TMC_DIR/fsrs_all.log" ]; then
    echo "Reusing this run's test output (TYLLUAN_TMC_DIR set; no re-run)..."
    kernel_count=$(lib_passed_from_log "$TYLLUAN_TMC_DIR/kernel_lib.log")
    link_count=$(lib_passed_from_log "$TYLLUAN_TMC_DIR/link_all.log")
    fsrs_count=$(lib_passed_from_log "$TYLLUAN_TMC_DIR/fsrs_all.log")
    for c in "$kernel_count" "$link_count" "$fsrs_count"; do
        if [ "$c" = "FAILED-SUITE" ]; then
            echo "❌ A reused test log contains a FAILED suite -- fix the tests, this gate will not compare against broken runs."
            exit 1
        fi
    done
    if [ "$kernel_count" -eq 0 ] || [ "$link_count" -eq 0 ] || [ "$fsrs_count" -eq 0 ]; then
        echo "⚠️  Reused logs yielded a zero count (wording change?) -- falling back to live runs."
        kernel_count=""; link_count=""; fsrs_count=""
    fi
fi
if [ -z "$kernel_count" ]; then
    echo "Running tylluan-kernel lib tests..."
    kernel_count=$(run_and_count -p tylluan-kernel --lib)
fi
if [ -z "$link_count" ]; then
    echo "Running tylluan-link lib tests..."
    link_count=$(run_and_count -p tylluan-link --lib)
fi
if [ -z "$fsrs_count" ]; then
    echo "Running tylluan-fsrs lib tests..."
    fsrs_count=$(run_and_count -p tylluan-fsrs --lib)
fi

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
        # Fall through to the AGENTS.md check below (fix both in one run).
        claimed_total="$real_total"
    fi
    if [ "$claimed_total" != "$real_total" ]; then
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
fi

echo "✅ README.md's test count matches reality ($real_total)."

# ── AGENTS.md: same claim, validated component-wise ───────────────────────
# (unchanged semantics; full rationale in the 2026-10-10 note above)
agents_line=$(grep -m1 -E '^\*\*Tests:\*\* [0-9]+ lib tests \(kernel\)' AGENTS.md || true)
if [ -z "$agents_line" ]; then
    echo "❌ Could not find the test-count line in AGENTS.md (expected a line matching"
    echo "   '**Tests:** <N> lib tests (kernel) + <N> (tylluan-link) + <N> (tylluan-fsrs) = <N> en verde')."
    echo "   Did the wording change? Update this script's grep patterns to match."
    exit 1
fi

a_kernel=$(echo "$agents_line" | grep -oE '[0-9]+ lib tests' | grep -oE '[0-9]+' | head -1)
a_link=$(echo "$agents_line" | grep -oE '\+ [0-9]+ \(tylluan-link\)' | grep -oE '[0-9]+' | head -1)
a_fsrs=$(echo "$agents_line" | grep -oE '\+ [0-9]+ \(tylluan-fsrs\)' | grep -oE '[0-9]+' | head -1)
a_total=$(echo "$agents_line" | grep -oE '= [0-9]+ en verde' | grep -oE '[0-9]+' | head -1)

echo "AGENTS.md claims: kernel=$a_kernel link=$a_link fsrs=$a_fsrs total=$a_total"
echo ""

if [ -z "$a_kernel" ] || [ -z "$a_link" ] || [ -z "$a_fsrs" ] || [ -z "$a_total" ]; then
    echo "❌ Could not parse all four numbers out of AGENTS.md's test-count line"
    echo "   (kernel=$a_kernel link=$a_link fsrs=$a_fsrs total=$a_total)."
    echo "   Did the wording change? Update this script's grep patterns to match."
    exit 1
fi

if [ "$a_kernel" != "$kernel_count" ] || [ "$a_link" != "$link_count" ] \
    || [ "$a_fsrs" != "$fsrs_count" ] || [ "$a_total" != "$real_total" ]; then
    if [ "${1:-}" = "--fix" ]; then
        sed -i -E "s/^\*\*Tests:\*\* [0-9]+ lib tests \(kernel\) \+ [0-9]+ \(tylluan-link\) \+ [0-9]+ \(tylluan-fsrs\) = [0-9]+ en verde/**Tests:** ${kernel_count} lib tests (kernel) + ${link_count} (tylluan-link) + ${fsrs_count} (tylluan-fsrs) = ${real_total} en verde/" AGENTS.md
        echo "✅ Fixed: AGENTS.md now says kernel=$kernel_count link=$link_count fsrs=$fsrs_count total=$real_total."
        echo "   STATUS.md's canonical line is NOT covered by this flag -- update it by hand too."
        exit 0
    fi
    echo "❌ MISMATCH: AGENTS.md says kernel=$a_kernel link=$a_link fsrs=$a_fsrs total=$a_total,"
    echo "   but reality is kernel=$kernel_count link=$link_count fsrs=$fsrs_count total=$real_total."
    echo ""
    echo "Fix: run scripts/check_test_count.sh --fix, or edit AGENTS.md's '**Tests:**' line by hand."
    echo "   AGENTS.md is auto-loaded by every agent on connect -- stale numbers here misinform"
    echo "   every session from its first second (its own header warns about exactly this)."
    exit 1
fi

echo "✅ AGENTS.md's test counts match reality ($real_total)."
