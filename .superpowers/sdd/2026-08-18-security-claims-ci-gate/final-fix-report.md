# Final fix report — security claims CI gate

All Critical and Important findings (C1, I1-I8) and M6 from
`final-fix-findings.md` are fixed. Nothing listed as "explicitly not in
scope" was touched. No new third-party dependencies were added.

## C1 — CI never builds `unapproved_peer_probe`

**Change:** `.github/workflows/ci.yml`, `security-claims-gate` job — added a
`Build unapproved peer probe` step (`cargo build --release -p tylluan-link
--bin unapproved_peer_probe`) right after the existing `tylluan-nexus`
release build step, before the gate runs.

**Verified:**
```
python -c "import yaml; d=yaml.safe_load(open('.github/workflows/ci.yml',encoding='utf-8')); print(d['jobs']['security-claims-gate']['steps'])"
```
Output confirms both build steps are present in order:
`[..., {'name': 'Build release binary', ...}, {'name': 'Build unapproved
peer probe', 'run': 'cargo build --release -p tylluan-link --bin
unapproved_peer_probe'}, {'name': 'Run security claims gate', ...}]`
YAML parses without error (`yaml.safe_load` succeeded).

## I1 — Guild subprocesses orphaned by all three scripts

**Change:** all three scripts (`host_devmode_force_corrects.sh`,
`p2p_rejects_unapproved.sh`, `write_gate_rejects.sh`) now start with `set -m`
(enables job control so the backgrounded kernel gets its own process
group == kernel PID) and their `cleanup()` trap now does
`kill -- "-$KERNEL_PID" 2>/dev/null || kill "$KERNEL_PID" 2>/dev/null`
(kills the whole process group; falls back to a plain kill if the group
kill fails for any reason), followed by `wait "$KERNEL_PID"`.

**Verified:** portability check —
```
$ command -v setsid          # not available in this environment (git-bash)
$ bash -c 'set -m; sleep 5 & echo $!; kill -- -$! 2>&1; echo done'
455
done
```
No error from `kill -- -$PID` under `set -m` job control; this is the
portable approach the findings doc asked for (works on git-bash and on
ubuntu-latest, unlike `setsid` which isn't present in git-bash). All three
scripts pass `bash -n` syntax check.

## I2 — `host_devmode_refuses.sh` trap never killed the kernel

**Change:** replaced the old trap (`trap 'rm -rf "$CONFIG_DIR"' EXIT`, which
had no kernel-kill clause at all) with the same `cleanup()` function used in
the other two scripts (kill process group, wait, then `rm -rf
"$CONFIG_DIR"`). This is bundled with the I1 fix in the same file
(now renamed per M6, see below).

**Verified:** `bash -n scripts/ci/claims/host_devmode_force_corrects.sh` → OK.
Manual read-through confirms every exit path (normal completion, early
`exit 1` on failure, or external SIGTERM from I3's `timeout` wrapper) now
runs through the trap, which kills the kernel's process group before
removing the config dir.

## I3 — `run_dynamic_claim` timeout crashes uncaught + orphans kernel

**Change (`scripts/ci/claims/run_claims.py`):**
- Wrapped `subprocess.run(...)` in `try/except subprocess.TimeoutExpired`,
  returning `(False, "timed out after ...s")` instead of letting the
  exception propagate and crash the whole runner with a traceback.
- The actual script invocation now goes through the GNU `timeout` coreutil
  (`timeout --signal=TERM 90 bash <script>`) instead of relying solely on
  `subprocess.run`'s own `timeout=` kwarg. `subprocess.run`'s timeout
  previously SIGKILLed the `bash` process directly on expiry, which skips
  bash's `EXIT` trap entirely (SIGKILL cannot be trapped) and orphans the
  kernel + its process group with no chance to clean up. Routing through
  `timeout --signal=TERM` sends SIGTERM to `bash` first — bash runs its
  `EXIT` trap on receipt of an untrapped terminating signal by default, so
  `cleanup()` still fires and kills the kernel's process group even under
  an external timeout. `subprocess.run`'s own `timeout=` kwarg is kept as a
  hard backstop 15s above the inner `timeout` value, in case the coreutil
  itself somehow doesn't fire; if the backstop trips, the except-block still
  reports a clean `FAIL` message instead of crashing.
- `result.returncode == 124` (GNU `timeout`'s own "time limit reached"
  exit code) is treated as a timeout FAIL, not a generic script failure.
- Raised the inner budget from 60s to 90s. I looked for concrete evidence
  in this session's own ledger (`progress.md`) of real boot times measured
  for these specific scripts and found none — the three scripts' own
  internal polling loops are bounded to <=15s (waiting only for early
  boot-log lines: `CRITICAL_SECURITY_TRIGGER` or the HTTP/P2P port
  announcements, both emitted well before any guild subsystem starts), so
  60s already had headroom for what these scripts actually wait on. The
  strongest concrete evidence available anywhere in this workspace for
  "cold boot + guild startup can be slow on CPU" is this project's own
  ForjaMCPo3 `CLAUDE.md` (`analysis_guild_ms = 60000` minimum,
  "knowledge guild tarda 60-120s"), which is about guild subsystems these
  three scripts don't actually wait on. Given the lack of direct evidence
  that 60s is tight *for these scripts*, but real evidence that CPU-bound
  boot/guild timing in this class of system is on this order elsewhere,
  I raised the value modestly (60s -> 90s) as cheap insurance against CI
  runner variance rather than a large, unjustified jump.

**Verified:**
```
python -c "import ast; ast.parse(open('scripts/ci/claims/run_claims.py',encoding='utf-8').read())"
```
→ no error (AST parses clean). Full static-claims run also exercises the
rest of the file end-to-end without error (see I4 verification below);
the dynamic path itself was not exercised against a live kernel in this
environment (per project convention: never spawn long-running processes
like the kernel via the Bash tool on this Windows setup — the AV sandbox
blocks/interferes with it). The timeout/signal logic was verified by
direct code reading against documented bash `EXIT`-trap-on-signal
semantics and confirmed GNU `timeout` (v8.32, present in this
environment's git-bash and standard on ubuntu-latest) exit-code
convention (124 on SIGTERM timeout).

## I4 — `#[cfg(test)]` exclusion only skipped the attribute line

**Change (`scripts/ci/claims/run_claims.py`):** added
`_test_block_line_numbers()`, a brace-depth state machine that reads a
whole matched file once, and for every `#[cfg(test)]` attribute line,
tracks brace depth from that line through the first balanced `{...}`
block that follows it, marking every line in that span (inclusive) as
excluded. `run_static_claim` now looks up each match's line number against
this per-file exclusion set instead of only checking whether the matched
line itself contains the literal string `#[cfg(test)]`.

Also fixed a **real pre-existing bug found while verifying this**: `rg`
omits the filename prefix (even with `--no-heading`) when a `scope` entry
is a single explicit file rather than a directory (this repo's
`p2p-dispatch-wires-peer-approval` claim scopes exactly one file,
`crates/tylluan-link/src/p2p.rs`). Without a filename prefix the
`path:lineno:content` split silently mis-assigned fields (e.g. `lineno`
became `"fn peer_is_approved(remote_x25519_hex"`), which crashed on
`int(lineno)` once I4 added a numeric line-number lookup. Fixed by adding
`--with-filename` to the `rg` invocation unconditionally.

**Verified — the requested live experiment against the real checker**, run
against `crates/tylluan-kernel/src/security/write_gate.rs` (a file with a
real `#[cfg(test)] mod tests { ... }` block, chosen instead of `config.rs`
because `config.rs` is `exclude_file`'d for this claim and wouldn't
exercise the new logic):

1. Added `let _c = Connection::open("/tmp/i4_probe_should_not_be_flagged.db");`
   *inside* the existing `#[cfg(test)] mod tests { ... }` block (inside
   `write_gate_grammar_is_binary`). Ran
   `python scripts/ci/claims/run_claims.py --static-only`:
   `encrypt-at-rest-single-choke-point` still reported
   `PASS  all matches excluded (comments/tests/exclude_file)` — correctly
   NOT flagged.
2. As a sanity check that the state machine actually does something (not
   just accidentally still filtering everything), added
   `let _i4_probe_should_be_flagged = Connection::open("/tmp/i4_probe_should_be_flagged.db");`
   *outside* any test block, inside `spawn_write_gate_judge` (real
   production code path). Ran the checker again:
   `encrypt-at-rest-single-choke-point` now reported
   `FAIL  crates/tylluan-kernel/src/security\write_gate.rs:37: let
   _i4_probe_should_be_flagged = Connection::open(...)` — correctly
   flagged as a real match.
3. Reverted both probes with `Edit`. Confirmed clean:
   `git diff --stat crates/tylluan-kernel/src/security/write_gate.rs`
   showed no diff (only a benign CRLF-normalization warning, no content
   change), and the checker returns to a full `PASS` table.

This directly confirms the brace-depth state machine correctly
distinguishes "inside a real `#[cfg(test)]` block" from "outside one" on
the actual target file for this claim's scope.

## I5 — `write_gate_rejects.sh` false-green shape

**Change:** replaced the sole assertion (`! grep -q '"node_id"'` on the
response) with two real checks:
1. **Positive rejection signal** — `grep -q "ACCESS_DENIED"` on the
   malicious-content response. Verified against the real source,
   `crates/tylluan-kernel/src/transport/server/handler_remember.rs` line
   66: on an ASI06 Layer 1 match it returns exactly
   `"ACCESS_DENIED: content matches a known prompt-injection pattern and
   was rejected before being written to memory."` — a literal, specific
   string, not an absence-of-signal heuristic.
2. **Control case** — a second POST with benign content
   (`"benign control-case note for write-gate CI claim"`), asserting the
   response contains `"Stored node"` (the real success-path text,
   `handler_remember.rs` — success responses embed
   `Stored node {node_id} (importance=...): "..."` as free text inside the
   MCP `Content::text` response, not as a JSON key) and does NOT contain
   `ACCESS_DENIED`. This proves the endpoint is actually reachable and
   functioning before trusting the rejection check — a 404, a crashed
   kernel, or a renamed tool would now fail the control case instead of
   silently passing the whole claim.

I also confirmed, while reading the source, that the *original* assertion
(`grep '"node_id"'`) would never have matched even a real success response:
`node_id` only ever appears as free text inside the `Content::text` string,
never as a literal JSON key `"node_id"` in the outer MCP response — so the
old check's "PASS on any failure" bug (per the finding) was actually even
worse than described; it was closer to "always PASS regardless of outcome,"
since the negative-match condition it looked for basically never existed on
either path.

**Verified:** `bash -n scripts/ci/claims/write_gate_rejects.sh` → OK. Not
exercised against a live kernel in this environment (Bash tool doesn't spawn
long-running kernel processes on this Windows setup — AV interference); the
literal `ACCESS_DENIED` and `Stored node` strings were confirmed directly
against `handler_remember.rs`'s real source, not guessed.

## I6 — `p2p_rejects_unapproved.sh` / `unapproved_peer_probe.rs` false-green shape

**Change (`crates/tylluan-link/src/bin/unapproved_peer_probe.rs`):** the
probe now distinguishes three outcomes instead of two:
- exit 0 `REJECTED` — only when the kernel's response is
  `success: false` **and** `error == "peer not approved"`, the literal
  string `crates/tylluan-link/src/p2p.rs` line 209 sends from the real
  `peer_is_approved()` rejection branch. This is the only outcome that
  actually proves the auth path fired.
- exit 1 `ACCEPTED` — `success: true` (dispatch ran). Claim FALSE, as
  before.
- exit 2 `INCONCLUSIVE` (new) — any other outcome: a generic
  connection/protocol `Err` (refused, reset, handshake failure), or a
  `success: false` with any error message OTHER than `"peer not
  approved"`. These no longer count as a pass — a broken listener would
  produce exactly this symptom, which is the false-green the finding
  described.

**Change (`scripts/ci/claims/p2p_rejects_unapproved.sh`):** added
`set -m` + process-group cleanup (I1), and replaced the binary
`probe_exit -ne 0` check with an explicit `case` over exit codes 0/1/other
— only exit 0 is a real `PASS`; both 1 and 2 (and any unexpected code)
produce `FAIL` with a message distinguishing "ACCEPTED" from
"INCONCLUSIVE."

**Verified:** `cargo check -p tylluan-link --bin unapproved_peer_probe` →
`Finished` clean, no warnings. `bash -n
scripts/ci/claims/p2p_rejects_unapproved.sh` → OK. The real rejection
string (`"peer not approved"`) was confirmed directly against
`crates/tylluan-link/src/p2p.rs` line 209's `GuildDispatchResponse` literal,
not guessed. Not exercised against a live P2P listener in this environment
(same Bash/process-spawn constraint as I5).

## I7 — Stale `doc_source` line numbers in `security-claims.toml`

**Change:** re-checked the real current line numbers in
`docs/concepts/SECURITY.md`:
- `encrypt-at-rest-single-choke-point`: `doc_source` corrected from
  `:48` to `:56` (`grep -n "config::open_db"` confirms the encrypt-at-rest
  row is now at line 56).
- `write-gate-hard-rejects-known-patterns`: `doc_source` corrected from
  `:106` to `:114` (`grep -n "Known injection patterns"` confirms the
  Layer 1 hard-rejection row in the write-gate table is now at line 114).

**Verified:**
```
grep -n "config::open_db\|Known injection patterns" docs/concepts/SECURITY.md
```
→ line 56 (encrypt-at-rest row) and line 114 (write-gate hard-rejection
row), matching the corrected `doc_source` values exactly.

## I8 — Tautological third assertion in `host_devmode_refuses.sh`

**Change:** removed the third check block entirely:
```bash
if grep -qi "host.*0\.0\.0\.0" "$CONFIG_DIR/out.log" && ! grep -qi "Forcing host to '127.0.0.1'" "$CONFIG_DIR/out.log"; then
  ...
fi
```
It could never fail independently of the `CRITICAL_SECURITY_TRIGGER` check
above it, since `config.rs` puts both strings on the exact same `warn!()`
call, and it silently no-ops rather than failing loud if the log format
ever drifted. The `CRITICAL_SECURITY_TRIGGER` presence check (kept, and now
the script's only substantive assertion) already covers what matters.

**Verified:** `bash -n` clean; read-through confirms the removed block is
gone and no other code depended on it (it never set any state used later).

## M6 — Filename rename to match the current claim id

**Change:** `git mv scripts/ci/claims/host_devmode_refuses.sh
scripts/ci/claims/host_devmode_force_corrects.sh`, and updated
`security-claims.toml`'s `host-devmode-force-corrects-with-warning` claim's
`script` field to point at the new path.

**Verified:**
```
git status --short scripts/ci/claims/
```
shows `RM scripts/ci/claims/host_devmode_refuses.sh ->
scripts/ci/claims/host_devmode_force_corrects.sh` (a real rename, tracked
by git). `python -c "import tomllib; ..."` confirms the TOML still parses
and the `script` value matches the new filename.

## Other verification run at the end

```
python scripts/ci/claims/run_claims.py --static-only
```
```
claim                                         type       result   detail
----------------------------------------------------------------------------------------------------
p2p-dispatch-wires-peer-approval              static     PASS     found 2 real match(es)
host-devmode-force-corrects-with-warning      dynamic    SKIP     skipped (--static-only)
encrypt-at-rest-single-choke-point            static     PASS     all matches excluded (comments/tests/exclude_file)
p2p-rejects-unapproved-peer                   dynamic    SKIP     skipped (--static-only)
write-gate-hard-rejects-known-patterns        dynamic    SKIP     skipped (--static-only)
```
Exit code 0. Both static claims genuinely evaluate (not just skip) and
pass against the real, current repo state, including through the new
brace-depth `#[cfg(test)]` exclusion logic and the `--with-filename` fix.

`cargo check -p tylluan-link --bin unapproved_peer_probe` and
`cargo check -p tylluan-link` both finish clean with no warnings.

`bash -n` passes on all three dynamic-claim scripts.

`git status --short` after all edits shows exactly the intended file set
touched: `.github/workflows/ci.yml`,
`crates/tylluan-link/src/bin/unapproved_peer_probe.rs`,
`docs/reference/security-claims.toml`, the
`host_devmode_refuses.sh -> host_devmode_force_corrects.sh` rename, and
`p2p_rejects_unapproved.sh` / `write_gate_rejects.sh` / `run_claims.py`.
No unrelated files were modified (a large set of pre-existing
CRLF/LF-normalization-only diffs elsewhere in the repo, visible in `git
status`, predate this session and were left untouched).

## Not exercised live (documented limitation)

The three dynamic claim scripts were not run against a real live kernel in
this session. This environment's convention (documented in this
workspace's own project instructions) is to never spawn long-running
process-like binaries (kernels, servers) via the Bash tool on this Windows
setup, since antivirus/sandbox interference makes that unreliable — such
commands are meant to be handed to the user to run in their own terminal.
All fixes to the dynamic scripts were instead verified by: (a) `bash -n`
syntax checks, (b) direct confirmation of every literal string/exit-code
assumption against the real Rust source it targets
(`handler_remember.rs`, `p2p.rs`), and (c) a portability check of the
`set -m` / `kill -- -$PID` process-group approach in this environment's
actual bash. A full live run of `python3 scripts/ci/claims/run_claims.py`
(all 5 claims, dynamic included) should still be done once against a real
`cargo build --release` kernel + probe binary before merging to `main`,
ideally by letting CI itself run it on the next push.

## Commit

Committed locally (not pushed), one commit covering all of the above.
