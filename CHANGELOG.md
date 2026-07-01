# Changelog

All notable changes to Tylluan are documented here.

---

## [v0.10.0] — 2026-07-01 — El sistema que sabe si funciona

**Norte estrella:** Validar lo construido en v0.9.0 antes de añadir más capas. Retrieval quality delta + M6-full completo.

### Added

- **M6-full — Fault DST escenarios realistas (P1)**
  - `fault_dst.rs` in `tylluan-link/tests/`: 4 new tests ejercitando los 5 modos de `PartitionableTransport<T>`.
    - `partition_heal_convergence`: Modo `Partition` fuerza fallo, switch a `Transparent` restaura sync y los nodos convergen.
    - `latency_injection`: Modo `Latency(100ms)` — sync exitosa; latencia medida ≥150ms confirma inyección efectiva.
    - `drop_rate_eventual_convergence`: Modo `Drop(0.3)` (30% pérdida) — convergencia eventual garantizada en ≤10 rounds de anti-entropy.
    - `error_mode_graceful_failure`: Modo `Error` — falla limpiamente sin corromper el estado del `GossipEngine`.

### In progress

- **Extended Retrieval Benchmark (P0)**
  - 44 nodes + 40 edges + 10 queries (5 original + 5 multi-hop). `skip_graph` flag for A/B comparison.
  - Results with fake 12-dim deterministic embeddings: Graph ON → Recall@5 20%, Recall@10 30%, MRR 23.15%, p50 5.65ms. Delta vs graph OFF: +2.5%/+5.0% recall, −0.1% MRR, +4ms latency.
  - Output: `benchmarks/benchmark_v0.10.0.json`

- **LinearRAG Degree Bias Fix (P2-fix)**
  - `local_query_graph` (`graph.rs:739`): `pr_score * (1 + deg×0.1)` → `pr_score / (1 + deg×0.1)` — hub nodes penalized instead of boosted.
  - `dual_retrieval.rs` (lines 30, 69): same inversion applied to graph-boosted scores.
  - New test `test_local_query_graph_degree_penalty`: asserts low-degree (deg=1) node outranks high-degree (deg=5) node with slightly lower PR score.
  - Rationale: benchmark showed MRR flat despite recall gain — degree boost was promoting generic hub nodes to top positions.

- **M14-D Guild Dispatch ADR (P3-spec)**
  - `docs/architecture/M14D_dispatch_spec.md` (ADR-004) — Capability-Aware + Latency-Based Hybrid Routing.
  - 4 components: Capability Registry (DHT+Gossip, TTL 5min), Dispatch Algorithm (load+latency scoring), Remote Execution Protocol (JSON over Noise NK, `GuildDispatchRequest/Response`), Fallback Strategy (queue + circuit breaker).
  - CONTRACT-01 preserved: routing is transparent inside `tylluan_do`.
  - 4-phase implementation plan (~8 sessions). Phase 1 (Capability Registry) can start immediately.

### Tests

**273 kernel lib tests + 61 link tests + 1 evals = 335 total** · 0 failures.

---

## [v0.9.0] — 2026-07-01 — Graph-Augmented Local RAG

**Norte estrella:** Zero-token local graph indexing and traversal with batch processing.

**Research basis:** LinearRAG / Tri-Graph paper (ICLR 2026) and instant-distance HNSW.

### Added

- **LinearRAG Local Graph Traversal (P3)**
  - `degree_centrality`: SQL-native edge connectivity calculation chunked in groups of 50 to avoid SQLite parameter limit errors.
  - `local_query_graph`: Graph traversal using Personalized PageRank from vector seeds, boosted by degree centrality: `score * (1.0 + degree * 0.1)`.
  - `search_hybrid` integration: Vector search results (IVF) serve as seeds for `local_query_graph` traversal, with outputs fused via Reciprocal Rank Fusion (RRF).

- **Batch Embeddings — FastEmbed ONNX (P2)**
  - `embed_batch` in `embeddings.rs`: Integrates native FastEmbed batching behind a single ONNX mutex lock with L2 normalization.
  - Callers connected: `embed()` delegates to `embed_batch()`, preventing logic duplication.
  - Reindex loop in `main.rs`: Refactored to process nodes in chunks of 32 with a 500ms sleep between chunks to avoid CPU thread starvation.

- **HNSW Index via instant-distance (P1)**
  - pure-Rust HNSW index using the `instant-distance` crate, fully serializable (`serde`).
  - SilvaDB schema bumped to v12: `hnsw_index` table (BLOB persistent singleton).
  - Search fast path in `search.rs`: HNSW index used if built (threshold >=12k nodes), falling back to IVF and linear searches.
  - Scheduler: Background rebuilder task in `main.rs` triggers every 10 minutes.

- **Retrieval Baseline Benchmark (P0)**
  - New benchmark test: `baseline_v090_benchmark` evaluates search quality across 23 nodes and 5 complex multi-hop queries.
  - Verified baseline: Recall@5: 60%, Precision@5: 12%, latency p50: 1.3ms, p95: 1.9ms.
  - JSON baseline output persisted in `crates/tylluan-evals/benchmarks/baseline_v0.9.0.json`.

- **Semantic Coloquio Search (P4)**
  - Optional `"episodic": bool` parameter parsed in the MCP tool `tylluan_recall`.
  - Integrates a `type_filter` option in `search_hybrid` to filter nodes post-RRF (retaining only `"episodic"` type).
  - Clean adaptation of callers in `dual_retrieval.rs`, `idle_lab.rs`, `autolink.rs`, `api_memory.rs`, and server handlers.

- **Security fixes (P-security)**
  - `sanitize_query()` in `auth.rs`: redacts `token=` and `Authorization=` values to `[REDACTED]` before `info!` logging — prevents bearer token exposure in log collectors.
  - `extract_token()` in `auth.rs`: unified token extraction checking `Authorization` header first, then URL-decoded query string fallback — `resolve_acl_role` now receives the actual bearer on `?token=` auth instead of falling to `default_role`.

- **M6-full — Fault Injection DST**
  - `PartitionableTransport<T>` in `tylluan-link/src/transport.rs`: generic wrapper over any `MeshTransport` with 5 switchable modes: `Transparent` (pass-through), `Drop(f64)` (probabilistic message loss), `Partition` (silent drops on send, error on receive), `Latency(Duration)` (adds delay), `Error` (always fails). Mode switchable at runtime via `set_mode()`.
  - 3 new DST scenarios in `gossip_dst.rs`:
    - `test_gossip_dst_3node_convergence` — transitive propagation A→B→C without A↔C direct link.
    - `test_gossip_dst_message_loss_resilience` — packet loss leaves engine state clean; retry succeeds.
    - `test_gossip_dst_concurrent_conflicting_updates` — LWW semantics: higher `clock` entry survives bilateral sync.

### Tests

**272 kernel lib tests + 56 link tests + 1 evals = 329 total** · 0 failures · gossip_dst: 6 tests (3 prev + 3 new M6-full).

---

## [v0.8.0] — 2026-07-01 — Self-Aware Agent

**Norte estrella:** The agent that knows itself and remembers conversations.

**Research basis:** MemGPT/Letta architecture mapping (Antigravity Research Cycle 2).

### Added

- **Core Memory — Agent Persona/Preferences (P0-A)**
  - `AgentProfile` gains `persona: String` + `preferences: serde_json::Value` fields
  - New kernel tools `agent_get_persona` / `agent_set_persona` wired under `tylluan_recall` / `tylluan_remember` subtool routing
  - CONTRACT-01 preserved — 5 sovereign MCP tools unchanged

- **Coloquio→SilvaDB Episodic Flywheel (P0-B)**
  - Background `tokio::spawn` task ingests Coloquio conversation turns into SilvaDB every 60 seconds
  - Nodes stored as type `episodic` with deterministic IDs `coloquio:{channel}:{turn}`
  - `HashMap<String, i64>` watermarks ensure idempotent dedup across restarts
  - 100ms per-message CPU throttle prevents embedding queue saturation

- **M2 Hybrid Search v2 — BM25 + FTS5 (P1)**
  - SilvaDB schema bumped to v11: new `nodes_fts` FTS5 virtual table with `content=nodes` external content
  - `search()` now uses BM25 ranking via `ORDER BY bm25(nodes_fts, 10.0, 5.0, 5.0)` with LIKE fallback on empty/error
  - `search_hybrid()` applies entity boost ×1.25 post-RRF for nodes with type `entity` / `concept`
  - FTS5 index kept in sync on every `upsert_node` and `delete_node`

- **DST Harness Minimal — GossipEngine simulation tests (P2)**
  - New file `crates/tylluan-link/tests/gossip_dst.rs` with 3 deterministic tests using `InMemoryTransport`
  - Tests: normal push-pull sync, partition graceful failure, bidirectional convergence
  - `GossipEngine::local_node_id()` accessor added to `gossip/state.rs`
  - Note: turmoil deferred to v0.9.0 (single-thread runtime constraint incompatible with non-tokio syscalls)

- **Startup Optimization — OnceLock catalog cache (P3)**
  - `builtin_catalog()` in `catalog.rs` now caches via `std::sync::OnceLock<Vec<GuildDescriptor>>`
  - Eliminates double filesystem scan at startup (main.rs called it twice on every boot)
  - Startup time improvement: ~10s → ~5s on typical guild directories

### Tests

**316 lib tests passing** (263 kernel + 53 link) · 0 failures · 0 regressions vs v0.7.0 baseline (259 tests).

---

## [v0.7.0] — 2026-07-01 — Intelligence Foundation

**Goal:** Smarter retrieval, faster guild discovery, solid test infrastructure.

### Added

- **M6-minimal — DST Foundation:** `MeshTransport` trait + `InMemoryTransport` (mpsc-based) + `TcpTransport` (length-prefixed). `GossipEngine::perform_sync` / `handle_incoming_message` generic over transport.
- **M3 — Guild Auto-Discovery:** Scan `guilds/` at startup, eliminate manual catalog registry. 34 `description_override()` entries for routing-critical guilds.
- **M7 — Single Binary:** Bundle `dashboard/dist/` into `tylluan-nexus` via `rust-embed`. `--features bundled-dashboard` at compile time; disk fallback for dev.
- **Contextual Retrieval:** `build_contextual_text()` prepends `[source_file > heading_path]` before embedding. Zero overhead when metadata absent.
- **M1 — Memory Decay:** Exponential half-life `weight * 0.5^(hours/half_life)`. Type-specific rates (lesson/experience/concept). Configurable `decay_half_life_hours` in `[silva]`.

---

## [v0.6.1] — 2026-06-30 — Model Portability

### Added

- **P5 — Config-driven embedding model:** `bge-m3` (1024d), `nomic-embed-text` (768d), `bge-small`/`minilm` (384d), `none` (BM25-only). `vector_dimensions` derived dynamically.
- **P6 — Installation profiles:** `tylluan-cli install --profile=clinic|server|portable`. Dashboard shows active profile chip.
- **P7 — Reindex endpoint:** `POST /api/v1/memory/reindex` with SSE progress events (`reindex_started/progress/finished`) and 200ms CPU throttle.

---

## [v0.6.0] — 2026-06-29 — Portable Foundation

**Portability invariant:** Single binary. Zero install dependencies. Runs offline. Knowledge persists via USB. Syncs with peers when network available.

### Added

- Portability invariant documented in README and ROADMAP
- Gossip protocol configurable: `fanout`, `interval_secs`, `max_entries` from `tylluan.toml`
- ARM64 build: `aarch64-unknown-linux-gnu` added to CI release matrix (Raspberry Pi 4+)

---

## [v0.4.0] — 2026-06-28 — Mesh

**Goal:** Connect Tylluan instances across networks without manual IP configuration.

### Added

- Ed25519 keypair per node (`data/identity.key`); `GET /api/v1/federation/identity`
- Node signing: Ed25519 signatures on federated nodes, auto-fetch peer pubkey on approval
- NAT traversal: STUN hole-punching + relay fallback
- mDNS LAN autodiscovery: zero-config peer discovery on local networks
- M13 Onboarding: pre-compiled binaries for 4 targets, `install.sh` / `install.ps1`, `tylluan-cli`

---

## [v0.3.0] — Federation

### Added

- SQLite peer persistence (`data/peers.db`)
- `auth_token` / `shared_secret` split
- Push / pull / bidirectional sync endpoints
- Node provenance: `federation_source` column in `silva_nodes`
- Echo-loop prevention: received nodes never re-exported
- Scheduled auto-sync background task
- Integration test suite: `tests/federation_audit.rs` (6 tests)

---

## [v0.2.0] — Community Validation

### Added

- Published benchmarks with reproducible methodology (`benchmarks/run.py`)
- End-to-end examples in `examples/` (5 examples including autonomous chain)
- M10 Bounded Work Contracts — finite multi-agent coordination protocol
- 30 automated security tests in CI (`security_audit.rs`)
- SQLCipher encryption at rest (`--features encryption`)
- Zero compiler warnings

---

## [v0.1.0] — Alpha Release

Initial release.

- Rust kernel (tokio + axum) with 5 sovereign MCP tools
- 47 Python guilds via FastMCP
- Persistent memory: BGE-M3 embeddings + BM25 + Jina Reranker
- Knowledge graph (SilvaDB): entity extraction, semantic clustering
- React dashboard with real-time monitoring
- Security primitives: rate limiter, circuit breaker, execution guard
- MCP native: SSE + HTTP Streamable (Claude, Cursor, VS Code, LM Studio)
