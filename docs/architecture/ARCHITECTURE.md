# Architecture — Tylluan o3

## Overview

Tylluan o3 is a **sovereign MCP hub** — a local-first memory and routing substrate for AI agents. It exposes exactly 5 tools to any MCP client while internally orchestrating 30+ Python guilds, a semantic router, and a knowledge graph.

```
MCP Client (Claude, Gemini, etc.)
        │  5 sovereign tools via MCP
        ▼
  tylluan-proxy :3030  (zero-downtime gateway, fixed port)
        │  reverse proxies to active kernel port
        ▼
  tylluan :303x  (dynamic port, written to data/active_port.json)
        │
        ├── HTTP/SSE transport  (MCP + REST API)
        ├── Semantic Router     (BGE-M3 embeddings + Jina reranker)
        ├── Guild Registry      (30+ Python fastmcp subprocesses)
        ├── SilvaDB             (SQLite knowledge graph, BGE-M3 vectors, IVF index)
        ├── HybridMemory        (FTS5 + vector search)
        └── IdleLab             (autonomous hyperparameter tuning)
```

## 5 Sovereign Tools

| Tool | Purpose |
|------|---------|
| `tylluan_do` | Universal intent router — natural language → guild → tool |
| `tylluan_remember` | Store content in SilvaDB with embedding |
| `tylluan_recall` | Hybrid search (semantic + BM25 + graph) over SilvaDB |
| `tylluan_think` | Sequential reasoning chains via `sequential_thinking` guild |
| `tylluan_graph` | Graph operations — add nodes/edges, query relationships |

## Key Components

### Semantic Router
BGE-M3 (ONNX, DirectML GPU) embeds the user intent. Cosine similarity against 31 guild descriptors selects the target guild. A Jina reranker refines the top candidates. Routing anchors (666 nodes) provide fast-path shortcuts for known patterns.

### Guild System
Python subprocesses using `fastmcp` stdio protocol. The kernel spawns them on demand and supervises restarts. Always-on guilds: `bash`, `filesystem`, `monitor`, `coloquio`, `knowledge`. Lazy guilds spawn on first call, idle-timeout after inactivity.

### SilvaDB
SQLite WAL database with:
- Node types: `episode`, `lesson`, `summary`, `document`, `concept`, `synthesis`, `agent_memory`, `routing_anchor`
- IVF index (941 centroids) for ANN search over BGE-M3 embeddings
- int8 quantization + memory-mapped storage (`.fjv1` files) for <10ms search
- Biological decay: node weight decays over time, pruning stale knowledge
- NightConsolidation: runs every 30 min, deduplicates summaries, links orphans

### Knowledge Flywheel (M36)
```
coloquio channels → coloquio_digest guild → SilvaDB nodes
                                          ↓
                              auto_reason_cycle (background)
                                          ↓
                              synthesis nodes → richer tylluan_recall
```
Digest is incremental (offset checkpoints), deduped (SHA256), and noise-filtered.

### Zero-Downtime Proxy
`tylluan-proxy` holds the fixed `:3030` port. On kernel rebuild, the new kernel writes its port to `data/active_port.json` and the proxy hot-switches. MCP clients never lose connection.

## Data Files

| File | Content |
|------|---------|
| `data/silva.db` | Knowledge graph (nodes + edges + embeddings) |
| `data/tylluan.db` | Sessions, coloquio channels, audit log |
| `data/active_port.json` | Current kernel port (read by proxy) |
| `data/coloquio_digest.db` | Digest checkpoints + SHA256 hash dedup table |
| `crates/tylluan-kernel/.tylluan-token` | Bearer token (untracked) |

## Source Layout

```
crates/
  tylluan-kernel/src/
    main.rs              — startup, config, transport init
    transport/http/
      api_v1.rs          — route tree (1,600 lines)
      api_v1/            — domain handlers (13 modules)
      server/            — MCP tool handlers (tylluan_do etc.)
    router/              — semantic matcher, embeddings, catalog
    registry/            — guild process lifecycle, supervisor
    memory/
      silva/             — knowledge graph, IVF, decay, autolink
      hybrid.rs          — FTS5 + vector hybrid search
      idle_lab.rs        — autonomous hyperparameter tuning
guilds/core/             — Python fastmcp guilds
dashboard_v3/            — React dashboard (port 5173 dev / bundled prod)
```

## Milestones Reference

See [docs/roadmap/ROADMAP_O3.md](../roadmap/ROADMAP_O3.md) for the full milestone sequence.
Critical path: M28 → M32 → M30 (A2A) → M2 (public).
