# Tylluan — Status

> Source of truth for the verified technical state. Updated on each release.
> Last updated: 2026-07-01 (v0.9.0 release)

## CI

| Job | Status |
|-----|--------|
| Rust — build + test | ✅ pass |
| Rust — cargo-deny (licenses + advisories + bans) | ✅ pass |
| Python — lint + test | ✅ pass |
| Dashboard — lint | ✅ pass |
| Rust — security audit tests | ✅ pass |

**Commit:** [488b68f](https://github.com/Forja-orca/tylluan/commit/488b68f0a04944b0b1bc67a7b8e5c82e3650228e) · 270 kernel + 53 link + 1 evals = **324 total** green as of 2026-07-01.

---

## Version

**v0.9.0** — Graph-Augmented Local RAG release (LinearRAG/LightRAG local graph traversal + batch embeddings in FastEmbed ONNX + HNSW index via instant-distance + retrieval baseline benchmark + semantic coloquio search P4).

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
- Guild catalog auto-discovered from `guilds/` at startup — zero-config for new guilds. 34 `description_override()` entries preserve routing quality (M3)
- `--features bundled-dashboard` embeds React build into binary at compile time via rust-embed; disk fallback preserved for dev (M7)
- `build_contextual_text()` prepends `[source_file > heading_path]` before embedding — zero overhead when metadata absent (Contextual Retrieval)
- Exponential half-life decay `weight * 0.5^(hours/half_life)` computed in Rust, configurable `decay_half_life_hours` in `[silva]` tylluan.toml (default 336h = 14d). Type-specific rates per node type (M1)
- Agent Core Memory: `AgentProfile` gains `persona: String` + `preferences: serde_json::Value`; kernel tools `agent_get_persona` / `agent_set_persona` (under `tylluan_recall`/`tylluan_remember` subtool routing) — CONTRACT-01 unchanged (P0-A)
- Coloquio→SilvaDB episodic flywheel: background `tokio::spawn` every 60s ingests Coloquio turns into SilvaDB as `episodic` nodes; deterministic IDs `coloquio:{channel}:{turn}`; 100ms throttle; watermark-based dedup (P0-B)
- M2 Hybrid Search v2: SilvaDB schema v11 adds FTS5 virtual table `nodes_fts`; `search()` uses BM25 (`bm25(nodes_fts, 10.0, 5.0, 5.0)`) with LIKE fallback; `search_hybrid()` applies entity boost ×1.25 post-RRF (P1)
- DST harness: `crates/tylluan-link/tests/gossip_dst.rs` — 3 InMemoryTransport-based GossipEngine tests (normal sync, partition graceful failure, bidirectional convergence); `GossipEngine::local_node_id()` accessor added (P2)
- Startup optimization: `builtin_catalog()` cached via `std::sync::OnceLock` — eliminates double filesystem scan at startup (~10s → ~5s) (P3)
- HNSW index via `instant-distance`: `hnsw.rs` + schema v12 (`hnsw_index` BLOB table) + fast path in `search.rs` (HNSW ≥12k nodes → IVF → linear fallback); rebuild scheduler every 10min; survives restart via SQLite BLOB (v0.9.0)
- LinearRAG local graph traversal: `degree_centrality` (SQL-native) + `local_query_graph` (Personalized PageRank local + degree boost) integrated into RRF hybrid search (v0.9.0)
- Batch Embeddings: Callers connected to `embed_batch` in `embeddings.rs`. Reindex loop in main.rs processed in chunks of 32 with 500ms sleep (v0.9.0)
- Retrieval baseline: `tylluan-evals` benchmark — Recall@5: 60%, Precision@5: 12%, p50: 1.3ms, p95: 1.9ms; persisted in `benchmarks/baseline_v0.9.0.json` (v0.9.0)
- Semantic Coloquio Search (P4): `tylluan_recall` parses optional `"episodic": bool` argument and filters by `"episodic"` node type via `search_hybrid` (v0.9.0)
- **270 kernel lib tests passing** + 53 link tests + 1 evals test = **324 total** · integration suite requires live kernel
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
