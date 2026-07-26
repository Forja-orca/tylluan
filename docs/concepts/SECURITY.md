# Security Policy — Tylluan o3

## Threat Model

Tylluan is designed as a **localhost-only** sovereign hub. The attack surface is intentionally minimal:

- Listens on `127.0.0.1` only — never `0.0.0.0` in production
- No inbound connections from the internet
- Bearer token auth on all MCP endpoints (disabled only in `dev_mode`)
- Guild subprocesses run with the same OS user as the kernel

## Critical Invariants

### Never ship together
```toml
host = "0.0.0.0"   # LAN-reachable
dev_mode = true     # auth disabled
```
This combination is an unauthenticated LAN RCE. The kernel logs a warning and refuses to start if both are set.

### Token management
- Bearer token lives in `.tylluan-token` at the project root (`.gitignore`d) for source builds; `~/.tylluan/.tylluan-token` for binary installs
- Backup: copy manually to a secure location outside the repo (e.g. `~/.tylluan/secrets`)
- Never write the token value in tracked files
- Rotate via `POST /api/v1/admin/rotate-token`

### Federation Security

Federation adds peer-to-peer attack surface beyond the local threat model. See [SECURITY_FEDERATION.md](SECURITY_FEDERATION.md) for the dedicated threat model covering:

- Malicious peer injecting false memories (approval gate = only mitigation)
- Provenance tracking without per-node cryptographic signatures
- Echo-loop and revocation gaps
- DHT poisoning (limited to peer discovery, not content)
- Network-level encryption coverage (Noise XK/NK)

## Known Limitations (Alpha)

| Area | Status | Notes |
|------|--------|-------|
| TLS | ❌ Not implemented | Localhost-only mitigates this |
| Rate limiting | ⚠️ Basic | Per-IP counting, no sliding window |
| Guild isolation | ⚠️ Same user | Guilds share OS user with kernel |
| Audit log | ✅ Active | All 5 sovereign tool calls logged to `data/audit.db` |
| Input validation | ✅ | Intent strings sanitized before guild routing |
| Docker Sandbox | ✅ Active | Windows UNC path prefix (`\\?`) is automatically stripped for cross-platform support. |
| ACL Check | ✅ Active | Full role-based validation applied to both `tylluan_do` and direct guild tool routes. |
| Encryption at Rest | ❌ Inactive | `open_db` is implemented in `config.rs` but not utilized in the codebase; databases are still opened via direct `Connection::open` in plaintext. |

## Reporting Vulnerabilities

Report security vulnerabilities via **GitHub Private Vulnerability Reporting**: https://github.com/forja-orca/tylluan/security/advisories/new. See [SECURITY.md](../../SECURITY.md) for the full disclosure process.

## OWASP Top 10 for Agentic Applications (2026)

Tylluan's posture against [OWASP ASI 2026](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/):

| Code | Risk | Tylluan Status |
|------|------|----------------|
| ASI01 | Agent Goal Hijack | ⚠️ No prompt injection filtering |
| ASI02 | Tool Misuse | ✅ Opt-in Docker sandbox for bash/code guilds |
| ASI03 | Identity Abuse | ⚠️ agent_id is self-reported |
| ASI04 | Supply Chain | ✅ Guilds loaded from local disk only |
| ASI05 | Code Execution | ✅ Optional Docker sandbox for bash/code guilds |
| ASI06 | Memory Poisoning | 🟡 Partial — `tylluan_remember` still has no ingestion-time validation, but every `tylluan_recall` now passes through the ADR-011 Coherence Gate before results reach the caller (see below) |
| ASI07 | Insecure Inter-Agent | ✅ Localhost-only mitigates |
| ASI08 | Cascading Failures | ✅ Supervisor with crash loop detection |
| ASI09 | Trust Exploitation | ⚠️ No confidence warnings on tylluan_think |
| ASI10 | Rogue Agents | ✅ Emergency kill switch (POST /api/v1/admin/emergency-kill) and per-guild kill |

See [DISCLAIMER.md](../../DISCLAIMER.md) for operator responsibilities.

## Coherence Gate (ADR-011) — recall-path memory poisoning defense

`tylluan_recall` already returns poisoned content as inert text (it is never
executed), but nothing stopped that content from being fed unfiltered into a
future generative model's context window. The **Coherence Gate**
(`security::coherence_gate::CoherenceGate`, in production) sits between
`search_hybrid` and the response on every recall — both the normal path and
the cache-hit path — and applies three layers, cheapest first:

| Layer | Detects | Action | Cost |
|-------|---------|--------|------|
| 1. Known injection patterns | Static regex list (`security/poison_patterns.rs`, 10 patterns — e.g. `[SYSTEM:`, `<\|im_start\|>`, `IGNORE ALL PREVIOUS`) | Eliminated silently | Sub-μs |
| 2. Untrusted provenance | Federation-sourced nodes with low trust weight | Penalized ×0.1, not removed | Sub-ms |
| 3. Semantic drift | Query/content cosine similarity below 0.85, reusing the already-stored BGE-M3 embedding — zero extra inference | Penalized ×0.1, not removed | ~0ms (no re-embedding) |

If more than 50% of results are eliminated or penalized in a single recall,
the response surfaces an explicit warning to the caller. Cumulative counters
since kernel start are exposed at `GET /api/v1/security/coherence-gate/stats`.

This defends against the recall-path variants documented in 2025-2026
agentic-memory-poisoning literature (ShadowMerge, eTAMP, Sleeper Memory
Poisoning — see [ADR-011](../reference/adr/ADR011_learned_reranker_coherence_gate.md)
§1 for primary sources). It does **not** validate content at ingestion time
(`tylluan_remember`) — that remains the ASI06 gap above.

A companion **Signal Loop** records implicit usefulness feedback per recall
(`recall_feedback` table) toward training a learned reranker once 5,000
resolved rows accumulate; progress is exposed at
`GET /api/v1/memory/recall-feedback/stats`.

## Dependency Scanning

```bash
cargo audit          # check CVEs in Rust deps
cargo deny check     # license + advisory compliance
```

Run before every release tag.
