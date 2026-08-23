# Tylluan — Status

> Source of truth for the verified technical state. Updated on each release.
> Last updated: 2026-08-22 · HEAD `10548ea` · v0.16.0 (Cargo.toml)

## Known Gaps (external audit, verified 2026-08-22)

An external reviewer cloned `d68fa5a`, built it, and ran the live kernel — not just the test suite. Every item below was independently re-verified against the real source before being listed here (file:line, not taken on the reviewer's word). This is what "verified" means on this line, not "reported."

- **No offline boot path**: `embedding_model = "none"` (recommended in the README to skip the BGE-M3 download) only disables that model — `RerankEngine::load_with_device()` (`main.rs:577`) still runs unconditionally and needs ONNX Runtime present. Missing `libonnxruntime.so`/`.dll` can panic the whole process (`ort` crate, no `catch_unwind` anywhere near this path) instead of degrading to BM25-only.
- **STUN fires unconditionally at boot** (`main.rs:430-442`) to `stun.l.google.com:19302`, regardless of whether federation/mesh is enabled. Backgrounded (won't block boot) but real outbound traffic on a host meant to stay offline. Workaround: `[nat] stun_servers = []`.
- **CI Python tests are non-blocking by construction**: `.github/workflows/ci.yml:110` runs `pytest tests/python/ || echo "No Python tests yet"` — any real failure is swallowed. The main CI job also doesn't run `tylluan-link`/`tylluan-fsrs` (81 of the 771 total tests only get exercised by someone running them manually).
- **`serverInfo.version` hardcoded to `"3.0.0"`** (`api_monitor.rs:289`, `mcp.rs:424`), out of sync with `Cargo.toml`'s `0.16.0`.
- **`[federation] auto_sync_interval_secs` is dead config**: defined and defaulted, never read anywhere except its own definition. The real auto-sync loop (`api_federation.rs:1025`) uses `[silva] sync_interval_ms` instead — a different config section, different units (ms vs secs).
- **1770 `.unwrap()` calls** across `crates/` (all crates, including tests) — not inherently wrong, but a real signal of how much of the panic surface hasn't been audited for graceful-failure conversion.
- **49 guilds require Python 3.12 + FastMCP separately** from the Rust kernel binary — without them, guilds crash-loop and `/api/v1/doctor` reports `degraded`. The README's "no Rust/Python/Node needed" line was true only for the kernel-as-MCP-memory use case, not for `tylluan_do` guild execution; corrected in README 2026-08-22.
- **Retrieval quality context**: the 82% Recall@5 headline figure (LongMemEval-S) sits alongside a **Precision@5 of 16.4%** in the same results file — both real, but only one made it to marketing copy. Live routing accuracy on that same external benchmark run was ~41% (hybrid), vs the 56-64% measured on the team's own curated I-7/J-13 dataset — the gap is dataset difficulty, not a regression, but worth stating plainly rather than leading with the friendlier number.
- Not a gap, confirmed correct on inspection: the MCP protocol version negotiation (`mcp.rs:407`) is real and dynamic (echoes whatever `protocolVersion` the client requests, 4 versions supported `2024-11-05`–`2026-07-28`). An external report reading a `2025-03-26` negotiated session as a bug was itself mistaken — that's what its own older test client asked for.

**Overall read**: this is a serious, fast-moving research lab with real engineering (compiled kernel, 771 real tests, a CI gate that catches doc/test-count drift and has already caught and fixed several real regressions this cycle) — not yet a hardened, installable product for strangers. The project's own `DISCLAIMER.md` already says this; the gap above is between that honest self-assessment and what the README's quick-start framing implies for a first-time user.

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
| Security — claims gate | ✅ pass |

**HEAD:** `10548ea` · **771 total** lib green (690 kernel lib + 69 link lib + 12 fsrs). CI real: todos los jobs verdes.

**✅ Kernel vivo al día (2026-08-22):** rebuild confirmado, `:4000/health` reporta el mismo commit que este HEAD (0 commits de gap) — cerrando la brecha de 16+ commits detectada antes en este mismo ciclo. Verificar en cualquier momento con `bash scripts/check_live_kernel_drift.sh` (local-only, no es un gate de CI — ver el propio script para por qué).

### Ciclo 2026-08-21: tylluan_do arg-forwarding bug + CI toolchain drift + frontend Fase 1

**`tylluan_do` argument-forwarding bug** (root cause + fix en 2 commits)
- `resolve_and_prepare_tool_call()` construía `tool_args` desde el texto de `intent` + un set fijo de campos, ignorando por completo los argumentos estructurados reales que el caller pasaba a `tylluan_do` — `offset`/`limit` en `read_channel` no tenían ningún efecto real (`0136271`-adyacente, ver commit previo de offset/limit)
- Extendido el mismo patrón (preferir argumento explícito, fallback a text-parsing) a `command` (bash/git, antes SIEMPRE sobreescrito sin fallback), `channel_id`, `content`/`message`/`intent`, `author_id` (`0fefdd5`, co-autoría Deep)
- Verificado independientemente: 683/683 lib tests, cargo check limpio

**CI toolchain drift (clippy)** — el runner de GitHub actualizó a rustc 1.98, introduciendo lints nuevos que el toolchain local (1.88) no conocía, rompiendo CI en 3 pushes seguidos sin relación con el código en sí
- `chunks_exact_to_as_chunks`: 15 sitios preexistentes en `memory/silva/*.rs` + `auto_link.rs`, puramente estilístico → `allow` a nivel de crate en `tylluan-kernel/src/lib.rs` (`639ff84`)
- `cloned_ref_to_slice_refs`: 3 sitios reales (`handler_graph.rs` ×2, `tylluan-link/src/p2p.rs` ×1) → fix real con `std::slice::from_ref()`, sin allow (`ab28e6a`, `cd45789`)
- Verificación local desde entonces vía `rustup run stable` (1.97.1, casi idéntico a CI) antes de cada push, para no gastar rondas de CI en tanteo

**`.gitignore` hardening** (`f838f70`) — 9.4GB de artefactos de build sin ignorar, riesgo real ante un `git add -A` descuidado: `target-codex-baseline/` (8.5GB, mismatch guión/guión-bajo en el patrón existente), `crates/*/src-tauri/target/` (862MB, solo `desktop/src-tauri/target/` estaba cubierto), `.freebuff/` (18MB, estado local del agente Freebuff)

**Frontend Fase 1 cerrada** — `crates/tylluan-gui/ui/` eliminado (26 archivos, 7453 líneas), consolidación en `dashboard/` como frontend único confirmada sin referencias rotas

### Trabajo desde v0.16.0 (32 commits, `fe954bb..HEAD`)

**Security Claims CI Gate** (ciclo completo: spec → manifest → checker → scripts → CI → cleanup)
- Design + spec (`d64265a`, `f8eef30`), claims manifest con 5 propiedades documentadas (`c8f632c`)
- Checker estático (`cc1243c`) + scripts dinámicos para host/dev_mode/P2P/write-gate (`2cbeed1`)
- CI wiring (`0667b92`) + fixes de fiabilidad (detail caps 300→20000, timeouts 120s, ONNX runtime, ripgrep)
- Feature complete + SDD cleanup (`404426f`)

**MCP spec compliance** (6 commits)
- `resultType` field en tools/list, prompts/list, resources/list (`e684f02`)
- Caching hints `ttlMs`/`cacheScope` a nivel resultado (`eee1a24`)
- Eliminación de headers autoinventados `Mcp-Method`/`Mcp-Name` (`7eb9dbb`)
- `resultType` en todas las success results reales (`d65b5e0`)

**PPR warnings (feat commits `3dda4ff` + `95eeb2c`)
- Fase 1: PPR distingue seeds no resueltos de subgrafo vacío via `warnings[]` array
- Fase 2: `query` y `expand` emiten `NODE_NOT_FOUND` warnings + tests (`95eeb2c`)

**Deps y kernel**
- `h2` 0.4.16 (RUSTSEC-2026-0258) + `spin` 0.9.9 (yanked) (`56f4364`)
- NAT/STUN discovery ya no bloquea bind HTTP (`8a6ec8c`)
- `NexusConfig.transport` default a empty, no stdio+http+sse (`7cd024b`)
- `llama_backend` port conflict fix (`a70d3ca`)
- Tauri UI: Vite entry point real commitiado (`61479bc`)

**Docs**
- Fix test count drift 764→766 after decay.rs fix added 2 tests (`1a5eaaa`)
- Note retrieval-gate idea from waku-agent as low-priority backlog (`12dca2e`)

---

## Version

**v0.16.0** (Cargo.toml) — MCP 2026-07-28 adoption (M39) + continuity/trust/action layer (M40), CoherenceGate→dataset circuit phase 1+2; **v0.15.0** — Full connection audit, mandatory mesh gossip encryption, CoherenceGate Layer 4 hybrid live in observation mode; **v0.14.0** — A2A protocol, Signal Loop + Coherence Gate, Sovereign Substrate dashboard; **v0.13.0** — Coordinator Cascade, query cache, modular canvas (M26), junior onboarding (M22) and first minute autostart (M23-P1).
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
- Memory lifecycle decay: FSRS-based retrievability `2^(-elapsed_days/fsrs_stability)` mapped to weight `0.01..1.0`, replacing the old exponential half-life formula — `decay_half_life_hours` in `[silva]` tylluan.toml is no longer consulted by `apply_decay()` (`decay.rs:44`), kept only for backward config compatibility. Nodes move through `active → quiet → consolidated → archived` per ADR-012 instead of a binary decay-then-delete. (Corrected 2026-08-22 — this line described the pre-FSRS formula; found stale during the full-project audit, Coloquio T197.)
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
- ADR-004 M14-D Guild Execution Channels spec published: `docs/reference/adr/M14D_dispatch_spec.md` — Capability-Aware Hybrid Routing, 4-phase implementation plan (~8 sessions), preserves CONTRACT-01
- M14-D Phase 1 — Capability Registry: `HardwareCaps { ram_mb, has_gpu, load_avg }` added to `GossipEntry`; `CapabilityRegistry` in `tylluan-link/src/capability.rs` with TTL-based peer store, `prune_expired()`, `ingest_from_engine()`; 6 unit tests (v0.11.0-dev)
- M14-F Phase 2 — `start_p2p_listener_noise(addr, identity, handler) -> (JoinHandle, SocketAddr)`: Noise XK responder loop (`noise_accept` → decrypt → handler → encrypt write); `DispatchDecision::RemoteTcp { node_id, addr, tcp_port }` variant; `route()` picks best-scoring peer first, then checks `supports_p2p` (bug fix: early return bypassed score threshold — fixed); `crates/tylluan-link/tests/p2p_dst.rs` 3 tests: TCP loopback roundtrip, error response, RemoteTcp routing. (v0.13.0)
- M14-F Phase 1 — `P2pSessionPool` (HashMap, LRU evict, TTL prune) + `execute_remote_tcp()` (Noise XK initiator, pool extract-before-use + reinsert-on-success-only bug fix); `HardwareCaps` gains `supports_p2p: bool` + `tcp_port: Option<u16>`. (v0.13.0)
- Moondream guild: `guilds/core/vision_moondream.py` — `analyze_image` + `caption_image` via `moondream` pip (0.5B local vision). (v0.13.0)
- M14-E — Mesh Integration Test Harness: `crates/tylluan-link/tests/mesh_simulation.rs` (full-mesh A↔B↔C, star topology B-hub, split-brain + heal LWW); `crates/tylluan-link/tests/dispatch_dst.rs` (GPU peer selection, capability filter, CB fallback, DispatchQueue FIFO/overflow/TTL); `DispatchQueue` moved from kernel to `tylluan-link/src/dispatch.rs`. **M14-D + M14-E both complete.** (v0.13.0)
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
- **588 kernel lib tests passing** + 63 tylluan-link + 12 tylluan-fsrs = **663 total** (verificado 2026-07-30, ver bloque CI arriba — no confiar en esta cifra sin recorrer `scripts/check_test_count.sh`, cambia cada ciclo)
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
