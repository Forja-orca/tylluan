# Tylluan Quick Start Guide

**Estimated time:** 15 minutes  
**Prerequisites:** Rust toolchain (1.75+), Python 3.11+, Git, 4GB RAM

---

## What is Tylluan?

Tylluan is a sovereign AI agent framework with persistent memory. It runs locally on your machine, orchestrates specialized sub-agents (called guilds) via MCP, and stores memories using embeddings + a knowledge graph.

**Key concepts:**
- **Kernel:** Rust process that orchestrates everything (binds to `127.0.0.1:3030` by default)
- **Guilds:** Python subprocesses that expose tools via fastmcp (web research, code execution, memory, etc.)
- **Sovereign tools:** MCP-accessible tools — `do` (execute), `remember` (store), `recall` (retrieve), `think` (analyze), `graph` (navigate)
- **Memory:** Persistent embeddings via BGE-M3 + graph traversal via SilvaDB

---

## Step 1 — Clone the Repository

```bash
git clone https://github.com/tylluan/tylluan.git
cd tylluan
```

*Note: Tylluan uses an orphan branch strategy. The commit history contains only the clean release commit — no development history is included.*

---

## Step 2 — Install Prerequisites

### Rust (if not installed)

```bash
# Windows: install from https://rustup.rs
# Linux/Mac:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Verify: `rustc --version` (requires 1.75+)

### Python 3.11+

```bash
# Windows: download from python.org or via winget
winget install Python.Python.3.11

# Verify:
python --version
```

### Git

```bash
# Windows: download from git-scm.com or via winget
winget install Git.Git

# Verify:
git --version
```

---

## Step 3 — Build the Kernel

```bash
cd tylluan
cargo build -p tylluan-kernel
```

This compiles the Rust kernel (~35K lines). Expected output:
```
Finished dev [unoptimized + debug-info] target(s)
```

**Build time:** 2-5 minutes on a modern machine.

---

## Step 4 — Configure

Copy the example config and review it:

```bash
cp tylluan.example.toml tylluan.toml
```

Open `tylluan.toml` and review these settings:

```toml
[server]
host = "127.0.0.1"   # localhost only — DO NOT change to 0.0.0.0
port = 3030
dev_mode = false     # MUST be false in production (disables auth if true)

[auth]
bearer_token = "change-me-to-a-random-string"  # CHANGE THIS

[guilds]
# Disable guilds you don't need by setting enabled = false
```

**Critical security notes:**
- `dev_mode = true` disables authentication. Never enable it on a networked machine.
- `host = "0.0.0.0"` exposes Tylluan to your LAN. Only use with a reverse proxy that handles auth.
- Change `bearer_token` to a long random string before exposing to any network.

---

## Step 5 — Start Tylluan

### Windows

```bash
.\tylluan-mcp.bat
```

### Linux / Mac

```bash
chmod +x tylluan-mcp.sh
./tylluan-mcp.sh
```

Or manually:

```bash
RUST_LOG=info cargo run -p tylluan-kernel
```

You should see:
```
INFO  tylluan_kernel: Tylluan kernel starting...
INFO  tylluan_kernel: Listening on 127.0.0.1:3030
INFO  tylluan_kernel: 34 guilds registered
```

**Leave this terminal open.** Tylluan runs in the foreground.

---

## Step 6 — Verify

In a new terminal:

```bash
curl http://127.0.0.1:3030/health
```

Expected response:
```json
{"commit":"...","status":"ok","version":"1.0.0"}
```

---

## Step 7 — Connect an MCP Client

Tylluan exposes MCP via SSE (`GET /sse`) and HTTP Streamable (`POST /messages`).

### Claude Desktop

Add to your Claude Desktop config file (`%APPDATA%\Claude\claude_desktop_config.json` on Windows):

```json
{
  "mcpServers": {
    "tylluan": {
      "url": "http://127.0.0.1:3030/sse"
    }
  }
}
```

Restart Claude Desktop.

### Claude Code

Add to your global MCP config (`~/.claude/mcp_servers.json`):

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

### Cursor

Add to Cursor settings (`~/.cursor/mcp_servers.json`):

```json
{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-sse", "http://127.0.0.1:3030/sse"]
    }
  }
}
```

### VS Code Copilot

VS Code Copilot does not currently support external SSE/SSE-based MCP servers. Use Claude Desktop or Cursor instead.

### Cline

Add to Cline MCP settings:

```json
{
  "mcpServers": {
    "tylluan": {
      "url": "http://127.0.0.1:3030/messages"
    }
  }
}
```

### LM Studio

LM Studio supports SSE MCP servers. Add as a remote server:
- URL: `http://127.0.0.1:3030/sse`
- Auth: Bearer token (if `bearer_token` is set in `tylluan.toml`)

---

## Step 8 — First Use

Send your first intent via any connected MCP client:

```
Use tylluan_do to tell me the current time
```

Or query memory:

```
Use tylluan_recall to find memories about the Tylluan project
```

Or store a memory:

```
Use tylluan_remember that Tylluan is a sovereign AI agent framework with persistent memory
```

---

## Step 9 — Review the Audit Log

Tylluan logs all operations to `audit.db`. To view recent operations:

```bash
sqlite3 audit.db "SELECT * FROM audit ORDER BY timestamp DESC LIMIT 20;"
```

Or use the `tylluan_do` tool:

```
Use tylluan_do to show me the last 10 audit entries
```

---

## Troubleshooting

### "Connection refused" when connecting MCP client

1. Is Tylluan running? (check `curl http://127.0.0.1:3030/health`)
2. Is the port correct? (default: 3030)
3. Is the URL correct for your client? (use `/sse` for SSE clients, `/messages` for HTTP Streamable)

### "Unauthorized" when sending requests

1. Is `dev_mode = false` in `tylluan.toml`? You need bearer token auth.
2. Is your client sending the `Authorization: Bearer <token>` header?
3. Is the token in the header the same as `bearer_token` in `tylluan.toml`?

### Guild not responding

1. Check if the guild is enabled in `tylluan.toml`
2. Check the kernel logs for guild startup errors
3. Try restarting Tylluan

### High CPU / memory usage

1. Disable unused guilds in `tylluan.toml`
2. Check `audit.db` for runaway loops
3. Monitor with `tasklist | findstr tylluan` (Windows) or `ps aux | grep tylluan` (Linux)

---

## Next Steps

- Read [SECURITY.md](../SECURITY.md) before deploying
- Read [DISCLAIMER.md](./DISCLAIMER.md)
- Review [CONTRIBUTING.md](./CONTRIBUTING.md) if you want to contribute
- Explore the [documentation](./docs/) for advanced configuration

---

## Quick Reference

| Command | Purpose |
|:---|:---|
| `curl http://127.0.0.1:3030/health` | Check if Tylluan is running |
| `curl http://127.0.0.1:3030/api/v1/silva/stats` | View memory graph stats |
| `sqlite3 audit.db "SELECT * FROM audit LIMIT 10"` | View recent audit entries |
| `cargo build -p tylluan-kernel` | Rebuild after code changes |
| `cargo test -p tylluan-kernel` | Run the test suite |