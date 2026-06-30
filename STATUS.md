# Tylluan — Status

> Source of truth for the verified technical state. Updated on each release.
> Last updated: 2026-06-30

## CI

| Job | Status |
|-----|--------|
| Rust — build + test | ✅ pass |
| Rust — cargo-deny (licenses + advisories + bans) | ✅ pass |
| Python — lint + test | ✅ pass |
| Dashboard — lint | ✅ pass |
| Rust — security audit tests | ✅ pass |

**Commit:** `5148f21` · All 5 jobs green as of 2026-06-30.

---

## Version

**v0.5.0** — Mesh Fabric release (M14-A DHT Kademlia delivered).

---

## What works (verified)

### Kernel (Rust)
- `tylluan-nexus` binary: tokio + axum HTTP server, MCP over SSE and HTTP Streamable
- `tylluan-cli` binary: `start / stop / status / logs / connect / download-models`
- 5 sovereign MCP tools: `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`
- SQLite-backed persistent memory (SilvaDB) with BGE-M3 embeddings + BM25 hybrid search + Jina Reranker
- Knowledge graph: entity extraction, triple storage, semantic clustering
- Security layer: rate limiter, circuit breaker, execution guard, per-guild ACL, intent filter (30 automated security tests)
- Federation: SQLite peer persistence, push/pull/bidirectional sync, provenance tracking, echo-loop prevention, auto-sync background task
- Ed25519 node identity + node signing (M12) — mesh-ready keypairs
- STUN NAT traversal + mDNS LAN autodiscovery (M12)
- DHT Kademlia: 256 K-buckets, Ed25519 XOR metric, mainline BitTorrent DHT bootstrap (M14-A)
- OAuth 2.0 + PKCE local server
- ChaCha20-Poly1305 encryption for federation payloads; optional SQLCipher for DB at rest
- Self-healing: doctor module, background maintenance, hormone-based load signalling
- Docker support (verified clean boot via `tylluan.docker.toml`)
- **454 tests passing** (250 kernel lib + 174 kernel integration + 30 link lib)

### Binary distribution (M13)
- Pre-compiled releases for linux-x64, mac-arm64, win-x64
- `install.sh` / `install.ps1` — curl-pipe and irm-pipe installers
- Installs to `~/.tylluan/bin/`, adds to PATH, prints MCP config + token hint

### Python guilds
- ~47 guilds in `guilds/core/` via FastMCP (bash, git, filesystem, code, vision, websearch, coloquio, scrapling, deep_web_research, comfy_ui, n8n_bridge, code_graph, and more)
- Guild catalog in `registry.json`; lazy on-demand loading
- Security: `_security.py` per-guild ACL layer

### Dashboard
- React + Vite dashboard in `dashboard/`; builds clean via pnpm
- Real-time monitoring, guild status, knowledge graph viewer

### Integrations
- MCP client configs in `integrations/` for: Claude Desktop, Claude Code, Cursor, VS Code, LM Studio (SSE), Qwen Desktop, Antigravity

---

## Crate structure

| Crate | Purpose |
|-------|---------|
| `tylluan-kernel` | Core binary + library: MCP, memory, federation, security |
| `tylluan-common` | Shared types, error types, constants |
| `tylluan-link` | Federation networking: mesh identity, DHT, NAT, mDNS |
| `tylluan-cli` | `start / stop / status / logs / connect / download-models` |
| `tylluan-evals` | Benchmark harness: Recall@N, Precision@N, latency percentiles |
| `tylluan-gui` | Tauri desktop GUI (early stage, not shipped) |

---

## What is NOT production-ready

- No external security audit
- No community validation (0 external contributors)
- No independent benchmark reproduction
- Kernel is a research lab — executes real code on your machine
- M14-B through M14-E (gossip, Noise Protocol, cross-datacenter, test harness) not yet implemented

---

## Running

```bash
# Binary install (recommended)
tylluan-cli start

# From source
cargo run --release -p tylluan-cli -- start
```

Verify: `curl http://127.0.0.1:3030/health`

Dashboard (dev): `cd dashboard && pnpm dev` → `http://localhost:5173`

See README.md for full setup.
