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
| ASI06 | Memory Poisoning | ⚠️ No content validation on tylluan_remember |
| ASI07 | Insecure Inter-Agent | ✅ Localhost-only mitigates |
| ASI08 | Cascading Failures | ✅ Supervisor with crash loop detection |
| ASI09 | Trust Exploitation | ⚠️ No confidence warnings on tylluan_think |
| ASI10 | Rogue Agents | ✅ Emergency kill switch (POST /api/v1/admin/emergency-kill) and per-guild kill |

See [DISCLAIMER.md](../../DISCLAIMER.md) for operator responsibilities.

## Dependency Scanning

```bash
cargo audit          # check CVEs in Rust deps
cargo deny check     # license + advisory compliance
```

Run before every release tag.
