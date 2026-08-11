# Changelog

All notable changes to Tylluan are documented here.

---

## [v0.16.0] — 2026-08-11 — MCP 2026-07-28 adoption (M39) · Continuity/Trust/Action layer (M40) · CoherenceGate dataset circuit

85 commits since v0.15.0 (2026-07-30). José's explicit gate for this release, set 2026-08-09: "v0.16.0 NO cierra hasta que M39 (P0-P2) y M40 estén ambos completos" — no shipping partial milestones. Both close in this release, each verified against the live running kernel with real curl/MCP calls, not just CI green.

**M39 — MCP spec 2026-07-28 adoption**
- P0: honest `protocolVersion` negotiation against what Tylluan actually implements — never echo back a version the server doesn't speak.
- P1: `tasks/get`/`tasks/update`/`tasks/cancel` over `JobQueue` with a closed 5-state enum and terminal-state guards (an external audit found the first cut accepted arbitrary strings); real MCP Apps — `ui://tylluan/knowledge-graph-canvas` resource with a self-contained SVG+JS canvas (no external fetch, verified), `_meta.ui` manifest on `tylluan_graph` only, `structuredContent` on its `tools/call` results — replacing what had been a bare `"apps": {}` capability flag with no manifest behind it.
- P2: the stateless core wired end-to-end in `mcp_handler` — a request claiming the 2026-07-28 protocol rejects `sessionId` outright, derives identity only from explicit `agent_id`, and gets 404 on `initialize`/`initialized` (no handshake exists in the new spec). Verified live with curl against the running kernel: legacy negotiation unaffected, stateless requests correctly accepted and rejected.

**M40 — Tylluan as an agent's continuity, trust, and action layer**
Eight phases, all closing real gaps found live in production, not speculative feature work:
- Self-documenting guild contracts (permissions, cost, side effects, rollback) surfaced through `list_available_guilds`, closing a real "guild wanted `path`, schema never said so" incident.
- Unified `agent_bootstrap` — identity, last session, pending approvals, repo-map summary, executable capabilities in one call, replacing what used to be four separate round-trips.
- Full `tylluan_do` action cycle — plan → risk review → approval → execution → verification → memory, with `undo_last_action` as a real, tested subtool.
- Explicit evidence and provenance on memory: `confidence`/`status` (v19) and now `source`/`author`/`evidence_url` (v20) surfaced directly in `tylluan_recall` output.
- Session continuity unified across MCP/HTTP/CLI resume paths onto one payload (`build_resume_context`), fixing a real asymmetry where `agent_bootstrap` silently omitted `last_task`.
- Trust Console dashboard panel — live kernel commit vs HEAD drift, MCP extension manifest, closing the exact gap that let a stale kernel run undetected for a full session earlier this cycle.
- A systematic concurrency test suite (8 parallel agents against the real HTTP router) plus the friction_log DB-isolation bug it surfaced along the way.
- Near-invisible setup: a startup banner with a copy-pasteable connect command, `quickstart_intents` in `agent_bootstrap`, and an opt-in (never silent) MCP client auto-configurator with dry-run-by-default and automatic backup.

**Three real bugs found live and fixed end-to-end (root cause → fix → regression test → CI → live kernel → real client), 2026-08-10/11**
- `explore_actionable_tools` silently returned zero results for any multi-word domain (`"explore memory and communication"`) — the old matcher required the whole phrase as one substring against single-word categories.
- `doctor_diagnose`/`doctor_repair` had zero deterministic routing and were being anchor-matched by the semantic router to an unrelated guild — found live by Qwen Desktop's own diagnostic report, verified before accepting.
- The headline fix of the cycle: `sse_handler` discarded every real client header on `POST /sse`, forcing the wrong MCP dialect regardless of what the client actually asked for — the confirmed root cause of Qwen Desktop's long-standing hang in SSE mode. Fixed, verified with curl before/after, and confirmed resolved by the affected client itself running live afterward.

**CoherenceGate → dataset circuit — phases 1 and 2**
Grounded in real research (Mixture-of-Agents, MIT multiagent debate, NVIDIA's small-language-models-for-agentic-AI thesis, PLaD/PAD preference distillation), reasoned through with José end to end, corrected against verbatim source text at least once along the way rather than trusting a paraphrase.
- Phase 1: `llm_decision_examples` — a real structured A/B pair (`gate_label` from what the deterministic gate actually penalized vs `llm_decision` from the Layer 4 LLM judge), deterministic 80/20 train/heldout split by node_id with no leak, JSONL export endpoint + CLI. Corrects an overstatement made earlier in the same research conversation: `friction_log` was free-text event logging, not a preference-pair dataset, until this shipped.
- Phase 2: real post-hoc ground truth via ADR-011's existing `recall_feedback` Signal Loop — was the memory actually referenced again afterward, independent of what either the gate or the LLM judge said. Nothing is trained yet; phase 3 (offline fine-tuning once enough labeled volume accumulates) is deliberately not started.

Test counts: 644 kernel lib + 65 `tylluan-link` + 12 `tylluan-fsrs` = **721**, all green. Full list of individual commits: `git log v0.15.0..v0.16.0`.

## [v0.15.0] — 2026-07-30 — Connection audit · mandatory mesh encryption · CoherenceGate Layer 4 live

155 commits since v0.14.0 (2026-07-27). The theme of this release is different from most: instead of adding new surface area, most of this cycle went into auditing what earlier milestones claimed was "wired" or "in production" and verifying it against a live, running kernel — real HTTP calls, real SQL queries, real inference, not just reading code and assuming it worked. Several real bugs only surfaced this way.

**Full connection audit — guild IPC, memory writes, dashboard data**
- 5 guilds (`coloquio`, `coloquio_digest`, `coordinator`, `scheduler`, `memory_bridge`) had their kernel IPC defaulting to port `3030` — ForjaMCPo3's port, not Tylluan's `4000` — with no live traffic ever confirming it worked. `memory_bridge.py` additionally read the wrong environment variable for its auth token and turned out to be dead code (never registered as a reachable guild).
- `silva_utils.py`'s `add_node()`/`add_edge_direct()` wrote directly to `silva.db` via SQLite from a Python guild process, bypassing the kernel entirely — nodes created this way never got a real embedding and were invisible to semantic search. Routed through the kernel's HTTP IPC instead, which now also generates the embedding it was missing. A near-duplicate copy of the same file under `guilds/scholars/plugins/` had the identical bug; confirmed unused by any real caller and replaced with a re-export of the canonical module.
- Dashboard: 3 panels showing fabricated or stuck-loading data (a hardcoded AutoResearch summary, a "Cargando..." spinner that never resolved on fetch failure, a captured-but-never-rendered error state) fixed to show honest empty states instead. 4 components made raw unauthenticated `fetch()` calls that bypassed the app's auth layer entirely — harmless under `dev_mode`, but would have silently 401'd the moment auth was ever required in production.

**Mandatory Noise NK encryption for mesh gossip**
- The production gossip loop (peer discovery / routing-table maintenance) sent its `GossipEntry` payloads over plain HTTP with zero encryption, despite the federation memory-sync path already having real Noise NK + ChaCha20 crypto wired and tested. Root cause: gossip peers only ever carried a `node_id` — a one-way SHA-256 hash of the Ed25519 public key — never the actual pubkey Noise NK needs.
- `GossipEntry` and the DHT's `KBucketEntry` now carry an optional Ed25519 pubkey field, propagated naturally as peers gossip with each other. Once a peer's pubkey is known, all further traffic with it uses real Noise NK (forward secrecy); a configurable shared secret covers first contact before a pubkey has propagated; plaintext remains the last-resort fallback for peers that don't have this fix yet.
- Documented directly in the code: this construction gives confidentiality but not sender authentication — anyone who knows a peer's public key can encrypt a message claiming to be any node ID. Not a regression (the previous plaintext gossip had zero protection against this either), but not to be assumed otherwise.

**CoherenceGate Layer 4 — hybrid filter live in production**
- The deterministic-heuristics-plus-LLM hybrid classifier (trigger zones on cosine similarity, provenance, score proximity, keyword overlap) is now wired into both real recall call sites, running in observation mode — it logs its verdict via the friction log without affecting scores yet, so its decisions can be evaluated against real traffic before anything depends on them.
- A 3-model SLM debate (propose → critique → synthesize) was tried as an alternative and NO-GO'd earlier this cycle: sub-2B models converge to a constant answer regardless of prompt design, confirming the deterministic-heuristics approach was the right call.

**Guild registry completeness**
- Activated 13 additional "v1-port" guilds via a new `[guilds.v2]` config section (`audit`, `code`, `memory`, `search`, `sequential_thinking`, `biome_warden`, `ast_surgeon`, `audio_tools`, `ffmpeg_tools`, `screenshot_tools`, `clipboard_tools`, `local_llm_proxy`, `cron_scheduler`), plus `n8n_bridge`.
- New structural test (`test_lazy_or_always_on_guilds_are_registered`) fails CI if a guild is ever present in the catalog but unreachable at runtime through any registration path — this exact bug (catalog entry, no runtime registration, `tylluan_do` answering "Unknown guild") had shipped silently twice before this test existed.
- `biome_warden` is registered but not functional yet — the `biome` CLI binary isn't installed on the reference machine; noted as a known gap, not hidden.

**Vision pipeline — real embedding, real GPU stability fix**
- Root-caused an intermittent MCP transport disconnect in the vision guild to Windows GPU driver TDR (Timeout Detection and Recovery): compiling 3 ONNX sessions on DirectML while the kernel simultaneously used DirectML for BGE-M3 triggered a driver reset that killed the guild subprocess. Forced CPU execution for vision inference — slower per call, but stable under real contention; documented the DirectML-warmup path as a future optimization rather than rushing it.
- `pyproject.toml` was missing `transformers` (needed by `AutoTokenizer`) despite `numpy`/`onnxruntime`/`huggingface_hub`/`tokenizers` already being declared from an earlier fix — the vision guild had been silently failing to load in the real kernel `.venv`.
- End-to-end verified with real evidence, not code review: a live `vision_analyze` call on a real image now produces a coherent description and a real `node_id`, confirmed via direct SQL against `node_embeddings` to carry an actual 1024-dim BGE-M3 vector.

**Also this cycle**
- `GET /api/v1/config/device/status` + a dashboard badge reporting the real active ONNX execution provider (no more assuming CPU without checking).
- `agent_roles` now exposed in `GET /api/v1/guilds` (previously tracked internally but never serialized).
- Friction logging (Sessions/Workflows/Events) shipped with a live dashboard panel; its composite scoring weights are explicitly documented as provisional pending real production data to calibrate against.
- A STUN hostname-resolution bug fixed in `tylluan-link` — NAT external-address discovery had never worked with the default (hostname-based) STUN servers, always silently falling back to LAN-only.

Test counts: 570 kernel lib + 63 `tylluan-link` + 12 `tylluan-fsrs` = **645**, all green, serial and parallel. Full list of individual commits: `git log v0.14.0..v0.15.0`.

## [v0.14.0] — 2026-07-27 — A2A protocol · Signal Loop + Coherence Gate · Dashboard Sovereign Substrate GUI

269 commits since v0.13.0 (2026-07-09), including a full week of production dogfooding
("vivir Tylluan") where the fleet used the kernel as its own working memory/coordination
substrate instead of only building it — several real bugs below were found exactly that way,
not by test coverage.

**A2A (Agent2Agent, Linux Foundation) interoperability — M38**
- Real Agent Card + JSON-RPC 2.0 server (`message/send`, `tasks/get`, `tasks/cancel`) on a single
  endpoint — deliberately not REST, to stay compatible with official A2A SDKs. Orthogonal to
  Tylluan's own P2P mesh (peer sync between trusted Tylluan instances); A2A is for delegating
  to/from any external agent framework (LangGraph, CrewAI, etc.).
- HITL grant coverage for `input-required` task states + mitigation for `client_agent_id`
  spoofing. Dashboard panel: Agent Card viewer, task inspector, approval flow.

**ADR-011 — Signal Loop + Coherence Gate + LightReranker**
- `recall_feedback` table (SilvaDB schema v18) logs which memories `tylluan_recall` returns per
  agent; `FeedbackSignalPhase` (NightConsolidation) resolves implicit usefulness via Jaccard
  word-overlap against subsequent tool calls.
- `CoherenceGate`: 3-layer recall-path defense against memory poisoning (ShadowMerge, eTAMP,
  Sleeper Memory Poisoning — 2025-2026 literature) — known injection patterns eliminated
  silently, untrusted federation provenance and query/content semantic drift penalized. Wired
  into both the live-query and cache-hit recall paths (the cache initially stored pre-gate
  candidates — fixed before merge).
- `LightReranker` (FFN 4→16→1, <10KB ONNX) scaffolded and unit-tested; deliberately not cut over
  into `search_hybrid` yet — needs ≥5,000 resolved `recall_feedback` rows before training, which
  is real production usage time, not more code.
- Dashboard: live Coherence Gate + Signal Loop telemetry panel (cumulative gate counters,
  progress bar toward the 5,000-row training threshold).

**ADR-010 — Embedded SLM research + sep-CMA-ES/TRINITY spike (closed NO-GO)**
- Real ONNX Runtime benchmarks (no simulated numbers) for T5-Small, DistilBERT, SmolLM2-135M,
  BGE-M3 on live hardware.
- sep-CMA-ES spike to replace `coordinator.py`'s fixed Thinker/Worker/Verifier role assignment
  with a trained coordinator: closed as an honest null result (33.3% win rate vs. HTTP-real
  fitness, below the 60% threshold) — the pipeline works end-to-end, but the MLP trained on
  simulated fitness learned "parallelize everything," which loses against the fixed pipeline on
  real wall-clock for small tasks. T5-Small vs. SmolLM2 (the ADR's original question) remains open.

**M25/M29-M32 — Canvas, sandbox, DX, CLI harness**
- M25: bidirectional Canvas↔dashboard event bridge + sandboxed local resource routing.
- M29: 1-click MCP config, real (non-simulated) P2P mesh map, guild capability badges,
  `tylluan new guild` scaffold, dry-run mode.
- M30: graduated sandbox profiles (Strict/Balanced/Permissive), hierarchical override
  (session > guild > global), escalated capability grant engine, CLI `tylluan sandbox`.
- M31 (P0-P7): deterministic pre/post hooks around sovereign tools, granular per-`agent_id`
  ACLs, `tylluan_do(plan=true)` pre-flight dry-run, cross-client session resume, repo map widget,
  project-scoped `@skill:` context, background `@job:` execution, `tylluan doctor --fix`.
- M32: real bidirectional MCP client — Tylluan can call out to external MCP servers, not just
  serve as one.

**M34-M37 — 2026 SOTA gap-closing (agent memory research backlog)**
- M34: read-time trust gate for federation-sourced context (OWASP ASI06) + active rewriting in
  `DreamCycle` (sleep-time compute).
- M35: bi-temporal `valid_from` + supersession for contradiction edges in the knowledge graph.
- M36: explicit self-correction via `@correct:` intent — an agent can edit its own past memory,
  not only accumulate.
- M37: OpenTelemetry GenAI-semantic spans + hierarchical `owner_scope` multi-tenant scopes,
  with a real dashboard panel (not simulated data).

**M19-P5 / ADR-009 — Declarative agent contracts**
- `.tylluan/agents.toml` — per-`agent_id` role assignment, resolved in the bearer-auth middleware
  with explicit precedence (explicit token mapping > contract role > default role).

**Persistent agent identity, Ouroboros Loop, and other fixes from 2026-07-20 → 07-25 below.**

**Real bugs found living the system, not by test coverage (2026-07-26 → 07-27)**
- `tylluan_recall` timed out on any shell command reading stdin (`git status`, `git --version`) —
  `run_command()` spawned children without `stdin=DEVNULL`, so they inherited the guild
  process's stdin and blocked waiting for an EOF that never came.
- On Windows, a timed-out command left orphaned grandchild processes running — `process.kill()`
  only signals the immediate child (`powershell.exe`), not `git.exe`/`cargo.exe` it spawned.
  Fixed with `taskkill /T /F` to kill the whole process tree.
- Coloquio's channel-list intent matcher used `lower.contains("lista")`, a substring match
  against the *entire* message body — any post mentioning "listando"/"artista" anywhere in its
  text got silently rerouted to `list_channels` and discarded instead of posted.
- `.tylluan/agents.toml` (M19-P5) silently loaded empty when the kernel's working directory was
  nested (e.g. `crates/tylluan-kernel`, the real layout when started via `tylluan-mcp.bat`) —
  `AgentsContract::load()` used `current_dir()` instead of the existing `find_workspace_root()`
  helper (which already correctly resolves the dashboard's static asset path the same way).
- `guilds/core/coordinator.py`'s `_split_intent` cap (`MAX_TASKS`) was raised 3→5 for real
  parallel execution but a test kept asserting the old limit — passed trivially without
  exercising the cap. Two more pre-existing, unrelated broken Python tests (a stale import path
  after a guild reorg; a flaky unseeded-RNG FFN training test) also repaired.
- `scripts/check_test_count.sh --fix` — the README/STATUS.md test-count sync check failed 4
  times in one afternoon from fast parallel commits; the flag now corrects the file in one
  command instead of a manual edit each time.

**Dashboard — Sovereign Substrate GUI**
- New visual identity replacing the generic default template look: deep slate (`#0B0F17`),
  neon cyan (`#00F5D4`), defensive crimson (`#FF2E93`), monospace tactical typography — plus the
  official animated Tylluan owl logo.
- New/rebuilt panels: Coherence Gate + Signal Loop telemetry, HITL Plan Mode cockpit
  (pre-flight preview + destructive-action risk badges), interactive 42-guild tester, Federation
  + Mesh node view with live Noise XK badges, Overview header — all reading real kernel data,
  zero mocked metrics.

### 2026-07-20 → 2026-07-25 — Persistent agent identity · Ouroboros Loop · docs reorg

**Persistent agent identity (`whoami` / `register_identity`)**
- Wired a fully-built but never-connected `IdentityManager` (biographical identity: name, role, purpose, philosophy — protected SilvaDB node, survives restarts) into two new `tylluan_do`-routed operations. No 6th sovereign tool added — `CONTRACT-01`'s exactly-5 contract is unchanged; `whoami`/`register_identity` reach the kernel via deterministic intent matching (`"quien soy"`, `"registra mi identidad"`, etc.), so every MCP client can use identity, not only ones told the literal tool name out of band.
- Auto-bootstrap: first authenticated call from a new `agent_id` creates a minimal identity so `whoami` never returns empty; self-documenting hints tell the caller exactly how to fill in a real biography instead of silently persisting a placeholder.
- Closed 4 gaps found immediately after wiring: auto-bootstrap ordering (after the impersonation/ACL guard, not before), context injection at the MCP `initialize` handshake (an agent gets its biography back the moment it connects, not only on request), persona/identity dedup (`whoami` merges both stores in one read), and confirmed no new impersonation surface (the existing bearer-token binding already covers the new tools).
- Temporal grounding: `whoami` and `initialize` now report UTC + a curated world clock (Madrid/London/New York/Tokyo/Shanghai/Sydney); `whoami` accepts an optional IANA `timezone` for any other zone. Adds `chrono-tz` (flagged Zona Roja per the engineering constitution, explicit sign-off given before adding it).
- Session continuity: `JournalDb.recover()` (crash-safe "what was I doing" checkpoint) existed only behind an unused REST endpoint — now surfaced in both `whoami` and `initialize`.
- World grounding, scoped honestly: deliberately did NOT auto-fetch news at connect time (would put a live network call in every handshake's critical path). `whoami` instead tells the agent it can request `guild='websearch'` on demand.

**Ouroboros Loop — per-agent outcome critique + autonomous failure harvest**
- `AgentMemoryManager::record_experience`/`get_relevant_critiques`: an agent can record its own reflection on how an action went (Reflexion, Shinn et al. NeurIPS 2023 — self-critique episodic memory, no LLM in Tylluan's critical path), scoped strictly per-agent (`owner_scope=agent:{id}`, the real indexed SilvaDB column). Failures are weighted higher than successes — the actionable lesson is what not to repeat. Reachable via `tylluan_do(intent="registra experiencia", action=..., outcome=..., verdict=..., lesson=...)`.
- Wired into `tylluan_think`: before reasoning, an agent now sees a "Tu experiencia previa relevante" block pulled from its own past outcomes.
- Autonomous half: hooked a new CERO-LLM harvest phase into the existing `NightConsolidation` pulse (every 30 min, no new timer) that scans `guild_audit_log` — ground truth already recorded per call — and promotes only repeated failure *patterns* (same agent+tool+intent failing ≥3× within 24h) into experience nodes. A one-off failure is treated as a transient blip and ignored, by design, to avoid polluting memory with noise.
- Separately confirmed Tylluan already had a mature routing-level self-learning loop (`lesson:*` nodes — success/failure tracked, decayed at 30 days, deprecated on >50% rejection rate) before any of the above — this work extends it to per-agent outcome critique, it does not replace or duplicate it.

**Security & correctness fixes**
- `bash_execute` allowlist bypass via shell chaining — the allowlist validated only the first `shlex` token of a command, but the raw string was then passed to a real shell (`bash -c` / `powershell -Command`), so `"echo x ; rm -rf /"` passed validation and the shell still executed the second statement. Fixed by rejecting shell metacharacters (`;`, `&&`, `||`, `|`, backticks, `$()`, newlines) outside quoted segments before the allowlist check runs.
- `/api/v1/memory/write` silently overwrote a single hardcoded node (`node_id="manual"`) on every call — every write via this endpoint (used by `coloquio_digest.py`) had been destroying the previous digest's content, undetected, since the feature was written. Fixed with a unique per-call node id; also now accepts `agent_id`/`owner_scope`/`node_type` (written to the real indexed `owner_scope` column, not buried in the metadata JSON blob) instead of discarding all caller-supplied context.
- Removed a vestigial `requires_hitl` field/dead-code path in `ExecutionGuard`/`logic.rs` — born unused in the initial commit, its intended behavior (soft-approval for high-risk actions on untrusted channels) was superseded by the simpler hard-deny path that actually shipped. The real HITL mechanism (`security/grants.rs`, exercised by the A2A HITL test suite) is untouched and remains the single source of truth for approval flows.
- Root-caused and fixed a CI failure where every plain `cargo check`/`test` run unconditionally shelled out to `npm install` for the dashboard, even in jobs that never enable the `bundled-dashboard` feature — `build.rs` now skips that path entirely unless the feature is active, matching the dashboard's own dedicated `pnpm` CI job which is unaffected.
- Corrected stale `AGPL-3.0`/`AGPL` license references left over from before the MIT relicense — found in `AGENTS.md`, `CLAUDE.md`, the AUR package (`PKGBUILD`/`.SRCINFO`), the Scoop manifest, `pyproject.toml`, and one roadmap entry.

**Documentation**
- Reorganized `docs/` from a flat, partially-miscategorized tree (a `docs/internal/` with a stale project map, `docs/research/` mixing internal investigation notes into the public repo, ADRs split across two locations) into `getting-started/ · concepts/ · guides/ · reference/adr/ · reference/integrations/ · roadmap/`, with a navigation index. Internal research notes relocated outside the public repo entirely, not just renamed.
- `SPEC.md` corrected: a comparison-table claim referencing a "TrustMem" consolidation feature that does not exist anywhere in the codebase, replaced with `DreamCycle` (the real dedup/decay mechanism it was presumably meant to describe).
- Test-count guard (`scripts/check_test_count.sh`, enforced in CI) kept honest through 3 rounds of new-test additions: 492 → 495 → 499, each verified against a live run rather than by arithmetic.

### Fixed

- **Security: agent impersonation via `post_to_channel`** — the MCP tool accepted a caller-supplied `role` parameter and passed it straight through, so any agent could post as `role="human", author_id="jose"` and produce a message indistinguishable from a genuine human post, with zero server-side verification (the kernel's own protected-author guard only blocks impersonation when `role != "human"`). Fixed by hardcoding `role="agent"` in the tool regardless of caller input. The raw HTTP endpoint is still reachable without auth under `dev_mode=true` — closing that fully requires a real auth decision, deliberately not rushed.
- **Kernel: audit-log write blocked the async runtime** — `log_audit_entry` (synchronous rusqlite write) was scheduled via `tokio::spawn` instead of `spawn_blocking`, blocking a worker thread during concurrent coordinator dispatches. This was the real root cause behind coordinator latency previously misdiagnosed as an `agent_id`/audit problem.
- **Kernel: zero-downtime port rotation had no safety bound** — a stale `active_port.json` could make the kernel send a real shutdown signal to a port outside its own range. Added an explicit guard.
- **Kernel: cosine similarity panicked on mismatched embedding dimensions** — SilvaDB can hold embeddings from different model dimensions side by side; the nightly consolidation pass compared them unguarded and crashed. Now returns 0.0 instead of panicking.
- **graph_rag: unbounded nested node-id growth** — the clustering pass included its own `type='summary'` output as eligible input, so a summary node could win as its own component's hub and get wrapped in another `graphrag_summary:cluster:` prefix every consolidation cycle (observed at 7+ nested levels in production). Fixed by excluding `summary` from the eligible node types.
- **Dashboard: production bundle hardcoded to one port** — `VITE_NEXUS_URL` was baked into the production build at `:3033`, breaking the dashboard on any other port/instance. Now falls back to `window.location.origin` in production; the env var override remains available in dev.

### Added

- **M18-P3b closed honestly** — the coordinator's real bottleneck (audit-log blocking) fixed; latency claim corrected twice before landing on an honest metric (mean of per-query deltas, not mean of absolute latencies) — still short of the original 30% target on that metric, documented as such rather than inflated.
- **Deterministic federation freshness resolution** (`consensus.rs`) — 5-rule cascade (identical hash / protected / peer priority / timestamp / lexicographic tiebreak), 9 tests, wired into all 4 federation sync paths.
- **ConsensusEngine test coverage** — a 237-line conflict-resolution engine that runs hourly in production (`NightConsolidation`) had zero tests before today. 7 tests added covering clear-winner, synthesis, ambiguous, protected-node, and human-override paths.
- **Event Bridge (P0)** — `dream_cycle_complete` and `federation_sync` (push/receive/pull/both) now broadcast over the existing SSE `/api/v1/events` stream, alongside the pre-existing `coloquio:*`/`tool_call`/`memory_added` events.
- **Federation verified for real between two live instances** — native (`:4000`) and a Docker-secondary instance (`:4040`), confirmed via a real completed sync (`last_sync` timestamp, nodes present with correct `federation_source` tagging on both sides), not just unit tests against a single process. Two open findings from this test, not yet fixed: auto-sync (`sync_interval_ms` exists in config but nothing consumes it), and an auth-middleware/field-name mismatch between `sync/receive` and the general bearer-auth layer.
- **M26 Canvas Sprint — real-time reactive whiteboard** — tldraw-based collaborative canvas wired to the Event Bridge for live updates, replacing the earlier graph-viewer direction (explicitly rejected as "Knowledge Graph disfrazado" — the canvas needed to be a real collaborative surface, not another way to look at the same graph).
- **SPEC.md: Sovereignty properties + comparative taxonomy** — 7 sovereign-AI properties mapped to Tylluan's actual primitives, plus a comparison table against Mem0/Letta.
- **docs-site synced to real implementation state** — roadmap statuses (Freshness, Dashboard, DreamCycle, Mem0 benchmark) corrected from "proposed"/"future" to "active" with checkable justification (test counts, wiring locations, closed milestones) after independent verification; tool name references corrected (`tylluan_remember`, not `tylluan_store`); test count corrected to 383 (310 kernel + 61 tylluan-link + 12 tylluan-fsrs).

### Process note

Several of the items above (M18-P3b's benchmark, an agent's feature comparison against an unrelated internal reference deployment, and federation "verified" being reported twice against the wrong instance before being confirmed for real) were corrected after independent verification against the running system caught inflated or misattributed claims. Documented here rather than silently fixed, since the corrections are as much a part of today's real progress as the features themselves.

## [v0.13.0] — 2026-07-09 — Coordinator Cascade + Dashboard Fork

Tagged and published on GitHub with binary releases (aarch64-apple-darwin,
aarch64/x86_64-unknown-linux-gnu, x86_64-pc-windows-msvc) — this entry was
missing from CHANGELOG.md until now, added retroactively from the real
release notes.

### Added

- **M18-P3 TRINITY Coordinator** with parallel/sequential dispatch
- **M20 Complexity Cascade** (proactive routing >= 0.6, reactive fallback >= 0.4)
- **M21 Query Embedding Cache** (TTL, LRU 256 entries)
- **Dashboard fork** with teal/emerald branding
- **Scheduler guild** with SQLite persistence
- 354 tests (291 kernel + 61 link + 2 evals), all green

No breaking changes — drop-in upgrade from v0.12.0.

## [v0.12.0] — 2026-07-05 — M15 Rufus Release · Zero-friction install · Docker oficial

**Norte estrella:** `tylluan-cli start` funciona en frío en una máquina que nunca ha visto Rust, en < 5 minutos, sin leer ningún documento. Rufus test: pasado.

### Added

- **M15-P0 — Install scripts desde cero** (`commit 2df8f73`) — `install.sh` (Linux/macOS) e `install.ps1` (Windows): detección OS+arch, descarga binario desde GitHub Releases, instala en `~/.tylluan/bin/`, arranca con `--profile portable` (BM25-only), health check wait 30s con spinner, imprime config MCP para 3 clientes directamente en terminal. Idempotente. Sin Rust, sin cargo, sin Python.

- **M15-P1 — First-run UX** (`commit 2df8f73`) — `GET /api/v1/setup-hint`: JSON con configs MCP para Claude Desktop, Claude Code y Cursor + comandos de verify. `embedding_model = "none"` como default en `--profile portable`. BM25 funciona sin descargar modelos.

- **M15-P2 — Docker imagen oficial** (`commits a2642da, 13086eb`) — `ghcr.io/forja-orca/tylluan:latest`: multi-stage build (`rust:1.88-bookworm` → `debian:bookworm-slim`), ONNX Runtime 1.22.0 instalado según arquitectura (x64/aarch64), `HEALTHCHECK` en `/health`, `VOLUME /data` del usuario. `dev_mode = false` por defecto. Sin telemetría, sin call-home, sin federación en el default. Soberanía intacta.

- **M15-P3 — Verificación OpenClaw** (`commit 5c9b32d`) — OpenClaw confirmado: 368,249 stars (fuente primaria), soporte MCP SSE nativo (`openclaw mcp add <url>`). Hermes Agent (NousResearch) compatible vía `~/.hermes/config.yaml`. **M17 Rama A decidida:** docs de integración en v0.13.0, sin cambios en kernel.

- **CI — Install smoke** (`.github/workflows/install-smoke.yml`) — smoke test en Ubuntu 22.04 y Windows Server 2022 limpios, activado en release publish.

- **CI — Docker smoke** (`.github/workflows/docker-smoke.yml`) — build + run + health check + setup-hint test, activado en push a main, PR, y release publish.

- **ADR-006** (`docs/architecture/ADR006_rufus_release.md`) — spec completa del Rufus Release: contexto, alternativas rechazadas, flujo exacto de los 4 scripts, DoD medible.

- **Roadmap** (`docs/roadmap/ROADMAP_O3.md`) — primer roadmap formal: M15-M19 con criterios de cierre medibles, reglas de disciplina permanentes, backlog de investigación.

### Fixed

- **Dockerfile — ONNX Runtime 1.22.0** (`commit 13086eb`) — La imagen anterior crasheaba al arrancar por `libonnxruntime.so` no encontrado. Fix: instalar `libgomp1` + descargar ONNX Runtime v1.22.0 oficial (la `ort` v2.0.0-rc.10 requiere `1.22.x`, no `1.16.x`). Detecta arquitectura automáticamente (amd64/arm64).

---

## [v0.11.0] — 2026-07-02 — M14-D + M14-E + M14-F complete · CI ARM64 green

**Norte estrella:** Los peers descubren capacidades entre sí, despachan guild tools remotamente sobre Noise XK, y el harness de tests valida routing multi-peer y topologías de red.

### Added

- **CI ARM64 portability** (`commit 4d010a8`) — `portability-check` job en `ci.yml`: cross-compila `tylluan-kernel` y `tylluan-cli` para `aarch64-unknown-linux-gnu` en cada push a main. Verifica que el código compila para RPi4 sin regresiones.

- **fix(ci)** (`commit c51357a`) — Clippy let-chain collapsible ifs en `p2p.rs` y `dispatch.rs`; campos `capability_registry`, `dispatch_router`, `dispatch_queue` añadidos a 5 test fixtures (`federation_audit`, `mesh_audit`, `blackboard_e2e`, `sovereign_e2e`, `pipeline_tests`).

- **M14-F Phase 3 — kernel wiring P2P dispatch** (`commit 538afcc`)
  - `P2pHandlerFn` type alias → `BoxFuture<'static, GuildDispatchResponse>` — permite llamadas async (`registry.call_tool()`) desde el listener.
  - `[p2p]` section en `config.rs` (`enabled: bool`, `listen_port: u16`).
  - `p2p_pool: Arc<tokio::sync::Mutex<P2pSessionPool>>` en `HttpState`.
  - Listener P2P arranca condicionalmente al iniciar el kernel (`config.p2p.enabled`).
  - `api_mesh.rs`: arm `RemoteTcp` nativo — llama `execute_remote_tcp` directamente (no HTTP fallback).
  - `guild_peers_handler` expone `supports_p2p` y `tcp_port` en la respuesta de peers.

- **M14-F Phase 2 — start_p2p_listener_noise + RemoteTcp routing + p2p_dst tests** (`commits 41b6194, f06fa0e`)
  - `p2p.rs` bug fix: `pool.remove()` antes de usar la sesión; reinserta solo en éxito — sesión rota se droppea, nunca vuelve al pool.
  - `start_p2p_listener_noise(addr, identity, handler) -> (JoinHandle, SocketAddr)` — Noise XK responder: `TcpListener::bind` → `noise_accept` → `async_decrypt_read` → `handler(req)` → `async_encrypt_write`. Puerto 0 → OS asigna dirección real.
  - `DispatchDecision::RemoteTcp { node_id, addr, tcp_port: u16 }` — nueva variante; `route()` la devuelve cuando `peer.supports_p2p=true && peer.tcp_port.is_some() && best_score > local_score * 1.2` (score evaluado primero, P2P elegido post-loop).
  - `api_mesh.rs`: arm `RemoteTcp` exhaustivo — HTTP fallback hasta Phase 3 (kernel wiring).
  - `catalog.rs`: `vision_moondream` añadido a `KNOWN_GUILDS` (guild de Padawan faltaba en la lista del anti-regresión test).
  - `tests/p2p_dst.rs` — 3 DST tests:
    - `test_p2p_noise_roundtrip` — TCP loopback real (puerto 0), listener Noise XK, roundtrip completo.
    - `test_p2p_error_response` — handler devuelve `success=false`, initiator recibe error correctamente.
    - `test_route_prefers_tcp` — `route()` con `supports_p2p=true + tcp_port=9001` → `RemoteTcp { tcp_port: 9001 }`.
  - **88 link tests** (61 lib + 27 integration), 273 kernel tests, 2 evals = **363 total** · 0 failures.

- **M14-F Phase 1 — P2pSessionPool + execute_remote_tcp** (`commit 022b0e1`)
  - `p2p.rs`: `P2pSessionPool` (HashMap, LRU evict, TTL prune) + `execute_remote_tcp()` (Noise XK initiator, length-prefixed framing, 30/120s timeouts).
  - `HardwareCaps` gains `supports_p2p: bool` + `tcp_port: Option<u16>` (both `#[serde(default)]`, backwards-compatible).
  - 4 unit tests: pool empty, prune noop, error display.

- **Moondream guild** (`commit 6a1906e`) — `guilds/core/vision_moondream.py`: `analyze_image` + `caption_image` via `moondream` pip package (0.5B local vision model).

- **ADR-005 M14-F spec** (`commit 6979795`) — `docs/architecture/M14F_p2p_dispatch_spec.md`: Noise XK session pool, Option A transparent routing, 6-phase plan.

- **M14-E Phase 1 — Mesh Topology Simulation** (`tests/mesh_simulation.rs`)
  - `test_full_mesh_3node_all_pairs` — A↔B, B↔C, A↔C convergencia completa tras 3 rounds de sync.
  - `test_star_topology_hub_propagation` — B como hub; A y C no se ven entre sí; info fluye por B.
  - `test_split_brain_partition_then_heal` — A y C divergen (clock distinto), se curan vía B, LWW resuelve conflicto.
  - 3 tests, todos `#[tokio::test]`, patrón `in_memory_pair` + `tokio::join!` para determinismo.

- **M14-E Phase 2 — DispatchRouter Multi-Peer Tests** (`tests/dispatch_dst.rs`)
  - `test_router_selects_gpu_peer_over_two_cpu_peers` — 3 peers en registry, GPU gana sobre 2 CPU peers.
  - `test_router_capability_filter_excludes_wrong_guild` — solo peers con guild correcta son candidatos.
  - `test_router_falls_back_to_second_peer_when_first_circuit_open` — CB abierto en primario → enruta a secundario.

- **M14-E Phase 3 — DispatchQueue moved to tylluan-link** (`src/dispatch.rs`)
  - `DispatchQueue` extraído de `tylluan-kernel/src/transport/http/mod.rs` → `tylluan-link/src/dispatch.rs`.
  - Kernel importa vía `use tylluan_link::dispatch::DispatchQueue`.
  - 4 tests: FIFO, max-size overflow, TTL expiry, TTL keeps fresh entries.
  - M14-E complete. 81 link tests, 273 kernel tests.

- **M14-D Phase 4 — Fallback Queue + Remote Dispatch + Peers Endpoint**
  - `DispatchQueue` in `mod.rs`: `VecDeque`-backed fallback buffer (max 1000), `enqueue/dequeue`, `peek_timed_out/remove_timed_out` (300s TTL cleanup).
  - `HttpState` gains `dispatch_router: Arc<Mutex<DispatchRouter>>` + `dispatch_queue: Arc<Mutex<DispatchQueue>>`.
  - `GET /api/v1/guilds/peers` — returns all `CapabilityRegistry` peers with `hardware` and `capabilities` fields.
  - `POST /api/v1/guilds/dispatch/remote` — asks `DispatchRouter` for routing decision; executes locally (`Local`) or forwards via HTTP to peer's `/dispatch/execute` (`Remote`); on success calls `record_success`; on failure enqueues body to `DispatchQueue` + calls `record_failure` (circuit breaker).
  - M14-D complete. All 4 phases delivered. CONTRACT-01 preserved.

- **M14-D Phase 3 — GuildDispatchRequest/Response + Noise NK handler**
  - `GuildDispatchRequest { guild, tool, args, request_id, sender_id, timeout_secs }` — Serde serialize/deserialize.
  - `GuildDispatchResponse { request_id, success, result, error, executor_id, duration_ms }`.
  - `send/receive_dispatch_request` and `send/receive_dispatch_response`: serialize → `noise_encrypt_payload` → `transport.send()` (Noise NK over `dyn MeshTransport`).
  - `POST /api/v1/guilds/dispatch/execute` endpoint: receives `GuildDispatchRequest`, calls `state.registry.call_tool()`, returns `GuildDispatchResponse` with executor node ID and wall-clock duration.
  - CONTRACT-01 preserved: all routing remains transparent inside `tylluan_do`.

- **M14-D Phase 2 — DispatchRouter**
  - `crates/tylluan-link/src/dispatch.rs`: `DispatchRouter` + `DispatchDecision` enum.
  - Scoring: `(1 - load_avg) × (1000 / max(1, latency_ms)) × gpu_multiplier`.
  - Circuit breaker: 3 consecutive failures → cooldown 60s (configurable). `record_latency / record_failure / record_success` public API.
  - Default latency 0.0 for peers without history (favors exploration at cluster start).
  - `HttpState` gains `capability_registry: Arc<Mutex<CapabilityRegistry>>` (TTL 300s).
  - Gossip background task (tick 60s): `ingest_from_engine` + `prune_expired` + debug log when peers pruned.
  - Lock ordering: `registry` → `stats` within `route()`; acquired post-.await, dropped pre-.await (no Send trap).
  - 5 unit tests: local fallback (no peers), remote GPU peer, unknown-latency exploration, circuit breaker trip+recovery, success reset.

- **M14-D Phase 1 — Capability Registry**
  - `HardwareCaps { ram_mb: u32, has_gpu: bool, load_avg: f32 }` struct added to `GossipEntry` with `#[serde(default)]` — backwards-compatible with v0.10.0 peers.
  - `CapabilityRegistry` in `crates/tylluan-link/src/capability.rs`: `HashMap<NodeId, (CapabilityRecord, Instant)>` with configurable TTL (default 300s).
  - Methods: `ingest(record)`, `lookup(node_id)`, `prune_expired()`, `ingest_from_engine(&GossipEngine)`.
  - 6 unit tests: new/is_empty, ingest+lookup, stale-clock rejection, prune_expired, ingest_from_engine, default TTL.
  - `prune_expired()` ready to wire into background gossip task in `main.rs` (Phase 2).

- **M14-F Phase 1 — P2pSessionPool + execute_remote_tcp** (`crates/tylluan-link/src/p2p.rs`)
  - `DispatchError { Io, Timeout, Protocol, Serialize }` — Display + From<io::Error> + std::error::Error
  - `PooledSession { noise: NoiseSession, write: OwnedWriteHalf, read: OwnedReadHalf, last_used }` — holds live XK session halves
  - `P2pSessionPool::new(max_per_peer, keepalive_secs)` — HashMap-backed pool, `prune()` removes stale sessions by TTL, LRU eviction when at capacity
  - `execute_remote_tcp(pool, request, peer_addr, peer_pubkey_hex, identity)` — reuses pooled session or TCP connect + Noise XK handshake; `async_encrypt_write` + `async_decrypt_read` with per-request timeout
  - `HardwareCaps` gains `supports_p2p: bool` + `tcp_port: Option<u16>` (both `#[serde(default)]`, backwards-compatible)
  - 4 unit tests added in `p2p.rs`; struct literal updates propagated to capability.rs, dispatch.rs, gossip/state.rs, dispatch_dst.rs
  - Phase 2 pending: `start_p2p_listener` (Noise XK responder), `DispatchRouter` extension for `RemoteTcp`, kernel wiring, `p2p_dst.rs` DST tests

- **Moondream Vision Guild** (`guilds/core/vision_moondream.py`)
  - `analyze_image(image_path, prompt)` — Moondream 0.5B Q&A sobre imagen local → JSON
  - `caption_image(image_path)` — caption corto → JSON
  - Lazy loading, PIL+moondream pip (no torch, no transformers), impresiones a stderr
  - Paralelo a `vision.py` (SmolVLM2 ONNX) — dos guilds de visión disponibles

- **ADR-005 M14-F — P2P Guild Dispatch over Noise XK** (`docs/architecture/M14F_p2p_dispatch_spec.md`)
  - Context: NK stateless dispatch (M14-D) repeats key exchange per request; XK amortizes 3-message handshake over a persistent session.
  - Q1: `execute_remote_tcp(request, peer_addr, identity, peer_pubkey_hex) -> Result<GuildDispatchResponse, TransportError>` — len-prefixed framing (u32 BE), same as noise.rs; 30s connect timeout, 120s per-request timeout.
  - Q2: Session pool — `HashMap<NodeId, NoisedPipe>` with 5min TTL, max 16 peers, keepalive ping every 60s, background prune task.
  - Q3: Option A (transparent) — `dispatch/remote` auto-detects `supports_p2p=true` and routes to TCP; `dispatch/send` reserved for v0.12.0 explicit API.
  - Q4: `tcp_addr` lives in `GossipEntry.addr` (already present); `HardwareCaps` gains `supports_p2p: bool` (default false, backwards-compatible).
  - Implementation: 6-phase plan — pool struct → execute_remote_tcp → HardwareCaps field → kernel wiring → DST tests → integration.

### Tests

**273 kernel lib tests + 88 link tests + 2 evals = 363 total** · 0 failures.

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

- **Extended Retrieval Benchmark (P0)**
  - 44 nodes + 40 edges + 10 queries (5 original + 5 multi-hop). `skip_graph: bool` param in `search_hybrid` for A/B comparison (internal, not exposed in MCP API).
  - Results with deterministic 12-dim embeddings (semantic caveat — real BGE-M3 delta expected higher): Graph ON → Recall@5 20%, Recall@10 30%, MRR 23.15%, p50 5.65ms. Delta vs graph OFF: +2.5%/+5.0% recall, −0.1% MRR (pre-fix), +4ms latency.
  - Output: `benchmarks/benchmark_v0.10.0.json`

- **M14-D Guild Dispatch ADR (P3-spec)**
  - `docs/architecture/M14D_dispatch_spec.md` (ADR-004) — Capability-Aware + Latency-Based Hybrid Routing.
  - 4 components: Capability Registry (DHT+Gossip, TTL 5min), Dispatch Algorithm (load+latency scoring), Remote Execution Protocol (JSON over Noise NK, `GuildDispatchRequest/Response`), Fallback Strategy (queue + circuit breaker).
  - CONTRACT-01 preserved: routing is transparent inside `tylluan_do`.
  - 4-phase implementation plan (~8 sessions).

### Fixed

- **LinearRAG Degree Bias (P2-fix)**
  - `local_query_graph` (`graph.rs:739`): `pr_score * (1 + deg×0.1)` → `pr_score / (1 + deg×0.1)` — hub nodes now penalized instead of boosted.
  - `dual_retrieval.rs` (lines 30, 69): same inversion applied to graph-boosted scores.
  - New test `test_local_query_graph_degree_penalty` verifies low-degree (deg=1) outranks high-degree (deg=5) with slightly lower PR score.
  - Root cause: benchmark revealed MRR was flat despite recall gain — degree boost promoted generic hub nodes to top positions instead of penalizing them.

### Tests

**273 kernel lib tests + 61 link tests + 2 evals = 336 total** · 0 failures.

---

## [v0.9.0] — 2026-07-01 — Graph-Augmented Local RAG

**Norte estrella:** Zero-token local graph indexing and traversal with batch processing.

**Research basis:** LinearRAG / Tri-Graph paper (ICLR 2026) and instant-distance HNSW.

### Added

- **LinearRAG Local Graph Traversal (P3)**
  - `degree_centrality`: SQL-native edge connectivity calculation chunked in groups of 50 to avoid SQLite parameter limit errors.
  - `local_query_graph`: Graph traversal using Personalized PageRank from vector seeds, boosted by degree centrality: `score * (1.0 + degree * 0.1)`. ⚠️ **This formula was identified as a bug in v0.10.0** (boosting hub nodes hurts MRR for specific queries) and corrected to `score / (1.0 + degree * 0.1)` (see v0.10.0 Fixed).
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
