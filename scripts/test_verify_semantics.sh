#!/usr/bin/env bash
# scripts/test_verify_semantics.sh -- meta-test for scripts/verify.sh itself.
#
# Why this exists (2026-09-13, real incident): check_no_predation.sh (ADR-013)
# is REPORT-ONLY by contract -- "the gate REPORTS, it never edits, reverts or
# blocks mechanically". But verify.sh originally wired it through fail(),
# which silently converted the report into a pre-push blocker; it actually
# blocked a real push before anyone noticed. The gate's philosophy died in
# the wiring, not in the design, and nothing caught it because every existing
# test exercises the GATES, not how verify.sh COMBINES them.
#
# This script is that missing layer. It copies the LIVE verify.sh into a
# sandbox, feeds it fabricated gate outputs, and asserts the exit-code
# semantics hold:
#
#   T1  every gate green                      -> --docs exits 0
#   T2  a REPORT-ONLY gate fails (exit 1)     -> --docs STILL exits 0
#   T3  a BLOCKING gate fails (exit 1)        -> --docs exits 1
#   T4  negative control: a report-only gate wired through fail() (the exact
#       pre-49950c2 pattern) DOES flip the exit -- proves T2 is load-bearing
#   T5  real doc drift (STATUS.md citing a bogus HEAD, detected by the REAL
#       check_head_sync.sh in a disposable git worktree) -> --docs exits 1
#
# Hermetic by design: T1-T4 never touch git or cargo (all gates stubbed);
# T5 runs inside a disposable git worktree that is removed afterwards. The
# real checkout, its refs and its working tree are never modified.
#
# Gate discovery is dynamic: whatever check_*.sh scripts verify.sh invokes
# are stubbed, and the report-only set is declared ONCE here
# (REPORT_ONLY_GATES). Consequences:
#   - If verify.sh ever wires a REPORT_ONLY gate through fail(), T2 catches
#     it on the REAL gate, not a synthetic one.
#   - If a NEW gate appears in verify.sh, T3 exercises it as blocking; if a
#     new gate is actually report-only, classify it in REPORT_ONLY_GATES --
#     until then T2 never claims protection it doesn't have.
#
# Usage:  scripts/test_verify_semantics.sh          (run the 5 tests)
#         scripts/test_verify_semantics.sh --list   (show discovered gates)
# Exit 0 if all tests pass; exit 1 with a summary otherwise.

set -uo pipefail
cd "$(dirname "$0")/.."

REPO_ROOT="$PWD"
VERIFY_SRC="$REPO_ROOT/scripts/verify.sh"

if [ ! -f "$VERIFY_SRC" ]; then
    echo "❌ $VERIFY_SRC not found -- run from the repo root."
    exit 2
fi

# ── Gate classification: the ONE place mapping gate -> blocking semantics ──
# Gates wired in verify.sh --docs that must NEVER flip FAILED (report-only).
REPORT_ONLY_GATES="check_no_predation"
# All other discovered gates are exercised as blocking by T3.

# ── Discover gates dynamically from verify.sh's own body ──
mapfile -t all_gates < <(grep -oE 'check_[a-z_]+\.sh' "$VERIFY_SRC" | sed 's/\.sh$//' | sort -u)
if [ "${#all_gates[@]}" -eq 0 ]; then
    echo "❌ No check_*.sh gates found inside verify.sh -- wiring changed?"
    exit 2
fi

is_report_only() {
    local g
    for g in $REPORT_ONLY_GATES; do
        [ "$g" = "$1" ] && return 0
    done
    return 1
}

is_wired() {
    local g
    for g in "${all_gates[@]}"; do
        [ "$g" = "$1" ] && return 0
    done
    return 1
}

if [ "${1:-}" = "--list" ]; then
    echo "Gates discovered in verify.sh: ${all_gates[*]}"
    for g in "${all_gates[@]}"; do
        if is_report_only "$g"; then echo "  $g -> REPORT-ONLY (must never flip FAILED)"; else echo "  $g -> blocking"; fi
    done
    exit 0
fi

failures=0
t()    { printf '%s\n' "-- $1"; }
okt()  { echo "   ✅ $1"; }
badt() { echo "   ❌ $1"; failures=$((failures+1)); }

# ── Sandbox plumbing ──────────────────────────────────────────────────────

# Stub EVERY discovered gate. Case "<gate>-fails" makes exactly that gate
# exit 1; "clean" makes all exit 0.
build_gate_stubs() {
    local sbx="$1" case_tag="$2" g rc
    for g in "${all_gates[@]}"; do
        rc=0
        case "$case_tag" in
            "$g-fails") rc=1 ;;
        esac
        {
            echo '#!/usr/bin/env bash'
            echo "# fabricated stub for meta-test (case: $case_tag)"
            echo 'echo "stub scripts/'"$g"'.sh output"'
            # NOTE: exit code goes in as FILE CONTENT, computed outside the
            # braced group -- putting exit inside { } > file would terminate
            # THIS script instead of the stub it writes (caught live, 1st run).
            echo "exit $rc"
        } > "$sbx/scripts/$g.sh"
    done
}

# make_sandbox <path>: live verify.sh + minimal STATUS.md/README.md copies.
# (T1-T4 only need the files verify.sh itself touches before gating.)
make_sandbox() {
    local sbx="$1"
    mkdir -p "$sbx/scripts"
    cp "$VERIFY_SRC" "$sbx/scripts/verify.sh"
    echo '# sandbox STATUS.md' > "$sbx/STATUS.md"
    echo '# sandbox README.md' > "$sbx/README.md"
}

# run_case <sandbox> <case-tag> -> sets RUN_OUT and RUN_RC
run_case() {
    local sbx="$1" case_tag="$2"
    build_gate_stubs "$sbx" "$case_tag"
    RUN_OUT=$(cd "$sbx" && bash scripts/verify.sh --docs 2>&1)
    RUN_RC=$?
}

# splice_broken_wiring <verify.sh copy> <gate>
# Rewrites the gate's wiring into the exact pre-49950c2 pattern: the
# report-only else-branch becomes fail(). Pure bash + one awk; leaves a
# recognizable marker so callers can verify the splice took effect.
splice_broken_wiring() {
    local vf="$1" g="$2"
    awk -v g="$g" '
        $0 ~ "^[[:space:]]*if bash scripts/" g "\\.sh; then[[:space:]]*$" { inblock=1; print; next }
        inblock && $0 ~ /^[[:space:]]*else[[:space:]]*$/ { print; skiprest=1;
            print "        fail \"" g " wired-blocking (T4 negative control)\""; next }
        inblock && skiprest && $0 ~ /^[[:space:]]*fi[[:space:]]*$/ { skiprest=0; inblock=0; print; next }
        inblock && skiprest { next }
        { print }
    ' "$vf" > "$vf.new" && mv "$vf.new" "$vf"
    grep -q "wired-blocking (T4 negative control)" "$vf"
}

SBASHOME="$(mktemp -d)"

# ── T1: all green -> exit 0 ───────────────────────────────────────────────
t "T1: all gates green -> --docs exits 0"
sbx="$SBASHOME/t1"; make_sandbox "$sbx"; run_case "$sbx" clean
if [ "$RUN_RC" -eq 0 ]; then
    okt "exit 0"
else
    badt "expected 0, got $RUN_RC"; echo "$RUN_OUT" | tail -5
fi

# ── T2: EVERY report-only gate failing must NOT flip the exit code ───────
for g in $REPORT_ONLY_GATES; do
    if ! is_wired "$g"; then
        badt "REPORT_ONLY_GATES lists '$g' but verify.sh does not wire it"
        continue
    fi
    t "T2: report-only gate '$g' fails (exit 1) -> --docs STILL exits 0"
    sbx="$SBASHOME/t2_$g"; make_sandbox "$sbx"; run_case "$sbx" "$g-fails"
    if [ "$RUN_RC" -eq 0 ] && echo "$RUN_OUT" | grep -q "$g"; then
        okt "exit 0, finding printed (report-only honored)"
    else
        badt "exit $RUN_RC -- report-only semantics broken (the 49950c2 bug class)"
        echo "$RUN_OUT" | tail -8
    fi
done

# ── T3: a blocking gate failing MUST flip the exit code ──────────────────
blocking_found=0
for g in "${all_gates[@]}"; do
    if is_report_only "$g"; then continue; fi
    blocking_found=1
    t "T3: blocking gate '$g' fails (exit 1) -> --docs exits 1"
    sbx="$SBASHOME/t3_$g"; make_sandbox "$sbx"; run_case "$sbx" "$g-fails"
    if [ "$RUN_RC" -eq 1 ]; then
        okt "exit 1 (real drift still blocks)"
    else
        badt "expected 1, got $RUN_RC"
        echo "$RUN_OUT" | tail -5
    fi
done
if [ "$blocking_found" -eq 0 ]; then
    badt "no blocking gates found -- REPORT_ONLY_GATES may be over-broad"
fi

# ── T4: negative control -- the exact pre-49950c2 wiring must be detectable ──
# Splice fail() back into the report-only gate's branch, then a FAILING run of
# that gate MUST flip --docs to 1. If it doesn't, T2 is not testing anything.
t "T4: negative control -- report-only gate re-wired through fail() flips the exit"
g="${REPORT_ONLY_GATES%% *}"
if is_wired "$g"; then
    sbx="$SBASHOME/t4"; make_sandbox "$sbx"
    if splice_broken_wiring "$sbx/scripts/verify.sh" "$g"; then
        run_case "$sbx" "$g-fails"
        if [ "$RUN_RC" -eq 1 ]; then
            okt "broken wiring flips exit to 1 -> T2's criterion is load-bearing"
        else
            badt "broken wiring did NOT flip exit -- T2 would pass even against the 49950c2 bug"
            echo "$RUN_OUT" | tail -8
        fi
    else
        badt "could not splice the broken wiring -- T4 control is inconclusive"
    fi
else
    badt "no wired report-only gate to use for T4"
fi

# ── T5: REAL doc drift must still block ──────────────────────────────────
# Runs the REAL check_head_sync.sh against a corrupted STATUS.md, inside a
# disposable git worktree (shares objects; refs and the real checkout are
# untouched). The heavy/unrelated gates are stubbed so the only variable is
# the drift itself.
t "T5: real doc drift (bogus HEAD citation, real check_head_sync.sh) -> --docs exits 1"
WT="$(mktemp -d)/wt5"
if git worktree add --detach "$WT" HEAD >/dev/null 2>&1; then
    cp "$REPO_ROOT/STATUS.md" "$WT/STATUS.md"
    sed -i -E 's/`[0-9a-f]{7,40}`/`deadbee`/g' "$WT/STATUS.md"
    if grep -q 'deadbee' "$WT/STATUS.md"; then
        # Stub the cargo-heavy gate and the report-only gate; keep the real
        # check_head_sync.sh so the drift is caught by production code.
        printf '#!/usr/bin/env bash\nexit 0\n' > "$WT/scripts/check_test_count.sh"
        printf '#!/usr/bin/env bash\nexit 0\n' > "$WT/scripts/check_no_predation.sh"
        RUN_OUT=$(cd "$WT" && bash scripts/verify.sh --docs 2>&1); RUN_RC=$?
        if [ "$RUN_RC" -eq 1 ] && echo "$RUN_OUT" | grep -qi "head"; then
            okt "exit 1, caught by the real gate (drift still blocks)"
        else
            badt "expected exit 1 naming the HEAD check, got $RUN_RC"
            echo "$RUN_OUT" | tail -8
        fi
    else
        badt "sandbox STATUS.md had no HEAD citation to corrupt -- T5 is vacuous"
    fi
    git worktree remove --force "$WT" >/dev/null 2>&1
    git worktree prune >/dev/null 2>&1
else
    badt "could not create a disposable worktree -- T5 skipped as failed"
fi

rm -rf "$SBASHOME"

echo
if [ "$failures" -eq 0 ]; then
    echo "✅ verify.sh semantics hold: report-only stays report-only, real drift still blocks."
    exit 0
else
    echo "❌ $failures semantic test(s) failed -- see above. The wiring between"
    echo "   verify.sh and the gates has drifted from the ADR-013 contract."
    exit 1
fi
