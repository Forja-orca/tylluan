# Tylluan Client Integration & Auto-Discovery Guide

Tylluan acts as a **Sovereign MCP Hub**. This document describes how external AI client agents (such as VS Code Cline/Roo Code, Cursor, Claude Desktop, Qwen Desktop, Codex, or custom Python/shell scripts) connect to Tylluan and dynamically discover available guilds and capabilities.

> **Corrección real (2026-08-09):** este documento citaba `@modelcontextprotocol/client-cli` como el puente stdio para clientes de escritorio — **ese paquete npm no existe** (`npm view` devuelve 404). Es la causa raíz real por la que Qwen Desktop, Claude Desktop y Codex no podían conectarse a Tylluan como herramienta MCP nativa en su panel, aunque el endpoint HTTP directo (`/messages`) siempre funcionó bien (verificado por los tres clientes vía JSON-RPC crudo). Corregido al puente real y verificado: [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) (MIT, `npm view mcp-remote` confirma versión real publicada).

---

## 1. Core Architecture & Discovery Philosophy

### The Sovereign Contract: CONTRACT-01
Tylluan preserves a strict contract where MCP clients are presented with exactly **5 sovereign tools**:
1. `tylluan_do` (natural language intent router)
2. `tylluan_remember` (write to long-term memory)
3. `tylluan_recall` (query long-term memory)
4. `tylluan_think` (cognitive graph reasoning)
5. `tylluan_graph` (direct triple store / GraphRAG operations)

Mounting dozens of individual guild tools directly into the top-level tool list is rejected because:
- It causes client agent context bloating.
- Frequent tool list updates (`listChanged` notifications) are irregularly supported by clients.
- It violates sovereign contract encapsulation.

### Why desktop clients need a bridge at all
Tylluan's kernel speaks HTTP (Streamable JSON-RPC at `/messages`, classic SSE at `/sse`) — it does not ship a stdio binary mode. Many desktop MCP clients (Claude Desktop, Qwen Desktop, Codex) only know how to launch a local **stdio** process for "Add MCP Server" flows; they don't offer a bare "paste an HTTP URL" option for unauthenticated local servers. `mcp-remote` is a small Node process that bridges the two: it speaks stdio to the desktop client on one side, and makes real HTTP/SSE requests to Tylluan on the other.

---

## 2. Client Configurations

All configs below assume Tylluan is running locally at `http://127.0.0.1:4000` (the real port — see `CLAUDE.md`, never `localhost`, always `127.0.0.1`). In `dev_mode = true` (the default until Tylluan's final hardening pass), no bearer token is required; the `--header` line is only needed once auth is enforced.

### Claude Desktop
Edit your configuration file:
- **Windows**: `%APPDATA%/Claude/claude_desktop_config.json`
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": ["mcp-remote@latest", "http://127.0.0.1:4000/messages", "--allow-http"]
    }
  }
}
```

If `dev_mode = false` and a bearer token is required, add `"--header", "Authorization:${TYLLUAN_TOKEN}"` to `args` and `"env": {"TYLLUAN_TOKEN": "Bearer <your-token>"}`.

### Codex
Codex reads MCP server definitions from the same `mcpServers` JSON shape as Claude Desktop (verified 2026-08-09: a Codex session confirmed it could reach Tylluan via raw JSON-RPC to `/messages`, but had no native panel entry — the missing piece was exactly this bridge). Use the identical config block above under Codex's own MCP servers config file.

### Qwen Desktop
Qwen Desktop also expects a stdio-launched MCP server. Same bridge, same shape:

```json
{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": ["mcp-remote@latest", "http://127.0.0.1:4000/messages", "--allow-http"]
    }
  }
}
```

Qwen Desktop has previously been documented (internal memory) as needing a `uvx`-launched stdio proxy specifically — if `npx mcp-remote` does not appear in Qwen's connector list after a restart, fall back to whatever local stdio launcher Qwen's own docs specify, pointed at the same `mcp-remote` command.

### VS Code (Cline / Roo Code / Roo Cline)
Cline/Roo Code supports custom stdio MCP servers. Open your `mcp_settings.json` (usually at `%APPDATA%/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json`) and append:

```json
{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": ["mcp-remote@latest", "http://127.0.0.1:4000/messages", "--allow-http"],
      "disabled": false,
      "alwaysOn": true
    }
  }
}
```

### Cursor
1. Open Cursor Settings -> **Features** -> **MCP**.
2. Click **+ Add New MCP Server**.
3. Configure the following:
   - **Name**: `tylluan`
   - **Type**: `command`
   - **Command**: `npx mcp-remote@latest http://127.0.0.1:4000/messages --allow-http`

### Claude Code / Antigravity (native HTTP support, no bridge needed)
Claude Code (`type: "sse"`) and Antigravity (`serverUrl`, HTTP Streamable) both speak Tylluan's HTTP endpoints directly — no `mcp-remote` bridge required. See `CLAUDE.md` for their exact config blocks.

---

## 3. Direct REST Integration

External scripts or lightweight agents can execute intents and interact with memory directly via Tylluan's HTTP/REST API endpoints — this is also the fastest way to *verify* a connection is possible at all before debugging a desktop client's UI.

### Authentication
In `dev_mode = true`, no header is required. Otherwise:
```http
Authorization: Bearer TU_TOKEN_AQUI
```

### Endpoints Reference

#### 1. Health check (`GET /health`)
```bash
curl http://127.0.0.1:4000/health
```

#### 2. List available guilds with full contracts (`POST /messages`, `list_available_guilds`)
```bash
curl -X POST http://127.0.0.1:4000/messages \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_available_guilds","arguments":{}}}'
```

#### 3. Execute Intent (`POST /messages`, `tylluan_do`)
```bash
curl -X POST http://127.0.0.1:4000/messages \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tylluan_do","arguments":{"intent":"list all folders in the current directory","agent_id":"rest-script-client"}}}'
```
