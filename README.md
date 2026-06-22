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
</p>

---

> **⚠️ Experimental research software.** Tylluan executes real code on your machine. It is a research lab, not an enterprise product. Read [DISCLAIMER.md](DISCLAIMER.md) before using.

---

## What is Tylluan?

A local Rust kernel that gives AI agents **persistent memory**, a **knowledge graph**, and **real tool execution** — all running on your machine with zero cloud dependencies.

| Capability | Details |
|------------|---------|
| **Memory** | Semantic search with BGE-M3 embeddings + BM25 + Jina Reranker. 90% Recall@5 on CPU |
| **Tools** | 40+ guilds: bash, git, filesystem, docker, code, vision, web search, and more |
| **Collaboration** | Multi-agent channels, shared documents, session persistence |
| **Federation** | Instances sync knowledge with ChaCha20-Poly1305 encrypted transport |
| **MCP Native** | SSE + HTTP Streamable. Works with Claude, Cursor, VS Code, LM Studio |

### 5 Sovereign Tools

Every MCP client sees exactly these tools — nothing more, nothing less:

```
tylluan_do        Route tasks to guilds via natural language
tylluan_recall    Search long-term memory (BM25 + vector + rerank)
tylluan_remember  Store knowledge persistently in the graph
tylluan_think     Reason over the knowledge graph
tylluan_graph     Direct graph operations (triples, paths, PageRank)
```

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
cargo build --release -p tylluan-kernel -p tylluan-proxy
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

```bash
# Windows
.\tylluan-mcp.bat

# Linux / Mac
./tylluan-mcp.sh
```

### 5. Verify

```bash
curl http://127.0.0.1:3030/health
# {"status":"ok","version":"3.0.0"}
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

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              MCP Clients                             │
│  (Claude, Cursor, VS Code, LM Studio, any client)  │
└──────────────┬──────────────────────────────────────┘
               │ SSE / HTTP Streamable
┌──────────────▼──────────────────────────────────────┐
│         tylluan-proxy (:3030)                        │
│       Zero-downtime gateway                          │
└──────────────┬──────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────┐
│        tylluan-kernel (dynamic port)                 │
│                                                      │
│  ┌────────┐  ┌──────────┐  ┌────────────────────┐  │
│  │ Router │  │ SilvaDB  │  │ Guild Registry     │  │
│  │ BGE-M3 │  │ Memory + │  │ 40+ Python tools   │  │
│  │ + BM25 │  │ Graph +  │  │ via fastmcp        │  │
│  │        │  │ Vectors  │  │                    │  │
│  └────────┘  └──────────┘  └────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## Performance

| Metric | Value |
|--------|-------|
| Recall@5 (CPU-only) | 90% |
| Embedding | BGE-M3 768-dim (local ONNX) |
| Reranker | Jina v1 Turbo (local ONNX) |
| Search latency | ~3ms @ 100K tokens |
| Storage | INT8 scalar quantized + mmap |

## Project Structure

```
tylluan/
├── crates/
│   ├── tylluan-kernel/    Core kernel (memory, routing, guilds, security)
│   ├── tylluan-proxy/     Zero-downtime reverse proxy
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

## Documentation

| Document | Purpose |
|----------|---------|
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Community standards (humans + AI) |
| [AI_POLICY.md](AI_POLICY.md) | Rules for AI-generated contributions |
| [docs/QUICKSTART.md](docs/QUICKSTART.md) | Detailed setup guide |

## License

[MIT](LICENSE) — use it, fork it, build on it.

---

<p align="center">
  <em>Tylluan (Welsh: owl) — sovereign memory for sovereign agents.</em>
</p>
