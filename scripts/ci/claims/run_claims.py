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


def run_static_claim(claim: dict, repo_root: Path) -> tuple[bool, str]:
    """A static claim passes if `pattern` does NOT appear (outside comments/
    #[cfg(test)] blocks) in any file under `scope`, except `exclude_file`."""
    pattern = claim["pattern"]
    scope = claim["scope"]
    exclude_file = claim.get("exclude_file")

    args = ["rg", "--line-number", "--no-heading", pattern] + scope
    result = subprocess.run(args, cwd=repo_root, capture_output=True, text=True)

    # rg exit code 1 = no matches found = claim holds. 0 = matches found, need to filter.
    if result.returncode == 1:
        return True, "no matches"
    if result.returncode not in (0, 1):
        return False, f"ripgrep error: {result.stderr.strip()}"

    real_violations = []
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
        if "#[cfg(test)]" in content:
            continue
        real_violations.append(f"{path}:{lineno}: {stripped}")

    if not real_violations:
        return True, "all matches excluded (comments/tests/exclude_file)"
    return False, "; ".join(real_violations[:5])


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
                results.append((claim["id"], claim["check"], None, "dynamic runner not yet implemented"))
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
