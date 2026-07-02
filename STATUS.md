# Tylluan — Status

> Source of truth for the verified technical state. Updated on each release.
> Last updated: 2026-07-02 (v0.11.0-dev: M14-D + M14-E complete + ADR-005 M14-F spec)

## CI

| Job | Status |
|-----|--------|
| Rust — build + test | ✅ pass |
| Rust — cargo-deny (licenses + advisories + bans) | ✅ pass |
| Python — lint + test | ✅ pass |
| Dashboard — lint | ✅ pass |
| Rust — security audit tests | ✅ pass |

**Commit:** HEAD `79f2641` · 273 kernel + 81 link + 2 evals = **356 total** green as of 2026-07-02.

---

## Version

**v0.11.0-dev** (HEAD) — M14-D Guild Execution Channels completo (all 4 phases) + M14-E Mesh Test Harness completo. 356 tests, 0 failures.
**v0.10.0** (tag) — El sistema que sabe si funciona (retrieval quality delta + degree bias fix + fault DST + M14-D spec).

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
- DST harness: `gossip_dst.rs` — 6 tests: normal sync, partition graceful failure, bidirectional convergence, 3-node transitive propagation, message loss + retry, LWW conflict resolution (M6-full)
- `PartitionableTransport<T>` in `tylluan-link`: 5 switchable modes (Transparent, Drop(f64), Partition, Latency(Duration), Error) for deterministic fault injection in tests (M6-full)
- `fault_dst.rs` — 4 realistic fault scenarios: `partition_heal_convergence` (Partition→Transparent→converge), `latency_injection` (Latency 100ms, +150ms measurable), `drop_rate_eventual_convergence` (Drop 0.3, ≤10 rounds), `error_mode_graceful_failure` (Error mode, no state corruption) (v0.10.0 P1)
- LinearRAG degree bias corrected (v0.10.0 P2-fix): `local_query_graph` in `graph.rs` and `dual_retrieval.rs` now divide by degree factor instead of multiply — penalizes generic hub nodes, improves MRR for specific queries. New test `test_local_query_graph_degree_penalty` added.
- Retrieval quality benchmark v0.10.0: 44 nodes, 40 edges, 10 queries (5 original + 5 multi-hop). With LinearRAG graph ON: Recall@5 20%, Recall@10 30%, MRR 23.15%, p50 5.65ms. Delta vs graph OFF: +2.5% Recall@5, +5% Recall@10, −0.1% MRR (pre-fix). Results with fake 12-dim embeddings; real BGE-M3 delta expected higher.
- ADR-004 M14-D Guild Execution Channels spec published: `docs/architecture/M14D_dispatch_spec.md` — Capability-Aware Hybrid Routing, 4-phase implementation plan (~8 sessions), preserves CONTRACT-01
- M14-D Phase 1 — Capability Registry: `HardwareCaps { ram_mb, has_gpu, load_avg }` added to `GossipEntry`; `CapabilityRegistry` in `tylluan-link/src/capability.rs` with TTL-based peer store, `prune_expired()`, `ingest_from_engine()`; 6 unit tests (v0.11.0-dev)
- M14-E — Mesh Integration Test Harness: `tests/mesh_simulation.rs` (full-mesh A↔B↔C, star topology B-hub, split-brain + heal LWW); `tests/dispatch_dst.rs` (GPU peer selection, capability filter, CB fallback, DispatchQueue FIFO/overflow/TTL); `DispatchQueue` moved from kernel to `tylluan-link/src/dispatch.rs`. **M14-D + M14-E both complete.** (v0.11.0-dev)
- M14-D Phase 4 — Fallback + Remote Dispatch: `DispatchQueue` (VecDeque + TTL 300s, max 1000); `HttpState` gains `dispatch_router` + `dispatch_queue`; `GET /api/v1/guilds/peers` returns CapabilityRegistry view; `POST /api/v1/guilds/dispatch/remote` routes via DispatchRouter (local or HTTP forward), fallback-enqueues on failure, wires record_success/record_failure. **M14-D milestone complete.** (v0.11.0-dev)
- M14-D Phase 3 — Guild Dispatch Protocol: `GuildDispatchRequest/Response` structs (Serde); `send/receive_dispatch_request/response` using Noise NK (`noise_encrypt/decrypt_payload` over `dyn MeshTransport`); `POST /api/v1/guilds/dispatch/execute` endpoint — receives request, calls `registry.call_tool()`, returns response with `executor_id` + `duration_ms`; CONTRACT-01 preserved (v0.11.0-dev)
- M14-D Phase 2 — DispatchRouter: `dispatch.rs` in `tylluan-link` — scoring `(1-load)×(1000/latency)×gpu_mult`, circuit breaker (3 failures + 60s cooldown), default latency 0.0 favoring unknown peers; `HttpState` gains `capability_registry`; gossip tick wires `ingest_from_engine + prune_expired`; 5 unit tests (v0.11.0-dev)
- Startup optimization: `builtin_catalog()` cached via `std::sync::OnceLock` — eliminates double filesystem scan at startup (~10s → ~5s) (P3)
- HNSW index via `instant-distance`: `hnsw.rs` + schema v12 (`hnsw_index` BLOB table) + fast path in `search.rs` (HNSW ≥12k nodes → IVF → linear fallback); rebuild scheduler every 10min; survives restart via SQLite BLOB (v0.9.0)
- LinearRAG local graph traversal: `degree_centrality` (SQL-native) + `local_query_graph` (Personalized PageRank local + degree penalty, corrected in v0.10.0) integrated into RRF hybrid search (v0.9.0)
- Batch Embeddings: Callers connected to `embed_batch` in `embeddings.rs`. Reindex loop in main.rs processed in chunks of 32 with 500ms sleep (v0.9.0)
- Retrieval baseline: `tylluan-evals` benchmark — Recall@5: 60%, Precision@5: 12%, p50: 1.3ms, p95: 1.9ms; persisted in `benchmarks/baseline_v0.9.0.json` (v0.9.0)
- Semantic Coloquio Search (P4): `tylluan_recall` parses optional `"episodic": bool` argument and filters by `"episodic"` node type via `search_hybrid` (v0.9.0)
- Security hardening (P-security): `sanitize_query()` redacts `token=`/`Authorization=` from `info!` logs; `extract_token()` fixes ACL role resolution for `?token=` query-string auth — no longer falls to `default_role` (v0.9.0)
- **273 kernel lib tests passing** + 81 link tests + 2 evals tests = **356 total** · integration suite requires live kernel
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
- M14-F (P2P direct TCP dispatch over Noise XK) — ADR-005 spec complete (`docs/architecture/M14F_p2p_dispatch_spec.md`); implementation backlog post-v0.11.0. Decisions: pool with TTL+keepalive, Option A transparent routing, tcp_addr from GossipEntry.addr + HardwareCaps.supports_p2p
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
