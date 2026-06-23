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
- [ ] Published benchmarks with reproducible methodology
- [ ] 3+ end-to-end examples in `examples/`
- [ ] GitHub Discussions active with community engagement
- [ ] Automated security tests (intent filter, ACL, rate limiter)
- [ ] Complete SQLCipher integration across all database modules
- [ ] Dashboard screenshots from Tylluan's own kernel (not ForjaMCPo3)
- [ ] Fix all compiler warnings
- [ ] First external contributor PR merged

Success criteria:
- 10+ GitHub issues with technical feedback
- 1+ external contributor
- Benchmarks independently reproduced by at least 1 person

## v0.3.0 — Federation

**Goal:** Connect multiple Tylluan instances securely.

Planned:
- [ ] Real federation protocol (not just crypto primitives)
- [ ] NAT traversal (Libp2p or WireGuard)
- [ ] Asymmetric cryptography (Ed25519) for node signing
- [ ] Peer discovery (DHT or bootstrap nodes)
- [ ] Cross-instance memory sharing with provenance tracking

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
