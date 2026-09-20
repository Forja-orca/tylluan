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

# ── Rust: kernel + tylluan-link, pinned to `stable` (CI's exact toolchain) ──
# Never use the ambient default toolchain here — that mismatch is the
# single most repeated real bug this project has hit. `rustup run stable`
# is not optional decoration.
if [ "$RUN_RUST" = "1" ]; then
    echo "── Rust (toolchain: stable, same as CI) ──"
    if command -v rustup >/dev/null 2>&1; then
        RUST_CARGO="rustup run stable cargo"
    elif command -v rustup.exe >/dev/null 2>&1; then
        RUST_CARGO="rustup.exe run stable cargo"
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

    if $RUST_CARGO test -p tylluan-kernel --lib; then
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

    if $RUST_CARGO test -p tylluan-link --lib --tests; then
        ok "cargo test -p tylluan-link --lib --tests"
    else
        fail "cargo test -p tylluan-link --lib --tests"
    fi

    if $RUST_CARGO clippy -p tylluan-link --lib --tests -- -D warnings; then
        ok "cargo clippy -p tylluan-link --lib --tests -D warnings"
    else
        fail "cargo clippy -p tylluan-link --lib --tests -D warnings"
    fi

    if $RUST_CARGO test -p tylluan-fsrs; then
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
    if bash scripts/check_head_sync.sh; then
        ok "STATUS.md HEAD citation"
    else
        fail "STATUS.md HEAD citation — run 'scripts/check_head_sync.sh --fix'"
    fi

    if bash scripts/check_test_count.sh; then
        ok "README.md test count"
    else
        fail "README.md test count — run 'scripts/check_test_count.sh --fix'"
    fi

    # BOM gate (blocking): the UTF-8-BOM bug class broke CI/builds four
    # separate times (a0a614a, 8c7d0bf, ad96253, 336ed13). check_bom.sh
    # detects it in ~5s; --fix strips exactly 3 bytes per offender.
    if bash scripts/check_bom.sh; then
        ok "no UTF-8 BOM in tracked files"
    else
        fail "UTF-8 BOM detected — run 'scripts/check_bom.sh --fix', review the diff, commit"
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
    if ! command -v python >/dev/null 2>&1; then
        PY_CMD="python3"
    fi
    if command -v "$PY_CMD" >/dev/null 2>&1; then
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
