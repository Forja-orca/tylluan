#!/usr/bin/env bash
# Compares the commit embedded in a LIVE, running kernel against the real
# current repo HEAD -- catches exactly the 2026-08-22 incident where the
# running :4000 process was 16 commits behind main and nobody noticed until
# an explicit question forced the check.
#
# Why this is a separate, local-only script and NOT a CI job (unlike
# check_test_count.sh / check_head_sync.sh): the drift this catches only
# exists in a process that's already running -- CI starts fresh every time,
# so there's no "stale running binary" for it to ever observe. A build-time
# test embedding the commit (env!("TYLLUAN_GIT_COMMIT") in build.rs, which
# already exists and is what /health reports) can only ever verify "the
# binary I just built knows its own commit correctly" -- it cannot detect
# "this already-running process was built 3 weeks and 15 commits ago and
# nobody has restarted it since". That fact only lives in the live process's
# memory, so checking it requires actually asking the live process.
#
# This script is the formalization of what got done by hand, repeatedly,
# during the 2026-08-22 session (`curl :4000/health` vs `git log -1`) --
# meant to be run by any agent/human at the START of a work session, the
# same way you'd check `git status` first.
#
# Usage: scripts/check_live_kernel_drift.sh [kernel_url]
# Default kernel_url: http://127.0.0.1:4000
# Exit 0 if the kernel is unreachable (not an error -- it may legitimately
#   not be running right now) or if it's within MAX_LAG commits of HEAD.
# Exit 1 if it's reachable AND meaningfully behind -- prints the exact gap
#   and the rebuild command to close it.

set -euo pipefail
cd "$(dirname "$0")/.."

KERNEL_URL="${1:-http://127.0.0.1:4000}"
MAX_LAG=2

real_head=$(git rev-parse --short=7 HEAD)

health_json=$(curl -fsS --max-time 3 "$KERNEL_URL/health" 2>/dev/null) || {
    echo "ℹ️  No live kernel reachable at $KERNEL_URL/health -- nothing to check (not an error)."
    exit 0
}

live_commit=$(echo "$health_json" | grep -oE '"commit"\s*:\s*"[0-9a-f]+"' | grep -oE '[0-9a-f]{6,40}' | head -1)

if [ -z "$live_commit" ]; then
    echo "⚠️  Kernel responded but /health had no parseable \"commit\" field: $health_json"
    exit 0
fi

echo "Real HEAD:        $real_head"
echo "Live kernel ($KERNEL_URL): $live_commit"

if [ "$live_commit" = "unknown" ]; then
    echo "⚠️  Live kernel reports commit=\"unknown\" -- built without git available (build.rs fallback). Can't measure drift."
    exit 0
fi

if ! git cat-file -e "$live_commit" 2>/dev/null; then
    echo "⚠️  Live kernel's commit ($live_commit) isn't a commit that exists in this local checkout -- different repo/remote, or history was rewritten. Can't measure drift reliably."
    exit 0
fi

if ! git merge-base --is-ancestor "$live_commit" HEAD 2>/dev/null; then
    echo "⚠️  Live kernel's commit ($live_commit) is not an ancestor of HEAD -- it may be running code from a different branch. Not a simple staleness case."
    exit 0
fi

lag=$(git rev-list --count "$live_commit"..HEAD)

if [ "$lag" -le "$MAX_LAG" ]; then
    echo "✅ Live kernel is current (${lag} commit(s) behind HEAD, within the ${MAX_LAG}-commit tolerance)."
    exit 0
fi

# A commit-count gap can be entirely cosmetic: build.rs embeds whatever HEAD
# was AT BUILD TIME, so any commit merged after that build -- even a docs
# typo fix or a CI-only change -- inflates $lag with zero actual behavior
# difference in the running binary. Found live 2026-08-23: a real rebuild
# reported "4 commits behind" when the diff across those 4 commits touched
# only scripts/CI/docs/a Python guild -- zero Rust kernel source changed.
# Check whether the gap actually touches anything the binary could care
# about before alarming over a number that means nothing on its own.
kernel_paths_changed=$(git diff --name-only "$live_commit"..HEAD -- \
    crates/tylluan-kernel/src crates/tylluan-link/src crates/tylluan-fsrs/src \
    Cargo.toml Cargo.lock crates/tylluan-kernel/build.rs 2>/dev/null | wc -l)

if [ "$kernel_paths_changed" -eq 0 ]; then
    echo ""
    echo "ℹ️  Live kernel is ${lag} commits behind HEAD, but none of those commits"
    echo "touch kernel source (crates/tylluan-kernel|link|fsrs/src, Cargo.toml/lock,"
    echo "build.rs) -- the running binary is functionally current. The commit hash"
    echo "mismatch is cosmetic (build.rs bakes in whatever HEAD was at build time;"
    echo "any later commit, even docs-only, inflates this count with zero real"
    echo "drift). No rebuild needed on this basis alone."
    exit 0
fi

echo ""
echo "❌ Live kernel is ${lag} commits behind HEAD, and ${kernel_paths_changed} of the"
echo "changed files touch real kernel source -- this IS functional drift."
echo ""
echo "Every commit merged since ${live_commit} is NOT active in the running process."
echo "Rebuild and restart to close the gap:"
echo ""
win_path=$(pwd -W 2>/dev/null | sed 's#/#\\#g' || pwd)
echo "    taskkill /IM tylluan-nexus.exe /F"
echo "    cd $win_path"
echo "    cargo build --release -p tylluan-kernel"
echo "    .\\tylluan-mcp.bat"
echo ""
exit 1
