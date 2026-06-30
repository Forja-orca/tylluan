# Tylluan — Status

> Source of truth for the verified technical state. Updated on each release.
> Last updated: 2026-07-01 (v0.6.1 release)

## CI

| Job | Status |
|-----|--------|
| Rust — build + test | ✅ pass |
| Rust — cargo-deny (licenses + advisories + bans) | ✅ pass |
| Python — lint + test | ✅ pass |
| Dashboard — lint | ✅ pass |
| Rust — security audit tests | ✅ pass |

**Commit:** `d363b92` · All 5 jobs green as of 2026-07-01.

---

## Version

**v0.6.1** — Model Portability release (P5 config-driven embeddings + P6 install profiles + P7 reindex endpoint).

---

## What works (verified)

### Kernel (Rust)
- `tylluan-nexus` binary: tokio + axum HTTP server, MCP over SSE and HTTP Streamable
- `tylluan-cli` binary: `start / stop / status / logs / connect / download-models / install --profile=portable|clinic|server` (P6)
- 5 sovereign MCP tools: `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`
- SQLite-backed persistent memory (SilvaDB) with configurable embeddings (bge-m3/bge-small/nomic/none) + BM25 hybrid search + Jina Reranker; `embedding_model = "none"` for zero-download BM25-only mode; `vector_dimensions` derived dynamically from model (P5)
- `POST /api/v1/memory/reindex` — manual embedding reindex trigger with SSE progress events (`reindex_started/progress/finished`) and 200ms CPU throttle (P7)
- Knowledge graph: entity extraction, triple storage, semantic clustering
- Security layer: rate limiter, circuit breaker, execution guard, per-guild ACL, intent filter (30 automated security tests)
- Federation: SQLite peer persistence, push/pull/bidirectional sync, provenance tracking, echo-loop prevention, auto-sync background task
- Ed25519 node identity + node signing (M12) — mesh-ready keypairs
- STUN NAT traversal + mDNS LAN autodiscovery (M12)
- DHT Kademlia: 256 K-buckets, Ed25519 XOR metric, mainline BitTorrent DHT bootstrap (M14-A)
- Gossip protocol: symmetric push-pull, LRU entry store (configurable max), anti-entropy cursor tracking, JSON persistence (M14-B)
- Noise Protocol XK encrypted transport: Ed25519→X25519 key conversion, 3-message handshake, ChaCha20-Poly1305 AEAD, length-prefixed async framing (M14-C)
- OAuth 2.0 + PKCE local server
- ChaCha20-Poly1305 encryption for federation payloads; optional SQLCipher for DB at rest
- Self-healing: doctor module, background maintenance, hormone-based load signalling
- Docker support (verified clean boot via `tylluan.docker.toml`)
- **293 lib tests passing** (250 kernel + 43 link) · integration suite requires live kernel
- Zero `openssl-sys` in dep tree — pure rustls-tls on all platforms, cross-compile clean

### Binary distribution (M13 + v0.6.0)
- Pre-compiled releases for 4 targets:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu` (Raspberry Pi 4+ — new in v0.6.0)
  - `aarch64-apple-darwin` (Apple Silicon)
  - `x86_64-pc-windows-msvc`
- `install.sh` / `install.ps1` — curl-pipe and irm-pipe installers
- Installs to `~/.tylluan/bin/`, adds to PATH, prints MCP config + token hint

### Python guilds
- ~47 guilds in `guilds/core/` via FastMCP (bash, git, filesystem, code, vision, websearch, coloquio, scrapling, deep_web_research, comfy_ui, n8n_bridge, code_graph, and more)
- Guild catalog in `registry.json`; lazy on-demand loading
- Security: `_security.py` per-guild ACL layer

### Dashboard
- React + Vite dashboard in `dashboard/`; builds clean via pnpm
- Real-time monitoring, guild status, knowledge graph viewer
- Profile chip (Portable·BM25 / Clinic·BGE-Small / Server·BGE-M3) in Overview (P6 UX)
- Reindex button + amber progress bar driven by SSE events (P7 UX)
- Dynamic BM25 banners with context-specific instructions per profile (P6 UX)

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
- M14-D through M14-E (cross-datacenter routing, mesh test harness) not yet implemented
- Noise transport (M14-C) wired to federation HTTP sync endpoints (encrypt_for_peer/decrypt_from_peer in federation/mod.rs); XK pattern for TCP mesh sessions, NK pattern for HTTP payloads. Not yet connected to guild execution channels.

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
