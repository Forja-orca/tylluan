# Tylluan Client Integration & Auto-Discovery Guide

Tylluan acts as a **Sovereign MCP Hub**. This document describes how external AI client agents (such as VS Code Cline/Roo Code, Cursor, Claude Desktop, or custom Python/shell scripts) connect to Tylluan and dynamically discover available guilds and capabilities.

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

### Capabilities Auto-Discovery
To allow client agents to discover specialized skills (e.g., executing shell commands, managing Git, scraping websites, reading databases) without breaking `CONTRACT-01`, Tylluan implements a **Handshake Auto-Discovery catalog** using standard MCP Prompts and Resources:
- **Prompt Catalog (`tylluan_guilds_catalog`)**: An MCP prompt that injects the complete guild tool catalog directly into the agent's system prompt context.
- **Resource Catalog (`tylluan://metadata/guilds`)**: An MCP resource that exposes a JSON schema database listing all active/registered guilds and their specialized tool signatures.

When the client agent connects, it checks Tylluan's resources, reads the catalog, and immediately understands which commands/guilds it can request through `tylluan_do` (e.g., calling `tylluan_do(intent="list git commits", guild="git")`).

---

## 2. Client Configurations

### VS Code (Cline / Roo Code / Roo Cline)
Cline/Roo Code supports custom stdio MCP servers. Configure it to spawn the client CLI connector.

Open your `mcp_settings.json` (usually at `%APPDATA%/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json`) and append:

```json
{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/client-cli"],
      "env": {
        "TYLLUAN_URL": "http://localhost:3030/api/v1/mcp",
        "TYLLUAN_TOKEN": "TU_TOKEN_AQUI"
      },
      "disabled": false,
      "alwaysOn": true
    }
  }
}
```

### Cursor
Cursor supports MCP Stdio connections. To connect Cursor to your sovereign hub:

1. Open Cursor Settings -> **Features** -> **MCP**.
2. Click **+ Add New MCP Server**.
3. Configure the following:
   - **Name**: `tylluan`
   - **Type**: `command`
   - **Command**: `npx -y @modelcontextprotocol/client-cli`
4. Set the following environment variables:
   - `TYLLUAN_URL`: `http://localhost:3030/api/v1/mcp`
   - `TYLLUAN_TOKEN`: `TU_TOKEN_AQUI`

### Claude Desktop
Add Tylluan to the official Claude Desktop client by editing your configuration file:
- **Windows**: `%APPDATA%/Claude/claude_desktop_config.json`
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`

Add the server:

```json
{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/client-cli"
      ],
      "env": {
        "TYLLUAN_URL": "http://127.0.0.1:3030/mcp",
        "TYLLUAN_TOKEN": "TU_TOKEN_AQUI"
      }
    }
  }
}
```

---

## 3. Direct REST Integration

External scripts or lightweight agents can execute intents and interact with memory directly via Tylluan's HTTP/REST API endpoints.

### Authentication
All REST requests require the bearer token inside the HTTP header:
```http
Authorization: Bearer TU_TOKEN_AQUI
```

### Endpoints Reference

#### 1. Query Capabilities (`GET /api/v1/capabilities`)
Returns the complete capability schema (sovereign tools, active guilds, underlying tool catalogs, and active sessions).

```bash
curl -X GET http://localhost:3030/api/v1/capabilities \
  -H "Authorization: Bearer TU_TOKEN_AQUI"
```

#### 2. Execute Intent (`POST /api/v1/do`)
Directs the natural language intent to the appropriate guild module.

```bash
curl -X POST http://localhost:3030/api/v1/do \
  -H "Authorization: Bearer TU_TOKEN_AQUI" \
  -H "Content-Type: application/json" \
  -d '{
    "intent": "List all folders in the current directory",
    "agent_id": "rest-script-client"
  }'
```

#### 3. Write Memory (`POST /api/v1/memory/write`)
Stores a document in the hybrid semantic database.

```bash
curl -X POST http://localhost:3030/api/v1/memory/write \
  -H "Authorization: Bearer TU_TOKEN_AQUI" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Agent is working on M24 auto-discovery guides.",
    "metadata": {
      "milestone": "M24",
      "category": "docs"
    }
  }'
```
