<p align="center">
  <img src="assets/branding/logo.jpg" alt="Tylluan" width="160">
</p>

<h1 align="center">Tylluan</h1>

<p align="center">
  <strong>Sovereign cognitive substrate for AI agents</strong><br>
  <em>Sees what others miss, remembers what others forget.</em>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="MIT License"></a>
  <img src="https://img.shields.io/badge/version-0.5.0-blue.svg" alt="v0.5.0">
  <img src="https://img.shields.io/badge/rust-1.82+-orange.svg" alt="Rust 1.82+">
  <img src="https://img.shields.io/badge/python-3.12+-blue.svg" alt="Python 3.12+">
  <img src="https://img.shields.io/badge/MCP-native-purple.svg" alt="MCP Native">
  <img src="https://img.shields.io/badge/cloud-none-brightgreen.svg" alt="No Cloud">
  <a href="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml"><img src="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="deny.toml"><img src="https://img.shields.io/badge/license%20audit-deny.toml-blue.svg" alt="License audit"></a>
</p>

---

> **⚠️ Experimental research software.** Tylluan executes real code on your machine. It is a research lab, not an enterprise product. Read [DISCLAIMER.md](DISCLAIMER.md) before using.

---

## What is Tylluan?

A local Rust kernel that gives AI agents **persistent memory**, a **knowledge graph**, **real tool execution**, and **federated peer sync** — all running on your machine with zero cloud dependencies.

| Capability | Details |
|------------|---------|
| **Memory** | Dual-level retrieval (LightRAG pattern): entity-level BGE-M3 vector search + graph expansion with degree centrality. Runs entirely on CPU |
| **Memory Decay** | Half-life exponential salience decay (T½=14d). Memories fade naturally; access reinforces them |
| **Tools** | 40+ guilds: bash, git, filesystem, docker, code, vision, web search, and more |
| **Collaboration** | Multi-agent channels (Coloquio), shared documents, Bounded Work Contracts |
| **Federation** | Peer-to-peer knowledge sync over LAN/VPN — ChaCha20-Poly1305 encrypted, provenance-tracked, echo-loop safe |
| **Encryption** | AES-256 at rest via SQLCipher (feature-gated: `cargo build --features encryption`, `PRAGMA hexkey`, no SQL injection vector) |
| **MCP Native** | SSE + HTTP Streamable. Works with Claude, Cursor, VS Code, LM Studio |

### Dashboard

<p align="center">
  <img src="assets/screenshots/overview.png" alt="Overview — system health and kernel pulse" width="45%">
  <img src="assets/screenshots/guilds.png" alt="Guilds — registered and running" width="45%">
</p>
<p align="center">
  <img src="assets/screenshots/knowledge_graph.png" alt="Knowledge Graph — SilvaDB visualizer" width="45%">
  <img src="assets/screenshots/coloquio.png" alt="Coloquio — multi-agent communication" width="45%">
</p>

### 5 Sovereign Tools

Every MCP client sees exactly these tools — nothing more, nothing less:

```
tylluan_do        Route tasks to guilds via natural language
tylluan_recall    Search long-term memory — dual-level (entity + graph) or standard
tylluan_remember  Store knowledge persistently in the graph
tylluan_think     Reason over the knowledge graph
tylluan_graph     Direct graph operations (triples, paths, PageRank)
```

### CI / Security

[![CI](https://github.com/forja-orca/tylluan/actions/workflows/ci.yml/badge.svg)](https://github.com/forja-orca/tylluan/actions/workflows/ci.yml)

Every push runs 5 jobs: Rust build+test (250 lib tests, 8 integration suites) + clippy, cargo-deny (bans, licenses, advisories), Python lint+test (ruff + pytest), Dashboard build (pnpm), and security audit tests. [Status](STATUS.md) — all green. See [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

---

## Quick Start

> **The honest toll:** First boot downloads the BGE-M3 embedding model (~2.2 GB, one-time). This is the cost of sovereign memory — no cloud, no API key, your hardware. Subsequent starts are instant.

### Step 1 — Install (30 seconds)

No Rust, Python, or Node needed.

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex
```

Downloads `tylluan-nexus` + `tylluan-cli` to `~/.tylluan/bin/` and adds them to your PATH. **Open a new terminal before continuing.**

### Step 2 — Start (5 seconds)

```bash
tylluan-cli start
```

On first boot, BGE-M3 downloads with a progress bar (5–15 min on a typical connection, one-time):

```
Downloading BGE-M3 embedding model... [##########] 2.2 GB
✅ Tylluan v0.5.0 running at http://127.0.0.1:3030
```

Verify it's up before connecting:

```bash
curl -s http://127.0.0.1:3030/health
```

> **Auth:** A bearer token is auto-generated at `.tylluan-token` on first boot. Dev mode (`--dev`) skips auth — never use on a network that isn't your own.

### Step 3 — Connect (15 seconds)

Add to any SSE-capable MCP client:

```json
{ "mcpServers": { "tylluan": { "type": "sse", "url": "http://127.0.0.1:3030/sse" } } }
```

| Client | Config file |
|--------|-------------|
| **Claude Code** | `claude mcp add --transport sse tylluan http://127.0.0.1:3030/sse` |
| **Claude Desktop** | `claude_desktop_config.json` |
| **Cursor** | `~/.cursor/mcp.json` |
| **VS Code** | `.vscode/mcp.json` in your workspace |

> **Always use `127.0.0.1`** — never `localhost` (Windows resolves IPv6 first and misses the kernel).

---

### Advanced

| Topic | Guide |
|-------|-------|
| Configuration, auth, troubleshooting | [docs/QUICKSTART.md](docs/QUICKSTART.md) |
| Python guilds (bash, vision, web search, 40+ tools) | [guilds/README.md](guilds/README.md) |
| Build from source | [docs/QUICKSTART.md#build-from-source](docs/QUICKSTART.md#build-from-source) |
| CLI reference | `tylluan-cli --help` |

---

## Status: v0.5.0 (Mesh Fabric)

| Milestone | Description | Status |
|-----------|-------------|--------|
| **M1** | Memory Decay & Salience — half-life exponential T½=14d | ✅ |
| **M2** | BGE-M3 1024-dim native — hybrid search, Matryoshka fix | ✅ |
| **M4** | rmcp integration — ServerHandler + stdio transport | ✅ |
| **M6** | Dual-Level Retrieval (LightRAG pattern) — entity + graph centrality | ✅ |
| **M7** | Single-binary + public release | ✅ |
| **Docker** | Production image at `:3030`, BGE-M3 persistent cache volume | ✅ |
| **M10** | Bounded Work Contracts — finite multi-agent protocol with budget gate | ✅ |
| **M10-B** | SQL persistence for contracts — survive kernel restarts | ✅ |
| **Security CI** | 30 automated security tests — intent filter, ACL, rate limiter, impersonation | ✅ |
| **M11-A** | Federation peer DB — SQLite `peers.db`, `auth_token`/`shared_secret` split | ✅ |
| **M11-B** | Pull sync — `/sync/export`, `/sync/pull`, `/sync/both` | ✅ |
| **M11-C** | Node provenance — `federation_source` SQL column, echo-loop prevention, `/federation/nodes` query | ✅ |
| **M11-D** | Scheduled auto-sync — background tokio loop, configurable interval and mode | ✅ |
| **M11-E** | Federation integration tests — `federation_audit.rs` (6 tests) | ✅ |
| **Encryption** | SQLCipher AES-256 at rest — `PRAGMA hexkey`, 4 DB modules, `--features encryption` | ✅ |
| **M12 Mesh** | Ed25519 identity · STUN NAT · mDNS LAN · node signing · integration tests | ✅ |
| **M13 Onboarding** | Binary releases (3 targets) · install scripts · `tylluan-cli` · README 3-step | ✅ |
| **M14-A DHT** | Kademlia routing table (256 K-buckets) · Ed25519 XOR metric · mainline DHT bootstrap · 23 tests | ✅ |
| **M14-B** | Gossip protocol — epidemic knowledge dissemination | 🔜 |
| **M14-C** | Encrypted transport overlay — Noise Protocol | 🔜 |
| **M14-D** | Cross-datacenter federation — latency-aware routing | 🔜 |
| **M14-E** | Mesh test harness — fault injection, partition, recovery | 🔜 |

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              MCP Clients                             │
│  (Claude, Cursor, VS Code, LM Studio, any client)  │
└──────────────┬──────────────────────────────────────┘
               │ SSE / HTTP Streamable
┌──────────────▼──────────────────────────────────────┐
│         tylluan-nexus (:3030)                        │
│                                                      │
│  ┌──────────────┐  ┌───────────────────┐  ┌───────────────┐  │
│  │ Dual-Level   │  │ SilvaDB           │  │ Guild Registry│  │
│  │ Retrieval    │  │ SQLite WAL        │  │ 40+ Python    │  │
│  │ BGE-M3 1024  │  │ IVF vectors       │  │ tools (MCP)   │  │
│  │ BM25 + Graph │  │ Knowledge graph   │  │               │  │
│  │ centrality   │  │ Salience decay    │  │               │  │
│  └──────────────┘  └───────────────────┘  └───────────────┘  │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │ Federation Layer                              │   │
│  │ peers.db · ChaCha20 encrypted · provenance  │   │
│  │ push / pull / auto-sync · echo-loop safe    │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
               │ ChaCha20-Poly1305 encrypted
        ┌──────▼──────┐
        │  Peer nodes │  (LAN / VPN)
        └─────────────┘
```

## Stack

| Component | Technology |
|-----------|------------|
| Kernel | Rust (tokio + axum) |
| Embeddings | BGE-M3 (local ONNX, CPU) |
| Reranker | Jina v1 Turbo (local ONNX) |
| Search | Dual-level: entity BM25 + BGE-M3 vector + graph expansion + degree centrality + cross-encoder rerank |
| Storage | SQLite WAL + mmap vector index |
| Federation | SQLite `peers.db` + ChaCha20-Poly1305 (per-peer keys) |
| Guilds | Python (fastmcp) |
| Dashboard | React + Vite + Tailwind |

## Project Structure

```
tylluan/
├── crates/
│   ├── tylluan-kernel/    Core kernel (memory, routing, guilds, federation, security)
│   ├── tylluan-common/    Shared types and errors
│   └── tylluan-evals/     Benchmarks (LongMemEval, BeamScale)
├── guilds/                Python tool plugins (fastmcp)
│   ├── builders/          DO things (bash, git, docker, code)
│   ├── scholars/          ANALYZE things (knowledge, deep analysis)
│   ├── wardens/           GUARD quality (audit, formatting)
│   └── watchers/          MONITOR health (metrics, cron)
├── dashboard/             React dashboard (Vite + Tailwind)
├── docs/                  Architecture and guides
├── integrations/          MCP client config examples
└── tests/                 E2E and integration tests
```

## Federation

Connect multiple Tylluan instances so they share knowledge securely:

```toml
# tylluan.toml
[federation]
auto_sync_interval_secs = 3600  # 0 = disabled
auto_sync_mode = "both"         # "push" | "pull" | "both"
```

```bash
# Add a peer
curl -X POST http://127.0.0.1:3030/api/v1/federation/peers \
  -H "Content-Type: application/json" \
  -d '{"name":"node-b","url":"http://192.168.1.10:3030","auth_token":"...","shared_secret":"..."}'

# Push local knowledge to all approved peers
curl -X POST http://127.0.0.1:3030/api/v1/federation/sync

# Pull from a specific peer
curl -X POST "http://127.0.0.1:3030/api/v1/federation/sync/pull?peer=node-b"

# Query provenance — which nodes came from which peer?
curl "http://127.0.0.1:3030/api/v1/federation/nodes?source=node-b"
curl "http://127.0.0.1:3030/api/v1/federation/nodes?source=local"
```

Security invariants: unapproved peers are never synced; protected nodes are never exported; received nodes carry `federation_source` provenance and are excluded from outbound sync by default (echo-loop prevention).

## Security

Tylluan runs **real code on your machine**. Please read these before deploying:

- [SECURITY.md](SECURITY.md) — Vulnerability reporting
- [DISCLAIMER.md](DISCLAIMER.md) — Operator responsibilities
- [docs/architecture/SECURITY.md](docs/architecture/SECURITY.md) — Threat model + OWASP ASI 2026 mapping

Key defaults (do not change without understanding the implications):
- `host = "127.0.0.1"` — localhost only
- `dev_mode = false` — auth enabled
- **Never** set `host = "0.0.0.0"` with `dev_mode = true`

## Examples

```bash
# Memory basics: remember, recall, think
python examples/01_memory_basics.py --port 3030

# Multi-agent communication via coloquio
python examples/02_multi_agent_coloquio.py --port 3030

# Knowledge graph exploration
python examples/03_knowledge_graph.py --port 3030

# Autonomous multi-hop chain — no orchestrator, no API keys needed
python examples/multi_model_coloquio/run.py --kernel http://127.0.0.1:3030

# Bounded Work Contract — 3 agents, shared budget, finite iterations
python examples/bounded_work_contract/run.py --kernel http://127.0.0.1:3030
```

See [examples/](examples/) for full source code.

## Documentation

| Document | Purpose |
|----------|---------|
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Community standards (humans + AI) |
| [AI_POLICY.md](AI_POLICY.md) | Rules for AI-generated contributions |
| [docs/QUICKSTART.md](docs/QUICKSTART.md) | Detailed setup guide |
| [ROADMAP.md](ROADMAP.md) | Versioned roadmap — v0.2, v0.3 done, v1.0 planned |
| [docs/architecture/FEDERATION_V3.md](docs/architecture/FEDERATION_V3.md) | Federation protocol spec |

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Forja-orca/tylluan&type=Date)](https://star-history.com/#Forja-orca/tylluan&Date)

## License

[MIT](LICENSE) — use it, fork it, build on it.

---

<p align="center">
  <em>Tylluan (Welsh: owl) — sovereign memory for sovereign agents.</em>
</p>
