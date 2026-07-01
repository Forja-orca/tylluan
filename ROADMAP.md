# Tylluan Roadmap

## v0.1.0 — Alpha Release

**Status:** Published. Historical baseline.

What was included:
- Rust kernel (tokio + axum) with 5 sovereign MCP tools
- 47 Python guilds (bash, git, filesystem, code, vision, web search, and more)
- Persistent memory with BGE-M3 embeddings + BM25 + Jina Reranker
- Knowledge graph (SilvaDB) with entity extraction and clustering
- React dashboard with real-time monitoring
- Security primitives: rate limiter, circuit breaker, execution guard
- MCP native (SSE + HTTP Streamable) — works with Claude, Cursor, VS Code, LM Studio

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
- [x] SQLCipher encryption at rest — AES-256 via `bundled-sqlcipher-vendored-openssl`, feature-gated (`cargo build --features encryption`), wired across silva, mailbox, audit, and federation DBs with `PRAGMA hexkey` (no SQL injection vector)
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

## v0.4.0 — Mesh

**Status:** Complete.

**Goal:** Connect Tylluan instances across networks without manual IP configuration.

Delivered:
- [x] M12-A — Ed25519 keypair per node: generated on first boot, stored in `data/identity.key`; `GET /api/v1/federation/identity` returns node_id + public_key
- [x] M12-B — Node signing: every federated node carries an Ed25519 signature; receiver verifies before accepting. Auto-fetch peer pubkey on approval. Backwards compat: skip verify if pubkey not yet stored
- [x] M12-C — NAT traversal: hole-punching via STUN + relay fallback (no WireGuard dependency)
- [x] M12-D — mDNS LAN autodiscovery: zero-config peer discovery on local networks (external_address populated on approval via M12-C auto-fetch)
- [x] M12-F — Integration tests: `tests/mesh_audit.rs` (10 tests) + `tests/federation_audit.rs` (6 tests) + `crates/tylluan-link/src/nat.rs` (8 tests). Covers keypair, signature, envelope, STUN RFC 5389 (CRC32, txid mismatch, missing attribute, IPv4 XOR), NAT HTTP endpoint, mDNS startup, federation sync
- [x] M13 — Onboarding: pre-compiled binaries via GitHub Actions (linux-x64, mac-arm64, win-x64), `install.sh` + `install.ps1` one-line installers, `tylluan-cli` management binary, README rewritten to 3-step Quick Start

## v0.5.0 — Mesh Fabric

**Goal:** True WAN peer discovery and resilient multi-node knowledge fabric without central coordination.

Planned:
- [x] M14-A — DHT peer discovery: Kademlia-style DHT for WAN peer lookup without a central registry. K-bucket routing table over Ed25519 node IDs (XOR metric), FIND_NODE/STORE/PING RPCs, mainline BitTorrent DHT bootstrap, 23 tests.
- [x] M14-B — Gossip protocol: epidemic dissemination of knowledge updates across the mesh. Eventual consistency without requiring all peers to be online simultaneously.
- [x] M14-C — Encrypted transport overlay: Noise Protocol XK (TCP sessions) + NK (HTTP payloads) wired to federation sync endpoints. Ed25519→X25519 key conversion, ChaCha20-Poly1305 AEAD, async length-prefixed framing.
- [ ] M14-E — Mesh integration tests: multi-node test harness with simulated network partitions and recovery.

Out of scope (v1.0.0):
- External security audit of the mesh layer
- Formal Byzantine fault tolerance guarantees

## v0.6.0 — Portable Foundation

**Goal:** Same binary, any hardware. RPi4 at 5W or a server cluster — different `tylluan.toml`, same code. No internet required at runtime. No cloud dependency in the critical path.

**Portability invariant:** Single binary. Zero install dependencies beyond the binary itself. Runs offline. Knowledge persists across machines via USB. Syncs with peers when a network is available.

- [x] P1 — Portability invariant documented in README and ROADMAP. Agents that drift from it can be corrected against this definition. *(closed v0.6.0)*
- [x] P2 — Gossip configurable: `fanout`, `interval_secs`, `max_entries` from `tylluan.toml` wired to `tylluan_link::gossip::GossipConfig` in `http/mod.rs`. Two `GossipConfig` structs unified via explicit field mapping. *(done silently during v0.6.0, closed here)*
- [x] P3 — `embedding_model = "none"` config flag: BM25-only mode, zero download. `resolve_model()` / `resolve_dimension()` in embeddings.rs. *(done in P5, v0.6.1)*
- [x] P4 — ARM64 build: `aarch64-unknown-linux-gnu` added to CI release matrix. Pre-compiled binary ships for RPi4+. *(done in v0.6.0 release)*

## v0.6.1 — Model Portability (backlog)

- [x] P5 — Config-driven embedding model: `bge-m3` (1024), `nomic-embed-text` (768), `bge-small`/`minilm` (384), `none` (BM25-only). `resolve_model()` / `resolve_dimension()` in embeddings.rs. `vector_dimensions` is now derived dynamically from the model.
- [x] P6 — Installation profiles: `tylluan-cli install --profile=clinic|server|portable` writes the correct `tylluan.toml` at install time (different embedding_model, fanout, timeouts per profile). Dashboard shows active profile chip (Portable·BM25 / Clinic·BGE-Small / Server·BGE-M3).
- [x] P7 — Reindex endpoint + dashboard progress: `POST /api/v1/memory/reindex` triggers immediate background reindex when switching models. Dashboard shows reindex progress bar (stale/total nodes). Context: the system already reindexes stale nodes automatically every 10 min in `main.rs:1235` via `get_stale_embeddings()` — P7 adds manual trigger + visibility.

## v0.7.0 — Intelligence Foundation

**Goal:** Smarter retrieval, faster discovery, solid test infrastructure. Everything in this milestone serves the portability invariant directly — offline knowledge quality improves, guild setup requires zero manual registration, and the mesh layer gets a deterministic test foundation for the first time.

**Research basis:** research-tylluan coloquio (T1-T13), Antigravity cycle 1, Qwen3.7 engineering review.

**Execution order (approved by research team 2026-07-01):**

- [x] M6-minimal — Deterministic Simulation Testing (DST) foundation: `MeshTransport` trait + `InMemoryTransport` (mpsc-based) + `TcpTransport` (length-prefixed). `GossipEngine::perform_sync/handle_incoming_message` generic over transport. 4 integration tests. Prerequisite for validating all mesh milestones.
- [x] M3 — Guild Auto-Discovery: scan `guilds/` at startup, eliminate manual `catalog.rs` registry. `scan_guilds_directory()` extracts name + trigger phrases from FastMCP docstrings. 34 `description_override()` entries for routing-critical guilds. Zero-config for new guilds.
- [x] M7 — Single Binary: bundle `dashboard/dist/` into `tylluan-nexus` binary via `rust-embed`. `--features bundled-dashboard` embeds assets at compile time; disk fallback preserved for dev. (~0.5 days)
- [x] Contextual Retrieval: `build_contextual_text()` prepends `[source_file > heading_path]` before embedding. Applied in background reindex loop and manual reindex endpoint. Zero overhead when metadata is absent.
- [x] M1 — Memory Decay: exponential half-life `weight * 0.5^(hours/half_life)` computed in Rust (no SQL POWER() dependency). Type-specific effective half-lives (lesson/experience/concept). Configurable `decay_half_life_hours` in `[silva]` tylluan.toml (default 336h = 14 days). Protected nodes exempt.

## v0.8.0 — Self-Aware Agent

**Status:** Complete.

**Goal:** The agent that knows itself and remembers conversations (norte estrella from MemGPT/Letta research cycle).

**Research basis:** Antigravity Research Cycle 2 (MemGPT/Letta architecture mapping), full team deliberation in Coloquio v0.8.0 planning cycle.

Delivered:
- [x] P0-A — Core Memory (persona/preferences): `AgentProfile` gains `persona: String` + `preferences: serde_json::Value`; `agent_get_persona` / `agent_set_persona` kernel tools wired under `tylluan_recall`/`tylluan_remember` subtool routing. CONTRACT-01 (5 sovereign tools) unchanged.
- [x] P0-B — Coloquio→SilvaDB episodic flywheel: background `tokio::spawn` every 60s ingests Coloquio turns into SilvaDB as `episodic` nodes; deterministic IDs `coloquio:{channel}:{turn}`; 100ms CPU throttle; `HashMap<String, i64>` watermarks for idempotent dedup.
- [x] P1 — M2 Hybrid Search v2: SilvaDB schema v11 adds FTS5 virtual table `nodes_fts`; `search()` uses BM25 ranking with LIKE fallback; `search_hybrid()` applies entity boost ×1.25 post-RRF; 2 new BM25 integration tests (multi_thread flavor).
- [x] P2 — DST harness minimal: `crates/tylluan-link/tests/gossip_dst.rs` — 3 InMemoryTransport-based GossipEngine simulation tests (normal sync, partition failure, bidirectional convergence); `GossipEngine::local_node_id()` accessor added to `state.rs`.
- [x] P3 — Startup optimization: `builtin_catalog()` cached via `std::sync::OnceLock` in `catalog.rs` — eliminates double filesystem scan, startup time ~10s → ~5s.

**316 lib tests passing** (263 kernel + 53 link) at release.

## v0.9.0 — Graph-Augmented Local RAG

**Status:** Complete.

**Goal:** Zero-token local graph retrieval and batch indexing.

Delivered:
- [x] P0 — Retrieval baseline: `tylluan-evals` benchmark — Recall@5: 60%, Precision@5: 12%, latency p50: 1.3ms, p95: 1.9ms (baseline_v0.9.0.json).
- [x] P1 — HNSW index via `instant-distance`: HnswIndex + schema v12 (`hnsw_index` BLOB table) + search fast path (threshold >=12k nodes) + background rebuild scheduler.
- [x] P2 — Batch embeddings: Connected callers to `embed_batch` (single ONNX lock) and main.rs reindexer loop chunked to 32 nodes.
- [x] P3 — LinearRAG local graph traversal: `degree_centrality` + `local_query_graph` (Personalized PageRank + degree centrality) integrated into RRF hybrid search. ⚠️ **The original degree formula `score * (1 + deg*0.1)` was identified as a bug in v0.10.0** — it boosted hub nodes (high connectivity = generic concepts) to top positions, hurting MRR for specific queries. Corrected to `score / (1 + deg*0.1)` (degree penalty).
- [x] P4 — Semantic Coloquio Search: Optional `"episodic"` filtering in `tylluan_recall` using `search_hybrid` type filter.

**272 kernel lib tests + 56 link tests + 1 evals = 329 total** · 0 failures · 2 new unit tests for P4.

## v0.10.0 — El sistema que sabe si funciona

**Status:** Complete (tag: v0.10.0).

**Goal:** Validate v0.9.0's foundations before adding more layers. Retrieval quality delta measurement + M6-full realistic fault scenarios + LinearRAG bug fix.

Delivered:
- [x] P0 — Extended Retrieval Benchmark: 44 nodes + 40 edges + 10 queries (5 original + 5 multi-hop), `skip_graph` A/B flag in `search_hybrid` (internal). Graph ON vs OFF: +2.5% Recall@5, +5% Recall@10, −0.1% MRR (pre-fix), +4ms latency. Results in `benchmarks/benchmark_v0.10.0.json`.
- [x] P1 — M6-full Fault DST: `fault_dst.rs` — 4 new tests exercising all 5 `PartitionableTransport<T>` modes: partition+heal convergence, latency injection, drop-rate eventual convergence, error mode graceful failure.
- [x] P2-fix — LinearRAG Degree Bias Fix: `local_query_graph` (`graph.rs:739`) and `dual_retrieval.rs` (lines 30, 69) inverted from multiply to divide — penalizes hub nodes. New test `test_local_query_graph_degree_penalty`. Root cause: degree boost promoted generic hub concepts to top MRR positions.
- [x] P3-spec — ADR-004 M14-D Guild Dispatch spec: `docs/architecture/M14D_dispatch_spec.md` — Capability-Aware + Latency-Based Hybrid Routing, 4-phase plan (~8 sessions), CONTRACT-01 preserved.

**273 kernel lib tests + 61 link tests + 2 evals = 336 total** · 0 failures.

## v0.11.0 — Guild Execution Channels (in progress)

**Goal:** Peers discover each other's capabilities and dispatch guild tools remotely over Noise XK.

Delivered so far:
- [x] M14-D Phase 1 — Capability Registry: `HardwareCaps { ram_mb, has_gpu, load_avg }` in `GossipEntry` (`#[serde(default)]`, backwards compatible); `CapabilityRegistry` in `tylluan-link/src/capability.rs` with TTL=300s, `prune_expired()`, `ingest_from_engine()`; 6 unit tests.

Remaining (v0.11.0 backlog):
- [ ] M14-D Phase 2 — `DispatchRouter`: load+latency scoring, wire `prune_expired` into background gossip task in `main.rs`.
- [x] M14-D Phase 3 — `GuildDispatchRequest/Response` + Noise NK handler + `POST /api/v1/guilds/dispatch/execute` endpoint.
- [ ] M14-D Phase 4 — Fallback (queue + circuit breaker) + dashboard UX for peer capability view.
- [ ] M14-E — Mesh test harness (turmoil-based simulated fault injection, network partitions, recovery).
- [ ] Portability compliance CI: RPi4 (aarch64) smoke test in release workflow.

**Current:** 273 kernel + 67 link + 2 evals = **342 total tests** · 0 failures.

## M14-D — Guild Execution Channels (in progress — see v0.11.0 above)

**Status:** Active. Phase 1 complete. Spec published in `docs/architecture/M14D_dispatch_spec.md` (ADR-004).

**ADR-004 design (2026-07-02):**
- Capability-Aware + Latency-Based Hybrid Routing — transparent inside `tylluan_do` (CONTRACT-01 preserved)
- `CapabilityRegistry`: DHT+Gossip peer capability store, TTL=5min, `HardwareCaps { ram_mb, has_gpu, load_avg }`
- `DispatchRouter`: load+latency scoring for peer selection
- Remote Execution Protocol: `GuildDispatchRequest/Response` structs, JSON over Noise NK
- Fallback Strategy: local queue + circuit breaker

**Original design context (2026-06-30 coloquio, T50-T66):**
- Latency-aware routing between regional clusters
- RTT metric: ICMP pre-handshake (Option A — simple, sufficient for 2-cluster minimal)
- Remote guild execution channels over Noise XK (prerequisite: guild proxy protocol)
- `trait MeshTransport: Send { send/recv }` — sits above `NoiseSession`, compatible with in-memory mock for M14-E tests

**Prerequisite before Phase 3:** M14-E test harness (turmoil-based) — latency routing cannot be validated without multi-node fault injection.

## v1.0.0 — Production Ready

**Goal:** Safe to deploy in real environments.

Requirements (all must be met):
- [ ] Docker smoke tests integrated into release verification checklist
- [ ] External security audit completed
- [ ] 6+ months of community usage without critical vulnerabilities
- [ ] Benchmarks validated by independent parties
- [ ] Kill switch tested under adversarial conditions
- [ ] Documentation reviewed by non-contributors
- [ ] Stable API (no breaking changes for 3+ months)

---

*This roadmap reflects the project's actual state, not aspirational marketing. Items move to "done" only when verified.*
