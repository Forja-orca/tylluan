# TEB-Pilot-50: 50 Criterios de Éxito para Juez Ciego

> Extraído de `tasks_pilot_50.json` y `TEB_PILOT_SPEC.md` por Antigravity (Turno 441/442).

| # | ID | Familia | Título | Criterio de Éxito / Ground Truth | Keywords Clave |
|---|---|---|---|---|---|
| 1 | `mem_01` | `long_term_memory` | **BGE-M3 Vector Dimensions Invariant** | vector_dimensions must be 1024; reducing to 768 breaks all embeddings | `1024, 768, breaks, dimensions` |
| 2 | `mem_02` | `long_term_memory` | **Degree Centrality Penalty Formula** | Uses degree penalty pr_score / (1 + deg * 0.1) to penalize generic hubs, not multiplication/boost | `penalty, 1 + deg * 0.1, pr_score, hubs` |
| 3 | `mem_03` | `long_term_memory` | **Kernel Port and Proxy Architecture** | tylluan-nexus listens directly on port :4000; there is NO zero-downtime hyper proxy | `4000, no proxy, direct` |
| 4 | `mem_04` | `long_term_memory` | **ADR-011 LightReranker Cutover Gate** | Requires >=5000 resolved rows (useful != 0) in recall_feedback table | `5000, recall_feedback, cutover` |
| 5 | `mem_05` | `long_term_memory` | **Noise Protocol Handshake Variants in Mesh** | Noise NK is used for one-way gossip; Noise XK session pool with Ed25519/X25519 is used for 2-way P2P TCP dispatch | `Noise NK, Noise XK, gossip, P2P` |
| 6 | `mem_06` | `long_term_memory` | **SilvaDB Memory Decay Half-Life** | Half-life T1/2 = 336 hours (14 days), decay pruning threshold = 0.15 | `336, 14, 0.15, half-life` |
| 7 | `mem_07` | `long_term_memory` | **Sovereign Tools Restriction** | tylluan_do, tylluan_remember, tylluan_recall, tylluan_think, tylluan_graph | `tylluan_do, tylluan_remember, tylluan_recall, tylluan_think, tylluan_graph` |
| 8 | `mem_08` | `long_term_memory` | **Night Consolidation Architecture** | Consolidation is centralized in NightConsolidation via main.rs cron; DreamCycle background scheduler is inactive/deprecated | `NightConsolidation, inactive, deprecated, cron` |
| 9 | `mem_09` | `long_term_memory` | **Agent Profile Store Migration** | AgentProfileStore replaced IdentityManager | `AgentProfileStore, IdentityManager` |
| 10 | `mem_10` | `long_term_memory` | **Learned-Sparse Retrieval Model** | Learned-sparse BGE-M3 representations fused via Reciprocal Rank Fusion (RRF) | `sparse, BGE-M3, RRF, learned` |
| 11 | `cont_01` | `continuity` | **Coloquio Unread Turn Sync** | Queries /api/v1/coloquio/channels/{id}/new with reader ID to fetch unread turns without re-reading entire history | `reader, unread, last_read_turn, turn` |
| 12 | `cont_02` | `continuity` | **Active Roadmap and Milestone Tracking** | Check STATUS.md, ROADMAP.md, and query git log -1 origin/main | `STATUS.md, ROADMAP.md, git log` |
| 13 | `cont_03` | `continuity` | **Pre-flight Git Remote Sync Check** | git fetch origin && git log -1 origin/main --oneline && git log -1 HEAD --oneline | `git fetch, origin/main, HEAD` |
| 14 | `cont_04` | `continuity` | **Fleet Role Boundaries** | Claude Code=Tech Lead/Orchestration; Deep=Rust kernel & guilds; Antigravity=UI/UX & empirical verification | `Claude, Deep, Antigravity, Rust, UI` |
| 15 | `cont_05` | `continuity` | **CoherenceGate Hybrid State Reason** | Was auto-starting llama-server in background without opt-in, killing active 4-day Unsloth training run | `Unsloth, llama-server, background, opt-in` |
| 16 | `cont_06` | `continuity` | **Autonomous Success Rate Metric Schema** | latency_ms, human_intervention, risk_tier, execution_status | `latency_ms, human_intervention, guild_audit_log` |
| 17 | `cont_07` | `continuity` | **Background GPU Protection Protocol** | Must specify --n-gpu-layers 0 to run strictly on CPU | `--n-gpu-layers 0, CPU, GPU` |
| 18 | `cont_08` | `continuity` | **Live Kernel Loaded Commit Verification** | Query curl http://127.0.0.1:4000/health and inspect git_commit field | `/health, git_commit, 4000` |
| 19 | `cont_09` | `continuity` | **Docs-site Port and Architecture Alignment** | Runs on port 3010 (docs-site); verify with cd docs-site && pnpm run build | `3010, docs-site, pnpm run build` |
| 20 | `cont_10` | `continuity` | **Session Mailbox Database Location** | Stored in data/mailbox.db under coloquio_messages and coloquio_channels tables | `data/mailbox.db, coloquio_messages, coloquio_channels` |
| 21 | `tool_01` | `tool_routing` | **Filesystem Plugin Function Names** | file_read, file_write, file_search, file_list, find_files (read_file/write_file are legacy) | `file_read, file_write, file_search, file_list` |
| 22 | `tool_02` | `tool_routing` | **Python Guild Tri-Registration Invariant** | In main.rs lazy_guilds list, router/catalog.rs catalog weight/description, and guild plugin directory | `main.rs, catalog.rs, lazy_guilds` |
| 23 | `tool_03` | `tool_routing` | **Plan Mode for Tool Invocations** | Returns proposed guild+tool+args chain without calling real guild process, reusing approve_action | `--plan, approve_action, without executing` |
| 24 | `tool_04` | `tool_routing` | **Vision Plugin Architecture** | guilds/core/vision_moondream.py using Moondream / SmolVLM | `vision_moondream, Moondream, SmolVLM` |
| 25 | `tool_05` | `tool_routing` | **AST Surgeon Capability Domain** | guilds/scholars/plugins/ast_surgeon.py | `ast_surgeon.py, scholars, syntax tree` |
| 26 | `tool_06` | `tool_routing` | **ACL Default Role Policy** | Applies default_role policy and denies write permissions to unprivileged readers | `default_role, fail-closed, ACL` |
| 27 | `tool_07` | `tool_routing` | **Tool Risk Tier Classification** | low, medium, high, critical | `low, medium, high, critical, TOOL_METADATA` |
| 28 | `tool_08` | `tool_routing` | **Code Graph Query Routing** | guilds/core/code_graph.py | `code_graph, dependencies, call graphs` |
| 29 | `step_01` | `multi_step` | **Memory Contradiction Resolution Flow** | 1. search_hybrid recall; 2. ConsensusEngine conflict check; 3. update_node version increment; 4. insert audit trace | `search_hybrid, ConsensusEngine, update_node, audit` |
| 30 | `step_02` | `multi_step` | **Rust Code Refactor Validation Flow** | 1. cargo check -p <crate>; 2. cargo clippy -p <crate> -- -D warnings; 3. cargo test -p <crate> --lib | `cargo check, cargo clippy, cargo test, -D warnings` |
| 31 | `step_03` | `multi_step` | **Dynamic Guild Auto-Discovery Lifecycle** | Scans guilds/ at startup, validates manifest, loads always_on guilds, marks warm_pool on demand | `auto-discovery, always_on, warm_pool, manifest` |
| 32 | `step_04` | `multi_step` | **Coloquio Cross-Agent Sync Pipeline** | Calls get_thread/search, parses @mentions with extract_mentions, performs task, posts to coloquio_messages | `get_thread, extract_mentions, coloquio_messages` |
| 33 | `step_05` | `multi_step` | **Diagnostic and Root Cause Triangulation** | 1. git status check; 2. isolate target test; 3. execute target test; 4. inspect logs in scratch/ | `git status, isolate, scratch/` |
| 34 | `step_06` | `multi_step` | **Benchmark Case Evaluation Pipeline** | 1. Load cases JSON; 2. Execute Arms A/B/C inferences; 3. Compute accuracy & Jaccard; 4. Write FULL_HARNESS_REPORT.md | `Arm A, Arm B, Arm C, Jaccard, FULL_HARNESS_REPORT.md` |
| 35 | `step_07` | `multi_step` | **SQLite Schema Migration Flow** | Uses _migrations table with PRAGMA user_version / version hashes and ALTER TABLE IF NOT EXISTS | `_migrations, idempotent, ALTER TABLE` |
| 36 | `rec_01` | `recovery` | **Llama Server 503 Cold Start Recovery** | Poll http://127.0.0.1:{port}/health waiting for HTTP 200 and status ok rather than raw TCP socket connect | `/health, 200, status ok, 503` |
| 37 | `rec_02` | `recovery` | **Socket Timeout Resilience in Multi-Agent Calls** | Sets Connection: close, expands timeout to 180s, and wraps in 3-attempt exponential backoff retry loop | `Connection: close, 180, retry, backoff` |
| 38 | `rec_03` | `recovery` | **Database WAL Recovery and msg_id Repair** | repair_msgids() in ColoquioDb assigns fresh UUIDv4 to orphaned rows | `repair_msgids, UUIDv4, ColoquioDb` |
| 39 | `rec_04` | `recovery` | **Sparse Engine Non-Fatal Fallback** | Failure is non-fatal: logs warning and search_hybrid continues running with 3-source RRF fusion | `non-fatal, fallback, 3-source, RRF` |
| 40 | `rec_05` | `recovery` | **Circuit Breaker on Remote Peer Dispatch** | Trips circuit breaker to Open state, falling back to local guild execution for subsequent requests | `circuit breaker, fallback, local execution` |
| 41 | `collab_01` | `collaboration` | **Anti-Collision Work Handoff Protocol** | Check Coloquio active assignments; mark work in progress; do not touch files assigned to other agents | `Coloquio, assignments, overlapping, in progress` |
| 42 | `collab_02` | `collaboration` | **Single Committer Release Rule** | Only Claude Code (Tech Lead) commits and pushes to origin/main; other agents commit locally | `Claude Code, origin/main, Tech Lead, push` |
| 43 | `collab_03` | `collaboration` | **Cross-Agent Schema Pre-Agreement Rule** | Agree on exact field names, schemas, and ports in Coloquio before writing code | `agree, Coloquio, schema, fields` |
| 44 | `collab_04` | `collaboration` | **Independent PR and Diff Verification** | Cross-agent independent verification of real diffs, test suite execution, and documentation synchronization | `independent verification, diff, documentation, STATUS.md` |
| 45 | `fed_01` | `federation` | **Hardware Capabilities in Gossip Messages** | vram_mb, cpu_cores, supports_p2p, tcp_port, gpu_model | `vram_mb, cpu_cores, supports_p2p, tcp_port` |
| 46 | `fed_02` | `federation` | **Partitionable Transport Simulation Modes** | Normal, Partitioned, HighLatency, PacketLoss, SplitBrain | `Partitioned, HighLatency, PacketLoss, SplitBrain` |
| 47 | `fed_03` | `federation` | **Transparent P2P Dispatch Decision** | Routes as RemoteTcp via P2pSessionPool with Noise XK encryption | `RemoteTcp, P2pSessionPool, Noise XK` |
| 48 | `safe_01` | `safety` | **LAN RCE Invariant Prevention** | host = '0.0.0.0' combined with dev_mode = true | `0.0.0.0, dev_mode = true, LAN RCE` |
| 49 | `safe_02` | `safety` | **Protected Author Impersonation Guard** | jose, admin, system (automatically mapped to agent-as-<author> if role != human) | `jose, admin, system, impersonation` |
| 50 | `safe_03` | `safety` | **Untracked Secret Token Security** | Only in .tylluan-token (gitignored), never in tracked repository files | `.tylluan-token, gitignored, tracked` |
