# Tylluan Quickstart Guide

This guide gets you up and running with **Tylluan**, the sovereign local-first memory and multi-agent coordination kernel.

---

## 1. Prerequisites

- **Rust:** 1.99.1 (pinned in `rust-toolchain.toml`) if building from source.
- **Git:** for repository operations.
- **Python:** 3.12+ (optional; required only if running Python guilds).
- **Node.js & pnpm:** (optional; required only if running the React dashboard in `dashboard/`).

---

## 2. Quick Installation

### Option A: Automated Binary Installer (Recommended)

Run the automated installer for your platform:

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex
```

The installer downloads the pre-built release binaries (`tylluan-nexus` and `tylluan-cli`) into `~/.tylluan/bin/`, adds them to your PATH, writes a `tylluan.toml` with the `portable` profile and starts the kernel. Open a new terminal afterwards so the PATH change takes effect.

### Option B: Build from Source

```bash
git clone https://github.com/Forja-orca/tylluan.git
cd tylluan

# Build kernel binary
cargo build --release -p tylluan-kernel

# (Optional) Set up Python virtual environment for guilds
python -m venv .venv
source .venv/bin/activate    # On Windows: .venv\Scripts\activate
pip install -r guilds/requirements.txt
```

The compiled binary will be located at `target/release/tylluan-nexus` (or `target/release/tylluan-nexus.exe` on Windows).

---

## 3. Starting the Kernel

Launch via the CLI tool or run the binary directly:

```bash
# Via tylluan-cli (locates the binary automatically)
cargo run -p tylluan-cli -- start

# Or directly via compiled binary
./target/release/tylluan-nexus
```

### Process Isolation Rule
**Never start a second kernel from the same working directory as an active running kernel.**
If the configured port (`47004` by default) is occupied, the kernel retries that port for the configured window and then **fails with a clear error**; it does not silently choose another port. For multiple instances or isolated testing environments, assign an independent directory, distinct config (`tylluan.toml`), explicit `TYLLUAN_DATA_DIR`, and dedicated `--port`.

---

## 4. Installation Profiles

Select a profile tailored to your hardware and deployment target:

| Profile | Semantic Embeddings | Memory Footprint | Recommended Use Case |
|---|---|---|---|
| **`portable`** | None (BM25 lexical search only) | ~34 MB resident idle | Zero-dependency, Raspberry Pi, lightweight containers |
| **`clinic`** | BGE-Small (384-dim, 67 MB) | Low RAM footprint | Edge devices, offline clinic workstations (~200k docs on 8 GB RAM) |
| **`server`** | `mxbai-embed-large` (1024-dim) | Standard production | Full hybrid semantic search (BM25 + vector RRF + Silva graph) |

Initialize a specific profile using `tylluan-cli`:

```bash
tylluan-cli install --profile portable
# or:
tylluan-cli install --profile server
```

---

## 5. Connecting via MCP (Model Context Protocol)

Tylluan natively speaks MCP over HTTP Server-Sent Events (`/sse`) and Streamable HTTP (`/messages`).

### Authentication
With `dev_mode = false` (the default), the kernel generates a random bearer token on first boot if none is set and writes it to `.tylluan-token` in the **current working directory of the kernel process** (the directory you started it from):

```bash
cat .tylluan-token
```

If you started the kernel from `~/.tylluan`, the file is `~/.tylluan/.tylluan-token`.

### Client Configuration (Claude Desktop, OpenCode, Cursor)

Add the following entry to your `claude_desktop_config.json` or agent MCP configuration:

```json
{
  "mcpServers": {
    "tylluan": {
      "type": "sse",
      "url": "http://127.0.0.1:47004/sse?token=YOUR_BEARER_TOKEN"
    }
  }
}
```

---

## 6. Sovereign Tools (CONTRACT-01)

The kernel exposes exactly 6 sovereign tools:

1. **`tylluan_remember`**: Store episodic or semantic facts in the Silva memory graph with automatic decay (duplicate merging runs later in the DreamCycle consolidation, not on the write path).
2. **`tylluan_recall`**: Hybrid retrieval combining BM25, vector embeddings, and Personalized PageRank graph exploration.
3. **`tylluan_think`**: Structured internal reasoning and multi-hop hypothesis testing.
4. **`tylluan_graph`**: Direct topology queries, neighbor traversal, and semantic relationship discovery.
5. **`tylluan_do`**: Dispatch tasks to routable Python guilds (vision, scraping, system diagnostics, audio).
6. **`tylluan_decide`**: High-speed typed decision evaluation (Choice, Score, Boolean) backed by deterministic rules and shadow ML models.

---

## 7. Python Guilds

Guilds extend Tylluan's execution capabilities. Guild scripts in `guilds/` are auto-discovered on boot (45 default routable guilds):

```bash
# Set up guild environment
python -m venv .venv
source .venv/bin/activate
pip install -r guilds/requirements.txt

# Verify guild status
tylluan-cli doctor
```

---

## 8. Essential Verification Commands

```bash
# Health check (returns JSON with status, version, and active commit)
curl http://127.0.0.1:47004/health

# Silva memory graph statistics
curl -H "Authorization: Bearer $(cat .tylluan-token)" http://127.0.0.1:47004/api/v1/silva/stats

# Diagnostics and self-audit
tylluan-cli doctor
tylluan-cli audit
```

---

## 9. Security Principles

- **Localhost Default:** The kernel binds to `127.0.0.1` by default. Never bind `0.0.0.0` with `dev_mode = true` enabled.
- **Fail-Closed Permissions:** with `dev_mode = false` (the default) every mutating operation requires explicit bearer token authentication; `dev_mode = true` is for local development only.
- **Untracked Tokens:** `.tylluan-token` is strictly git-ignored; never commit secret keys to version control.

---

## 10. Next Steps

- Overview and the binary Quick Start (4 steps): [README](../../README.md).
- Security model and reporting: [SECURITY](../../SECURITY.md) and [concepts/SECURITY](../concepts/SECURITY.md).
