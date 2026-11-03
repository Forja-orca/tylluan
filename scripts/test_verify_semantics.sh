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
#   T0  parallel-harness self-test (added 2026-11-03, TL go-ahead T2610):
#       planted fake jobs run through the SAME background/out+rc-file/wait
#       collect layer that carries T1-T4; exactly one planted failing verdict
#       MUST come back named. If the aggregator can swallow a verdict, exit 2
#       before any real test runs -- every green below would be meaningless.
#   T1  every gate green                      -> --docs exits 0
#   T2  a REPORT-ONLY gate fails (exit 1)     -> --docs STILL exits 0
#   T3  a BLOCKING gate fails (exit 1)        -> --docs exits 1
#   T4  negative control: a report-only gate wired through fail() (the exact
#       pre-49950c2 pattern) DOES flip the exit -- proves T2 is load-bearing
#   T5  real doc drift (STATUS.md citing a bogus HEAD, detected by the REAL
#       check_head_sync.sh in a disposable git worktree) -> --docs exits 1
#   T6  gate coverage & zero-orphan invariant (MD-9) -> all check_*/verify_*
#       and portability scripts on disk must be wired in verify.sh, CI, or caller gates
#   T7  negative control for gate coverage: an uninvoked gate in sandbox
#       MUST cause the coverage test to fail (proves T6 is load-bearing)
#
# Hermetic by design: T0-T4 never touch git or cargo (all gates stubbed);
# T5 runs inside a disposable git worktree that is removed afterwards. The
# real checkout, its refs and its working tree are never modified; T6-T7 inspect
# file trees and sandbox copies.
#
# Concurrency (2026-11-03, TL go-ahead T2610): PS4 traces measured the 14
# sequential sandbox runs of verify.sh at 278 s of 391 s total (71%) --
# dominated by the bash fork/exec tax on Windows (~1.4 s per stub spawn).
# T1-T4 cases now LAUNCH concurrently (bounded by META_MAX_JOBS, default 8)
# and are JUDGED sequentially afterwards: verdicts, messages and check order
# are identical to the serial version. T6 used to spawn one grep per
# candidate per source (~75 processes); it is now ONE awk pass over the same
# inputs with identical semantics (exemptions, self-file exclusion,
# verify/CI/caller wiring).
#
# T5 Windows root cause, fixed not masked (the "vacuous" flake, T2606):
# native git resolves a /tmp/... argument against the CURRENT DRIVE root
# (D:\tmp\...), while MSYS bash reads /tmp as %LOCALAPPDATA%\Temp -- so
# `git worktree add` returned 0 and materialized the tree where the script
# never looked. Green on Linux/CI (both agree), red here. When cygpath
# exists the worktree path goes to git as C:/... which git AND bash resolve
# to the same directory; elsewhere the raw path is used unchanged.
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
# Usage:  scripts/test_verify_semantics.sh          (run all tests)
#         scripts/test_verify_semantics.sh --list   (show discovered gates)
#         META_MAX_JOBS=<n> scripts/test_verify_semantics.sh   (cap, default 8)
# Exit 0 if all tests pass; exit 1 with a summary otherwise; exit 2 if a
# precondition (gate discovery or the T0 parallel harness) is broken.

set -uo pipefail
cd "$(dirname "$0")/.."

REPO_ROOT="$PWD"
VERIFY_SRC="$REPO_ROOT/scripts/verify.sh"
META_MAX_JOBS="${META_MAX_JOBS:-8}"

if [ ! -f "$VERIFY_SRC" ]; then
    echo "❌ $VERIFY_SRC not found -- run from the repo root."
    exit 2
fi

# ── Gate classification: the ONE place mapping gate -> blocking semantics ──
# Gates wired in verify.sh --docs that must NEVER flip FAILED (report-only).
# check_readme_claims left verify.sh for CI (continue-on-error step in the
# docs job) on 2026-11-03, TL order T2590 punto 3: ~146 s per push for a
# non-blocking notice. It is NOT listed here while unwired from verify.sh --
# this list is only for gates the T1-T4 sandbox can exercise; the
# zero-orphan invariant (T6) still guards it via its CI wiring. If it ever
# returns to verify.sh, re-add it here FIRST (so T2 covers it from birth)
# and note the pre-push cost that got it moved.
REPORT_ONLY_GATES="check_no_predation check_dead_code_tests check_live_kernel_drift check_dead_config check_supply_chain"
# All other discovered gates are exercised as blocking by T3.

# ── Discover gates dynamically from verify.sh's own body ──
mapfile -t all_gates < <(grep -oE '(check_[a-z_]+|no-absolute-paths)\.sh' "$VERIFY_SRC" | sed -E 's/\.sh$//' | sort -u)
if [ "${#all_gates[@]}" -eq 0 ]; then
    echo "❌ No check_*.sh gates found inside verify.sh -- wiring changed?"
    exit 2
fi

# ── Gate coverage helper (MD-9): checks for unwired/orphan gate scripts ──
# EXEMPT_GATE_HELPERS lists tools that are not run as pre-commit/CI gates
# directly but serve as standalone developer/build utilities with documented rationale.
# verify_restart.sh: operator tool run by hand after a kernel restart (T-DSH-60); not a gate.
EXEMPT_GATE_HELPERS="verify_portable_guild.py verify_restart.sh"

# ONE awk pass over every wiring source (verify.sh + CI workflows + scripts/*.sh)
# replacing the old grep-per-candidate-per-source loop (~75 spawns). Semantics
# preserved exactly: same candidate globs and exclusions, same exempt list, and
# the old self-file exclusion reproduced by tracking the current source's
# basename at FNR==1 -- a candidate is NEVER considered wired by its own file.
find_orphan_gates() {
    local root="$1"
    local candidates=() f bname
    for f in "$root"/scripts/check_*.sh "$root"/scripts/check_*.py \
             "$root"/scripts/verify_*.py "$root"/scripts/verify_*.sh \
             "$root"/scripts/no-absolute-paths.sh; do
        [ -e "$f" ] || continue
        bname="$(basename "$f")"
        [ "$bname" = "verify.sh" ] && continue
        [ "$bname" = "test_verify_semantics.sh" ] && continue
        candidates+=("$bname")
    done

    local inputs=()
    [ -f "$root/scripts/verify.sh" ] && inputs+=("$root/scripts/verify.sh")
    for f in "$root"/.github/workflows/*.yml "$root"/.github/workflows/*.yaml; do
        [ -f "$f" ] && inputs+=("$f")
    done
    for f in "$root"/scripts/*.sh; do
        [ -f "$f" ] && inputs+=("$f")
    done
    [ "${#inputs[@]}" -gt 0 ] || return 0

    awk -v cands="${candidates[*]}" -v exempt=" $EXEMPT_GATE_HELPERS " '
        BEGIN {
            n = split(cands, C, " ")
            for (i = 1; i <= n; i++) { need[C[i]] = 1; hit[C[i]] = 0 }
        }
        FNR == 1 { src = FILENAME; sub(/.*[\/\\]/, "", src) }
        {
            for (c in need)
                if (src != c && hit[c] == 0 && index($0, c) > 0) hit[c] = 1
        }
        END {
            out = ""; miss = 0
            for (i = 1; i <= n; i++) {
                c = C[i]
                if (index(exempt, " " c " ") > 0) continue
                if (hit[c] == 0) { out = out (out == "" ? "" : " ") c; miss = 1 }
            }
            print out
            exit miss
        }
    ' "${inputs[@]}"
}

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
    # check_async_guild_io.py is invoked directly by its .py filename
    # (scripts/check_async_guild_io.py), not via a "check_*.sh" name, so the
    # `all_gates` auto-discovery above (which only matches "check_*.sh")
    # never finds or stubs it -- real bug caught live 2026-09-22: T1 ("all
    # gates green -> exit 0") failed in CI because this gate is real and
    # blocking (fail()) in the sandboxed verify.sh, but the file it needs
    # doesn't exist there. This gate isn't part of what T1-T5 test (they
    # validate OTHER gates' wiring), so it always gets a neutral pass stub.
    printf '#!/usr/bin/env python3\nimport sys\nsys.exit(0)\n' > "$sbx/scripts/check_async_guild_io.py"
    # Same situation for check_doc_links.py (2026-10-14): a blocking .py gate
    # that the "check_*.sh" discovery never stubs; without this T1-T5 fail
    # because the sandbox has no such file. Neutral pass stub, same reason.
    printf '#!/usr/bin/env python3\nimport sys\nsys.exit(0)\n' > "$sbx/scripts/check_doc_links.py"
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

# ── Parallel launch / sequential judge (T2610) ────────────────────────────
# Every T1-T4 case: sandbox prepared synchronously (cheap cp), then its
# stub-build + verify.sh run in the background writing $RES/<id>.out and
# $RES/<id>.rc. Judging happens SEQUENTIALLY afterwards with the exact same
# conditions and messages as the serial version. META_MAX_JOBS bounds
# concurrency so ~15 sandboxes x ~12 stubs don't stampede the OS into
# inflating every number (the fork tax is the whole reason for parallelism).
SBASHOME="$(mktemp -d)"
RES="$SBASHOME/res"
mkdir -p "$RES"
OPEN_JOBS=0

launch_case() {
    local id="$1" sbx="$2" tag="$3"
    {
        build_gate_stubs "$sbx" "$tag"
        ( cd "$sbx" && exec bash scripts/verify.sh --docs ) > "$RES/$id.out" 2>&1
        echo $? > "$RES/$id.rc"
    } &
    OPEN_JOBS=$((OPEN_JOBS + 1))
    if [ "$OPEN_JOBS" -ge "$META_MAX_JOBS" ]; then
        wait -n 2>/dev/null || true
        OPEN_JOBS=$((OPEN_JOBS - 1))
    fi
}

get_rc()  { cat "$RES/$1.rc"  2>/dev/null || echo 99; }
get_out() { cat "$RES/$1.out" 2>/dev/null || true; }

# ── T0: parallel-harness self-test (hermetic proof the collector is honest) ──
# Three planted jobs through the SAME collect layer (backgrounded braces +
# out/rc files + wait + get_rc/get_out): two passing, one failing hidden in
# the middle. All three verdicts must come back intact; a swallowed failure
# makes every green T1-T7 below meaningless, so exit 2 instead.
t "T0: parallel-harness self-test -- planted failure MUST survive the wait/collect layer"
launch_fake() {
    local id="$1" should_fail="$2"
    {
        ( if [ "$should_fail" = "1" ]; then echo "planted-boom"; exit 1; else echo "planted-ok"; exit 0; fi ) > "$RES/$id.out" 2>&1
        echo $? > "$RES/$id.rc"
    } &
}
launch_fake st_pass_a 0
launch_fake st_fail_b 1
launch_fake st_pass_c 0
wait
rc_a=$(get_rc st_pass_a); rc_b=$(get_rc st_fail_b); rc_c=$(get_rc st_pass_c)
out_b=$(get_out st_fail_b)
if [ "$rc_a" = "0" ] && [ "$rc_b" = "1" ] && [ "$rc_c" = "0" ] && echo "$out_b" | grep -q "planted-boom"; then
    okt "3/3 planted verdicts collected exactly (the one failure surfaced, none swallowed)"
else
    badt "aggregator lost a verdict: rc_a=$rc_a rc_b=$rc_b rc_c=$rc_c out_b=[$out_b]"
    echo "❌ parallel harness itself is broken -- refusing to trust T1-T7."
    rm -rf "$SBASHOME"
    exit 2
fi

# ── Spec collection (T1-T4): prepare sandboxes, launch concurrently, judge ──
# spec entries: "id|kind|gate"
SPECS=()
SPECS+=("t1|T1|")
for g in $REPORT_ONLY_GATES; do
    if ! is_wired "$g"; then
        badt "REPORT_ONLY_GATES lists '$g' but verify.sh does not wire it"
        continue
    fi
    SPECS+=("t2_$g|T2|$g")
done
blocking_found=0
for g in "${all_gates[@]}"; do
    if is_report_only "$g"; then continue; fi
    blocking_found=1
    SPECS+=("t3_$g|T3|$g")
done
if [ "$blocking_found" -eq 0 ]; then
    badt "no blocking gates found -- REPORT_ONLY_GATES may be over-broad"
fi
g4="${REPORT_ONLY_GATES%% *}"
prep_t4=0
if is_wired "$g4"; then
    SPECS+=("t4|T4|$g4")
else
    badt "no wired report-only gate to use for T4"
fi

for spec in "${SPECS[@]}"; do
    id="${spec%%|*}"; rest="${spec#*|}"; kind="${rest%%|*}"
    sbx="$SBASHOME/$id"
    make_sandbox "$sbx"
    if [ "$kind" = "T4" ] && ! splice_broken_wiring "$sbx/scripts/verify.sh" "$g4"; then
        prep_t4=1
    fi
done

OPEN_JOBS=0
for spec in "${SPECS[@]}"; do
    id="${spec%%|*}"; rest="${spec#*|}"; kind="${rest%%|*}"; gate="${rest#*|}"
    tag="clean"; [ "$kind" != "T1" ] && tag="$gate-fails"
    launch_case "$id" "$SBASHOME/$id" "$tag"
done
wait

for spec in "${SPECS[@]}"; do
    id="${spec%%|*}"; rest="${spec#*|}"; kind="${rest%%|*}"; gate="${rest#*|}"
    RUN_RC=$(get_rc "$id"); RUN_OUT=$(get_out "$id")
    case "$kind" in
        T1)
            t "T1: all gates green -> --docs exits 0"
            if [ "$RUN_RC" -eq 0 ]; then
                okt "exit 0"
            else
                badt "expected 0, got $RUN_RC"; echo "$RUN_OUT" | tail -5
            fi
            ;;
        T2)
            t "T2: report-only gate '$gate' fails (exit 1) -> --docs STILL exits 0"
            if [ "$RUN_RC" -eq 0 ] && echo "$RUN_OUT" | grep -q "$gate"; then
                okt "exit 0, finding printed (report-only honored)"
            else
                badt "exit $RUN_RC -- report-only semantics broken (the 49950c2 bug class)"
                echo "$RUN_OUT" | tail -8
            fi
            ;;
        T3)
            t "T3: blocking gate '$gate' fails (exit 1) -> --docs exits 1"
            if [ "$RUN_RC" -eq 1 ]; then
                okt "exit 1 (real drift still blocks)"
            else
                badt "expected 1, got $RUN_RC"
                echo "$RUN_OUT" | tail -5
            fi
            ;;
        T4)
            t "T4: negative control -- report-only gate re-wired through fail() flips the exit"
            if [ "$prep_t4" -ne 0 ]; then
                badt "could not splice the broken wiring -- T4 control is inconclusive"
            elif [ "$RUN_RC" -eq 1 ]; then
                okt "broken wiring flips exit to 1 -> T2's criterion is load-bearing"
            else
                badt "broken wiring did NOT flip exit -- T2 would pass even against the 49950c2 bug"
                echo "$RUN_OUT" | tail -8
            fi
            ;;
    esac
done

# ── T5: REAL doc drift must still block ──────────────────────────────────
# Runs the REAL check_head_sync.sh against a corrupted STATUS.md, inside a
# disposable git worktree (shares objects; refs and the real checkout are
# untouched). The heavy/unrelated gates are stubbed so the only variable is
# the drift itself. Sequential (touches git); unaffected by META_MAX_JOBS.
# GITWT: the C:/... form of WT when cygpath exists (Windows) so native git
# and MSYS bash materialize and READ THE SAME DIRECTORY -- T2606 flake fix.
t "T5: real doc drift (bogus HEAD citation, real check_head_sync.sh) -> --docs exits 1"
WT="$(mktemp -d)/wt5"
GITWT="$WT"
if command -v cygpath >/dev/null 2>&1; then
    GITWT="$(cygpath -m "$WT")"
fi
if git worktree add --detach "$GITWT" HEAD >/dev/null 2>&1 && [ -d "$GITWT/scripts" ]; then
    cp "$REPO_ROOT/STATUS.md" "$GITWT/STATUS.md"
    sed -i -E 's/`[0-9a-f]{7,40}`/`deadbee`/g' "$GITWT/STATUS.md"
    if grep -q 'deadbee' "$GITWT/STATUS.md"; then
        # Stub the cargo-heavy gate and the report-only gate; keep the real
        # check_head_sync.sh so the drift is caught by production code.
        printf '#!/usr/bin/env bash\nexit 0\n' > "$GITWT/scripts/check_test_count.sh"
        printf '#!/usr/bin/env bash\nexit 0\n' > "$GITWT/scripts/check_no_predation.sh"
        RUN_OUT=$(cd "$GITWT" && bash scripts/verify.sh --docs 2>&1); RUN_RC=$?
        if [ "$RUN_RC" -eq 1 ] && echo "$RUN_OUT" | grep -qi "head"; then
            okt "exit 1, caught by the real gate (drift still blocks)"
        else
            badt "expected exit 1 naming the HEAD check, got $RUN_RC"
            echo "$RUN_OUT" | tail -8
        fi
    else
        badt "sandbox STATUS.md had no HEAD citation to corrupt -- T5 is vacuous"
    fi
    git worktree remove --force "$GITWT" >/dev/null 2>&1
    git worktree prune >/dev/null 2>&1
else
    badt "could not create a disposable worktree -- T5 skipped as failed"
fi

# ── T6: Gate coverage & zero-orphan invariant (MD-9) ────────────────────
# Every check_*.sh, check_*.py, verify_*.py, verify_*.sh, and no-absolute-paths.sh
# script present in scripts/ must be wired into verify.sh, CI, or a caller gate.
t "T6: gate coverage & zero-orphan invariant (MD-9)"
if uninvoked=$(find_orphan_gates "$REPO_ROOT"); then
    okt "all check_*/verify_* scripts on disk are wired in verify.sh, CI, or caller gates"
else
    badt "orphan verification script(s) found on disk with no caller or CI wiring: $uninvoked"
fi

# ── T7: negative control for gate coverage ──────────────────────────────
# An uninvoked gate placed into a sandbox MUST fail the coverage check.
t "T7: negative control -- synthetic orphan gate in sandbox MUST fail coverage check"
SBX_T7="$SBASHOME/t7_neg"
mkdir -p "$SBX_T7/scripts" "$SBX_T7/.github/workflows"
cp "$VERIFY_SRC" "$SBX_T7/scripts/verify.sh"
cp "$REPO_ROOT/.github/workflows/ci.yml" "$SBX_T7/.github/workflows/ci.yml"
touch "$SBX_T7/scripts/check_orphan_test_dummy.sh"
if uninvoked_neg=$(find_orphan_gates "$SBX_T7"); then
    badt "synthetic orphan gate was NOT detected -- coverage check is vacuous"
else
    if echo "$uninvoked_neg" | grep -q "check_orphan_test_dummy.sh"; then
        okt "synthetic orphan gate detected as expected -> T6 criterion is load-bearing"
    else
        badt "coverage check failed but did not name check_orphan_test_dummy.sh: $uninvoked_neg"
    fi
fi

rm -rf "$SBASHOME"

echo
if [ "$failures" -eq 0 ]; then
    echo "✅ verify.sh semantics hold: report-only stays report-only, real drift still blocks, zero orphan gates."
    exit 0
else
    echo "❌ $failures semantic test(s) failed -- see above. The wiring between"
    echo "   verify.sh and the gates has drifted from the ADR-013 contract."
    exit 1
fi
