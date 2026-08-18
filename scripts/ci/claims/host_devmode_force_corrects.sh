#!/usr/bin/env bash
# Claim: with host=0.0.0.0 + dev_mode=true, the kernel logs a
# CRITICAL_SECURITY_TRIGGER warning naming the unsafe host and
# force-corrects it to 127.0.0.1 before boot -- it does NOT refuse to start.
# doc_source: docs/concepts/SECURITY.md:19
#
# NOTE (verified against crates/tylluan-kernel/src/main.rs and config.rs,
# 2026-08-18): the config table is `[nexus]`, not `[server]` -- there is no
# `[server]` table in TylluanConfig. `--config <path>` is the real CLI flag.
#
# HISTORY: this claim originally said "refuses to start" (SECURITY.md:19's
# wording at the time). This script's first version tested that and,
# correctly, found it FALSE -- validate_security() (config.rs ~line 1707)
# auto-corrects `host` back to "127.0.0.1" and logs a warning, and main.rs
# calls validate_security() BEFORE enforce_security_guard()'s hard-refusal
# branch ever runs, so the refusal path is unreachable for this combination.
# José ruled (2026-08-18) to fix the doc to match this real, already
# reasonably safe (warned, fail-safe-by-correction) behavior instead of
# changing the code to hard-refuse. This script now tests the corrected
# claim: kernel boots (exit 0) AND the CRITICAL_SECURITY_TRIGGER warning is
# present in its log.
#
# I1/I2 (2026-08-18 review fix): the kernel spawns Python guild subprocesses
# that `kill $KERNEL_PID` alone does not reap. `set -m` gives this script job
# control so the backgrounded kernel gets its own process group (pgid ==
# kernel pid); cleanup below kills the whole group with `kill -- -$PGID`,
# not just the kernel process itself. This trap now ALSO actually kills the
# kernel (the pre-fix version only did `rm -rf "$CONFIG_DIR"` and leaked the
# kernel on every run, per I2).
# I8: the old third assertion block (grep for "Forcing host to '127.0.0.1'")
# was removed -- it can never fail independently of the CRITICAL_SECURITY_TRIGGER
# check because config.rs emits both strings on the exact same warn!() line,
# and it silently no-ops (rather than failing loud) if the log format ever
# drifts. The CRITICAL_SECURITY_TRIGGER check above already covers what matters.
set -uo pipefail
set -m

BINARY="${TYLLUAN_BINARY:-./target/release/tylluan-nexus}"
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

cat > "$CONFIG_DIR/tylluan.toml" <<'EOF'
[nexus]
host = "0.0.0.0"
dev_mode = true
port = 0
EOF

# stdbuf -oL -eL: see write_gate_rejects.sh for the full explanation --
# real CI bug found 2026-08-18, stdout block-buffers when piped to a file.
# This particular script passed once by chance (its warning happens to be
# emitted early enough to hit the buffer's first flush), but is at the same
# real risk of flaking without this fix.
stdbuf -oL -eL "$BINARY" --config "$CONFIG_DIR/tylluan.toml" > "$CONFIG_DIR/out.log" 2>&1 &
KERNEL_PID=$!

# Give it a few seconds to boot (or crash) -- we expect it to keep running.
booted=0
for _ in $(seq 1 20); do
  if grep -qi "CRITICAL_SECURITY_TRIGGER" "$CONFIG_DIR/out.log" 2>/dev/null; then
    booted=1
    break
  fi
  if ! kill -0 "$KERNEL_PID" 2>/dev/null; then
    break
  fi
  sleep 0.5
done

# Record whether the kernel already exited on its own (crash) before cleanup
# tears down the process group -- cleanup() (trap) still runs afterward.
if ! kill -0 "$KERNEL_PID" 2>/dev/null; then
  wait "$KERNEL_PID" 2>/dev/null
  exit_code=$?
  if [ "$exit_code" -ne 0 ] && [ "$booted" -eq 0 ]; then
    echo "FAIL: kernel exited non-zero (code $exit_code) before ever logging CRITICAL_SECURITY_TRIGGER -- it crashed instead of auto-correcting"
    cat "$CONFIG_DIR/out.log"
    exit 1
  fi
fi

if ! grep -qi "CRITICAL_SECURITY_TRIGGER" "$CONFIG_DIR/out.log"; then
  echo "FAIL: kernel ran with host=0.0.0.0 + dev_mode=true but never logged the expected CRITICAL_SECURITY_TRIGGER warning -- the auto-correction may have been silently removed"
  cat "$CONFIG_DIR/out.log"
  exit 1
fi

echo "PASS: kernel logged CRITICAL_SECURITY_TRIGGER and force-corrected host to 127.0.0.1, booted safely"
exit 0
