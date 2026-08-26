#!/usr/bin/env bash
# Points git at the repo's tracked hooks (.githooks/) instead of the
# untracked, unshared default (.git/hooks/). Run this once per checkout.
#
# Why a tracked hooks dir at all (2026-08-26): every agent working this
# repo — human-guided or not — has, at least once, pushed a commit whose
# author-local "verified" claim didn't match what CI actually found. A
# hook that isn't version-controlled can't fix that: nobody new inherits
# it, and it silently stops applying the moment someone re-clones. This
# makes the check travel with the repo itself.
set -euo pipefail
cd "$(dirname "$0")/.."

git config core.hooksPath .githooks
chmod +x .githooks/pre-push scripts/verify.sh 2>/dev/null || true

echo "✅ core.hooksPath -> .githooks (pre-push now runs scripts/verify.sh before every push)"
echo "   To bypass once: git push --no-verify"
echo "   To undo entirely: git config --unset core.hooksPath"
