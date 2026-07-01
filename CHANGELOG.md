# Changelog

All notable changes to Tylluan are documented here.

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
