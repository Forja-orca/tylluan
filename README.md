<p align="center">
  <img src="assets/branding/logo.jpg" alt="Tylluan" width="160">
</p>

<h1 align="center">Tylluan</h1>

<p align="center">
  <strong>Persistent memory, a knowledge graph, and real tool execution for AI agents — running entirely on your machine.</strong><br>
  <em>Sees what others miss, remembers what others forget.</em>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="MIT License"></a>
  <img src="https://img.shields.io/badge/version-0.16.0-blue.svg" alt="v0.16.0">
  <img src="https://img.shields.io/badge/rust-1.88+-orange.svg" alt="Rust 1.88+">
  <img src="https://img.shields.io/badge/python-3.12+-blue.svg" alt="Python 3.12+">
  <img src="https://img.shields.io/badge/MCP-native-purple.svg" alt="MCP Native">
  <img src="https://img.shields.io/badge/cloud-none-brightgreen.svg" alt="No Cloud">
  <a href="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml"><img src="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="deny.toml"><img src="https://img.shields.io/badge/license%20audit-deny.toml-blue.svg" alt="License audit"></a>
</p>

---

## Why Tylluan exists

Most AI memory systems ask you to trust someone else's server with your data: an API key, a subscription, a vendor who can change the terms — or cut your access — tomorrow. Tylluan takes the opposite approach. It's a single Rust binary that gives an AI agent long-term memory, a knowledge graph, and the ability to actually run tools, and none of it leaves your machine unless you explicitly tell it to.

Concretely, that means:

- **Your data stays yours.** Memory lives in a local SQLite database with BGE-M3 embeddings. There's no cloud round-trip in the critical path, and no proprietary format underneath — you can open the database with any standard SQLite tool.
- **It works without an internet connection.** We've run it on a Raspberry Pi 4 with 12,000 stored memories, federated with three peers over encrypted Noise XK, on a network with no internet access at all.
- **Nothing here can be taken away from you.** MIT licensed, no vendor lock-in, no feature gated behind a subscription.

Agent memory is a crowded space right now — Mem0, Letta, Zep, Cognee, Graphiti, A-MEM, and others all take real, different approaches, and are worth evaluating on their own terms depending on what you need. What Tylluan specifically bets on is running well on modest, offline, or air-gapped hardware, with a compiled binary instead of a Python service you have to keep alive, and a mesh where peers share knowledge directly without a coordinator node in the middle.

The honest trade-off: several of those other projects have years of community history and production hardening that Tylluan doesn't have yet. [ROADMAP.md](ROADMAP.md) is where we track what's actually shipped versus what's still planned — we'd rather you find out something isn't ready from us than from a broken deploy.

---

## What it actually does

At its core, Tylluan is a local Rust kernel your agent talks to over MCP. It remembers things across restarts, builds a knowledge graph out of what it learns, and can execute real tools — read files, run git commands, search the web, query a database — on your behalf. If you run more than one instance, they can sync knowledge with each other over an encrypted peer-to-peer mesh, with no central server required.

**Design north star:** one binary, a different `tylluan.toml` per environment, the same code everywhere. Memory persists whether or not a network is available; peers sync opportunistically when one shows up, never as a requirement.

| Capability | Details |
|------------|---------|
| **Memory** | BM25 + FTS5 + BGE-M3 vector search, fused with RRF, plus LightRAG-style graph traversal (PageRank + degree penalty) |
| **Agent Identity** | Declarative agent contracts (`.tylluan/agents.toml`) — role assignment per `agent_id`, no manual wiring |
| **Tools** | 49 guilds — bash, git, filesystem, docker, code analysis, vision, web search, and more — auto-discovered at startup |
| **Collaboration** | Multi-agent channels (Coloquio), shared documents, Bounded Work Contracts |
| **Federation** | Peer-to-peer knowledge sync, Noise NK / ChaCha20-Poly1305 encrypted, provenance-tracked, echo-loop safe |
| **Mesh** | Kademlia DHT + Gossip dissemination — encrypted with Noise NK once peers know each other's pubkey; a legacy no-discriminator wire path exists for backward compat with older peers and does carry plaintext, see [docs/concepts/SECURITY_FEDERATION.md](docs/concepts/SECURITY_FEDERATION.md) |
| **A2A Protocol** | Agent Card discovery + JSON-RPC 2.0 server — interoperates with any Agent2Agent-compliant client (LangGraph, CrewAI, etc.), not just other Tylluan instances |
| **MCP Native** | SSE + HTTP Streamable — works with Claude, Cursor, VS Code, LM Studio, any MCP client |
| **GPU Acceleration** | Optional DirectML (Windows, any GPU vendor) or CUDA execution provider for local ONNX inference — CPU stays the zero-config default |

<details>
<summary>Full technical capabilities →</summary>

| Capability | Details |
|------------|---------|
| **Signal Loop (ADR-011)** | `recall_feedback` table tracks which memories actually got used; resolved during `NightConsolidation` via word-overlap against downstream tool calls |
| **Coherence Gate** | Layered defense on every recall against memory poisoning — deterministic pattern/provenance/drift filters always active, plus an LLM-backed hybrid classifier for genuinely ambiguous cases, currently running in observation mode |
| **Plan Mode (M31-P2)** | `tylluan_do(plan=true)` returns the proposed guild/tool/args without executing anything — a dry run you can inspect first |
| **Agent Contracts (M19-P5)** | `.tylluan/agents.toml` — per-agent role assignment committed alongside `AGENTS.md` |
| **HNSW Index** | Approximate nearest-neighbor search for larger datasets (kicks in above ~12k nodes) |
| **Episodic Memory** | Coloquio conversations are automatically stored in the knowledge graph as episodic nodes |
| **Memory Decay** | Salience fades on a 14-day half-life; using a memory reinforces it |
| **Guild Dispatch** | Peers discover each other's tool capabilities and can dispatch guild calls remotely over Noise NK, with load/latency-aware routing and a circuit breaker for degraded peers |
| **Encryption** | AES-256 at rest via SQLCipher (`--features encryption`) — active by default on binaries built with that feature, off otherwise; every real database goes through the same `open_db()` path, see [docs/concepts/SECURITY.md](docs/concepts/SECURITY.md) |
| **Query Cache** | TTL LRU embedding cache, avoids redundant inference on repeated queries |
| **Complexity Cascade** | Heuristic scoring escalates multi-step or ambiguous intents to a coordinator, no LLM required |
| **TRINITY Coordinator** | Thinker/Worker/Verifier pattern for tasks that need real synthesis across steps |

</details>

### Dashboard

Tylluan ships with a React dashboard for watching the kernel work.

- **Production (single binary):** served automatically at [http://127.0.0.1:4000/](http://127.0.0.1:4000/) once the kernel is running.
- **Development:** `cd dashboard && pnpm dev` for the hot-reloading dev server at [http://localhost:5173/](http://localhost:5173/).

<p align="center">
  <img src="assets/screenshots/overview.png" alt="Overview — system health and kernel pulse" width="45%">
  <img src="assets/screenshots/guilds.png" alt="Guilds — registered and running" width="45%">
</p>
<p align="center">
  <img src="assets/screenshots/knowledge_graph.png" alt="Knowledge Graph — SilvaDB visualizer" width="45%">
  <img src="assets/screenshots/coloquio.png" alt="Coloquio — multi-agent communication" width="45%">
</p>

### Five tools, nothing hidden behind them

Every MCP client that connects to Tylluan sees exactly these five tools — no matter how many guilds are running underneath:

```
tylluan_do        Route a task to a guild, described in natural language
tylluan_recall    Search long-term memory (hybrid keyword + vector) or an agent's persona
tylluan_remember  Store knowledge, or update an agent's persona persistently
tylluan_think     Reason over the knowledge graph
tylluan_graph     Direct graph operations — triples, paths, PageRank
```

That's deliberate. Whatever guild ends up doing the work — git, vision, a database query — the client only ever sees this one clean interface.

### How well does retrieval actually work?

We evaluated on **LongMemEval-S** (50 human-authored questions covering episodic memory, multi-hop reasoning, and temporal questions), using real BGE-M3 embeddings on CPU — no simulated numbers:

| Metric | Value | Backend |
|--------|-------|---------|
| Recall@5 | **82%** | BGE-M3 + BM25 + RRF |
| Recall@10 | **90%** | BGE-M3 + BM25 + RRF |
| Recall@1 | 46% | BGE-M3 + BM25 + RRF |
| Latency p50 | 12.9 ms | CPU, no GPU |

For comparison, a synthetic corpus of short descriptions only reaches 50% Recall@5 — real human queries actually do better, which tells us the pipeline degrades gracefully rather than overfitting to easy cases. Full results: [`benchmarks/longmemeval_v0.12.0.json`](benchmarks/longmemeval_v0.12.0.json).

#### If you're on modest hardware

Three profiles trade retrieval quality for footprint — pick based on what you're running on:

| Profile | Model | Download | RAM | R@5 | R@10 | Latency p50 |
|---------|-------|----------|-----|-----|------|--------------|
| `portable` | BM25 only | 0 MB | ~30 MB | 38% | 42% | 0.4 ms |
| `clinic` | BGE-Small (384d) | ~100 MB | ~300 MB | 61% | 68% | 3.2 ms |
| `server` | BGE-M3 (1024d) | ~1.2 GB | ~1.5 GB | 82% | 90% | 12.9 ms |

`portable` is the right call for a Raspberry Pi Zero or a fully offline deployment; `clinic` suits a RAM-constrained laptop; `server` is for a desktop or server where retrieval quality is the priority.

### Agent skills

Once connected, an agent can call any guild through `tylluan_do` just by describing what it wants:

| Skill | Example |
|-------|---------|
| Run code | *"run this Python script and return the output"* |
| Web search | *"search for the latest Rust async patterns"* |
| Vision | *"describe what's in this screenshot"* |
| Git | *"show me the last 10 commits in this repo"* |
| Docker | *"list running containers and their memory usage"* |
| Database | *"query the SQLite database at ./data.db"* |
| PDF | *"extract the key points from this paper"* |
| Deep research | *"research and summarize the state of MCP tooling in 2026"* |

### Does routing need an LLM call to the cloud?

No — routing is 100% local, and no LLM sits in that path.

When you call `tylluan_do("search for Rust async patterns")`, the kernel embeds the intent with BGE-M3 (local ONNX, CPU — or falls back to keyword scoring if you've set `embedding_model = "none"`), scores it against every guild's description, escalates to a coordinator if the intent looks multi-step or ambiguous, and returns the best match with structured arguments. No HTTP call leaves your machine, no API key required — the escalation logic is pure heuristics running in-process on your CPU.

Two things worth not conflating here:

- **Embeddings (BGE-M3) are always in the path.** Every routing decision and every memory search runs through a local neural network — that's not optional, and it's not a generative LLM.
- **Generative LLM inference is optional and never in the routing hot path.** Tylluan can run one internally via `llama.cpp` + GGUF (auto-downloads a precompiled binary, no external service needed) for specific, non-blocking uses — an offline evaluation judge, and a calibrated second opinion on memory candidates already flagged by the cheaper deterministic filters. You can also point it at an Ollama, LM Studio, or `llama.cpp` instance you already have running — it detects a live backend before starting its own.

So "no cloud required" is the real invariant here. "No LLM at all" was never quite accurate for the embedding layer, and it isn't for the optional generative layer either — what stays true is that nothing in Tylluan depends on a cloud service to function.

### CI

[![CI](https://github.com/forja-orca/tylluan/actions/workflows/ci.yml/badge.svg)](https://github.com/forja-orca/tylluan/actions/workflows/ci.yml)

730 tests across Rust kernel (lib), `tylluan-link`, and `tylluan-fsrs` — all green. Every push runs Rust build + test, clippy, `cargo-deny` (bans, licenses, advisories), Python lint + test, a dashboard build, and the security audit suite. Details in [STATUS.md](STATUS.md) and [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

---

## Quick Start

> **Setup takes about 10 minutes**, most of it spent downloading the BGE-M3 model on first boot (~1.2 GB, one-time). Set `embedding_model = "none"` in `tylluan.toml` if you'd rather skip that download entirely.

**Supported platforms:**

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `tylluan-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 (Raspberry Pi 4+) | `tylluan-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `tylluan-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `tylluan-x86_64-pc-windows-msvc.tar.gz` |

### 1 — Install

No Rust, Python, or Node needed on your end.

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex
```

This drops `tylluan-nexus` and `tylluan-cli` into `~/.tylluan/bin/` and adds them to your PATH. **Open a new terminal before continuing** so the PATH change takes effect.

### 2 — Start

```bash
tylluan-cli start
```

On the very first run, BGE-M3 downloads with a progress bar (5–15 minutes depending on your connection, one-time only):

```
Downloading BGE-M3 embedding model... [##########] 1.2 GB
✅ Tylluan v0.16.0 running at http://127.0.0.1:4000
```

Check it's actually up:

```bash
curl -s http://127.0.0.1:4000/health
```

> [!TIP]
> **On something smaller than a typical dev machine?**
> * Zero-download, BM25-only: `tylluan-cli install --profile=portable`
> * ~100 MB, BGE-Small: `tylluan-cli install --profile=clinic`

> **Auth:** a bearer token is generated automatically at `.tylluan-token` on first boot. Dev mode (`--dev`) skips auth entirely — only use that on a network you fully control.

### 3 — Connect your agent

```json
{ "mcpServers": { "tylluan": { "type": "sse", "url": "http://127.0.0.1:4000/sse" } } }
```

| Client | Where |
|--------|-------|
| **Claude Code** | `claude mcp add --transport sse tylluan http://127.0.0.1:4000/sse` |
| **Claude Desktop** | `claude_desktop_config.json` |
| **Cursor** | `~/.cursor/mcp.json` |
| **VS Code** | `.vscode/mcp.json` in your workspace |

> **Use `127.0.0.1`, not `localhost`.** On Windows, `localhost` resolves to IPv6 first and can silently miss the kernel.

### 4 — Try it

```bash
export TYLLUAN_TOKEN=$(cat .tylluan-token)

# Store a memory
curl -X POST http://127.0.0.1:4000/api/v1/memory/write \
  -H "Authorization: Bearer $TYLLUAN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"content": "Tylluan is running local graph RAG."}'

# Retrieve it
curl "http://127.0.0.1:4000/api/v1/memory/search?q=How+does+Tylluan+query+graphs" \
  -H "Authorization: Bearer $TYLLUAN_TOKEN"
```

<details>
<summary>PowerShell equivalent (Windows)</summary>

```powershell
$env:TYLLUAN_TOKEN = Get-Content .tylluan-token
```
</details>

> **⚠️ This is research software.** Tylluan runs real code on your machine. It's a research lab, not a hardened enterprise product — read [DISCLAIMER.md](DISCLAIMER.md) before you put anything sensitive near it.

---

### Going further

| Topic | Guide |
|-------|-------|
| Configuration, auth, troubleshooting | [docs/getting-started/QUICKSTART.md](docs/getting-started/QUICKSTART.md) |
| Python guilds (49 tools) | [guilds/README.md](guilds/README.md) |
| Build from source | [docs/getting-started/QUICKSTART.md#build-from-source](docs/getting-started/QUICKSTART.md#build-from-source) |
| CLI reference | `tylluan-cli --help` |
| Installation profiles | `tylluan-cli install --profile=portable` |

---

## Where things stand — v0.16.0

This release adopts the MCP 2026-07-28 spec end to end (stateless core, Tasks, real MCP Apps manifests) and closes an 8-phase push to make Tylluan itself an agent's continuity, trust, and action layer — self-documenting guild contracts, unified bootstrap/resume, evidence-backed memory, a Trust Console for runtime/code drift, and a first real dataset circuit turning CoherenceGate's LLM judge decisions into structured, ground-truth-labeled examples. Three real bugs were found live and fixed the same way as always: root cause first, then a regression test, then verified against the live kernel — including a long-standing SSE-mode hang traced to a header-forwarding bug and confirmed fixed by the affected client itself.

| Milestone | Description | Status |
|-----------|-------------|--------|
| **MCP 2026-07-28 adoption (M39)** | Stateless core wired end-to-end and verified live with curl, Tasks with closed-state guards, real MCP Apps manifests (`ui://tylluan/knowledge-graph-canvas`) replacing a bare capability flag | ✅ |
| **Continuity/Trust/Action layer (M40)** | 8 phases: self-documenting guild contracts, unified `agent_bootstrap`/resume, full plan→act→verify→undo cycle, evidence/provenance on memory, Trust Console drift detection, concurrency test suite, near-invisible setup | ✅ |
| **CoherenceGate → dataset circuit** | Structured A/B examples (gate vs LLM judge) with real post-hoc ground truth from the existing Signal Loop — phase 1+2 shipped, nothing trained yet by design | ✅ |
| **Connection audit (v0.15.0)** | Full stack re-verified against a live kernel: guild IPC pointed at the wrong port in 5 places, vision inference crashed under real GPU/kernel contention (fixed by forcing CPU after root-causing a Windows driver timeout), silent writes bypassing the embedding pipeline, dashboard panels showing stale or fabricated data | ✅ |
| **Mesh encryption (v0.15.0)** | Production gossip loop now encrypts with Noise NK once a peer's public key has propagated, with a config-gated fallback for first contact — previously sent entirely in the clear despite the crypto layer existing and being tested | ✅ |
| **Guild registry completeness (v0.15.0)** | 13 additional guilds activated via `[guilds.v2]`, plus a structural test that fails CI if a guild is ever registered in the catalog but unreachable at runtime — this exact class of bug had shipped silently twice before | ✅ |
| **CoherenceGate Layer 4 hybrid** | Deterministic trigger zones + an LLM classifier for genuinely ambiguous cases, now wired into the live recall path in observation mode (logs its verdict without affecting results yet) | ✅ |
| **A2A Protocol (M38)** | Agent Card + JSON-RPC 2.0 server, interoperable with any Agent2Agent-compliant client | ✅ |
| **Signal Loop + Coherence Gate (ADR-011)** | `recall_feedback` tracks real memory usefulness; layered defense against memory-poisoning attacks on every recall | ✅ |
| **llama.cpp integration** | Real GGUF inference via an auto-downloaded `llama-server` binary, detects and defers to an external Ollama/LM Studio if one is already running | ✅ |
| **Mesh — DHT, Gossip, Noise (M14)** | Kademlia routing, epidemic dissemination, Noise XK/NK transport encryption | ✅ |
| **Federation (M11)** | Peer sync — push/pull/auto-sync, encrypted, provenance-tracked, echo-loop safe | ✅ |
| **Single binary (M7)** | `--features bundled-dashboard` embeds the React dashboard at compile time | ✅ |
| **v1.0.0** | External security audit, community validation, stable API, Docker smoke CI | 🔜 |

For the full history, see [CHANGELOG.md](CHANGELOG.md). For what's genuinely still open — including a few things this release found and deliberately chose *not* to rush — see [ROADMAP.md](ROADMAP.md).

---

## Architecture

```
┌───────────────────────────┐   ┌───────────────────────────────┐
│      MCP Clients          │   │   External A2A agents         │
│ (Claude, Cursor, VS Code, │   │ (LangGraph, CrewAI, any        │
│  LM Studio, any SSE)      │   │  Agent2Agent-compliant client) │
└─────────────┬─────────────┘   └───────────────┬────────────────┘
              │ SSE / HTTP Streamable            │ JSON-RPC 2.0
┌─────────────▼──────────────────────────────────▼────────────────┐
│                    tylluan-nexus (:4000)                         │
│                                                                   │
│  ┌─────────────────┐  ┌──────────────────┐  ┌──────────────────┐│
│  │  Core Memory     │  │  SilvaDB         │  │  A2A Server      ││
│  │  persona         │  │  SQLite WAL      │  │  Agent Card      ││
│  │  preferences     │  │  BGE-M3 vectors  │  │  message/send    ││
│  └─────────────────┘  │  FTS5 BM25       │  │  tasks/get       ││
│                        │  knowledge graph │  └──────────────────┘│
│  ┌─────────────────┐  │  episodic nodes  │                      │
│  │  Guild Registry  │  │  salience decay  │  ┌──────────────────┐│
│  │  49 Python tools │  └──────────────────┘  │  Embeddings: ONNX ││
│  │  auto-discovered │  ┌──────────────────┐  │  CPU / DirectML / ││
│  └─────────────────┘  │  Coloquio         │  │  CUDA · Generative││
│                        │  multi-agent      │  │  llama.cpp+GGUF  ││
│                        │                   │  └──────────────────┘│
│  ┌──────────────────────────────────────┐                        │
│  │  Federation + Mesh Layer             │                        │
│  │  peers.db · Noise NK / ChaCha20     │                        │
│  │  DHT Kademlia · Gossip · Noise XK   │                        │
│  └──────────────────────────────────────┘                        │
└───────────────────────────────────────────────────────────────────┘
               │ Noise NK / ChaCha20-Poly1305 encrypted
        ┌──────▼──────┐
        │  Peer nodes │  (LAN / VPN / WAN via DHT)
        └─────────────┘
```

## Stack

| Component | Technology |
|-----------|------------|
| Kernel | Rust (tokio + axum) |
| Embeddings | BGE-M3 (local ONNX, CPU) — configurable: bge-small, nomic, none |
| Reranker | Jina v1 Turbo (local ONNX) |
| Generative inference | `llama.cpp` (`llama-server`, auto-downloaded), agnostic to an external Ollama/LM Studio if one's already running |
| Search | BM25 + FTS5 + BGE-M3 vector + RRF hybrid fusion + entity boost |
| Storage | SQLite WAL + mmap vector index |
| Federation | SQLite `peers.db` + Noise NK / ChaCha20-Poly1305 |
| Mesh | Kademlia DHT + Gossip + Noise Protocol XK/NK |
| Guilds | Python (fastmcp) |
| Dashboard | React + Vite + Tailwind, embedded in the binary |

## Project structure

```
tylluan/
├── crates/
│   ├── tylluan-kernel/    Core kernel — memory, routing, guilds, federation, security
│   ├── tylluan-common/    Shared types and errors
│   ├── tylluan-link/      Federation networking — mesh identity, DHT, NAT, mDNS, Gossip, Noise
│   ├── tylluan-cli/       CLI management binary — start / stop / status / install
│   └── tylluan-evals/     Benchmarks — Recall@N, Precision@N, latency percentiles
├── guilds/                Python tool plugins (fastmcp), auto-discovered at startup
├── dashboard/             React dashboard (Vite + Tailwind), embedded in the binary
├── docs/                  Architecture and guides
├── integrations/          MCP client config examples (Claude, Cursor, LM Studio)
└── tests/                 Integration and E2E tests
```

## Federation

Point two or more Tylluan instances at each other and they'll share knowledge securely:

```toml
# tylluan.toml
[silva]
sync_interval_ms = 3600000      # the key the auto-sync loop actually reads; 0 = disabled
```

> `[federation] auto_sync_interval_secs`/`auto_sync_mode` are defined in config but currently unread by any sync loop — dead keys, don't rely on them (tracked in ROADMAP_O3.md).

```bash
# Add a peer
curl -X POST http://127.0.0.1:4000/api/v1/federation/peers \
  -H "Content-Type: application/json" \
  -d '{"name":"node-b","url":"http://192.168.1.10:4000","auth_token":"...","shared_secret":"..."}'

# Push local knowledge to all approved peers
curl -X POST http://127.0.0.1:4000/api/v1/federation/sync

# Pull from a specific peer
curl -X POST "http://127.0.0.1:4000/api/v1/federation/sync/pull?peer=node-b"

# See where a given node's knowledge came from
curl "http://127.0.0.1:4000/api/v1/federation/nodes?source=node-b"
```

A few invariants that hold regardless of configuration: unapproved peers are never synced, protected nodes are never exported, and anything received from a peer is tagged with `federation_source` and excluded from further outbound sync by default — so knowledge can't loop endlessly between instances.

## Security

Tylluan runs **real code on your machine**. Before you deploy it anywhere that matters, read:

- [SECURITY.md](SECURITY.md) — how to report a vulnerability
- [DISCLAIMER.md](DISCLAIMER.md) — what's on you as the operator
- [docs/concepts/SECURITY.md](docs/concepts/SECURITY.md) — the threat model, mapped to OWASP ASI 2026, including how the Coherence Gate (ADR-011) defends every `tylluan_recall` against memory-poisoning attacks

A few defaults you shouldn't change without understanding the consequences:
- `host = "127.0.0.1"` — localhost only
- `dev_mode = false` — auth enabled
- **Never** set `host = "0.0.0.0"` together with `dev_mode = true`

## Examples

```bash
# Memory basics: remember, recall, think
python examples/01_memory_basics.py

# Multi-agent communication via Coloquio
python examples/02_multi_agent_coloquio.py

# Knowledge graph exploration
python examples/03_knowledge_graph.py

# Autonomous multi-hop chain — no orchestrator, no API keys
python examples/multi_model_coloquio/run.py

# Bounded Work Contract — 3 agents, shared budget, finite iterations
python examples/bounded_work_contract/run.py
```

> Examples resolve the active kernel port automatically from `data/active_port.json` or `TYLLUAN_PORT` (defaults to `4000`). Override with `--port <PORT>` or `--kernel http://127.0.0.1:<PORT>`.

Full source in [examples/](examples/).

## Documentation

| Document | Purpose |
|----------|---------|
| [CHANGELOG.md](CHANGELOG.md) | Full version history |
| [ROADMAP.md](ROADMAP.md) | Versioned roadmap |
| [STATUS.md](STATUS.md) | Verified technical state — the source of truth |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Community standards |
| [docs/getting-started/QUICKSTART.md](docs/getting-started/QUICKSTART.md) | Detailed setup guide |
| [docs/concepts/FEDERATION_V3.md](docs/concepts/FEDERATION_V3.md) | Federation protocol spec |

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Forja-orca/tylluan&type=Date)](https://star-history.com/#Forja-orca/tylluan&Date)

## How to help

Tylluan is in active pre-production, and the thing it needs most right now is real-world testing on hardware and networks we don't have on hand:

1. **Hardware reports** — run it on a Raspberry Pi 4, an old laptop, a mini PC, and share your latency and RAM numbers in [GitHub Discussions](https://github.com/Forja-orca/tylluan/discussions).
2. **Retrieval quality** — try the hybrid search on your own data and tell us honestly whether it found what you expected. Failure reports are at least as useful as success stories here.
3. **Bug reports** — if installation or model loading breaks for you, open an issue with the output of `tylluan-cli logs` attached.

## License

[MIT](LICENSE) — use it, fork it, build on it.

---

<p align="center">
  <em>Tylluan (Welsh: owl) — sovereign memory for sovereign agents.</em>
</p>
