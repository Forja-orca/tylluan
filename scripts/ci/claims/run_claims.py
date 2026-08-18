#!/usr/bin/env python3
"""Security Claims CI Gate -- runner.

Parses docs/reference/security-claims.toml and executes each claim's check.
Static claims: ripgrep pattern scoped to files, excluding test/comment lines.
Dynamic claims: shell out to a per-claim script against a live kernel.

Exit code 0 if all claims pass, 1 if any claim fails (prints a table either way).
"""
import argparse
import subprocess
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
MANIFEST = REPO_ROOT / "docs" / "reference" / "security-claims.toml"


def load_claims() -> list[dict]:
    with MANIFEST.open("rb") as fh:
        return tomllib.load(fh)["claim"]


def _test_block_line_numbers(file_text: str) -> set[int]:
    """Return the set of 1-indexed line numbers that fall inside a
    `#[cfg(test)]`-attributed item's block (usually `mod tests { ... }`,
    but also covers a `#[cfg(test)]` directly on a single fn/struct/etc).

    I4 (2026-08-18 review fix): the previous exclusion only skipped the
    literal `#[cfg(test)]` attribute line itself -- every line inside the
    actual test block below it still counted as a real match. That made
    "expect=absent" claims scoped over a whole crate (e.g.
    encrypt-at-rest-single-choke-point) pass only by accident, until some
    future test module called the excluded pattern. This is a simple
    brace-depth state machine over the whole file, not a real Rust parser:
    once we see a `#[cfg(test)]` attribute line, we track brace depth from
    the next opening `{` we encounter through to its matching close, and
    mark every line in between (inclusive) as excluded. This is a heuristic
    (doesn't understand strings/comments containing braces) but is good
    enough for this repo's straightforward `mod tests { ... }` style and is
    strictly more correct than the single-line exclusion it replaces.
    """
    # Re-review fix (2026-08-18): the original version only broke its inner
    # loop on `opened and depth <= 0` -- if the attributed item has no
    # braces at all (Rust's `#[cfg(test)] mod tests;`, pointing at a
    # separate file -- a standard idiom, and one that already exists in
    # this repo at crates/tylluan-kernel/src/memory/silva/mod.rs:401-402),
    # `opened` never becomes True and the scan ran to end-of-file, silently
    # excluding every remaining line. Only harmless today because that
    # occurrence happens to be the last two lines of its file. Fixed by
    # capping the "look for an opening brace" phase: if we hit a bare
    # statement terminator (`;`) before ever seeing `{`, the item has no
    # block -- stop there. Also hard-capped at 5 lookahead lines regardless,
    # so no single malformed/unusual attribute can blind the rest of a file.
    MAX_LOOKAHEAD_LINES_FOR_BRACE = 5
    excluded: set[int] = set()
    lines = file_text.splitlines()
    i = 0
    n = len(lines)
    while i < n:
        if "#[cfg(test)]" in lines[i]:
            j = i
            depth = 0
            opened = False
            while j < n and (j - i) < MAX_LOOKAHEAD_LINES_FOR_BRACE:
                line = lines[j]
                for ch in line:
                    if ch == "{":
                        depth += 1
                        opened = True
                    elif ch == "}":
                        depth -= 1
                excluded.add(j + 1)  # 1-indexed
                if opened and depth <= 0:
                    break
                if not opened and line.rstrip().endswith(";"):
                    # Braceless item (e.g. `mod tests;`) -- nothing more to exclude.
                    break
                j += 1
            i = j + 1
            continue
        i += 1
    return excluded


def run_static_claim(claim: dict, repo_root: Path) -> tuple[bool, str]:
    """A static claim's `expect` field controls the polarity:
    - "absent" (default): pattern must NOT appear (outside comments/
      #[cfg(test)] blocks) in any file under `scope`, except `exclude_file`.
      Use for claims like "no LAN bind" -- the pattern is a bad sign.
    - "present": pattern MUST appear at least once (real code, not a
      comment/test) in every file under `scope`. Use for claims like
      "the approval check is wired in" -- absence is the bad sign, and
      "no matches" must NOT be silently treated as compliance."""
    pattern = claim["pattern"]
    scope = claim["scope"]
    exclude_file = claim.get("exclude_file")
    expect = claim.get("expect", "absent")

    # --with-filename is required in addition to --no-heading: ripgrep
    # omits the filename prefix entirely (even with --no-heading) when a
    # scope entry is a single explicit file rather than a directory, which
    # silently corrupts the path:lineno:content split below (found while
    # verifying the I4 fix -- pre-existing bug, not introduced by it).
    args = ["rg", "--line-number", "--with-filename", "--no-heading", pattern] + scope
    try:
        result = subprocess.run(args, cwd=repo_root, capture_output=True, text=True)
    except FileNotFoundError:
        # Real incident (2026-08-18): the CI runner didn't have ripgrep
        # installed, and this crashed with an uncaught traceback instead of
        # a clean FAIL -- fixed the CI job to install it, but also fail
        # cleanly here as defense-in-depth (local runs, other CI providers).
        return False, "ripgrep ('rg') not found on PATH -- required for static claim checks"

    if result.returncode not in (0, 1):
        return False, f"ripgrep error: {result.stderr.strip()}"

    # Pre-compute, per matched file, the set of line numbers that fall
    # inside a #[cfg(test)] block (I4) so we only read/parse each file once.
    test_block_lines_by_file: dict[str, set[int]] = {}

    real_matches = []
    if result.returncode == 0:
        for line in result.stdout.splitlines():
            # format: path:lineno:content
            parts = line.split(":", 2)
            if len(parts) < 3:
                continue
            path, lineno, content = parts
            if exclude_file and path.replace("\\", "/") == exclude_file:
                continue
            stripped = content.strip()
            if stripped.startswith("//") or stripped.startswith("#"):
                continue
            if path not in test_block_lines_by_file:
                try:
                    text = (repo_root / path).read_text(encoding="utf-8")
                except OSError:
                    text = ""
                test_block_lines_by_file[path] = _test_block_line_numbers(text)
            if int(lineno) in test_block_lines_by_file[path]:
                continue
            real_matches.append(f"{path}:{lineno}: {stripped}")

    if expect == "present":
        if real_matches:
            return True, f"found {len(real_matches)} real match(es)"
        return False, "pattern not found in real (non-comment, non-test) code -- expected wiring is missing"

    # expect == "absent" (default)
    if not real_matches:
        return True, "no matches" if result.returncode == 1 else "all matches excluded (comments/tests/exclude_file)"
    return False, "; ".join(real_matches[:5])


# I3 (2026-08-18 review fix): each dynamic script boots a real kernel
# (Rust binary + Python guild subprocesses), not a mock -- cold boot with
# guild startup is real minutes-scale work on CPU-only hardware elsewhere
# in this project's own stack (see ForjaMCPo3's CLAUDE.md: knowledge guild
# alone can take 60-120s under CPU inference). We now run the
# script through the `timeout` coreutil (SIGTERM, not SIGKILL) so bash gets
# a chance to run its EXIT trap and clean up the process group itself,
# with subprocess.run's own timeout kept as a hard backstop slightly above
# the inner one in case `timeout` itself somehow doesn't fire.
#
# REAL FIX (2026-08-19): 90s was based on an assumption that the port-wait
# polling loops these scripts run internally only needed <=15s of headroom
# (an early boot-log line, well before guild subsystems). Live CI proved
# that assumption wrong -- a cold GitHub Actions runner did not bind the
# HTTP listener within 15s at all, only the earliest tracing line ever
# appeared. Each script's own internal polling loop was raised to 120s to
# match this project's documented CPU-bound-startup floor; this outer
# budget must comfortably exceed that, so raised accordingly.
#
# Re-review fix (2026-08-18): that Python-side backstop, on its own, could
# NOT actually clean up the kernel if it ever fired -- subprocess.run's
# timeout= only SIGKILLs its immediate child (the `timeout` coreutil
# process), not bash (its child) or the kernel (bash's child); Python never
# creates a new session/process-group for the subprocess, so proc.kill()
# would leave bash and the kernel's whole process group orphaned -- exactly
# the failure this was meant to prevent, just one layer further out. Fixed
# by adding `--kill-after` to the `timeout` invocation itself: if SIGTERM
# doesn't make bash exit (and run its EXIT trap) within the grace period,
# `timeout` sends SIGKILL directly to the process it's supervising. This
# keeps cleanup ownership inside `timeout`'s own well-tested supervision
# instead of trying to manage process groups from Python, which is more
# correct than teaching subprocess.run to signal a whole tree it never
# owned a handle to.
DYNAMIC_CLAIM_INNER_TIMEOUT_SECS = 180
DYNAMIC_CLAIM_KILL_AFTER_SECS = 10
DYNAMIC_CLAIM_OUTER_TIMEOUT_SECS = DYNAMIC_CLAIM_INNER_TIMEOUT_SECS + DYNAMIC_CLAIM_KILL_AFTER_SECS + 15


def run_dynamic_claim(claim: dict, repo_root: Path) -> tuple[bool, str]:
    script = repo_root / claim["script"]
    args = [
        "timeout",
        "--signal=TERM",
        f"--kill-after={DYNAMIC_CLAIM_KILL_AFTER_SECS}",
        str(DYNAMIC_CLAIM_INNER_TIMEOUT_SECS),
        "bash",
        str(script),
    ]
    try:
        result = subprocess.run(
            args,
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=DYNAMIC_CLAIM_OUTER_TIMEOUT_SECS,
        )
    except subprocess.TimeoutExpired:
        return False, f"timed out after {DYNAMIC_CLAIM_OUTER_TIMEOUT_SECS}s (outer backstop -- inner `timeout` did not clean up in time)"

    if result.returncode == 124:
        # GNU `timeout`'s own exit code for "the command was terminated
        # because the time limit was reached" (SIGTERM sent, bash's EXIT
        # trap should have run and cleaned up).
        return False, f"timed out after {DYNAMIC_CLAIM_INNER_TIMEOUT_SECS}s (SIGTERM)"
    if result.returncode == 137:
        # 128+SIGKILL: SIGTERM didn't make it exit within --kill-after, so
        # `timeout` force-killed it -- cleanup may not have run (trap
        # doesn't fire on SIGKILL), flagged distinctly so this is visible.
        return False, f"timed out and had to be force-killed after {DYNAMIC_CLAIM_INNER_TIMEOUT_SECS}+{DYNAMIC_CLAIM_KILL_AFTER_SECS}s -- cleanup may not have run, check for orphaned processes"
    if result.returncode == 0:
        return True, result.stdout.strip().splitlines()[-1] if result.stdout.strip() else "ok"
    # REAL BUG (2026-08-19): this used to cap FAIL detail at 300 chars, which
    # silently ate the failing scripts' `cat "$CONFIG_DIR/kernel.log"` dump --
    # exactly the data needed to diagnose a startup hang. Live CI output
    # looked like the kernel only ever printed one boot line, when in fact
    # that was just where the 300-char cutoff landed; the real log could have
    # gone much further. Raised to 6000 chars (a real kernel.log dump on a
    # boot failure runs a few KB at most) so a FAIL always carries enough of
    # the real log to diagnose from the printed table alone.
    return False, (result.stdout.strip() + " " + result.stderr.strip()).strip()[:20000]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--static-only", action="store_true", help="Skip dynamic claims (before dynamic scripts exist)")
    args = parser.parse_args()

    claims = load_claims()
    results = []

    for claim in claims:
        if claim["check"] == "static":
            passed, msg = run_static_claim(claim, REPO_ROOT)
            results.append((claim["id"], claim["check"], passed, msg))
        elif claim["check"] == "dynamic":
            if args.static_only:
                results.append((claim["id"], claim["check"], None, "skipped (--static-only)"))
            else:
                passed, msg = run_dynamic_claim(claim, REPO_ROOT)
                results.append((claim["id"], claim["check"], passed, msg))
        else:
            results.append((claim["id"], claim["check"], False, f"unknown check type: {claim['check']}"))

    print(f"{'claim':45} {'type':10} {'result':8} detail")
    print("-" * 100)
    any_failed = False
    for claim_id, check_type, passed, msg in results:
        status = "SKIP" if passed is None else ("PASS" if passed else "FAIL")
        if passed is False:
            any_failed = True
        print(f"{claim_id:45} {check_type:10} {status:8} {msg}")

    sys.exit(1 if any_failed else 0)


if __name__ == "__main__":
    main()
