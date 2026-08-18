# Design: Security Claims CI Gate

**Date:** 2026-08-18
**Status:** Draft, pending José's review
**Author:** Claude (ForjaMCPo3), directed by José

## Problem

Tylluan's `ROADMAP_O3.md` documents 7-8 "wired but not connected" incidents across
7 weeks: a documented security/architecture property (in `SECURITY.md`,
`SECURITY_FEDERATION.md`) that the running code did not actually implement.
Two consecutive rounds of *external* multi-model audit found these — not
Tylluan's own 665-test suite, not internal review. Named examples already on
record:

- P2P dispatch listener bound to `0.0.0.0` by default with no pubkey check
  before executing `bash`/`git`/`filesystem`/`docker` tool calls — directly
  contradicted `SECURITY.md` ("Listens on 127.0.0.1 only — never 0.0.0.0")
  and `SECURITY_FEDERATION.md` ("peers must be explicitly approved").
- `sse_handler` silently discarded real client headers on `POST /sse`,
  forcing one dialect regardless of what the client actually requested —
  caught only by manual `curl` before/after a real client hung.
- `coherence_gate.rs` called `llama_backend` without an `Authorization`
  header, working only when auth was disabled entirely.
- Federation gossip claimed "never sent in the clear" while a legacy
  plaintext fallback path existed for real.
- `encrypt_at_rest` was documented as inactive when it was in fact wired
  (the inverse failure mode: a doc that was *more pessimistic* than reality,
  found only because someone finally read `config.rs::open_db()` directly).

**Root cause common to all of them:** the existing test suite exercises
functions in isolation. Nothing in CI ever asks "does the live, running
kernel actually behave the way `SECURITY.md` says it does, from outside,
the way an attacker or an external auditor would check it." That question
was only ever asked by humans (external auditors, or José manually with
`curl`), not by an automated, repeatable gate.

## Goal

Close that gap with an automated CI job that fails a PR when a documented
security/architecture claim no longer holds against the real code or the
real running kernel -- turning what external audits do by hand into a
permanent, cheap, repeatable check that runs on every push, not once every
few weeks when someone happens to audit again.

## Non-Goals

- This does not replace external audits -- it raises the floor so the *known
  and already-documented* claims can never silently regress again. Novel
  vulnerability classes still need human/external review.
- Not a general "lint every sentence in every doc" tool. Only claims
  explicitly registered in the manifest are checked -- registering a claim
  is a deliberate act, not automatic text-mining, so the manifest stays
  precise and doesn't drown in false positives.
- Not solving ASI06 (write-gate design, already closed per `SECURITY.md`)
  or the P2P Ed25519->X25519 mapping (separate, already-tracked roadmap
  item) -- this spec is the *verification mechanism*, not a fix for any
  specific remaining open item.

## Design

### 1. The claims manifest

A new file, `docs/reference/security-claims.toml`, lists every
machine-checkable claim currently made in `SECURITY.md` /
`SECURITY_FEDERATION.md`. Each entry:

```toml
[[claim]]
id = "no-lan-bind-p2p-listener"
doc_source = "docs/concepts/SECURITY.md:7"
statement = "Listens on 127.0.0.1 only -- never 0.0.0.0 in production"
check = "static"
pattern = '0\.0\.0\.0'
scope = ["crates/tylluan-link/src/p2p.rs", "crates/tylluan-kernel/src/transport/http/mod.rs"]
allow_if = "commented_out_or_test_only"   # static checker excludes matches inside #[cfg(test)] blocks and comments

[[claim]]
id = "host-devmode-refuses-to-start"
doc_source = "docs/concepts/SECURITY.md:19"
statement = "Kernel logs a warning and refuses to start if host=0.0.0.0 and dev_mode=true are both set"
check = "dynamic"
script = "scripts/ci/claims/host_devmode_refuses.sh"

[[claim]]
id = "encrypt-at-rest-single-choke-point"
doc_source = "docs/concepts/SECURITY.md:48"
statement = "Every real database goes through config::open_db() -- zero direct rusqlite::Connection::open calls elsewhere"
check = "static"
pattern = 'Connection::open\('
scope = ["crates/tylluan-kernel/src/"]
exclude_file = "crates/tylluan-kernel/src/config.rs"

[[claim]]
id = "p2p-rejects-unapproved-peer"
doc_source = "docs/concepts/SECURITY_FEDERATION.md:13"
statement = "Peers are not discovered automatically -- they must be explicitly approved before dispatch is accepted"
check = "dynamic"
script = "scripts/ci/claims/p2p_rejects_unapproved.sh"

[[claim]]
id = "write-gate-hard-rejects-known-patterns"
doc_source = "docs/concepts/SECURITY.md:106"
statement = "tylluan_remember hard-rejects known injection patterns -- the node is never created"
check = "dynamic"
script = "scripts/ci/claims/write_gate_rejects.sh"
```

`check = "static"` entries are pure grep/ripgrep assertions against the
repo -- cheap, run first, no kernel needed. `check = "dynamic"` entries
start the real kernel binary (already built earlier in the same CI job,
reusing the existing build step, not a separate compile) and run a script
that exercises the claim from outside via real HTTP/TCP calls, then asserts
the response. This mirrors exactly the `curl` methodology that closed the
`sse_handler` bug for real the first time.

### 2. The CI job

New job in `.github/workflows/ci.yml`, `Security claims gate`, runs after
the existing build step (reuses the compiled binary, no extra build cost):

1. Parse `security-claims.toml`.
2. Run every `static` claim's grep check; collect pass/fail per claim.
3. Start the kernel once (`dev_mode=false`, a throwaway `tylluan.toml`,
   ephemeral port) for the `dynamic` claims; run each claim's script against
   it; collect pass/fail.
4. Stop the kernel.
5. Print a table: claim id, doc source, pass/fail. Any failure fails the
   job (blocks merge).

### 3. Runner scripts (`scripts/ci/claims/*.sh`)

Small, single-purpose bash scripts, one per dynamic claim, each doing
exactly one real network/process assertion and exiting non-zero on
mismatch. Example shape for `p2p_rejects_unapproved.sh`: start a second,
throwaway P2P identity that is *not* in the running kernel's `peers.db`,
attempt a dispatch call over the real Noise XK listener, assert the
response is a rejection, not tool execution.

### 4. Registering a new claim (the actual process change)

The real behavior change for the team: **whenever `SECURITY.md` or
`SECURITY_FEDERATION.md` gains a new "this is safe because X" sentence, a
claim entry with a real check is added to the manifest in the same PR** --
this becomes part of definition-of-done for any security-relevant change,
enforced by a CI check that diffs the two docs against the manifest's
`doc_source` line numbers and fails if a new claim-shaped sentence
(heuristic: contains "only", "never", "must", "always") has no matching
manifest entry. This is a soft nudge (can be silenced with an explicit
`# claims-exempt: <reason>` doc comment for non-checkable claims like "no
current plan"), not a hard block -- the goal is making *forgetting* to
register a claim visible, not making every sentence mandatory-checkable.

## Testing

- Each new dynamic script gets a red/green sanity check during
  implementation: intentionally break the property in a throwaway branch,
  confirm the script fails; fix it back, confirm it passes. Same
  red-green discipline as `superpowers:test-driven-development`.
- The gate itself gets a CI test: a fixture PR (not merged) that
  intentionally violates one static claim (e.g. adds a `0.0.0.0` bind)
  should fail the new job -- run once manually before shipping the gate,
  not kept as a permanent fixture.

## Risks

- **False sense of security**: registering a claim and passing its check
  is not equivalent to "this is unhackable" -- only that the *specific
  documented property* holds against the *specific check written*. Keep
  this explicit in the manifest's own header comment.
- **Maintenance cost**: dynamic checks that start a real kernel add CI
  time. Mitigated by reusing the already-built binary and running all
  dynamic checks against one kernel instance, not one per claim.
- **Manifest drift**: a claim's check can go stale if the code path it
  targets gets refactored under a different file/function name. The
  `doc_source` line-number nudge (section 4) catches new *unregistered*
  claims but not claims whose *check* silently stopped covering the real
  path -- worth a periodic (not per-PR) manual review, out of scope for
  this spec's first version.

## Open Questions for José

1. Confirm the initial claim set (5 claims drafted above) is the right
   starting scope, or trim/add before implementation.
2. Who implements: assign to Deep (backend Rust + CI is his usual lane) or
   keep it as Claude's own implementation given the security sensitivity?
