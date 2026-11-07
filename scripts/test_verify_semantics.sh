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
#   T8  integration test coverage (MD-10, TL decision T2620-C, claimed T2629;
#       H4/M4/M8 addendum per TL T2682): every binary tests/*.rs under the
#       kernel must be wired (an EXACT stem token after --test/--test= in
#       verify.sh or a CI chain -- substring citations and comment lines do
#       NOT count, and schedule-gated jobs like the M9 weekly ignore runner
#       do NOT count as chain wiring), marked manual via a REASONED ignore
#       (#[ignore = "..."] or #[ignore] // why -- M8: a bare #[ignore] hides
#       a test for no stated cause and no longer qualifies new files), or
#       grandfathered in scripts/integration_tests_baseline.txt (the known
#       orphans at base 2016fa85, debt owned by the (B) wiring project).
#       M4 reverses the check on the manifest: every entry must still exist
#       as a file and must still be an orphan -- wired entries leave stale
#       lines that MUST be deleted (the manifest may only shrink).
#       A NEW file satisfying none of those is red.
#   T9  negative control for T8: a planted unwired binary MUST fire the rule;
#       planted #[ignore]-with-reason and baseline-grandfathered variants MUST
#       NOT; a planted PREFIX citation (--test t9_prefix, no such binary) MUST
#       NOT silence the real t9_prefix_child -- proves H4 load-bearing.
#   T10 build-inputs gate (T-HER-NEW-1, TL T2673): scripts/check_build_inputs.sh
#       is green on the real tree, and mutations (Dockerfile COPY line dropped,
#       .dockerignore excluding an embedded asset) flip it red -- the mechanism
#       that push 266's Docker Smoke proved was missing.
#
# Hermetic by design: T0-T4 never touch git or cargo (all gates stubbed);
# T5 runs inside a disposable git worktree that is removed afterwards. The
# real checkout, its refs and its working tree are never modified; T6-T9 inspect
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

# ── Integration test coverage (MD-10, TL decision T2620-C, hermes claim T2629) ──
# Every binary tests/*.rs under crates/tylluan-kernel/tests must be:
#   wired   -- its stem cited in verify.sh or any CI workflow (the future
#              integration-tests job will cite --tests per binary; substring
#              inclusion has the same mild leniency as MD-9 gate coverage), or
#   manual  -- the file itself contains #[ignore (TL's literal criterion), or
#   grandfathered -- listed in scripts/integration_tests_baseline.txt (the
#              98 orphans existing at base 2016fa85; the manifest shrinks as
#              the (B) project wires them and must NEVER grow for new files).
# One awk pass, no per-file grep. A stem is never wired by its OWN source file
# (a test mentioning its own name does not wire itself).
INTEGRATION_BASELINE_REL="scripts/integration_tests_baseline.txt"

find_orphan_integration_tests() {
    local root="$1"
    local stems=() f
    for f in "$root"/crates/tylluan-kernel/tests/*.rs; do
        [ -e "$f" ] || continue
        stems+=("$(basename "$f" .rs)")
    done
    [ "${#stems[@]}" -gt 0 ] || return 0

    local inputs=()
    [ -f "$root/scripts/verify.sh" ] && inputs+=("$root/scripts/verify.sh")
    for f in "$root"/.github/workflows/*.yml "$root"/.github/workflows/*.yaml; do
        [ -f "$f" ] && inputs+=("$f")
    done
    for f in "$root"/crates/tylluan-kernel/tests/*.rs; do
        [ -f "$f" ] && inputs+=("$f")
    done

    local bl=""
    # one entry per LINE now: "<stem>  <verde|vacuo|rojo>  <evidence>" --
    # the parser takes the FIRST token as the stem and requires a status
    # token (T2734-3: the manifest may never again be a mute exemption list).
    [ -f "$root/$INTEGRATION_BASELINE_REL" ] && bl="$(grep -v '^#' "$root/$INTEGRATION_BASELINE_REL")"

    awk -v cands="${stems[*]}" -v baseline="$bl" '
        BEGIN {
            n = split(cands, C, " ")
            for (i = 1; i <= n; i++) { need[C[i]] = 1; wired[C[i]] = 0; ign[C[i]] = 0 }
            m = split(baseline, B, "\n")
            for (i = 1; i <= m; i++) {
                split(B[i], P, /[ \t]+/)
                if (P[1] == "") continue
                inbase[P[1]] = 1
                bstat[P[1]] = P[2]
                bs[i] = P[1]
            }
        }
        FNR == 1 {
            src = FILENAME; sub(/.*[\/\\]/, "", src)
            stem = src; sub(/\.rs$/, "", stem)
            is_testsrc = (src ~ /\.rs$/)
            sched_job = 0
        }
        {
            if (is_testsrc) {
                # M8 (TL T2682): only a REASONED ignore counts as manual:
                #   #[ignore = "..."]  or  #[ignore] // reason
                # A bare #[ignore] is hiding a test with no stated cause and
                # buys no coverage (grandfathered baseline entries are still
                # tolerated as debt; new bare ignores are red until explained).
                if ((stem in need) && ign[stem] == 0 &&
                    ($0 ~ /#\[ignore[ \t]*=/ || $0 ~ /#\[ignore\][ \t]*\/\//)) ign[stem] = 1
            } else {
                # Schedule-gated CI jobs (the M9 weekly ignore runner) are NOT
                # chain wiring: they exercise only the ignored half of a file
                # and only once a week. Track job boundaries to turn this off.
                if ($0 ~ /^  [A-Za-z0-9_-]+:[ \t]*$/) sched_job = 0
                if ($0 ~ /if:[ \t]*github\.event_name[ \t]*==[ \t]*.schedule./) sched_job = 1
                if (sched_job) next
                # H4 (Buffy proved b20_auth silenced every b20_auth_* stem):
                # wiring is an EXACT token after --test/--test=, never a
                # substring, and comment lines never wire anything.
                if ($0 ~ /^[ \t]*#/) next
                line = $0
                while (match(line, /--test[= ]+[A-Za-z0-9_]+/)) {
                    tok = substr(line, RSTART, RLENGTH)
                    sub(/--test[= ]+/, "", tok)
                    if (tok in need) wired[tok] = 1
                    line = substr(line, RSTART + RLENGTH)
                }
            }
        }
        END {
            out = ""; miss = 0
            for (i = 1; i <= n; i++) {
                c = C[i]
                if (wired[c] == 1 || ign[c] == 1 || (c in inbase)) continue
                out = out (out == "" ? "" : " ") c; miss = 1
            }
            # M4 (TL T2682): manifest hygiene, both directions.
            for (i = 1; i <= m; i++) {
                c = bs[i]
                if (c == "") continue
                if (!(c in need)) { out = out (out == "" ? "" : " ") "[stale-no-file]" c; miss = 1 }
                else if (wired[c] == 1) { out = out (out == "" ? "" : " ") "[paid-off-remove-line]" c; miss = 1 }
                else if (bstat[c] != "verde" && bstat[c] != "vacuo" && bstat[c] != "rojo") {
                    out = out (out == "" ? "" : " ") "[no-status]" c; miss = 1
                }
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
    # And again for check_cfg_deadcode.py (T-HER-3, TL T2944, 2026-11-06):
    # blocking .py gate in the pre-push chain; the sandbox has no crates/ tree
    # for it to scan, so it gets the same neutral pass stub. Its own semantics
    # are tested by T11 below against real fixtures.
    printf '#!/usr/bin/env python3\nimport sys\nsys.exit(0)\n' > "$sbx/scripts/check_cfg_deadcode.py"
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

# ── T8: Integration test coverage invariant (MD-10, T2620-C) ────────────
# No binary tests/*.rs may exist that (a) no chain runs (uncited in verify.sh
# and CI), (b) is not manual (no #[ignore in the file), and (c) is not
# grandfathered in the baseline manifest. The 98 known orphans at base
# 2016fa85 are in the manifest -- they belong to the (B) wiring project.
t "T8: every tests/ binary is wired, #[ignore]-manual, or baseline-grandfathered"
if uninvoked_it=$(find_orphan_integration_tests "$REPO_ROOT"); then
    okt "no integration test binary hides from every chain outside the grandfather manifest"
else
    badt "integration test binary(ies) run by NO chain, not ignored, not grandfathered: $uninvoked_it"
fi

# ── T9: negative control for T8 ─────────────────────────────────────────
# Four planted binaries: unwired (MUST fire), reasoned-#[ignore] (must NOT),
# baseline (must NOT), and a PREFIX pair (M3/H4): a fake --test t9_prefix
# citation of a NONEXISTENT binary must NOT silence the real t9_prefix_child.
# Proves T8 is load-bearing without being a hair trigger.
t "T9: negative control -- planted unwired + prefix-child MUST fire; ignored/grandfathered MUST NOT"
SBX_T9="$SBASHOME/t9_neg"
mkdir -p "$SBX_T9/scripts" "$SBX_T9/.github/workflows" "$SBX_T9/crates/tylluan-kernel/tests"
cp "$VERIFY_SRC" "$SBX_T9/scripts/verify.sh"
cp "$REPO_ROOT/.github/workflows/ci.yml" "$SBX_T9/.github/workflows/ci.yml"
printf '#[test]\nfn t_unwired() { assert!(false); }\n' > "$SBX_T9/crates/tylluan-kernel/tests/t9_planted_unwired.rs"
printf '#[ignore = "meta-test fixture: manual by design"]\n#[test]\nfn t_manual() {}\n' > "$SBX_T9/crates/tylluan-kernel/tests/t9_planted_ignored.rs"
printf '#[test]\nfn t_manifested() {}\n' > "$SBX_T9/crates/tylluan-kernel/tests/t9_planted_manifested.rs"
printf '#[test]\nfn t_child() {}\n' > "$SBX_T9/crates/tylluan-kernel/tests/t9_prefix_child.rs"
printf '          cargo test -p tylluan-kernel --test t9_prefix\n' >> "$SBX_T9/.github/workflows/ci.yml"
printf '# grandfather fixture for T9\nt9_planted_manifested  vacuo  0/0\n' > "$SBX_T9/$INTEGRATION_BASELINE_REL"
if uninvoked_it_neg=$(find_orphan_integration_tests "$SBX_T9"); then
    badt "planted unwired test binary was NOT detected -- T8 rule is vacuous"
else
    fired_ok=0
    echo "$uninvoked_it_neg" | grep -q "t9_planted_unwired" && \
    echo "$uninvoked_it_neg" | grep -q "t9_prefix_child" && fired_ok=1
    false_hits=""
    for unexpected in t9_planted_ignored t9_planted_manifested; do
        if echo "$uninvoked_it_neg" | grep -q "$unexpected"; then false_hits="$false_hits $unexpected"; fi
    done
    if [ "$fired_ok" -eq 1 ] && [ -z "$false_hits" ]; then
        okt "unwired + prefix-child fired; ignored/grandfathered/silent-prefix-token behaved -> T8 load-bearing, H4 closes the prefix loophole"
    else
        badt "T9 mismatch: fired=[$uninvoked_it_neg] expected t9_planted_unwired+t9_prefix_child (false_positives=[$false_hits])"
    fi
fi

# ── T10: build-inputs gate (T-HER-NEW-1, TL T2673) ──────────────────────
# The gate is green on the real tree; dropping the Dockerfile's COPY assets
# line, or letting .dockerignore exclude the embedded anchor seed, MUST flip
# it red -- exactly the push-266 Docker Smoke failure class, now caught before
# any push instead of inside the image build.
t "T10: check_build_inputs green on real tree; COPY/dockerignore mutations MUST flip it red"
if bash scripts/check_build_inputs.sh >/dev/null 2>&1; then
    okt "check_build_inputs.sh: every escaping include_str!/include_bytes! resolves to a tracked, docker-context, COPYed file"
else
    badt "check_build_inputs.sh is RED on the real tree -- run it and fix the inputs it names"
fi
T10_DIR="$SBASHOME/t10"
mkdir -p "$T10_DIR"
if grep -q '^COPY assets/' Dockerfile; then
    grep -v '^COPY assets/' Dockerfile > "$T10_DIR/Dockerfile.drop_copy"
    out_mut=$(bash scripts/check_build_inputs.sh --dockerfile "$T10_DIR/Dockerfile.drop_copy" 2>&1); rc_mut=$?
    if [ "$rc_mut" -eq 0 ] || ! echo "$out_mut" | grep -q 'routing_anchors_seed.jsonl'; then
        badt "mutation 'DROP COPY assets/' did NOT flip the gate on the anchor seed -- T10 is vacuous"
    else
        okt "dropping the Dockerfile COPY line flips the gate red on the exact file (assets/routing_anchors_seed.jsonl)"
    fi
else
    badt "Dockerfile no longer has a 'COPY assets/' line -- T10 mutation fixture stale, update this test"
fi
printf 'assets/\n' > "$T10_DIR/dockerignore.excludes"
out_mut2=$(bash scripts/check_build_inputs.sh --dockerignore "$T10_DIR/dockerignore.excludes" 2>&1); rc_mut2=$?
if [ "$rc_mut2" -eq 0 ] || ! echo "$out_mut2" | grep -q 'EXCLUDED'; then
    badt "mutation '.dockerignore excludes assets/' did NOT flip the gate -- T10 dockerignore arm vacuous"
else
    okt "excluding assets in .dockerignore flips the gate red (EXCLUDED)"
fi

# --- T11: check_cfg_deadcode.py semantics (TL T2944, T-HER-3, hermes 2026-11-06) ---
# The gate must flag EXACTLY the push-271 class: a top-level fn with no cfg guard
# whose only uses sit in regions impossible on Linux non-test. It must NOT flag
# guarded fns, zero-use fns (compiler's own dead_code arm, deliberate limitation),
# or impl methods like `fn new` (Deep's false positive, T2944).
T11_DIR="$SBASHOME/t11"
mkdir -p "$T11_DIR/crates/fixture/src"
T11_PY=""
for cand in python3 python; do
    if command -v "$cand" >/dev/null 2>&1; then T11_PY="$cand"; break; fi
done
# Windows python no entiende rutas MSYS (/tmp/...); en git-bash local hay cygpath,
# en los runners ubuntu no existe y la ruta POSIX ya es nativa. Medido 2026-11-06:
# sin esta conversión el fixture se creaba pero el script veia "no hay crates/" (rc=0)
# y el brazo rojo del criterio salia falsamente verde-por-ausencia.
T11_ROOT="$T11_DIR"
if command -v cygpath >/dev/null 2>&1; then T11_ROOT="$(cygpath -w "$T11_DIR")"; fi
if [ -z "$T11_PY" ]; then
    badt "T11 needs python (python3 or python) on PATH -- gate untestable"
else
    cat > "$T11_DIR/crates/fixture/src/lib.rs" <<'FIXTURE'
pub(crate) fn solo_bajo_windows() -> u32 {
    7
}

/// ver solo_bajo_windows en doc: NO debe contar como uso fuera (trampa T2944)
#[cfg(target_os = "windows")]
pub fn usa_aqui() {
    let _ = solo_bajo_windows();
}

#[cfg(test)]
mod tests {
    #[test]
    fn usa_en_test() {
        let _ = super::solo_bajo_windows();
    }
}

pub struct Tunel;
impl Tunel {
    pub fn new() -> Tunel { Tunel }
}
FIXTURE
    out11=$( "$T11_PY" scripts/check_cfg_deadcode.py "$T11_ROOT" 2>&1 ); rc11=$?
    if [ "$rc11" -ne 1 ] || ! echo "$out11" | grep -q 'fn solo_bajo_windows'; then
        badt "T11 fixture: unguarded fn used only under cfg(windows)/cfg(test) was NOT flagged (rc=$rc11)"
    elif echo "$out11" | grep -q 'fn new'; then
        badt "T11 fixture: impl method 'fn new' leaked into findings -- the exact T2944 false positive"
    elif [ "$(echo "$out11" | grep -c '^CFG-DEADCODE crates/')" -ne 1 ]; then
        badt "T11 fixture: expected exactly 1 finding, got $(echo "$out11" | grep -c '^CFG-DEADCODE crates/')"
    else
        okt "T11: the push-271 class is flagged, exactly once, without the 'fn new' false positive"
    fi
    # mutacion inversa exigida por el TL: la misma fn CON guarda -> verde
    cat > "$T11_DIR/crates/fixture/src/lib.rs" <<'FIXTURE2'
#[cfg(any(target_os = "windows", test))]
pub(crate) fn solo_bajo_windows() -> u32 {
    7
}

#[cfg(target_os = "windows")]
pub fn usa_aqui() {
    let _ = solo_bajo_windows();
}
FIXTURE2
    out11b=$( "$T11_PY" scripts/check_cfg_deadcode.py "$T11_ROOT" 2>&1 ); rc11b=$?
    if [ "$rc11b" -ne 0 ]; then
        badt "T11 mutation arm: guarded fn still flagged (rc=$rc11b) -- the fix for resumen_sonda would not clear the gate"
    else
        okt "T11: adding #[cfg(any(target_os = \"windows\", test))] clears the gate (mirrors 6daa3906)"
    fi
    # zero-use queda fuera por diseno (lo cubre el compilador): fijar la limitacion
    cat > "$T11_DIR/crates/fixture/src/lib.rs" <<'FIXTURE3'
pub(crate) fn nadie_la_usa_nunca() -> u32 {
    7
}
FIXTURE3
    out11c=$( "$T11_PY" scripts/check_cfg_deadcode.py "$T11_ROOT" 2>&1 ); rc11c=$?
    if [ "$rc11c" -ne 0 ]; then
        badt "T11 zero-use arm: fn with NO occurrences was flagged -- documented limitation violated (that arm belongs to rustc dead_code)"
    else
        okt "T11: zero-use fns stay out of the gate by design (rustc owns that arm)"
    fi
    # brazo 4: fichero declarada bajo `#[cfg(windows)] mod iso;` -> sus fns sin
    # guarda NO son aviso en Linux (el fichero entero no existe ahi)
    cat > "$T11_DIR/crates/fixture/src/lib.rs" <<'FIXTURE4'
#[cfg(target_os = "windows")]
mod iso;
FIXTURE4
    cat > "$T11_DIR/crates/fixture/src/iso.rs" <<'FIXTURE4B'
pub(crate) fn helper_solo_windows() -> u32 {
    7
}

pub fn usa_helper() {
    let _ = helper_solo_windows();
}
FIXTURE4B
    out11d=$( "$T11_PY" scripts/check_cfg_deadcode.py "$T11_ROOT" 2>&1 ); rc11d=$?
    if [ "$rc11d" -ne 0 ]; then
        badt "T11 windows-only-file arm: fn inside a cfg(windows) 'mod iso;' was flagged (false positive -- the file never compiles on Linux)"
    else
        okt "T11: fns in a file declared under a dead cfg(windows) mod stay out of the gate"
    fi
    rm -f "$T11_DIR/crates/fixture/src/iso.rs"
    # --- T11e/T11f (TL T3076, 2026-11-07): falso negativo del wrapper de UNA LINEA ---
    # Causa: la clasificacion usaba 's2 < li <= e2'; para un item de una sola linea
    # s2 == li == e2 y el llamador nunca se consideraba dentro de su propia region.
    # Es la forma mas comun de wrapper en Rust ('pub fn usa() -> u32 { solo() }').
    # Fix: 's2 <= li <= e2'. T11e fija el rojo; T11f que la guarda propia siga verde.
    cat > "$T11_DIR/crates/fixture/src/lib.rs" <<'FIXTURE5'
pub(crate) fn solo_bajo_windows() -> u32 {
    7
}

#[cfg(target_os = "windows")]
pub fn usa_una_linea() -> u32 { solo_bajo_windows() }
FIXTURE5
    out11e=$( "$T11_PY" scripts/check_cfg_deadcode.py "$T11_ROOT" 2>&1 ); rc11e=$?
    if [ "$rc11e" -ne 1 ] || ! echo "$out11e" | grep -q 'fn solo_bajo_windows'; then
        badt "T11e single-line-caller arm: unguarded fn used ONLY by a one-line cfg(windows) caller was NOT flagged (rc=$rc11e) -- the s2<li<=e2 false negative is back"
    elif [ "$(echo "$out11e" | grep -c '^CFG-DEADCODE crates/')" -ne 1 ]; then
        badt "T11e single-line-caller arm: expected exactly 1 finding, got $(echo "$out11e" | grep -c '^CFG-DEADCODE crates/')"
    else
        okt "T11e: a one-line cfg(windows) caller counts as a real use (wrapper class flagged)"
    fi
    cat > "$T11_DIR/crates/fixture/src/lib.rs" <<'FIXTURE6'
#[cfg(any(target_os = "windows", test))]
pub(crate) fn solo_bajo_windows() -> u32 {
    7
}

#[cfg(target_os = "windows")]
pub fn usa_una_linea() -> u32 { solo_bajo_windows() }
FIXTURE6
    out11f=$( "$T11_PY" scripts/check_cfg_deadcode.py "$T11_ROOT" 2>&1 ); rc11f=$?
    if [ "$rc11f" -ne 0 ]; then
        badt "T11f guarded single-line arm: fn with its own cfg guard still flagged despite one-line caller (rc=$rc11f) -- guarded fns must stay out"
    else
        okt "T11f: fn with its own cfg guard stays clean with a one-line caller"
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
