#!/usr/bin/env bash
# Claim: an unapproved peer's dispatch call is rejected, not executed.
# doc_source: docs/concepts/SECURITY_FEDERATION.md:13
#
# Investigation (2026-08-18): crates/tylluan-kernel/tests/mesh_audit.rs's
# test_kernel_remote_dispatch_routes_via_real_noise_xk_p2p already proves
# tylluan_link::p2p::execute_remote_tcp / start_p2p_listener_noise is a real,
# reusable client/server pair -- no new Noise XK protocol code was needed.
# This script drives a small new CLI wrapper,
# crates/tylluan-link/src/bin/unapproved_peer_probe.rs, which:
#   1. generates a throwaway NodeIdentity (never present in the kernel's
#      peers.db, so it can never be in the approved set -- p2p.rs's
#      approved_x25519 closure is fail-closed on empty/no-match, see
#      crates/tylluan-kernel/src/transport/http/mod.rs ~line 991-999)
#   2. calls the real execute_remote_tcp() against the kernel's live P2P
#      listener
#   3. exits 0 (REJECTED) if the dispatch did not execute (success=false or
#      a protocol/connection error), exit 1 (ACCEPTED) if it actually ran.
#
# Config verified against crates/tylluan-kernel/src/config.rs: the real
# table is `[p2p]` with `enabled` (default false) and `listen_port` (default
# 9123) -- there is no `[federation]` table with a `p2p_port` key as an
# earlier draft of this script guessed. The listener always binds
# "0.0.0.0:{listen_port}" (transport/http/mod.rs ~line 942), and logs
# "P2P dispatch listener started on {bound_addr}" (~line 1002) -- NOT the
# "P2P listening on 127.0.0.1:PORT" text an earlier draft guessed.
# I1 (2026-08-18 review fix): `set -m` + killing the whole process group
# (not just $KERNEL_PID) so the kernel's Python guild subprocesses are
# actually reaped on cleanup, not orphaned.
set -uo pipefail
set -m

BINARY="${TYLLUAN_BINARY:-./target/release/tylluan-nexus}"
PROBE="${UNAPPROVED_PEER_PROBE:-./target/release/unapproved_peer_probe}"
CONFIG_DIR=$(mktemp -d)
KERNEL_PID=""
cleanup() {
  if [ -n "$KERNEL_PID" ]; then
    kill -- "-$KERNEL_PID" 2>/dev/null || kill "$KERNEL_PID" 2>/dev/null
    wait "$KERNEL_PID" 2>/dev/null
  fi
  rm -rf "$CONFIG_DIR"
}
trap cleanup EXIT

if [ ! -x "$PROBE" ]; then
  echo "FAIL: probe binary not found/executable at $PROBE (build with: cargo build --release -p tylluan-link --bin unapproved_peer_probe)"
  exit 1
fi

cat > "$CONFIG_DIR/tylluan.toml" <<'EOF'
[nexus]
host = "127.0.0.1"
dev_mode = true
port = 0

[p2p]
enabled = true
listen_port = 0
EOF

# stdbuf -oL -eL: see write_gate_rejects.sh for the full explanation --
# real CI bug found 2026-08-18, stdout block-buffers when piped to a file.
stdbuf -oL -eL "$BINARY" --config "$CONFIG_DIR/tylluan.toml" > "$CONFIG_DIR/kernel.log" 2>&1 &
KERNEL_PID=$!

# Wait for the kernel's HTTP port (to fetch its real pubkey) and P2P port.
# REAL FIX (2026-08-19): see write_gate_rejects.sh for the full explanation --
# 15s was never realistic for a cold-boot kernel (BGE-M3 embedding load +
# always-on guild startup) on a GitHub Actions runner. Raised to 120s
# (240 * 0.5s), matching CLAUDE.md's documented CPU-bound guild timeout floor.
http_port=""
p2p_port=""
for _ in $(seq 1 240); do
  [ -z "$http_port" ] && http_port=$(grep -oP 'listening on 127\.0\.0\.1:\K[0-9]+' "$CONFIG_DIR/kernel.log" | head -1)
  [ -z "$p2p_port" ] && p2p_port=$(grep -oP 'P2P dispatch listener started on \S+:\K[0-9]+' "$CONFIG_DIR/kernel.log" | head -1)
  [ -n "$http_port" ] && [ -n "$p2p_port" ] && break
  sleep 0.5
done

if [ -z "$http_port" ]; then
  echo "FAIL: kernel never logged its bound HTTP port within 120s"
  cat "$CONFIG_DIR/kernel.log"
  exit 1
fi

if [ -z "$p2p_port" ]; then
  echo "FAIL: kernel never logged a bound P2P port within 120s"
  cat "$CONFIG_DIR/kernel.log"
  exit 1
fi

peer_pubkey=$(curl -s "http://127.0.0.1:$http_port/api/v1/federation/identity" | grep -oP '"public_key"\s*:\s*"\K[0-9a-f]+')

if [ -z "$peer_pubkey" ]; then
  echo "FAIL: could not read the kernel's public_key from /api/v1/federation/identity"
  exit 1
fi

probe_output=$("$PROBE" "127.0.0.1:$p2p_port" "$peer_pubkey" 2>&1)
probe_exit=$?

# I6 (2026-08-18 review fix): the probe now distinguishes a real auth
# rejection (exit 0) from a merely INCONCLUSIVE generic connection/protocol
# failure (exit 2) -- treating exit 2 as a pass would let a broken P2P
# listener look exactly as secure as a correctly-enforced one. Only exit 0
# is a real PASS.
case "$probe_exit" in
  0)
    echo "PASS: unapproved peer's dispatch was rejected by the real auth path: $probe_output"
    exit 0
    ;;
  1)
    echo "FAIL: unapproved peer's dispatch was ACCEPTED (should have been rejected): $probe_output"
    exit 1
    ;;
  *)
    echo "FAIL: probe result was INCONCLUSIVE, not a confirmed rejection -- a broken listener would look identical: $probe_output"
    exit 1
    ;;
esac
