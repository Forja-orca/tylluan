# Tylluan Roadmap

## v0.1.0 — Alpha Release (current)

**Status:** Published. Research laboratory.

What's included:
- Rust kernel (tokio + axum) with 5 sovereign MCP tools
- 47 Python guilds (bash, git, filesystem, code, vision, web search, and more)
- Persistent memory with BGE-M3 embeddings + BM25 + Jina Reranker
- Knowledge graph (SilvaDB) with entity extraction and clustering
- React dashboard with real-time monitoring
- Security primitives: rate limiter, circuit breaker, execution guard
- Opt-in security: Docker sandbox, per-guild ACL, intent filter, SQLCipher encryption
- MCP native (SSE + HTTP Streamable) — works with Claude, Cursor, VS Code, LM Studio
- Docker support (verified clean boot)

What's NOT included:
- Community validation (0 external users)
- Independent benchmarks
- Production hardening
- Federation protocol

## v0.2.0 — Community Validation

**Goal:** Get real users, real feedback, real benchmarks.

Planned:
- [x] Published benchmarks with reproducible methodology (`benchmarks/run.py`)
- [x] 3+ end-to-end examples in `examples/` (5 examples including autonomous chain and BWC demo)
- [x] GitHub Discussions active with community engagement (Discussion #2 live)
- [x] Dashboard screenshots from Tylluan's own kernel (not ForjaMCPo3)
- [x] Fix all compiler warnings (0 warnings as of v0.2.0)
- [x] M10 Bounded Work Contracts — finite multi-agent coordination protocol
- [x] Automated security tests in CI (intent filter, ACL, rate limiter) — 30 tests in `security_audit.rs`
- [ ] Complete SQLCipher integration across all database modules
- [ ] First external contributor PR merged

Success criteria:
- 10+ GitHub issues with technical feedback
- 1+ external contributor
- Benchmarks independently reproduced by at least 1 person

## v0.3.0 — Federation

**Status:** Complete.

**Goal:** Connect multiple Tylluan instances securely over LAN/VPN.

Delivered:
- [x] SQLite peer persistence (`data/peers.db`) — replaces fragile TOML storage (M11-A)
- [x] `auth_token` / `shared_secret` split — HTTP bearer auth and ChaCha20 key are separate fields (M11-A)
- [x] Push sync: `POST /api/v1/federation/sync` — encrypt and push local nodes to all approved peers (M11-A)
- [x] Pull sync: `GET /api/v1/federation/sync/export` + `POST /api/v1/federation/sync/pull?peer=N` (M11-B)
- [x] Bidirectional sync: `POST /api/v1/federation/sync/both?peer=N` — push then pull in one call (M11-B)
- [x] Node provenance: `federation_source TEXT` column in `silva_nodes` — local vs. received nodes distinguishable at SQL level (M11-C)
- [x] Echo-loop prevention: `get_shareable_nodes()` filters `federation_source IS NULL` — received nodes are never re-exported by default (M11-C)
- [x] Provenance query: `GET /api/v1/federation/nodes?source={peer|local}` (M11-C)
- [x] Scheduled auto-sync: background `tokio::spawn` loop driven by `[federation] auto_sync_interval_secs` and `auto_sync_mode` (M11-D)
- [x] Integration test suite: `tests/federation_audit.rs` — 6 tests covering DB, approval gate, token isolation, provenance, echo-loop, auto-sync disable (M11-E)

Out of scope (v0.4.0):
- NAT traversal (Libp2p or WireGuard)
- Asymmetric cryptography (Ed25519) for node signing
- DHT peer discovery

## v1.0.0 — Production Ready

**Goal:** Safe to deploy in real environments.

Requirements (all must be met):
- [ ] External security audit completed
- [ ] 6+ months of community usage without critical vulnerabilities
- [ ] Benchmarks validated by independent parties
- [ ] Kill switch tested under adversarial conditions
- [ ] Documentation reviewed by non-contributors
- [ ] Stable API (no breaking changes for 3+ months)

---

*This roadmap reflects the project's actual state, not aspirational marketing. Items move to "done" only when verified.*
