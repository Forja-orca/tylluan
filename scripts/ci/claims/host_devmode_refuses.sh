#!/usr/bin/env bash
# Claim: kernel logs a warning and refuses to start with host=0.0.0.0 + dev_mode=true.
# doc_source: docs/concepts/SECURITY.md:19
#
# NOTE (verified against crates/tylluan-kernel/src/main.rs and config.rs,
# 2026-08-18): the config table is `[nexus]`, not `[server]` -- there is no
# `[server]` table in TylluanConfig. `--config <path>` is the real CLI flag
# (see main.rs args.iter().position(|r| r == "--config"), ~line 252/298).
#
# IMPORTANT CAVEAT found during investigation: `TylluanConfig::validate_security()`
# (config.rs ~line 1707) silently rewrites `host` back to "127.0.0.1" whenever
# `dev_mode == true` and host is neither "127.0.0.1" nor "localhost", logging
# "CRITICAL_SECURITY_TRIGGER: ... Forcing host to '127.0.0.1' for safety."
# main.rs calls `config.validate_security()` (line ~318) unconditionally AFTER
# applying the `--config` override and BEFORE `enforce_security_guard()`
# (line ~331) ever runs. That means the hard-refusal branch in
# enforce_security_guard() (the "⛔ INSECURE CONFIG REFUSED" eprintln + exit(1)
# at main.rs ~197-211) is unreachable via a `--config <file>` startup for this
# exact combination -- validate_security() has already neutralized the
# dangerous host value first. The kernel does NOT refuse to start in this
# path; it logs the CRITICAL_SECURITY_TRIGGER warning, rewrites host to
# 127.0.0.1, and boots successfully (exit 0).
#
# This script tests the claim AS DOCUMENTED (refuses to start / non-zero
# exit). Given the above, it is expected to genuinely FAIL when run against
# the real binary -- which is the correct, intended behavior of a claims
# gate: it should catch this doc/code drift, not paper over it. See
# task-3-report.md for the full writeup. This is not something Task 3 is
# scoped to fix (do not edit main.rs/config.rs or SECURITY.md here).
set -uo pipefail

BINARY="${TYLLUAN_BINARY:-./target/release/tylluan-nexus}"
CONFIG_DIR=$(mktemp -d)
trap 'rm -rf "$CONFIG_DIR"' EXIT

cat > "$CONFIG_DIR/tylluan.toml" <<'EOF'
[nexus]
host = "0.0.0.0"
dev_mode = true
port = 0
EOF

"$BINARY" --config "$CONFIG_DIR/tylluan.toml" > "$CONFIG_DIR/out.log" 2>&1 &
KERNEL_PID=$!
# Give it a few seconds to either exit (refusal) or finish booting.
for _ in $(seq 1 20); do
  if ! kill -0 "$KERNEL_PID" 2>/dev/null; then
    break
  fi
  sleep 0.5
done

if kill -0 "$KERNEL_PID" 2>/dev/null; then
  # Still running after the wait window: it did NOT refuse to start.
  kill "$KERNEL_PID" 2>/dev/null
  wait "$KERNEL_PID" 2>/dev/null
  echo "FAIL: kernel is still running with host=0.0.0.0 + dev_mode=true (should have refused to start)"
  cat "$CONFIG_DIR/out.log"
  exit 1
fi

wait "$KERNEL_PID" 2>/dev/null
exit_code=$?

if [ "$exit_code" -eq 0 ]; then
  echo "FAIL: kernel exited 0 (started successfully) with host=0.0.0.0 + dev_mode=true (should have refused)"
  cat "$CONFIG_DIR/out.log"
  exit 1
fi

if ! grep -qi "INSECURE CONFIG REFUSED\|refus" "$CONFIG_DIR/out.log"; then
  echo "FAIL: kernel exited non-zero but without the expected refusal message"
  cat "$CONFIG_DIR/out.log"
  exit 1
fi

echo "PASS: kernel refused to start, warning present"
exit 0
