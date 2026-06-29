# Tylluan Advanced Setup Guide

**Prerequisites:** Rust 1.82+, Python 3.12+, Git, 4GB RAM
**Alternative to:** The 3-step binary install in [README.md](../README.md)

---

## Build from Source

```bash
git clone https://github.com/Forja-orca/tylluan.git && cd tylluan
cargo build --release -p tylluan-kernel
python -m venv .venv
source .venv/bin/activate   # or .venv\Scripts\activate on Windows
pip install -r guilds/requirements.txt
```

The binary is at `target/release/tylluan-nexus` (or `target/release/tylluan-nexus.exe` on Windows).

---

## Start

```bash
# Binary directly
target/release/tylluan-nexus

# Or via CLI (auto-finds the binary)
cargo run -p tylluan-cli -- start
```

Config `tylluan.toml` is auto-generated on first run, but you can pre-configure:

```toml
[nexus]
host = "127.0.0.1"
port = 3030
dev_mode = false

[federation]
auto_sync_interval_secs = 3600
auto_sync_mode = "both"
```

---

## Auth

On first boot, a bearer token is written to `.tylluan-token` in the working directory.

| Mode | Auth required | Use case |
|------|---------------|----------|
| `dev_mode = true` | None | Local development only |
| `dev_mode = false` (default) | Bearer token | Single-user production |
| External network | Bearer token + HTTPS reverse proxy | Multi-user / LAN |

Pass the token as `?token=...` on the SSE URL or as `Authorization: Bearer ...` header.

```json
{ "mcpServers": { "tylluan": { "type": "sse", "url": "http://127.0.0.1:3030/sse?token=YOUR_TOKEN" } } }
```

---

## Python Guilds

The kernel auto-discovers guilds in `guilds/` on startup. Install dependencies:

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r guilds/requirements.txt
```

Then restart `tylluan-nexus`. Guilds appear as tools under `tylluan_do`.

---

## CLI Reference

```text
tylluan-cli
  start         Launch the kernel (handles PATH, port, flags)
  stop          Kill the running kernel
  status        Health check on port 3030
  logs          View kernel logs (--follow for tail)
  connect       Handshake with a remote Tylluan instance
  download-models  Pre-download BGE-M3 before first boot
```

---

## Troubleshooting

### "Connection refused" on Step 3

1. Is the kernel running? `curl http://127.0.0.1:3030/health`
2. Is the port correct? Default is 3030. Check `tylluan.toml` if you changed it.
3. Is the URL correct? SSE clients need `/sse`, HTTP Streamable needs `/messages`.

### "Unauthorized"

1. `dev_mode = false` requires a bearer token.
2. Find it: `cat .tylluan-token` (file in kernel working directory).
3. Append `?token=...` to the SSE URL, or set `Authorization: Bearer ...` header.

### BGE-M3 download fails

- The model is ~560 MB. Ensure stable internet for the first boot.
- On slow connections, pre-download: `tylluan-cli download-models`
- Cached at `~/.cache/fastembed/` — it survives reinstalls.

### Guild not responding

1. Is `guilds/requirements.txt` installed? Check with `pip list | grep fastmcp`.
2. Is the guild enabled in `tylluan.toml`? Search for `enabled = false`.
3. Restart the kernel — guilds are detected at startup only.

### High CPU / memory

- Disable unused guilds in `tylluan.toml`.
- Check `data/audit.db` for runaway loops: `sqlite3 data/audit.db "SELECT * FROM audit ORDER BY timestamp DESC LIMIT 10;"`

---

## Quick Reference

| Command | Purpose |
|---------|---------|
| `curl http://127.0.0.1:3030/health` | Health check |
| `curl http://127.0.0.1:3030/api/v1/silva/stats` | Memory graph stats |
| `tylluan-cli logs --follow` | Live kernel logs |
| `sqlite3 data/audit.db "SELECT * FROM audit LIMIT 10"` | Recent audit entries |
| `cargo test -p tylluan-kernel --lib` | Run unit tests |
| `cargo test -p tylluan-link --lib` | Run all tylluan-link tests (memory, mesh) |

---

## Security

- `host = "127.0.0.1"` — localhost only. Default. Safe.
- `dev_mode = true` — disables auth. Never on a shared network.
- **Never** combine `host = "0.0.0.0"` + `dev_mode = true` (LAN RCE).
- Federated peers authenticate via `shared_secret` — protect it like a password.

See [SECURITY.md](../SECURITY.md) and [docs/architecture/SECURITY.md](architecture/SECURITY.md) for the full threat model.
