#!/usr/bin/env bash
# Verifies that formal JSON Schemas (schemas/) are valid and tests live endpoints
# against them if the kernel is running.
#
# Part of Pilar 2 (repo-pillars): explicit executable contracts across process boundaries.
#
# Usage: scripts/check_contracts.sh [--strict]

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Pilar 2 Contract Verification Gate ==="

# Resolve the real interpreter FIRST -- some environments (this one included,
# Windows/Git Bash) only expose `python`, not `python3`. A hardcoded `python3`
# call fails with exit 127 before this gate even checks a single schema.
PY_BIN=$(command -v python3 2>/dev/null || command -v python 2>/dev/null || true)
if [ -z "$PY_BIN" ]; then
    echo "❌ No python or python3 interpreter found on PATH."
    exit 1
fi

# 1. Verify schema files exist
SCHEMAS=("schemas/api_v1_embed_request.json" "schemas/api_v1_embed_response.json" "schemas/health_response.json")
for s in "${SCHEMAS[@]}"; do
    if [ ! -f "$s" ]; then
        echo "❌ Missing contract schema: $s"
        exit 1
    fi
    # Validate JSON syntax
    "$PY_BIN" -c "import json; json.load(open('$s', encoding='utf-8'))"
    echo "✅ Valid JSON schema: $s"
done

# 2. Execute verify_contracts.py against the live kernel, if one is reachable
# (won't be in a fresh CI runner -- degrades gracefully to static-only).
if curl -s -m 2 http://127.0.0.1:4000/health >/dev/null 2>&1; then
    echo "Live kernel detected on :4000 — running live contract verification..."
    "$PY_BIN" scripts/verify_contracts.py
else
    echo "No live kernel on :4000 — schemas validated statically (Rust integration tests cover struct contracts)."
fi

echo "🎯 Contract gate passed successfully."
exit 0
