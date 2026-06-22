# Tylluan o3 — Architecture Blueprints

> Technical schematics only. No prose. Generated 2026-06-18.

---

## 1. System Topology

```
┌─────────────────────────────────────────────────────────────────────┐
│                        CLIENTS (MCP)                                │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │Claude    │ │Gemini    │ │Qwen      │ │LM Studio │ │Custom    │ │
│  │Code      │ │Client    │ │Desktop   │ │(local)   │ │Client    │ │
│  │(Sonnet)  │ │(Gemini)  │ │(Qwen3)   │ │(any)     │ │(any LLM) │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ │
│       │SSE          │HTTP        │SSE          │SSE          │SSE   │
└───────┼─────────────┼────────────┼─────────────┼─────────────┼──────┘
        │             │            │             │             │
        ▼             ▼            ▼             ▼             ▼
┌───────────────────────────────────────────────────────────────────┐
│                    tylluan-proxy :3030                               │
│              Zero-Downtime Gateway (Rust)                         │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │  GET /sse ──► SSE transport                                 │  │
│  │  POST /messages ──► HTTP Streamable MCP                     │  │
│  │  POST /api/v1/* ──► REST passthrough                        │  │
│  │  GET / ──► Dashboard static (prod)                          │  │
│  └─────────────────────────┬───────────────────────────────────┘  │
│                            │ reverse proxy                        │
│                            ▼                                      │
│              tylluan :303X (dynamic port)                     │
│              active_port.json                                     │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│                    tylluan KERNEL                              │
│                    (Rust · tokio · axum)                           │
└───────────────────────────────────────────────────────────────────┘
```

---

## 2. Kernel Internal Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           TYLLUAN-NEXUS KERNEL                            │
│                                                                         │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────────┐  │
│  │  TRANSPORT   │    │  SOVEREIGN   │    │      MEMORY LAYER           │  │
│  │             │    │  TOOLS (5)   │    │                             │  │
│  │  HTTP/SSE   │───▶│             │───▶│  ┌────────┐  ┌───────────┐ │  │
│  │  api_v1.rs  │    │ tylluan_do    │    │  │HybridDB│  │  SilvaDB  │ │  │
│  │  sse.rs     │    │ tylluan_recall│    │  │(tylluan. │  │  (silva.  │ │  │
│  │  auth.rs    │    │ tylluan_remem.│    │  │  db)   │  │    db)    │ │  │
│  │  oauth.rs   │    │ tylluan_think │    │  │ BM25   │  │  Graph   │ │  │
│  │             │    │ tylluan_graph │    │  │ FTS5   │  │  Memory  │ │  │
│  └──────┬──────┘    └──────┬──────┘    │  └───┬────┘  └────┬──────┘ │  │
│         │                  │           │      │            │        │  │
│         │                  │           │      ▼            ▼        │  │
│         │                  │           │  ┌─────────────────────┐   │  │
│         │                  │           │  │   Hybrid Search     │   │  │
│         │                  │           │  │ BGE-M3 + BM25 + RRF│   │  │
│         │                  │           │  │ + Jina Reranker     │   │  │
│         │                  │           │  └─────────────────────┘   │  │
│  ┌──────▼──────┐    ┌──────▼──────┐    │                           │  │
│  │   ROUTER    │    │  REGISTRY   │    │  ┌─────────────────────┐   │  │
│  │             │    │             │    │  │  Hot Context Buffer │   │  │
│  │ Semantic    │    │ Guild       │    │  │  (Letta-style 20)   │   │  │
│  │ Intent      │    │ Lifecycle   │    │  └─────────────────────┘   │  │
│  │ Matcher     │    │ Supervisor  │    │                           │  │
│  │ Fractal     │    │ MCP Proxy   │    │  ┌─────────────────────┐   │  │
│  │ Tool Tree   │    │ Actor Model │    │  │    Mailbox (WAL)    │   │  │
│  └─────────────┘    └──────┬──────┘    │  │   Agent Journal     │   │  │
│                            │           │  └─────────────────────┘   │  │
│                            │           └─────────────────────────────┘  │
│  ┌─────────────────────────▼──────────────────────────────────────────┐ │
│  │                     COGNITIVE SUBSYSTEMS                           │ │
│  │                                                                    │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌─────────┐ │ │
│  │  │ Dream    │ │ Graph    │ │ Louvain  │ │ Idle     │ │ Consen- │ │ │
│  │  │ Cycle    │ │ RAG      │ │ Commun.  │ │ Lab      │ │ sus     │ │ │
│  │  │ (Night   │ │ (Triple  │ │ (Module  │ │ (Auto    │ │ (Node   │ │ │
│  │  │  Consol.)│ │  Extract)│ │  Detect) │ │  Resrch) │ │  Merge) │ │ │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └─────────┘ │ │
│  │                                                                    │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐             │ │
│  │  │Hormones  │ │Identity  │ │Auto-Link │ │IVF Index │             │ │
│  │  │(Homeo-   │ │(Agent    │ │(Orphan   │ │(k-means++│             │ │
│  │  │ stasis)  │ │ Profile) │ │ Pass)    │ │ 941 ctr) │             │ │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘             │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                      SECURITY                                    │   │
│  │  Circuit Breaker │ Rate Limiter │ Guard │ Process Isolation      │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 3. MCP Protocol Flow

```
CLIENT                    PROXY :3030              KERNEL :303X
  │                          │                         │
  │──GET /sse───────────────▶│                         │
  │◀─── SSE stream ─────────│──GET /sse──────────────▶│
  │                          │◀── SSE stream ──────────│
  │                          │                         │
  │──POST /messages─────────▶│                         │
  │  {initialize}            │──POST /messages────────▶│
  │                          │◀── InitializeResult ────│
  │◀── {protocolVersion,     │     {5 tools,           │
  │     capabilities}        │      prompts,           │
  │                          │      resources}         │
  │                          │                         │
  │──POST /messages─────────▶│                         │
  │  {tools/call:            │──POST /messages────────▶│
  │   tylluan_recall}          │                         │
  │                          │  ┌─────────────────┐    │
  │                          │  │ BGE-M3 embed    │    │
  │                          │  │ IVF ANN search  │    │
  │                          │  │ BM25 FTS5       │    │
  │                          │  │ RRF merge       │    │
  │                          │  │ Jina rerank     │    │
  │                          │  └─────────────────┘    │
  │◀── {results[]} ──────────│◀── {results[]} ────────│
  │                          │                         │

  Protocol versions supported:
  ├── 2024-11-05 (Claude Desktop, Qwen)
  └── 2025-06-18 (HTTP Streamable clients)
```

---

## 4. Guild System Architecture

```
                    tylluan_do("analyze my code")
                           │
                           ▼
                  ┌─────────────────┐
                  │ Semantic Router  │
                  │ (intent matcher) │
                  └────────┬────────┘
                           │ score > threshold
                           ▼
                  ┌─────────────────┐
                  │  Guild Registry  │
                  │  42 registered   │
                  │  ~23 running     │
                  └────────┬────────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
      ┌──────────┐  ┌──────────┐  ┌──────────┐
      │ always_on│  │ on_demand│  │  ghost   │
      │          │  │          │  │ (missing) │
      └──────────┘  └──────────┘  └──────────┘

  GUILD LIFECYCLE:
  ┌────────┐    spawn     ┌─────────┐   ready    ┌─────────┐
  │DORMANT │─────────────▶│STARTING │───────────▶│ RUNNING │
  └────────┘              └─────────┘            └────┬────┘
       ▲                                              │
       │              ┌──────────┐    crash/timeout    │
       └──────────────│  STOPPED │◀────────────────────┘
                      └─────┬────┘
                            │ supervisor retry
                            │ (backoff: 2^n, max 600s)
                            ▼
                      ┌──────────┐
                      │ DEGRADED │ (after 3 failures)
                      └──────────┘

  GUILD COMMUNICATION:
  ┌────────────┐    stdio     ┌────────────┐
  │   Kernel   │◀════════════▶│  Python    │
  │ McpProxy   │   (fastmcp)  │  Guild     │
  │ (Rust)     │              │  Process   │
  └────────────┘              └────────────┘

  ALWAYS-ON GUILDS:           ON-DEMAND GUILDS:
  ├── bash                    ├── deep_analysis
  ├── git                     ├── vision (SmolVLM2)
  ├── monitor                 ├── knowledge
  ├── docker                  ├── web_research
  ├── code                    ├── browser
  ├── filesystem              ├── code_analysis
  ├── system_metrics          ├── database
  └── coloquio_digest         └── ... (34 more)
```

---

## 5. Memory & Knowledge Graph

```
  ┌──────────────────────────────────────────────────────────────────┐
  │                    MEMORY ARCHITECTURE                           │
  │                                                                  │
  │  ┌──────────────────────┐    ┌───────────────────────────────┐  │
  │  │   HybridMemory       │    │         SilvaDB               │  │
  │  │   (tylluan.db)          │    │        (silva.db)              │  │
  │  │                      │    │                               │  │
  │  │  ┌────────────────┐  │    │  ┌─────────────────────────┐  │  │
  │  │  │ documents      │  │    │  │ nodes (1,961)           │  │  │
  │  │  │ (3,137 rows)   │  │    │  │ ├── id (UUID)           │  │  │
  │  │  │ FTS5 BM25      │  │    │  │ ├── type (fact/event/   │  │  │
  │  │  └────────────────┘  │    │  │ │   summary/identity/..)│  │  │
  │  │                      │    │  │ ├── content (text)       │  │  │
  │  │  ┌────────────────┐  │    │  │ ├── embedding (1024-dim) │  │  │
  │  │  │ embeddings     │  │    │  │ ├── weight (0.0-1.0)    │  │  │
  │  │  │ (BGE-M3 ONNX)  │  │    │  │ ├── topic_key          │  │  │
  │  │  │ 1024-dim        │  │    │  │ └── protected/conflict │  │  │
  │  │  └────────────────┘  │    │  └─────────────────────────┘  │  │
  │  │                      │    │                               │  │
  │  │  ┌────────────────┐  │    │  ┌─────────────────────────┐  │  │
  │  │  │ sessions       │  │    │  │ edges (2,894)           │  │  │
  │  │  │ agent_journal  │  │    │  │ ├── source_id           │  │  │
  │  │  │ coloquio       │  │    │  │ ├── target_id           │  │  │
  │  │  └────────────────┘  │    │  │ ├── relation_type       │  │  │
  │  │                      │    │  │ └── weight              │  │  │
  │  └──────────────────────┘    │  └─────────────────────────┘  │  │
  │                              │                               │  │
  │                              │  ┌─────────────────────────┐  │  │
  │                              │  │ IVF Index               │  │  │
  │                              │  │ 941 centroids           │  │  │
  │                              │  │ k-means++ clustering    │  │  │
  │                              │  └─────────────────────────┘  │  │
  │                              │                               │  │
  │                              │  ┌─────────────────────────┐  │  │
  │                              │  │ Louvain Communities     │  │  │
  │                              │  │ 12 modules detected     │  │  │
  │                              │  └─────────────────────────┘  │  │
  │                              └───────────────────────────────┘  │
  │                                                                  │
  │  ┌──────────────────────────────────────────────────────────┐    │
  │  │              SEARCH PIPELINE                              │    │
  │  │                                                          │    │
  │  │  Query ──▶ BGE-M3 embed ──▶ IVF ANN (top-100)           │    │
  │  │              │                    │                       │    │
  │  │              ▼                    ▼                       │    │
  │  │         BM25 FTS5 ──────▶ RRF Merge (k=60)              │    │
  │  │                               │                          │    │
  │  │                               ▼                          │    │
  │  │                      Jina Reranker v1                    │    │
  │  │                          Turbo                           │    │
  │  │                               │                          │    │
  │  │                               ▼                          │    │
  │  │                      Hot Context Buffer                  │    │
  │  │                      (recency ×2.0)                      │    │
  │  │                               │                          │    │
  │  │                               ▼                          │    │
  │  │                         Results[]                        │    │
  │  └──────────────────────────────────────────────────────────┘    │
  └──────────────────────────────────────────────────────────────────┘
```

---

## 6. Cognitive Cycles

```
  ┌─────────────────────────────────────────────────────┐
  │              NIGHT CONSOLIDATION                     │
  │              (every 1800s + boot tick)                │
  │                                                     │
  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌────────┐│
  │  │ Orphan  │  │ Decay   │  │ Dedup   │  │ Graph  ││
  │  │ Pass    │  │ Weights │  │ SHA-256 │  │ RAG    ││
  │  │(hybrid) │  │         │  │ Hash   │  │ Triple ││
  │  │emb+topic│  │         │  │ Cache  │  │ Extrctr││
  │  │+keyword │  │         │  │         │  │         ││
  │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬───┘│
  │       │            │            │             │     │
  │       ▼            ▼            ▼             ▼     │
  │  ┌─────────────────────────────────────────────┐   │
  │  │         SilvaDB Mutations                   │   │
  │  │  merge / decay / link / delete              │   │
  │  └─────────────────────────────────────────────┘   │
  └─────────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────┐
  │              IDLE LAB (AutoResearch)                  │
  │              Hill-climbing retrieval params           │
  │                                                     │
  │  Probe ──▶ Modify param ──▶ Benchmark ──▶ Accept?   │
  │              (k, rrf_k,      (recall@N)    │  │     │
  │               rerank_top)                  Yes No    │
  │                                            │  │     │
  │                                   Keep ◄───┘  │     │
  │                                   Revert ◄────┘     │
  └─────────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────┐
  │              HOMEOSTASIS (Hormones)                   │
  │                                                     │
  │  Metrics ──▶ Stress Level ──▶ Throttle/Boost        │
  │  (CPU, RAM,    (0.0-1.0)       guild spawns         │
  │   queue depth)                                      │
  └─────────────────────────────────────────────────────┘
```

---

## 7. Federation Architecture

```
  ┌──────────────────────┐         ┌──────────────────────┐
  │  PRIMARY :3030       │         │  SECONDARY :3040      │
  │  (Windows native)    │         │  (Docker)             │
  │                      │  sync   │                      │
  │  SilvaDB ◀══════════════════▶ SilvaDB              │
  │  1,961 nodes         │  355    │  data-docker/        │
  │                      │  nodes  │                      │
  │  POST /api/v1/       │  ────▶  │  POST /api/v1/       │
  │  federation/push     │         │  federation/push     │
  │                      │  ◀────  │                      │
  │  POST /api/v1/       │         │  POST /api/v1/       │
  │  federation/pull     │         │  federation/pull     │
  │                      │         │                      │
  │  tylluan.toml:         │         │  tylluan.docker.toml:  │
  │  [federation]        │         │  [federation]        │
  │  peers = [":3040"]   │         │  peers = [":3030"]   │
  └──────────────────────┘         └──────────────────────┘
```

---

## 8. Dashboard Component Tree

```
  App.tsx
  ├── OverviewTab
  │   ├── CanaryStatusWidget
  │   ├── DreamStatusWidget
  │   ├── HomeostasisWidget
  │   └── SparklineChart
  ├── FleetTab
  │   └── McpRegistryPanel
  ├── ConnectorsTab
  │   └── FederationPanel
  ├── GuildsTab
  ├── BlackboardTab
  ├── LaboratoryTab
  │   └── ModelConfigPanel
  ├── ColoquioTab
  │   ├── ColoquioChannelsPanel
  │   ├── ColoquioMessagesPanel
  │   ├── ColoquioAgentsPanel
  │   ├── ColoquioGraphPanel
  │   └── ColoquioCanvasWorkspace
  ├── KnowledgeGraphTab
  │   └── HippocampusGraph (Sigma.js)
  ├── NodesTab
  ├── FederationTab
  ├── InteroceptionTab
  ├── CollectiveTab
  ├── SessionsTab
  ├── VisionTab
  ├── SystemTab
  ├── MaintenanceTab
  ├── LogsTab
  └── IngestPanel

  Data hooks:
  ├── useNexus.tsx (REST polling)
  ├── useNexusSSE.ts (SSE real-time)
  └── useLoadingState.ts
  Bridge: nexus-bridge.ts → 127.0.0.1:3030
  Worker: graphLayout.worker.ts (Web Worker)
```

---

## 9. Crate Dependency Graph

```
  tylluan-kernel ◀─── main binary (tylluan.exe)
       │
       ├── tylluan-common (shared types)
       │
       ├── rmcp (MCP protocol)
       ├── axum + tower (HTTP)
       ├── tokio (async runtime)
       ├── rusqlite (SQLite)
       ├── ort (ONNX Runtime)
       ├── serde + serde_json
       ├── tracing (structured logging)
       └── chrono, uuid, anyhow, thiserror

  tylluan-proxy ◀─── gateway binary (tylluan-proxy.exe)
       │
       ├── hyper + hyper-util
       └── tokio

  tylluan-cli ◀─── CLI tool
  tylluan-gui ◀─── Tauri desktop (spike)
  tylluan-link ◀─── federation library
  tylluan-evals ◀─── LongMemEval benchmark
  tylluan-common ◀─── shared types & constants
```

---

## 10. Sovereign Tool Contract

```
  ┌────────────────────────────────────────────────────────────┐
  │  5 SOVEREIGN TOOLS — all_tools() filters to exactly these  │
  │  test_sovereign_count_is_exactly_5 enforces this           │
  │                                                            │
  │  ┌─────────────┐  Natural language intent → guild router   │
  │  │ tylluan_do    │  → keyword-scored tool selection          │
  │  │             │  → rich JSON args → guild execution       │
  │  └─────────────┘                                           │
  │                                                            │
  │  ┌─────────────┐  BGE-M3 + BM25 + RRF + Jina reranker    │
  │  │ tylluan_recall│  → semantic memory retrieval              │
  │  └─────────────┘                                           │
  │                                                            │
  │  ┌─────────────┐  Ingest → embed → SilvaDB + HybridDB    │
  │  │tylluan_remember│  → graph edges auto-linked               │
  │  └─────────────┘                                           │
  │                                                            │
  │  ┌─────────────┐  Multi-hop graph traversal               │
  │  │ tylluan_think │  → reasoning over knowledge graph         │
  │  └─────────────┘                                           │
  │                                                            │
  │  ┌─────────────┐  SilvaDB stats, node CRUD, edge ops     │
  │  │ tylluan_graph │  → direct graph manipulation              │
  │  └─────────────┘                                           │
  │                                                            │
  │  "Herramientas ilimitadas detrás, 5 soberanas delante"    │
  └────────────────────────────────────────────────────────────┘
```

---

## 11. Data Flow: tylluan_recall

```
  Client: tylluan_recall("architectural decisions about decay")
      │
      ▼
  handler_recall.rs
      │
      ├──▶ Cache check (RECALL_CACHE_TTL)
      │         hit? → return cached
      │
      ├──▶ BGE-M3 ONNX embed query (1024-dim)
      │         │
      │         ├──▶ IVF Index: ANN search (top-100 candidates)
      │         │
      │         └──▶ BM25 FTS5: keyword search
      │                   │
      │                   ▼
      │              RRF merge (k=60)
      │                   │
      │                   ▼
      │           Jina Reranker v1 Turbo
      │                   │
      │                   ▼
      │           Hot Context Buffer boost (recency ×2.0)
      │                   │
      │                   ▼
      │           Top-N results with scores
      │
      ▼
  Response: [{content, score, type, topic_key, metadata}, ...]
```

---

## 12. Security Perimeter

```
  ┌─────────────────────────────────────────────────────┐
  │                 SECURITY LAYERS                      │
  │                                                     │
  │  Layer 1: Network                                   │
  │  ├── Bind 127.0.0.1 only (NEVER 0.0.0.0 + dev)    │
  │  └── INVARIANT: host=0.0.0.0 + dev_mode=true = RCE │
  │                                                     │
  │  Layer 2: Authentication                            │
  │  ├── Bearer token (.tylluan-token)                    │
  │  ├── OAuth 2.0 (partial)                            │
  │  └── dev_mode=true bypasses auth                    │
  │                                                     │
  │  Layer 3: Rate Limiting                             │
  │  ├── Per-session rate limiter                       │
  │  └── Circuit breaker (3 errors → 30s cooldown)     │
  │                                                     │
  │  Layer 4: Process Isolation                         │
  │  ├── CPU affinity per guild                         │
  │  └── Guild process sandboxing                       │
  │                                                     │
  │  Layer 5: Data                                      │
  │  ├── Token NEVER in tracked files                   │
  │  ├── Secrets in ~/.tylluan/secrets                    │
  │  └── SQLite WAL mode                                │
  └─────────────────────────────────────────────────────┘
```

---

## 13. File System Layout

```
  E:\TylluanMCPo3\
  ├── crates/
  │   ├── tylluan-kernel/          # Main binary (tylluan.exe)
  │   │   └── src/
  │   │       ├── main.rs        # Startup, config, transport init
  │   │       ├── transport/     # HTTP, SSE, MCP handlers
  │   │       │   ├── http/      # axum routes (13 api_v1 sub-modules)
  │   │       │   └── server/    # Sovereign tool handlers
  │   │       ├── memory/        # HybridDB, SilvaDB, cognitive cycles
  │   │       │   └── silva/     # Graph memory (11 sub-modules)
  │   │       ├── registry/      # Guild lifecycle, supervisor, proxy
  │   │       ├── security/      # Circuit breaker, rate limiter, guard
  │   │       ├── router/        # Semantic intent routing
  │   │       ├── federation/    # Peer sync protocol
  │   │       └── doctor/        # Self-diagnostics
  │   ├── tylluan-proxy/           # Zero-downtime gateway
  │   ├── tylluan-cli/             # CLI interface
  │   ├── tylluan-gui/             # Tauri desktop (spike)
  │   ├── tylluan-link/            # Federation library
  │   ├── tylluan-evals/           # Benchmarks
  │   └── tylluan-common/          # Shared types
  ├── guilds/
  │   └── core/                  # 34 Python guild scripts (fastmcp)
  ├── dashboard_v3/              # React + Vite + Sigma.js
  │   └── src/ (44 files)
  ├── data/                      # Runtime databases
  │   ├── tylluan.db               # HybridMemory
  │   ├── silva.db               # Graph memory
  │   └── mailbox.db             # Agent messaging
  ├── docker-secondary/          # Docker instance config
  ├── tylluan.toml                 # Runtime configuration
  ├── tylluan.bat              # Startup script (proxy + kernel)
  └── Cargo.toml                 # Workspace manifest (7 crates)
```
