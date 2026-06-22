# Tylluan

> Sovereign Memory Kernel for AI Agents

Local-first cognitive infrastructure. No cloud. No vendor lock-in.

*Tylluan (Welsh: owl) — sees what others miss, remembers what others forget.*

---

## What is Tylluan?

A local Rust kernel that provides persistent memory, tool routing, and multi-agent coordination for AI agents — all running on your machine.

**Memory** — Semantic search over a knowledge graph. BGE-M3 embeddings + BM25 + Jina Reranker. 90% Recall@5 on CPU, zero cloud dependencies.

**Tools** — 40+ guilds (bash, git, filesystem, docker, code analysis, web search, vision, and more) accessible through natural language via 5 MCP tools.

**Collaboration** — Multi-agent channels with shared documents, session persistence, and real-time coordination.

**Federation** — Instances sync knowledge bidirectionally with ChaCha20-Poly1305 encrypted transport.

**MCP Native** — Speaks Model Context Protocol (SSE + HTTP Streamable). Works with Claude, Cursor, VS Code, LM Studio, and any MCP client.

## Quick Start

### Prerequisites

- Rust 1.82+
- Python 3.12+
- Node.js 20+ with pnpm

### Build

```bash
cargo build --release -p tylluan-kernel -p tylluan-proxy
pip install -r guilds/requirements.txt
cd dashboard && pnpm install && pnpm build && cd ..
```

### Run

```bash
# Windows
.\tylluan.bat

# Linux / Mac
./tylluan.sh
```

### Verify

```bash
curl http://127.0.0.1:3030/health
# {"status":"ok","version":"3.0.0"}
```

### Connect your editor

```json
{"mcpServers":{"tylluan":{"url":"http://127.0.0.1:3030/sse"}}}
```

Place this in your MCP client config. See `integrations/` for editor-specific examples.

## Architecture

```
Clients ──► tylluan-proxy :3030 ──► tylluan-kernel :303X
             (zero-downtime)         (dynamic port)

5 sovereign tools:
  tylluan_do       — Route tasks to guilds via natural language
  tylluan_recall   — Search long-term memory
  tylluan_remember — Store knowledge persistently
  tylluan_think    — Reason over the knowledge graph
  tylluan_graph    — Direct graph operations

40+ Python guilds behind the router:
  bash · git · filesystem · docker · code · vision
  knowledge · web_search · coloquio · codebase_memory · ...
```

## Performance

| Metric | Value |
|--------|-------|
| Recall@5 (CPU-only) | 90% |
| Embedding | BGE-M3 1024-dim (local ONNX) |
| Reranker | Jina v1 Turbo (local ONNX) |
| Search latency | 3ms @ 100K tokens |
| Storage | INT8 scalar quantized + mmap |
| Federation | ChaCha20-Poly1305, bidirectional |

## Project Structure

```
crates/
  tylluan-kernel/    — Rust kernel (memory, routing, guilds, security)
  tylluan-proxy/     — Zero-downtime reverse proxy
  tylluan-evals/     — Benchmarks
  tylluan-common/    — Shared types
guilds/            — Python tool plugins (fastmcp)
dashboard/         — React dashboard
docs/              — Architecture and guides
integrations/      — MCP client configs
skills/            — Agent onboarding documentation
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
