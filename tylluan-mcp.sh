#!/usr/bin/env bash
# Tylluan — Start kernel (single binary)
# Usage: ./tylluan-mcp.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Activate Python venv if available
if [ -f ".venv/bin/activate" ]; then
    source .venv/bin/activate
fi

# Copy config if needed
if [ ! -f "tylluan.toml" ] && [ -f "tylluan.example.toml" ]; then
    echo "No tylluan.toml found — copying from tylluan.example.toml"
    cp tylluan.example.toml tylluan.toml
fi

# Build if binary doesn't exist
KERNEL="target/release/tylluan-nexus"
if [ ! -f "$KERNEL" ]; then
    echo "Building tylluan-kernel (release)..."
    cargo build --release -p tylluan-kernel
fi

# Start kernel in foreground
echo "Starting tylluan-nexus..."
cd crates/tylluan-kernel
../../$KERNEL
