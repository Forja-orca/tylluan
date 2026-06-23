#!/usr/bin/env bash
# Tylluan — Start kernel + proxy
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
PROXY="target/release/tylluan-proxy"
if [ ! -f "$KERNEL" ]; then
    echo "Building tylluan-kernel (release)..."
    cargo build --release -p tylluan-kernel -p tylluan-proxy
fi

# Start proxy in background
echo "Starting tylluan-proxy on :3030..."
$PROXY &
PROXY_PID=$!

# Small delay for proxy to bind
sleep 1

# Start kernel in foreground
echo "Starting tylluan-nexus..."
cd crates/tylluan-kernel
../../$KERNEL

# Cleanup proxy on exit
kill $PROXY_PID 2>/dev/null
