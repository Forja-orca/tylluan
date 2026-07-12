# Tylluan + Hermes Agent (NousResearch)

> This document is for external users of
> [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent)
> who want to connect it to Tylluan.

> **Use case:** Add sovereign persistent memory to any NousResearch Hermes agent instance. Hermes natively supports MCP servers — Tylluan plugs in with zero kernel changes.

## Prerequisites

- Tylluan running: `tylluan-cli start` → `http://127.0.0.1:3030`
- Hermes Agent installed: [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent)
- Health check: `curl http://127.0.0.1:3030/health` returns `{"status":"ok"}`

## Connect Tylluan to Hermes

Add to `~/.hermes/config.yaml`:

```yaml
mcp_servers:
  tylluan:
    url: "http://127.0.0.1:3030/sse"
```

Hermes infers transport from the `url` field (no `transport:` key needed). Restart Hermes after editing — it auto-discovers the 5 sovereign tools on startup.

**Hermes Desktop:** The desktop app uses the same `~/.hermes/config.yaml`. Add the entry above, then use the "Reload MCP" button in Settings instead of restarting.

## Verify the connection

```bash
# In your Hermes session:
> tylluan_remember: "Hermes connected to Tylluan. Sovereign memory active."
> tylluan_recall: "sovereign memory"
# Expected: returns the node you just stored
```

## Authentication

If Tylluan has auth enabled (`dev_mode = false` in `tylluan.toml`):

```yaml
mcp_servers:
  tylluan:
    url: "http://127.0.0.1:3030/sse"
    headers:
      Authorization: "Bearer <your-token>"
```

Read token with: `cat ~/.tylluan/.tylluan-token`

## Available tools

| Tool | What it does |
|------|-------------|
| `tylluan_remember` | Store information with BGE-M3 embeddings + auto contradiction detection |
| `tylluan_recall` | Hybrid search (vector + BM25) — R@5 82% on real queries |
| `tylluan_think` | Graph analysis: paths, hub nodes, contradictions |
| `tylluan_graph` | Knowledge graph operations: add triples, query relationships |
| `tylluan_do` | Route any natural-language task to a guild (bash, git, web search, etc.) |

## Profile selection

Default mode is `portable` (BM25 only). For better recall:

```bash
tylluan-cli download-models   # ~400MB BGE-M3, one-time
tylluan-cli stop && tylluan-cli start
```

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Tools not appearing | Verify Hermes loaded config: check `hermes status` |
| `connection refused` | Kernel not running — `tylluan-cli start` |
| `401 Unauthorized` | Add Bearer header (see Authentication above) |
| `localhost` failing on Windows | Use `127.0.0.1` explicitly |

## Sovereign data guarantee

All memory in `~/.tylluan/` (SQLite). No cloud, no telemetry.

## See also

- [OpenClaw integration](openclaw.md)
- [MCP client configs](../../integrations/) — Claude Desktop, Claude Code, Cursor
- [NousResearch/hermes-agent official docs](https://github.com/NousResearch/hermes-agent)
