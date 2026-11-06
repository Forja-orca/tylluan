#!/usr/bin/env bash
# scripts/verify.sh — the ONE canonical "is this actually done" check.
#
# Why this exists (2026-08-26): in a single day, three separate agents
# (including Claude) claimed "clippy limpio" / "tests en verde" and were
# wrong, every single time for the same root cause: whoever verified used
# a different toolchain or a narrower command than CI actually runs
# (plain `cargo clippy` instead of `--all-targets`, local default
# toolchain instead of `stable`, `pnpm lint` instead of a frozen-lockfile
# install). Each mismatch reached `main` before anyone caught it by hand.
#
# This script is not a suggestion of what to check — it is the literal
# command set CI runs, with the same toolchain pin, run locally. Any
# agent (human-guided, Claude, DeepSeek, Codebuff, whatever comes next)
# who runs this and gets a clean exit should trust the result exactly as
# much as they'd trust a green CI run, because it's the same commands.
#
# Usage:
#   scripts/verify.sh              # everything (matches CI exactly)
#   scripts/verify.sh --rust       # kernel + tylluan-link only (fast-ish)
#   scripts/verify.sh --dashboard  # dashboard only
#   scripts/verify.sh --docs       # doc-drift gates only (seconds)
#
# Exit code 0 = every selected section passed. Non-zero = read the output,
# something above told you exactly what and where.

set -uo pipefail
cd "$(dirname "$0")/.."

RUN_RUST=1
RUN_DASHBOARD=1
RUN_DOCS=1

if [ $# -gt 0 ]; then
    RUN_RUST=0
    RUN_DASHBOARD=0
    RUN_DOCS=0
    for arg in "$@"; do
        case "$arg" in
            --rust) RUN_RUST=1 ;;
            --dashboard) RUN_DASHBOARD=1 ;;
            --docs) RUN_DOCS=1 ;;
            *) echo "Unknown flag: $arg (expected --rust, --dashboard, --docs)"; exit 2 ;;
        esac
    done
fi

FAILED=0
fail() { echo "❌ $1"; FAILED=1; }
ok()   { echo "✅ $1"; }

# ── Live kernel drift (local-only, informational, NEVER wired to CI) ─────
# Deliberately outside RUN_RUST/RUN_DASHBOARD/RUN_DOCS and always runs: its
# whole point (per its own header) is "check this at the START of a work
# session", the same way you'd run `git status` first. Never fails the push
# -- restarting a stale production kernel is José's call alone (CLAUDE.md),
# not something an agent can self-remediate by re-running verify.sh, so
# blocking on it here would just leave a push stuck for a decision no agent
# in this chain can make. And it is NEVER added to CI: CI starts a fresh
# runner with no live kernel every time, so there is no "stale running
# process" for it to ever observe there -- wiring it to CI would only add a
# permanent no-op step. Wired here 2026-09-27, fifth external audit round.
if bash scripts/check_live_kernel_drift.sh; then
    ok "live kernel drift (informational, local-only)"
else
    echo "⚠️  live kernel drift detected — see above. Informational only, does not block this push (production restart is José's call)."
fi
echo

# ── Rust: kernel + tylluan-link, pinned to 1.99.1 (CI exact toolchain, see rust-toolchain.toml) ──
# Never use the ambient default toolchain here — that mismatch is the
# single most repeated real bug this project has hit. `rustup run 1.99.1`
# is not optional decoration.
if [ "$RUN_RUST" = "1" ]; then
    echo "── Rust (toolchain: 1.99.1, same as CI) ──"
    # T2590 punto 6 (T2603): the three lib-suite gates below tee their output
    # into a fresh temp dir exported as TYLLUAN_TMC_DIR, so the check_test_count
    # gate in the docs section reuses this run's numbers instead of re-running
    # kernel+link+fsrs a second time (122 s measured duplicate, T2589). Fresh
    # mktemp dir per run = no stale reuse ever; trap cleans both exit paths.
    TYLLUAN_TMC_DIR=$(mktemp -d "${TMPDIR:-/tmp}/tylluan-tmc.XXXXXXXX")
    export TYLLUAN_TMC_DIR
    trap 'rm -rf "$TYLLUAN_TMC_DIR"' EXIT
    if command -v rustup >/dev/null 2>&1; then
        RUST_CARGO="rustup run 1.99.1 cargo"
    elif command -v rustup.exe >/dev/null 2>&1; then
        RUST_CARGO="rustup.exe run 1.99.1 cargo"
    elif command -v cargo >/dev/null 2>&1; then
        RUST_CARGO="cargo"
    elif command -v cargo.exe >/dev/null 2>&1; then
        RUST_CARGO="cargo.exe"
    else
        RUST_CARGO="cargo"
    fi

    if $RUST_CARGO check -p tylluan-kernel; then
        ok "cargo check -p tylluan-kernel"
    else
        fail "cargo check -p tylluan-kernel"
    fi

    if $RUST_CARGO test -p tylluan-kernel --lib 2>&1 | tee "$TYLLUAN_TMC_DIR/kernel_lib.log"; then
        ok "cargo test -p tylluan-kernel --lib"
    else
        fail "cargo test -p tylluan-kernel --lib"
    fi

    # --all-targets is load-bearing: it lints tests/benches/examples too.
    # Every false "clippy limpio" today came from someone dropping this flag.
    if $RUST_CARGO clippy -p tylluan-kernel --all-targets -- -D warnings; then
        ok "cargo clippy -p tylluan-kernel --all-targets -D warnings"
    else
        fail "cargo clippy -p tylluan-kernel --all-targets -D warnings"
    fi

    # Pilar 2 deterministic contract tests: production structs (EmbedRequest/
    # EmbedResponse/HealthResponse) validated against schemas/ — no network, no
    # live kernel. Wired in 2026-11-03 (T2590 punto 4, from the hermes pre-push
    # profiling T2589): before this, NOTHING ran contracts_test.rs — neither
    # verify.sh (only --lib/--lib --tests) nor CI's security-audit job. It is
    # the deterministic coverage that the live probe below can never guarantee.
    # F9 workspace broker: deterministic overlap-guard test (fails if globs_overlap is
    # disabled). Wired 2026-11-03: integration tests under tests/ run in NO chain unless named.
    if $RUST_CARGO test -p tylluan-kernel --test workspace_overlap_guard; then
        ok "cargo test -p tylluan-kernel --test workspace_overlap_guard"
    else
        fail "cargo test -p tylluan-kernel --test workspace_overlap_guard -- the F9 overlap guard no longer rejects overlapping claims"
    fi

    if $RUST_CARGO test -p tylluan-kernel --test contracts_test; then
        ok "cargo test -p tylluan-kernel --test contracts_test"
    else
        fail "cargo test -p tylluan-kernel --test contracts_test — schema/struct contract drift, see crates/tylluan-kernel/tests/contracts_test.rs"
    fi

    if $RUST_CARGO test -p tylluan-link --lib --tests 2>&1 | tee "$TYLLUAN_TMC_DIR/link_all.log"; then
        ok "cargo test -p tylluan-link --lib --tests"
    else
        fail "cargo test -p tylluan-link --lib --tests"
    fi

    if $RUST_CARGO clippy -p tylluan-link --lib --tests -- -D warnings; then
        ok "cargo clippy -p tylluan-link --lib --tests -D warnings"
    else
        fail "cargo clippy -p tylluan-link --lib --tests -D warnings"
    fi

    if $RUST_CARGO test -p tylluan-fsrs 2>&1 | tee "$TYLLUAN_TMC_DIR/fsrs_all.log"; then
        ok "cargo test -p tylluan-fsrs"
    else
        fail "cargo test -p tylluan-fsrs"
    fi
    echo
fi

# ── Dashboard: frozen-lockfile install is the check that actually caught
# today's real CI failure. `pnpm lint`/`pnpm test` without it passes
# locally even when package.json and pnpm-lock.yaml have drifted apart —
# CI's default IS --frozen-lockfile, so that drift only shows up there
# unless this script reproduces it first. ──
if [ "$RUN_DASHBOARD" = "1" ]; then
    echo "── Dashboard ──"
    PNPM_CMD="pnpm"
    if ! command -v pnpm >/dev/null 2>&1; then
        if command -v pnpm.cmd >/dev/null 2>&1; then
            PNPM_CMD="pnpm.cmd"
        fi
    elif [[ "${OSTYPE:-}" == "linux"* ]] && grep -qi microsoft /proc/version 2>/dev/null && command -v cmd.exe >/dev/null 2>&1; then
        PNPM_CMD="cmd.exe /c pnpm"
    fi

    if command -v pnpm >/dev/null 2>&1 || command -v pnpm.cmd >/dev/null 2>&1; then
        # CI builds the shared package FIRST (dashboard jobs, step "Build shared
        # package (@tylluan/ui-core)"): dashboard imports it as sources needing
        # their own deps (clsx etc). Without this mirror, pnpm test fails on a
        # CLEAN checkout (unresolved import) — caught by the 2026-11-03 pre-push
        # profiling (T2589/T2590 punto 2); only persisted node_modules were
        # hiding it. npm is deliberate here: ui-core is npm-managed (AGENTS.md
        # rule 3 exception), dashboard stays on pnpm.
        NPM_CMD="npm"
        if ! command -v npm >/dev/null 2>&1; then
            if command -v npm.cmd >/dev/null 2>&1; then NPM_CMD="npm.cmd"; fi
        fi
        # T2603-P3-BEGIN — conditional ui-core install (Deep's change, TL order):
        # unconditional `npm ci` was wrong — it wipes node_modules, needs network
        # and fails against a dev server holding files open. Rule: reuse when
        # node_modules/.package-lock.json (npm's install stamp) is NEWER than the
        # committed package-lock.json (installed from current lock, still on
        # disk); otherwise --prefer-offline ci (works from npm's local cache when
        # the lock matches, only hits network when it doesn't). --no-audit
        # --no-fund: no telemetry round-trips in a pre-push hot path.
        UICORE_DIR="packages/tylluan-ui-core"
        NEED_CI=1
        if [ -d "$UICORE_DIR/node_modules" ] && [ -f "$UICORE_DIR/node_modules/.package-lock.json" ] \
           && [ "$UICORE_DIR/node_modules/.package-lock.json" -nt "$UICORE_DIR/package-lock.json" ]; then
            NEED_CI=0
        fi
        if command -v "$NPM_CMD" >/dev/null 2>&1 || command -v npm.cmd >/dev/null 2>&1; then
            if [ "$NEED_CI" = "1" ]; then
                if (cd "$UICORE_DIR" && $NPM_CMD ci --prefer-offline --no-audit --no-fund && $NPM_CMD run build); then
                    ok "npm ci --prefer-offline + build in $UICORE_DIR (node_modules absent, stale vs lock, or first run)"
                else
                    fail "packages/tylluan-ui-core build failed (or npm ci out of sync with its package-lock) — dashboard lint/test below need this built"
                fi
            else
                if (cd "$UICORE_DIR" && $NPM_CMD run build); then
                    ok "npm run build only in $UICORE_DIR (node_modules installed from current lock — ci skipped on purpose)"
                else
                    fail "packages/tylluan-ui-core build failed — dashboard lint/test below need this built"
                fi
            fi
        else
            echo "⚠️  npm not found on PATH — skipping @tylluan/ui-core build; dashboard tests may fail on a clean checkout (CI runs this step itself)."
        fi
        # T2603-P3-END

        if (cd dashboard && $PNPM_CMD install --frozen-lockfile); then
            ok "pnpm install --frozen-lockfile (package.json matches pnpm-lock.yaml)"
        else
            fail "pnpm install --frozen-lockfile — package.json/pnpm-lock.yaml drifted, run 'pnpm install --no-frozen-lockfile' in dashboard/ and commit the lockfile"
        fi

        if (cd dashboard && $PNPM_CMD run lint); then
            ok "pnpm run lint"
        else
            fail "pnpm run lint"
        fi

        if (cd dashboard && $PNPM_CMD test); then
            ok "pnpm test"
        else
            fail "pnpm test"
        fi
    else
        echo "⚠️  pnpm not found on PATH — skipping dashboard checks (CI will still run them)"
    fi
    echo
fi

# ── Docs: cheap, seconds-fast, and it's the ONLY thing standing between
# STATUS.md/README.md and quietly lying about the project's real state. ──
if [ "$RUN_DOCS" = "1" ]; then
    echo "── Docs drift ──"
    # Doc-path/doc-port/version-consistency gate. Wired 2026-09-27 (fifth
    # external audit round found it existed since the 2026-09-11 cycle but
    # was never cabled in): needed two real fixes first to run clean --
    # (1) it died with awk fatal + exit 2 in a clean clone because line 47
    # read the gitignored tylluan.toml instead of the versioned
    # tylluan.example.toml; (2) 26 false positives from file:line citations
    # mismatched as 4-5-digit ports, and from docs that document a
    # deliberate second/other kernel instance (Docker-secondary, a restore
    # drill, a federation peer) by design -- see the script's own comments.
    if bash scripts/check_docs_reality.sh; then
        ok "doc-path/doc-port/version consistency (check_docs_reality.sh)"
    else
        fail "doc-path/doc-port/version drift — see scripts/check_docs_reality.sh output above"
    fi

    # Agent read-route gate (2026-10-28, Jose): the same read-order table in every
    # agentic markdown file, REVIEW.md always third, CLAUDE.md imports REVIEW.md.
    if bash scripts/check_read_routes.sh; then
        ok "agent read routes consistent, REVIEW.md third (check_read_routes.sh)"
    else
        fail "agent read-route drift -- see scripts/check_read_routes.sh output above"
    fi

    # Build-inputs gate (2026-11-04, TL T2673 T-HER-NEW-1, hermes): every
    # include_str!/include_bytes! leaving its crate must resolve to a git-tracked
    # file that is present in the Docker build context and COPYed by the
    # Dockerfile when production code needs it. Push 266's red Docker Smoke
    # (Dockerfile never copied assets/ behind an include_str!) is the incident
    # this closes; verify.sh never builds the image, so this static gate does.
    if bash scripts/check_build_inputs.sh; then
        ok "build inputs resolvable in clean clones and Docker (check_build_inputs.sh)"
    else
        fail "include_str!/include_bytes! input drift -- see scripts/check_build_inputs.sh output above"
    fi

    # Markdown link gate (2026-10-14 documentation audit): 20 relative links
    # were broken or pointed at one machine's absolute paths (file:///E:/...);
    # check_docs_reality.sh only judges backtick-cited paths.
    DOC_LINK_PY=""
    for cand in python3 python; do
        if command -v "$cand" >/dev/null 2>&1; then DOC_LINK_PY="$cand"; break; fi
    done
    if [ -z "$DOC_LINK_PY" ]; then
        fail "doc link check needs python (python3 or python) on PATH"
    elif "$DOC_LINK_PY" scripts/check_doc_links.py; then
        ok "Markdown relative links resolve (check_doc_links.py)"
    else
        fail "broken Markdown links — see scripts/check_doc_links.py output above"
    fi

    # README claims guardian (H-24/H-25) moved OUT of the pre-push chain
    # 2026-11-03 (T2590 punto 3, TL): it is report-only by design (never flips
    # FAILED — old T2 assertion preserved in CI wiring) but cost ~146 s per
    # push for a notice nothing acted on locally. It stays wired in CI
    # (docs job, continue-on-error step), which is what the meta-test's
    # zero-orphan invariant (T6) requires: wired in verify.sh OR CI. Do NOT
    # re-add it here without re-classifying: scripts/test_verify_semantics.sh
    # REPORT_ONLY_GATES still lists check_readme_claims should it return.

    # Pilar 2 (repo-pillars) contract gate: JSON schemas under schemas/ are
    # valid, and if a kernel happens to be live on :47004 its real endpoints
    # get checked against them too. Degrades gracefully with no live kernel
    # (CI never has one) -- exit 0 either way once schemas are valid. Wired
    # 2026-09-27 after the fifth external audit round found it existed but
    # was never invoked from here or from CI. Lives here (not --rust) so
    # scripts/test_verify_semantics.sh's --docs-only test harness exercises
    # its blocking semantics for real.
    if bash scripts/check_contracts.sh; then
        ok "Pilar 2 contract gate (schemas/ + live endpoints if reachable)"
    else
        fail "Pilar 2 contract gate — see scripts/check_contracts.sh output above"
    fi

    # Dead-code/stale-test heuristic (G3, name-based grep, not type-aware).
    # Report-only by its own design until the suspect list is triaged and
    # stable (see the script's own header) -- do NOT pass --strict here.
    # Wired 2026-09-27, same audit round as above; non-blocking on purpose,
    # matching check_no_predation.sh's pattern below. Same reason as
    # check_contracts.sh above for living here, not --rust.
    if bash scripts/check_dead_code_tests.sh; then
        ok "dead-code/stale-test heuristic (G3, report-only)"
    else
        echo "⚠️  dead-code/stale-test heuristic (G3) found suspects — see above. Report-only, does not block this push."
    fi

    # Dead-config heuristic (report-only, matches CI dead-config-report).
    if bash scripts/check_dead_config.sh; then
        ok "dead config fields heuristic (check_dead_config.sh, report-only)"
    else
        echo "⚠️  dead config fields heuristic found suspects — see above. Report-only, does not block this push."
    fi

    # Supply-chain status table (report-only; T-DSH-56): cargo deny advisories,
    # npm audit, certifi/pip floors. Always exits 0, prints OK/FALLA per row.
    if bash scripts/check_supply_chain.sh; then
        ok "supply-chain status (check_supply_chain.sh, report-only)"
    else
        echo "⚠️  supply-chain status found issues — see above. Report-only, does not block this push."
    fi

    if bash scripts/check_head_sync.sh; then
        ok "STATUS.md HEAD citation"
    else
        fail "STATUS.md HEAD citation — run 'scripts/check_head_sync.sh --fix'"
    fi

    if bash scripts/check_test_count.sh; then
        ok "README/AGENTS test counts"
    else
        fail "README/AGENTS test counts — run 'scripts/check_test_count.sh --fix'"
    fi

    # BOM gate (blocking): the UTF-8-BOM bug class broke CI/builds four
    # separate times (a0a614a, 8c7d0bf, ad96253, 336ed13). check_bom.sh
    # detects it in ~5s; --fix strips exactly 3 bytes per offender.
    if bash scripts/check_bom.sh; then
        ok "no UTF-8 BOM in tracked files"
    else
        fail "UTF-8 BOM detected — run 'scripts/check_bom.sh --fix', review the diff, commit"
    fi

    # Mojibake gate (blocking): distinct bug class from byte-0 BOM above --
    # mid-file double-encoded UTF-8 (PowerShell Set-Content re-encoding, or
    # similar). Recurred a 4th time in Deep's ADR-015 Fase 2 commit (517b77d,
    # ~70 lines of config.rs, caught and repaired before push). No --fix:
    # repair needs the original characters, not a mechanical reverse-decode.
    if bash scripts/check_mojibake.sh; then
        ok "no mid-file mojibake in tracked files"
    else
        fail "Mojibake detected — restore affected lines from a clean prior revision, see scripts/check_mojibake.sh output"
    fi

    # Portability path gate: detects hardcoded machine-specific absolute paths
    # (drive letters, home directories) in relocatable source.
    if bash scripts/no-absolute-paths.sh; then
        ok "no machine-specific absolute paths in portable source (no-absolute-paths.sh)"
    else
        fail "machine-specific absolute paths found — see scripts/no-absolute-paths.sh output"
    fi

    # Async event-loop I/O gate (blocking, ratchet): FastMCP guild servers run
    # every @mcp.tool() on ONE event loop — a sync network call inside an async
    # def stalls ALL concurrent tool calls for its whole timeout (T633: 120s
    # local inference did exactly that; contract bwc-8c0dc35a). The gate runs
    # its own fixture self-test first: it must catch the bug class it exists
    # for. Baseline (ratchet): scripts/async_io_baseline.json lists the 30
    # pre-existing violations found on first run (browser/comfy_ui/n8n_bridge,
    # cleanup decision belongs to the TL) — NEW violations fail the push,
    # known ones warn, stale entries are reported for removal.
    PY_CMD="python"
    if command -v python >/dev/null 2>&1; then
        PY_CMD="python"
    elif command -v python.exe >/dev/null 2>&1; then
        PY_CMD="python.exe"
    elif command -v python3 >/dev/null 2>&1; then
        PY_CMD="python3"
    elif command -v py.exe >/dev/null 2>&1; then
        PY_CMD="py.exe"
    elif [ -f ".venv/Scripts/python.exe" ]; then
        PY_CMD=".venv/Scripts/python.exe"
    elif [ -f ".venv/bin/python" ]; then
        PY_CMD=".venv/bin/python"
    fi
    if command -v "$PY_CMD" >/dev/null 2>&1 || [ -f "$PY_CMD" ]; then
        if "$PY_CMD" scripts/check_async_guild_io.py; then
            ok "guild async I/O gate (no NEW blocking network calls on the MCP event loop)"
        else
            fail "guild async I/O gate — NEW blocking network call reachable from async def without asyncio.to_thread"
        fi
    else
        fail "guild async I/O gate — python/python3 not found (this project's guilds ARE python; install it)"
    fi

    # ADR-013 4th gate (non-predation). Runs in CI too, as the non-blocking
    # job 'no-predation-report' (continue-on-error, same pattern as
    # dead-config-report). Report-only per ADR-013 §2 and the gate's own
    # header ("the gate REPORTS, it never edits, reverts or blocks
    # mechanically") -- deliberately does NOT call fail()/set FAILED, unlike
    # every other check in this section: this one must never turn into a
    # pre-push block. That property is LOCKED by scripts/test_verify_semantics.sh
    # (T2 + T4 negative control) -- re-introducing fail() here fails the
    # meta-test and CI's verify-semantics-test job. Output (findings + exit
    # code) is printed either way for human/agent triage.
    if bash scripts/check_no_predation.sh; then
        ok "non-predation gate (ADR-013)"
    else
        echo "⚠️  non-predation gate (ADR-013) found something — see above. Report-only, does not block this push."
    fi
    echo
fi

if [ "$FAILED" = "0" ]; then
    echo "✅ All selected checks passed. This is what CI will see too."
    exit 0
else
    echo "❌ At least one check failed. Do not commit/push claiming this is verified until it's fixed."
    exit 1
fi
