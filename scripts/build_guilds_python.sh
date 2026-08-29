#!/usr/bin/env bash
# ==============================================================================
# Tylluan — Portable Python Environment Builder for Guilds (Linux x86_64)
# ==============================================================================
# Uses Astral uv and python-build-standalone to produce a self-contained,
# portable Python 3.12 environment with all guild dependencies preinstalled.
#
# Idempotent: can be executed repeatedly without re-downloading existing files.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

TARGET_DIR="${REPO_ROOT}/guilds-python/linux-x86_64"
REQUIREMENTS_FILE="${REPO_ROOT}/guilds/requirements.txt"

echo "=== [1/4] Checking prerequisites ==="
# Ensure uv is in PATH or check ~/.local/bin / ~/.cargo/bin
if ! command -v uv >/dev/null 2>&1; then
    if [ -x "${HOME}/.local/bin/uv" ]; then
        export PATH="${HOME}/.local/bin:${PATH}"
    elif [ -x "${HOME}/.cargo/bin/uv" ]; then
        export PATH="${HOME}/.cargo/bin:${PATH}"
    else
        echo "❌ 'uv' is not installed. Installing uv..."
        curl -LsSf https://astral.sh/uv/install.sh | sh
        export PATH="${HOME}/.local/bin:${PATH}"
    fi
fi

echo "✅ Using uv version: $(uv --version)"

echo "=== [2/4] Installing standalone Python 3.12 ==="
mkdir -p "${TARGET_DIR}"
uv python install --install-dir "${TARGET_DIR}" 3.12

# Locate the installed Python binary
PYTHON_BIN="$(find "${TARGET_DIR}" -path "*/bin/python3" -type f -o -path "*/bin/python3" -type l | head -n 1)"

if [ -z "${PYTHON_BIN}" ] || [ ! -x "${PYTHON_BIN}" ]; then
    echo "❌ Failed to locate installed python3 executable in ${TARGET_DIR}"
    exit 1
fi

echo "✅ Python standalone binary located at: ${PYTHON_BIN}"
echo "   Version: $("${PYTHON_BIN}" --version)"

echo "=== [3/4] Installing guild dependencies from requirements.txt ==="
if [ ! -f "${REQUIREMENTS_FILE}" ]; then
    echo "❌ Requirements file not found: ${REQUIREMENTS_FILE}"
    exit 1
fi

# Use --break-system-packages because python-build-standalone marks itself EXTERNALLY-MANAGED
uv pip install --break-system-packages -r "${REQUIREMENTS_FILE}" --python "${PYTHON_BIN}"

echo "=== [4/4] Verifying portable environment ==="
PYTHONNOUSERSITE=1 "${PYTHON_BIN}" -c "import mcp, fastmcp, psutil; print('✅ Imported core dependencies successfully in portable environment')"

echo "=== Guilds portable Python build complete! ==="
echo "Target directory: ${TARGET_DIR}"
echo "Python binary:    ${PYTHON_BIN}"
