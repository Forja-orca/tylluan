# Tylluan — Status

> Source of truth for the verified technical state. Updated on each release.
> Last updated: 2026-06-28

## CI

| Job | Status |
|-----|--------|
| Rust — build + test | ✅ pass |
| Rust — cargo-deny (licenses + advisories + bans) | ✅ pass |
| Python — lint + test | ✅ pass |
| Dashboard — lint | ✅ pass |
| Rust — security audit tests | ✅ pass |

**Commit:** `a315b4b` · All 5 jobs green as of 2026-06-28.

---

## Version

**v0.3.0** — Federation release.

---

## What works (verified)

### Kernel (Rust)
- `tylluan-nexus` binary: tokio + axum HTTP server, MCP over SSE and HTTP Streamable
- 5 sovereign MCP tools: `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`
- SQLite-backed persistent memory (SilvaDB) with BGE-M3 embeddings + BM25 hybrid search + Jina Reranker
- Knowledge graph: entity extraction, triple storage, semantic clustering
- Security layer: rate limiter, circuit breaker, execution guard, per-guild ACL, intent filter
- Federation: SQLite peer persistence, push/pull/bidirectional sync, provenance tracking, echo-loop prevention, auto-sync background task
- OAuth 2.0 + PKCE local server
- ChaCha20-Poly1305 encryption for federation payloads; optional SQLCipher for DB at rest
- Self-healing: doctor module, background maintenance, hormone-based load signalling
- Docker support (verified clean boot via `tylluan.docker.toml`)
- **250 lib tests passing** (`cargo test -p tylluan-kernel --lib`)

### Python guilds
- ~47 guilds in `guilds/core/` via FastMCP (bash, git, filesystem, code, vision, websearch, coloquio, scrapling, deep_web_research, comfy_ui, n8n_bridge, code_graph, and more)
- Guild catalog in `registry.json`; lazy on-demand loading
- Security: `_security.py` per-guild ACL layer

### Dashboard
- React + Vite dashboard in `dashboard/`; builds clean via pnpm
- Real-time monitoring, guild status, knowledge graph viewer

### Integrations
- MCP client configs in `integrations/` for: Claude Desktop, Claude Code, Cursor, VS Code, LM Studio (SSE), Qwen Desktop, Antigravity, Hermes

---

## Crate structure

| Crate | Purpose |
|-------|---------|
| `tylluan-kernel` | Core binary + library: MCP, memory, federation, security |
| `tylluan-common` | Shared types, error types, constants |
| `tylluan-evals` | Benchmark harness: Recall@N, Precision@N, latency percentiles |
| `tylluan-cli` | Interactive REPL client |
| `tylluan-link` | Federation networking primitives |
| `tylluan-gui` | Tauri desktop GUI (early stage) |

---

## What is NOT production-ready

- No external security audit
- No community validation (0 external contributors)
- No independent benchmark reproduction
- SQLCipher encryption not wired across all DB modules
- NAT traversal / DHT peer discovery not implemented (planned v0.4.0)
- Kernel is a research lab — executes real code on your machine

---

## Running

```bat
tylluan-mcp.bat
```

Verify: `curl http://127.0.0.1:3030/health`

Dashboard (dev): `cd dashboard && pnpm dev` → `http://localhost:5173`

See README.md for full setup.
