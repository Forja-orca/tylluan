# Tylluan — Integrations

Tylluan speaks standard MCP and connects to any MCP-compatible client. Below is the index of documented integrations.

| Client | Transport | File | Description |
|--------|-----------|------|-------------|
| OpenClaw | SSE (`/sse`) | [`openclaw.md`](openclaw.md) | Native MCP SSE — 368k stars, declarative `mcp add`, zero config |
| Hermes Agent (NousResearch) | HTTP Streamable (`/messages`) | [`hermes-agent.md`](hermes-agent.md) | NousResearch Hermes Agent — `url:` field in config, no transport key needed |

## Common prerequisites

- Tylluan kernel running on `http://127.0.0.1:3030`
- Health check: `curl http://127.0.0.1:3030/health` returns `{"status":"ok"}`
- No auth required in `dev_mode` (`tylluan.toml` → `dev_mode = true`)
