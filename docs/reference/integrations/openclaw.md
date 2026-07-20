# Tylluan + OpenClaw Integration Guide

> **Use case:** Add sovereign persistent memory to any OpenClaw agent. Your agent gains `tylluan_remember`, `tylluan_recall`, `tylluan_think`, and `tylluan_graph` — all memory stays local, on your machine.

## Prerequisites

- Tylluan running: `tylluan-cli start` → `http://127.0.0.1:3030`
- OpenClaw installed: [openclaw.io](https://openclaw.io)
- Health check: `curl http://127.0.0.1:3030/health` returns `{"status":"ok"}`

## Connect Tylluan to OpenClaw

### Option A — CLI (fastest)

```bash
openclaw mcp add tylluan http://127.0.0.1:3030/sse
```

OpenClaw will detect the 5 sovereign tools automatically on next restart.

### Option B — Config file

Add to `~/.config/openclaw/openclaw.json` (Linux/macOS) or `%APPDATA%\openclaw\openclaw.json` (Windows):

```json
{
  "mcpServers": {
    "tylluan": {
      "type": "sse",
      "url": "http://127.0.0.1:3030/sse"
    }
  }
}
```

Restart OpenClaw after editing the config.

## Verify the connection

After OpenClaw restarts, run a quick memory test:

```
# In your OpenClaw agent:
tylluan_remember: "OpenClaw connected to Tylluan on 2026-07-05. Test memory."

tylluan_recall: "test memory"
# Expected: returns the node you just stored
```

## Available tools

| Tool | What it does |
|------|-------------|
| `tylluan_remember` | Store information with BGE-M3 embeddings + auto contradiction detection |
| `tylluan_recall` | Hybrid search (vector + BM25) — R@5 82% on real queries |
| `tylluan_think` | Graph analysis: paths, hub nodes, contradictions |
| `tylluan_graph` | Knowledge graph operations: add triples, query relationships |
| `tylluan_do` | Route any natural-language task to a guild (bash, git, web search, etc.) |

## Authentication

By default Tylluan runs with `dev_mode = true` (no auth required). To enable auth:

1. Edit `tylluan.toml`: set `dev_mode = false`
2. Restart: `tylluan-cli stop && tylluan-cli start`
3. Read token: `cat ~/.tylluan/.tylluan-token`
4. Add to OpenClaw config:
   ```json
   {
     "mcpServers": {
       "tylluan": {
         "type": "sse",
         "url": "http://127.0.0.1:3030/sse",
         "headers": { "Authorization": "Bearer <your-token>" }
       }
     }
   }
   ```

## Profile selection

Tylluan starts in `portable` mode (BM25 only, no download required). For better recall quality:

```bash
# Download BGE-M3 model (~400MB, one-time)
tylluan-cli download-models

# Restart with full hybrid search
tylluan-cli stop && tylluan-cli start
```

After this, retrieval uses BGE-M3 1024-dim embeddings + BM25 fusion (R@5 82% on LongMemEval-S).

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Tool not appearing in OpenClaw | Run `openclaw mcp list` and verify `tylluan` is listed |
| `connection refused` | Check `curl http://127.0.0.1:3030/health` — kernel may not be running |
| `401 Unauthorized` | Add Bearer token header (see Authentication above) |
| `localhost` not resolving | Use `127.0.0.1` explicitly — Windows resolves `localhost` to IPv6 first |

## Sovereign data guarantee

All memory is stored in `~/.tylluan/` as SQLite databases. Nothing leaves your machine. OpenClaw reads and writes to Tylluan via local HTTP — no cloud calls, no telemetry, no vendor lock-in.

```
~/.tylluan/
├── silva.db          # knowledge graph + embeddings
├── mailbox.db        # agent coloquio channels
└── .tylluan-token    # auth token (if dev_mode=false)
```

## See also

- [Hermes Agent integration](hermes-agent.md)
- [Setup hint endpoint](../adr/ADR006_rufus_release.md) — `GET /api/v1/setup-hint`
- [MCP client configs](../../integrations/) — Claude Desktop, Claude Code, Cursor, LM Studio
