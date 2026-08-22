#!/usr/bin/env bash
# Verifies that STATUS.md cites exactly one HEAD value and that it matches
# the real current commit.
#
# Why this exists: during the 2026-08-22 full-project audit (Coloquio T196),
# STATUS.md was found citing TWO different HEAD values on two different
# lines (`12dca2e` and `4dae7f6`), and NEITHER matched the real HEAD at the
# time. Nothing had ever forced this file to stay honest about which commit
# it describes -- unlike scripts/check_test_count.sh, which has caught test-
# count drift automatically for months. This is that same pattern applied to
# the HEAD citation instead of the test count.
#
# This is a repo-only check (git log + a doc grep) -- it does NOT reach out
# to a live kernel, so it's safe to run in CI. For checking a *running*
# kernel against this repo's HEAD, see the separate, local-only workflow
# documented in STATUS.md's "Known Gaps" section (curl :4000/health).
#
# Usage: scripts/check_head_sync.sh [--fix]
# Exit 0 if STATUS.md has exactly one HEAD citation and it matches git HEAD.
# Exit 1 otherwise, printing what's wrong.
# --fix: rewrite STATUS.md's HEAD citation(s) to the real current HEAD.

set -euo pipefail
cd "$(dirname "$0")/.."

real_head=$(git rev-parse --short=7 HEAD)

# Match `HEAD `abc1234`` (backtick-quoted short hash) anywhere in STATUS.md.
mapfile -t found_heads < <(grep -oE 'HEAD:?\*{0,2} *`[0-9a-f]{7,40}`' STATUS.md | grep -oE '[0-9a-f]{7,40}' || true)

echo "Real HEAD:          $real_head"
echo "STATUS.md citations: ${found_heads[*]:-<none found>}"
echo ""

problems=0

if [ "${#found_heads[@]}" -eq 0 ]; then
    echo "❌ STATUS.md has no \`HEAD \`...\`\` citation at all."
    problems=1
elif [ "${#found_heads[@]}" -gt 1 ]; then
    # Distinct values?
    unique_count=$(printf '%s\n' "${found_heads[@]}" | sort -u | wc -l)
    if [ "$unique_count" -gt 1 ]; then
        echo "❌ STATUS.md cites ${#found_heads[@]} different HEAD values across ${unique_count} distinct commits — exactly the 2026-08-22 bug this script exists to catch."
        problems=1
    fi
fi

# Tolerance note: a commit literally cannot cite its own hash inside its own
# content (changing the file changes the hash) -- so "cited HEAD == current
# HEAD" is only achievable by citing the PARENT commit, one commit behind.
# We tolerate up to MAX_LAG commits of staleness for that reason, and only
# fail on real drift (the kind that let a kernel run 15+ commits behind
# unnoticed in the 2026-08-22 incident this script exists to prevent).
MAX_LAG=2

for h in "${found_heads[@]}"; do
    if ! git cat-file -e "$h" 2>/dev/null; then
        echo "❌ STATUS.md cites HEAD \`$h\`, which isn't a commit that exists in this repo at all."
        problems=1
        continue
    fi
    if ! git merge-base --is-ancestor "$h" HEAD 2>/dev/null; then
        echo "❌ STATUS.md cites HEAD \`$h\`, which is not an ancestor of the current HEAD (\`$real_head\`) -- wrong branch or rewritten history."
        problems=1
        continue
    fi
    lag=$(git rev-list --count "$h"..HEAD)
    if [ "$lag" -gt "$MAX_LAG" ]; then
        echo "❌ STATUS.md cites HEAD \`$h\`, which is $lag commits behind the real current HEAD (\`$real_head\`) -- more than the $MAX_LAG-commit tolerance for \"can't cite my own hash\". Update it."
        problems=1
    fi
done

if [ "$problems" -eq 0 ]; then
    echo "✅ STATUS.md's HEAD citation matches reality ($real_head)."
    exit 0
fi

if [ "${1:-}" = "--fix" ]; then
    echo ""
    echo "--fix: rewriting all HEAD citations in STATUS.md to $real_head"
    # macOS/BSD sed vs GNU sed both accept -E; use a temp file for portability.
    sed -E "s/(HEAD:?\*{0,2} *)\`[0-9a-f]{7,40}\`/\1\`$real_head\`/g" STATUS.md > STATUS.md.tmp
    mv STATUS.md.tmp STATUS.md
    echo "✅ Fixed. Re-run without --fix to verify."
    exit 0
fi

echo ""
echo "Run 'scripts/check_head_sync.sh --fix' to correct STATUS.md automatically."
exit 1
