# Architecture — Tylluan

## Overview

Tylluan is a **sovereign MCP kernel** — a local-first memory and routing substrate for AI agents. It exposes exactly 5 tools to any MCP client while internally orchestrating 47+ Python guilds, a semantic router, a knowledge graph, and a P2P mesh.

```
MCP Client (Claude, Cursor, VS Code, LM Studio, any SSE client)
        │  SSE / HTTP Streamable
        ▼
  tylluan-nexus (:3030)            ← single process, no proxy
        │
        ├── MCP Transport          (SSE + HTTP Streamable)
        ├── Intent Router          (BGE-M3 embeddings → guild selection + keyword scoring)
        ├── Guild Registry         (47+ Python fastmcp processes, auto-discovered from guilds/)
        ├── SilvaDB                (SQLite WAL · BGE-M3 vectors · FTS5 BM25 · knowledge graph)
        ├── Core Memory            (agent persona + preferences — always available, never retrieved)
        ├── Coloquio               (multi-agent channels, episodic flywheel)
        ├── Federation Layer       (peers.db · ChaCha20-Poly1305 · provenance · echo-loop safe)
        └── Mesh Layer             (DHT Kademlia · Gossip push-pull · Noise Protocol XK)
```

**There is no proxy.** `tylluan-nexus` binds directly to `:3030`. Zero-downtime restarts are handled by the OS — clients reconnect on the SSE retry loop.

## 5 Sovereign Tools (CONTRACT-01)

Every MCP client sees exactly these 5 tools — nothing more, nothing less:

| Tool | Purpose |
|------|---------|
| `tylluan_do` | Universal intent router — natural language → guild → tool |
| `tylluan_remember` | Store content in SilvaDB with BGE-M3 embedding |
| `tylluan_recall` | Hybrid search (BM25 + FTS5 + vector + LinearRAG graph) over SilvaDB |
| `tylluan_think` | Sequential reasoning chains via `sequential_thinking` guild |
| `tylluan_graph` | Graph operations — add nodes/edges, query relationships, PageRank |

CONTRACT-01 is inviolable. New capabilities route through `tylluan_do`, never as new tools.

## Key Components

### Intent Router (Semantic Guild Dispatcher)
BGE-M3 (local ONNX, CPU) embeds the user intent. Cosine similarity against guild descriptors selects the target guild. 34 `description_override()` entries in the catalog preserve routing quality for ambiguous patterns. Keyword scoring provides a fast-path for deterministic intents.

### Guild System
Python subprocesses using `fastmcp` stdio protocol. Auto-discovered from `guilds/` at startup — zero manual registration. Guild catalog cached via `OnceLock` (startup ~5s). Always-on guilds: `bash`, `filesystem`, `monitor`, `coloquio`, `knowledge`, `websearch`, and more. On-demand guilds spawn on first call.

### SilvaDB (Memory Engine)
SQLite WAL database with:
- **Node types:** `episodic`, `lesson`, `concept`, `entity`, `decision`, `document`, `agent_memory`, and more
- **Hybrid search pipeline:** FTS5 BM25 → IVF ANN (BGE-M3 1024d) → LinearRAG graph traversal (Personalized PageRank + degree penalty) → RRF fusion (k=60) → entity boost ×1.25
- **HNSW fast path:** `instant-distance` index for datasets ≥12k nodes; falls back to IVF, then linear
- **Salience decay:** `weight * 0.5^(hours / half_life)` — configurable per node type (default T½=336h=14d)
- **Schema:** v12 (FTS5 at v11, HNSW BLOB at v12)

### Core Memory
`AgentProfile` stores `persona: String` + `preferences: serde_json::Value` — always loaded, never retrieved on demand. Accessible via `tylluan_recall` and `tylluan_remember` subtool routing without adding new sovereign tools.

### Episodic Flywheel
Background task (every 60s): ingests Coloquio conversation turns into SilvaDB as `episodic` nodes. Deterministic IDs `coloquio:{channel}:{turn}`, watermark-based dedup. Enables agents to search past conversations via `tylluan_recall episodic:true`.

### Federation Layer
Peer-to-peer knowledge sync:
- SQLite `peers.db` — persistent peer registry with approval gate
- ChaCha20-Poly1305 encryption per peer (no shared global key)
- Push / pull / bidirectional sync endpoints
- `federation_source` provenance on all received nodes
- Echo-loop prevention: received nodes never re-exported by default
- Background auto-sync driven by `[federation] auto_sync_interval_secs`

### Mesh Layer (tylluan-link crate)
P2P discovery and transport:
- **DHT Kademlia (M14-A):** 256 K-buckets, Ed25519 XOR metric, mainline BitTorrent DHT bootstrap
- **Gossip (M14-B):** Symmetric push-pull, LRU entry store, anti-entropy cursor tracking, hardware capability fields (`ram_mb`, `has_gpu`, `load_avg`) in `GossipEntry`
- **Noise Protocol XK (M14-C):** 3-message handshake, Ed25519→X25519 key conversion, ChaCha20-Poly1305 AEAD, length-prefixed async framing — wired to federation sync endpoints
- **CapabilityRegistry:** In-memory TTL store of peer hardware capabilities, pruned via background task — foundation for M14-D guild dispatch

## Data Files

| File | Content |
|------|---------|
| `data/silva.db` | Knowledge graph (nodes + edges + embeddings + FTS5 + HNSW) |
| `data/peers.db` | Federation peer registry |
| `data/identity.key` | Ed25519 node keypair (mesh identity) |
| `.tylluan-token` | Bearer token — in working directory for source builds, `~/.tylluan/` for binary installs |

## Source Layout

```
crates/
  tylluan-kernel/src/
    main.rs                   — startup, config, transport init, background tasks
    transport/
      http/                   — axum routes, auth middleware, SSE handler
      server/                 — MCP tool handlers (handler_recall.rs, handler_do.rs, etc.)
    memory/
      silva/
        graph.rs              — degree_centrality, local_query_graph, PageRank
        search.rs             — search_hybrid (RRF fusion, type filter, skip_graph)
        embeddings.rs         — embed_batch (ONNX, single mutex, L2-norm)
    router/
      catalog.rs              — guild catalog (OnceLock cached)
      embeddings.rs           — BGE-M3 intent embedding
    security/
      guard.rs                — execution guard, secure_compare
      auth.rs                 — bearer auth, sanitize_query, extract_token
  tylluan-link/src/
    gossip/                   — GossipEngine, GossipEntry (with HardwareCaps)
    dht/                      — Kademlia routing table
    noise/                    — Noise XK/NK transport
    capability.rs             — CapabilityRegistry (M14-D Phase 1)
    transport.rs              — MeshTransport trait + PartitionableTransport (5 fault modes)
  tylluan-cli/                — start / stop / status / install --profile=...
  tylluan-evals/              — Recall@N, MRR, latency benchmark harness
  tylluan-common/             — Shared types and error types
guilds/core/                  — 47+ Python fastmcp guild scripts
dashboard/                    — React + Vite dashboard (embedded in binary via rust-embed)
```

## Security Invariants

- `host = "127.0.0.1"` — localhost-only binding by default
- `dev_mode = false` — bearer auth required by default
- **Never** `host = "0.0.0.0"` + `dev_mode = true` (LAN RCE)
- Tokens never in tracked files — only `.tylluan-token` (gitignored)
- `vector_dimensions = 1024` — BGE-M3 is 1024d; never set to 768
