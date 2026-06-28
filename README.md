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

A local Rust kernel that gives AI agents **persistent memory**, a **knowledge graph**, and **real tool execution** — all running on your machine with zero cloud dependencies.

| Capability | Details |
|------------|---------|
| **Memory** | Dual-level retrieval (LightRAG pattern): entity-level BGE-M3 vector search + graph-expansion with degree centrality. Runs entirely on CPU |
| **Memory Decay** | Half-life exponential salience decay (T½=14d). Memories fade naturally; access reinforces them |
| **Tools** | 40+ guilds: bash, git, filesystem, docker, code, vision, web search, and more |
| **Collaboration** | Multi-agent channels, shared documents, session persistence |
| **Federation** | Instances sync knowledge with ChaCha20-Poly1305 encrypted transport |
| **MCP Native** | SSE + HTTP Streamable. Works with Claude, Cursor, VS Code, LM Studio |

### Dashboard

<p align="center">
  <img src="assets/screenshots/overview.png" alt="Overview — system health and kernel pulse" width="45%">
  <img src="assets/screenshots/guilds.png" alt="Guilds — 33 registered, 13 running" width="45%">
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

Every push runs: `cargo build` + `cargo test` + `cargo clippy`, Python lint (ruff), dashboard lint (eslint), CVE scanning (`cargo audit`), license compliance (`cargo deny`), and 7 security audit test suites. See [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.82+
- [Python](https://python.org/) 3.12+
- Node.js 20+ with pnpm (for dashboard only)

### 1. Clone and build

```bash
git clone https://github.com/Forja-orca/tylluan.git
cd tylluan
cargo build --release -p tylluan-kernel
```

### 2. Set up Python guilds

```bash
python -m venv .venv
source .venv/bin/activate  # Linux/Mac
# .venv\Scripts\activate   # Windows
pip install -r guilds/requirements.txt
```

### 3. Configure

```bash
cp tylluan.example.toml tylluan.toml
# Defaults are safe: localhost only, auth enabled
```

### 4. Start

> **Note:** The first boot downloads the BGE-M3 embedding model (~560 MB). This takes a few minutes. Subsequent starts are instant.



```bash
# Windows
.\tylluan-mcp.bat

# Linux / Mac
./tylluan-mcp.sh
```

### 5. Verify

```bash
curl http://127.0.0.1:3030/health
# {"status":"ok","version":"0.1.0"}
```

### 6. Connect your MCP client

<details>
<summary><strong>Claude Desktop</strong></summary>

Add to `claude_desktop_config.json`:
```json
{
  "mcpServers": {
    "tylluan": {
      "url": "http://127.0.0.1:3030/sse"
    }
  }
}
```
</details>

<details>
<summary><strong>Claude Code</strong></summary>

```bash
claude mcp add tylluan sse http://127.0.0.1:3030/sse
```
</details>

<details>
<summary><strong>Cursor / VS Code / Cline / Windsurf</strong></summary>

Add to your MCP settings:
```json
{
  "mcpServers": {
    "tylluan": {
      "url": "http://127.0.0.1:3030/sse"
    }
  }
}
```

See `integrations/` for editor-specific config files.
</details>

> **⚠️** Always use `127.0.0.1`, never `localhost` (IPv6 resolution trap on Windows).
>
> **Auth:** If `dev_mode = false` (default), the kernel generates a bearer token in `.tylluan-token` on first boot. Append it to the SSE URL: `http://127.0.0.1:3030/sse?token=YOUR_TOKEN`. In dev mode, no token is needed.

---

## Status: v0.2.0

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M2** | BGE-M3 1024-dim nativo — 1024-dim hybrid search, Matryoshka fix | ✅ Completo |
| **M1** | Memory Decay & Salience — half-life exponencial T½=14d en SilvaDB | ✅ Completo |
| **Docker** | Entry point de producción en :3033, BGE-M3 cache persistente | ✅ Operativo |
| **M4** | rmcp integration — ServerHandler + stdio transport via rmcp crate | ✅ Completo |
| **M6** | Dual-Level Retrieval (LightRAG pattern) — entity + graph centrality | ✅ Completo |
| **M7** | Single-Binary + release público | ✅ Completo |

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
│  ┌──────────────┐  ┌───────────────────┐  ┌─────────────────┐ │
│  │ Dual-Level   │  │ SilvaDB           │  │ Guild Registry  │ │
│  │ Retrieval    │  │ SQLite WAL        │  │ 40+ Python tools│ │
│  │ BGE-M3 1024  │  │ IVF vectors       │  │ via fastmcp     │ │
│  │ BM25 + Graph │  │ Knowledge graph   │  │                 │ │
│  │ centrality   │  │ Salience decay    │  │                 │ │
│  └──────────────┘  └───────────────────┘  └─────────────────┘ │
└─────────────────────────────────────────────────────┘
```

## Stack

| Component | Technology |
|-----------|------------|
| Kernel | Rust (tokio + axum) |
| Embeddings | BGE-M3 (local ONNX, CPU) |
| Reranker | Jina v1 Turbo (local ONNX) |
| Search | Dual-level: entity BM25 + BGE-M3 vector + graph expansion + degree centrality + cross-encoder rerank |
| Storage | SQLite + mmap vector index |
| Guilds | Python (fastmcp) |
| Dashboard | React + Vite + Tailwind |

## Project Structure

```
tylluan/
├── crates/
│   ├── tylluan-kernel/    Core kernel (memory, routing, guilds, security)

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
python examples/01_memory_basics.py --port 3033

# Multi-agent communication via coloquio
python examples/02_multi_agent_coloquio.py --port 3033

# Knowledge graph exploration
python examples/03_knowledge_graph.py --port 3033
```

See [examples/](examples/) for full source code.

## Documentation

| Document | Purpose |
|----------|---------|
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Community standards (humans + AI) |
| [AI_POLICY.md](AI_POLICY.md) | Rules for AI-generated contributions |
| [docs/QUICKSTART.md](docs/QUICKSTART.md) | Detailed setup guide |
| [ROADMAP.md](ROADMAP.md) | What's planned for v0.2, v0.3, v1.0 |

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Forja-orca/tylluan&type=Date)](https://star-history.com/#Forja-orca/tylluan&Date)

## License

[MIT](LICENSE) — use it, fork it, build on it.

---

<p align="center">
  <em>Tylluan (Welsh: owl) — sovereign memory for sovereign agents.</em>
</p>
