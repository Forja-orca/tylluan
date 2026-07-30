# Tylluan — Status

> Source of truth for the verified technical state. Updated on each release.
> Last updated: 2026-07-30 (v0.15.0: full connection audit across the stack, verified against a live kernel rather than code review alone — 5 guilds had IPC pointing at the wrong port, SilvaDB writes bypassing the kernel's embedding pipeline, dashboard panels showing fabricated/stuck data, 4 components skipping the auth layer, all fixed; production mesh gossip now encrypts with Noise NK once a peer's pubkey has propagated, previously sent in the clear; CoherenceGate Layer 4 hybrid filter wired live into both recall call sites in observation mode; 13 more guilds activated via `[guilds.v2]` plus a structural test preventing the catalog/runtime registration drift bug from shipping silently a third time; vision pipeline's intermittent crash root-caused to Windows GPU driver TDR under contention with the kernel's own DirectML usage, fixed by forcing CPU; embedding fix from v0.14.0 verified end-to-end for the first time via direct SQL against `node_embeddings`)

## CI

| Job | Status |
|-----|--------|
| Rust — build + test | ✅ pass (Rust 1.88+) |
| Rust — cargo-deny (licenses + advisories + bans) | ✅ pass |
| Python — lint + test | ✅ pass |
| Dashboard — lint | ✅ pass |
| Rust — security audit tests | ✅ pass |
| Rust — ARM64 portability (aarch64-unknown-linux-gnu) | ✅ pass |
| Install smoke (Linux + Windows) | ✅ pass (triggers on release publish) |
| Docker smoke | ✅ pass (local validated by Antigravity) |

**Commit:** c05fe7e · **650 total** green (575 kernel lib + 63 link lib + 12 fsrs) — 0 fallos, verificado en serie y en paralelo con `scripts/check_test_count.sh --fix` (2026-07-30). Auditoría de conexión real completa (Deep + Mimo + Claude): 5 guilds con IPC apuntando al puerto 3030 de ForjaMCPo3 en vez del 4000 real de Tylluan, escrituras a SilvaDB saltándose el pipeline de embedding del kernel, 3 paneles de dashboard con datos falsos/atascados, 4 componentes saltándose la capa de auth — los 14 hallazgos arreglados y verificados en vivo. Gossip de producción ahora cifra con Noise NK real una vez se propaga la pubkey del peer (antes viajaba en texto plano pese a que la capa Noise ya existía y estaba testeada). CoherenceGate Layer 4 híbrido wireado en los 2 puntos reales de recall, modo observación. `[guilds.v2]` activó 13 guilds más + test estructural que impide que el bug de catálogo↔runtime vuelva a colarse en silencio. Pipeline de visión: causa raíz real del crash intermitente confirmada (TDR de GPU en Windows por contención con el propio DirectML del kernel), arreglado forzando CPU; el fix de embedding de v0.14.0 quedó verificado end-to-end por primera vez con SQL directo contra `node_embeddings`.

---

## Version

**v0.15.0** (Cargo.toml) — Full connection audit, mandatory mesh gossip encryption, CoherenceGate Layer 4 hybrid live in observation mode; **v0.14.0** — A2A protocol, Signal Loop + Coherence Gate, Sovereign Substrate dashboard; **v0.13.0** — Coordinator Cascade, query cache, modular canvas (M26), junior onboarding (M22) and first minute autostart (M23-P1).
**v0.12.0** (tag) — Single binary target release and automated installer profiles.
**v0.11.0** — Saga mesh P2P completa + M18-P3 Coordinator Synthesis y M20 Complexity Cascade integrados nativamente.

---

## What works (verified)

### Kernel (Rust)
- `tylluan-nexus` binary: tokio + axum HTTP server, MCP over SSE and HTTP Streamable
- M21 Query Embedding Cache: `QueryEmbeddingCache` in `memory/silva/query_cache.rs` — Mutex<HashMap>, TTL 300s, LRU 256 entries, normalized key (split_whitespace+lowercase). Injected in `handler_recall.rs` — `tylluan_recall` queries cache before ONNX inference. Invalidated on `tylluan_remember`. Ingesta, batch embed, and DCR paths bypass cache (always fresh). 5 unit tests.
- M20 Complexity Cascade: heuristic intent scoring (≥0.6 proactive → coordinator; ≥0.4 reactive on failure → fallback). Guarded by `registry.has_guild("coordinator")` — activates automatically when coordinator is registered. Zero external deps, 13 unit tests.
- M18-P3a Coordinator Parallelism ✅ / P3b Re-benchmark ✅ CLOSED (2026-07-12): TRINITY coordinator detects synthesis intents via 30+ signals (EN/ES) in `_is_synthesis_intent()`. `ThreadPoolExecutor(max_workers=4)` verified in code (real parallelism exists), plus thread-local HTTP Keep-Alive + TCP_NODELAY (legitimate latency win, kept). An earlier attempt anonymized `agent_id` to skip SQLite audit writes to hit the 30% target — reverted (commit 1381664) as a real accountability regression; a benchmark-keyword-overfitting heuristic was reverted alongside it. Root cause investigated further: `log_audit_entry` was scheduled via `tokio::spawn` around a synchronous rusqlite write, blocking a runtime worker thread during concurrent coordinator dispatches — fixed via `spawn_blocking` (commit 5698051). The prior benchmark script that produced the +10.2% result was never committed and was lost; rewrote it as `benchmarks/coordinator_bench.py` (reproducible, no external deps, sequential-per-subtask baseline vs single coordinator-routed call) and re-ran live against the running kernel (`:4000`, post-`spawn_blocking` fix): **delta of means +62.0%, mean of per-query deltas +57.7%, 0 errors across 5/5 queries** — clears the 30% roadmap threshold on both metrics. Result saved in `benchmarks/results/coordinator_latencies.json`. M18 fully closed.
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
- ADR-011 Signal Loop + Coherence Gate: `recall_feedback` table (SilvaDB schema v18) logs which memories `tylluan_recall` returned per agent; `FeedbackSignalPhase` (NightConsolidation, 9th parallel phase) resolves them against `guild_audit_log` word-overlap into useful/not-useful. `security::coherence_gate::CoherenceGate` runs 3-layer defense (injection-pattern elimination, provenance penalty, query-content cosim penalty) on every `tylluan_recall` response — both the live-query path and the cache-hit path (the cache stored pre-gate candidates; fixed before merge). `router::light_reranker::LightReranker` (P1 scaffold, <10KB ONNX FFN) is built and tested but not wired into `search_hybrid` — no real model exists until `recall_feedback` accumulates ≥5,000 resolved rows per ADR-011 §3.3 (2026-07-25)
- Sovereign Consensus: `memory::consensus::ConsensusEngine` runs hourly in `main.rs` (`GuardedTask` background job, 120s guard) — resolves conflicting nodes sharing a `topic_key` (or a semantic cluster via cosine similarity > 0.80 over conflicted embeddings) via `score = weight*trust + evidence_bonus*2.0`: clear winner (diff ≥15%) reinforces + decays losers, close scores (5-15%) synthesize a unified protected node linking all sources, ties (<5%) mark all candidates `Ambiguous` pending `human_override()`. Found running unwired/undocumented/untested during the 2026-07-11 reflection cycle audit (added 2026-06-27, commit a92e480) — 7 unit tests added covering all three resolution paths plus the protected-node skip, evidence-bonus tie-break, single-candidate no-op, and human override (2026-07-11)
- M2 Hybrid Search v2: SilvaDB schema v11 adds FTS5 virtual table `nodes_fts`; `search()` uses BM25 (`bm25(nodes_fts, 10.0, 5.0, 5.0)`) with LIKE fallback; `search_hybrid()` applies entity boost ×1.25 post-RRF (P1)
- DST harness: `gossip_dst.rs` — 6 tests: normal sync, partition graceful failure, bidirectional convergence, 3-node transitive propagation, message loss + retry, LWW conflict resolution (M6-full)
- `PartitionableTransport<T>` in `tylluan-link`: 5 switchable modes (Transparent, Drop(f64), Partition, Latency(Duration), Error) for deterministic fault injection in tests (M6-full)
- `fault_dst.rs` — 4 realistic fault scenarios: `partition_heal_convergence` (Partition→Transparent→converge), `latency_injection` (Latency 100ms, +150ms measurable), `drop_rate_eventual_convergence` (Drop 0.3, ≤10 rounds), `error_mode_graceful_failure` (Error mode, no state corruption) (v0.10.0 P1)
- LightRAG degree bias corrected (v0.10.0 P2-fix): `local_query_graph` in `graph.rs` and `dual_retrieval.rs` now divide by degree factor instead of multiply — penalizes generic hub nodes, improves MRR for specific queries. New test `test_local_query_graph_degree_penalty` added.
- Retrieval quality benchmark v0.10.0: 44 nodes, 40 edges, 10 queries (5 original + 5 multi-hop). With LightRAG graph ON: Recall@5 20%, Recall@10 30%, MRR 23.15%, p50 5.65ms. Delta vs graph OFF: +2.5% Recall@5, +5% Recall@10, −0.1% MRR (pre-fix). Results with fake 12-dim embeddings; real BGE-M3 delta expected higher.
- ADR-004 M14-D Guild Execution Channels spec published: `docs/architecture/M14D_dispatch_spec.md` — Capability-Aware Hybrid Routing, 4-phase implementation plan (~8 sessions), preserves CONTRACT-01
- M14-D Phase 1 — Capability Registry: `HardwareCaps { ram_mb, has_gpu, load_avg }` added to `GossipEntry`; `CapabilityRegistry` in `tylluan-link/src/capability.rs` with TTL-based peer store, `prune_expired()`, `ingest_from_engine()`; 6 unit tests (v0.11.0-dev)
- M14-F Phase 2 — `start_p2p_listener_noise(addr, identity, handler) -> (JoinHandle, SocketAddr)`: Noise XK responder loop (`noise_accept` → decrypt → handler → encrypt write); `DispatchDecision::RemoteTcp { node_id, addr, tcp_port }` variant; `route()` picks best-scoring peer first, then checks `supports_p2p` (bug fix: early return bypassed score threshold — fixed); `tests/p2p_dst.rs` 3 tests: TCP loopback roundtrip, error response, RemoteTcp routing. (v0.13.0)
- M14-F Phase 1 — `P2pSessionPool` (HashMap, LRU evict, TTL prune) + `execute_remote_tcp()` (Noise XK initiator, pool extract-before-use + reinsert-on-success-only bug fix); `HardwareCaps` gains `supports_p2p: bool` + `tcp_port: Option<u16>`. (v0.13.0)
- Moondream guild: `guilds/core/vision_moondream.py` — `analyze_image` + `caption_image` via `moondream` pip (0.5B local vision). (v0.13.0)
- M14-E — Mesh Integration Test Harness: `tests/mesh_simulation.rs` (full-mesh A↔B↔C, star topology B-hub, split-brain + heal LWW); `tests/dispatch_dst.rs` (GPU peer selection, capability filter, CB fallback, DispatchQueue FIFO/overflow/TTL); `DispatchQueue` moved from kernel to `tylluan-link/src/dispatch.rs`. **M14-D + M14-E both complete.** (v0.13.0)
- M14-D Phase 4 — Fallback + Remote Dispatch: `DispatchQueue` (VecDeque + TTL 300s, max 1000); `HttpState` gains `dispatch_router` + `dispatch_queue`; `GET /api/v1/guilds/peers` returns CapabilityRegistry view; `POST /api/v1/guilds/dispatch/remote` routes via DispatchRouter (local or HTTP forward), fallback-enqueues on failure, wires record_success/record_failure. **M14-D milestone complete.** (v0.13.0)
- M14-D Phase 3 — Guild Dispatch Protocol: `GuildDispatchRequest/Response` structs (Serde); `send/receive_dispatch_request/response` using Noise NK (`noise_encrypt/decrypt_payload` over `dyn MeshTransport`); `POST /api/v1/guilds/dispatch/execute` endpoint — receives request, calls `registry.call_tool()`, returns response with `executor_id` + `duration_ms`; CONTRACT-01 preserved (v0.13.0)
- M14-D Phase 2 — DispatchRouter: `dispatch.rs` in `tylluan-link` — scoring `(1-load)×(1000/latency)×gpu_mult`, circuit breaker (3 failures + 60s cooldown), default latency 0.0 favoring unknown peers; `HttpState` gains `capability_registry`; gossip tick wires `ingest_from_engine + prune_expired`; 5 unit tests (v0.13.0)
- Startup optimization: `builtin_catalog()` cached via `std::sync::OnceLock` — eliminates double filesystem scan at startup (~10s → ~5s) (P3)
- HNSW index via `instant-distance`: `hnsw.rs` + schema v12 (`hnsw_index` BLOB table) + fast path in `search.rs` (HNSW ≥12k nodes → IVF → linear fallback); rebuild scheduler every 10min; survives restart via SQLite BLOB (v0.9.0)
- LightRAG local graph traversal: `degree_centrality` (SQL-native) + `local_query_graph` (Personalized PageRank local + degree penalty, corrected in v0.10.0) integrated into RRF hybrid search (v0.9.0)
- Batch Embeddings: Callers connected to `embed_batch` in `embeddings.rs`. Reindex loop in main.rs processed in chunks of 32 with 500ms sleep (v0.9.0)
- Retrieval baseline: `tylluan-evals` benchmark — Recall@5: 60%, Precision@5: 12%, p50: 1.3ms, p95: 1.9ms; persisted in `benchmarks/baseline_v0.9.0.json` (v0.9.0)
- Semantic Coloquio Search (P4): `tylluan_recall` parses optional `"episodic": bool` argument and filters by `"episodic"` node type via `search_hybrid` (v0.9.0)
- Security hardening (P-security): `sanitize_query()` redacts `token=`/`Authorization=` from `info!` logs; `extract_token()` fixes ACL role resolution for `?token=` query-string auth — no longer falls to `default_role` (v0.9.0)
- **575 kernel lib tests passing** + 63 tylluan-link + 12 tylluan-fsrs = **650 total** (verificado 2026-07-30, ver bloque CI arriba — no confiar en esta cifra sin recorrer `scripts/check_test_count.sh`, cambia cada ciclo)
- Zero `openssl-sys` in dep tree — pure rustls-tls on all platforms, cross-compile clean

### Binary distribution (M13 + v0.6.0)
- Pre-compiled releases for 4 targets:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu` (Raspberry Pi 4+ — new in v0.6.0)
  - `aarch64-apple-darwin` (Apple Silicon)
  - `x86_64-pc-windows-msvc`
- `install.sh` / `install.ps1` — curl-pipe and irm-pipe installers
- Installs to `~/.tylluan/bin/`, adds to PATH, prints MCP config + token hint
- `tylluan install` auto-downloads embedding model and auto-starts kernel
- `tylluan start` polls `/health` until kernel is ready (30s timeout)

### Python guilds
- 49 guilds across 5 categories (core/builders/scholars/wardens/watchers) via FastMCP — auto-discovered at startup
- Guild catalog in `registry.json`; lazy on-demand loading
- Security: `_security.py` per-guild ACL layer

### Dashboard
- React + Vite dashboard in `dashboard/`; builds clean via pnpm
- Real-time monitoring, guild status, knowledge graph viewer
- Profile chip (Portable·BM25 / Clinic·BGE-Small / Server·BGE-M3) in Overview (P6 UX)
- Reindex button + amber progress bar driven by SSE events (P7 UX)
- Dynamic BM25 banners with context-specific instructions per profile (P6 UX)
- Empty State onboarding widget (M23-P1): welcome card + quick-start instructions when no memories or guilds are loaded
- Hierarchical Scopes Panel (M37-P2): interface under System Tab to query nodes by hierarchical scope prefix (`user:id/session:id/agent:id`) and inspect multi-tenant node isolation.

### Integrations
- MCP client configs in `integrations/` for: Claude Desktop, Claude Code, Cursor, VS Code, LM Studio (SSE), Qwen Desktop, Antigravity

---

## Crate structure

| Crate | Purpose |
|-------|---------|
| `tylluan-kernel` | Core binary + library: MCP, memory, federation, security |
| `tylluan-common` | Shared types, error types, constants |
| `tylluan-link` | Federation networking: mesh identity, DHT, NAT, mDNS |
| `tylluan-cli` | `start / stop / status / logs / connect / download-models / install` |
| `tylluan-evals` | Benchmark harness: Recall@N, Precision@N, latency percentiles |

---

## What is NOT production-ready

- No external security audit
- No community validation (0 external contributors)
- No independent benchmark reproduction
- Kernel is a research lab — executes real code on your machine

---

## Running

```bash
# Binary install (recommended)
tylluan-cli start

# From source
cargo run --release -p tylluan-cli -- start
```

Verify: `curl http://127.0.0.1:4000/health`

Dashboard (dev): `cd dashboard && pnpm dev` → `http://localhost:5173`

See README.md for full setup.
