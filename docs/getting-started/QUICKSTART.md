# Tylluan Advanced Setup Guide

**Prerequisites:** Rust 1.88 (pinned in `rust-toolchain.toml`), Git, and Python 3.12+ if you want the Python guilds.
**Alternative to:** The 3-step binary install in [README.md](../../README.md)

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
# Via CLI, run from the repository root (it looks for tylluan-nexus next to itself,
# in ~/.tylluan/bin, on PATH, then in ., target/release and target/debug)
cargo run -p tylluan-cli -- start

# Or the binary directly
target/release/tylluan-nexus
```

**Never start a second kernel from the same working directory as a running one.** If the configured port is busy, the kernel binds the next free port, records it in `data/active_port.json` and tells the previous kernel (the port recorded there) to shut down; it replaces the running kernel instead of coexisting with it (`crates/tylluan-kernel/src/transport/http/mod.rs`, `start_http_server`; read from the code, not exercised here). Test instances need their own directory, config, data directory (`TYLLUAN_DATA_DIR`) and `--port`.

A `tylluan.toml` is optional: without one the kernel starts with built-in defaults. Those defaults serve **http** and **sse** only. The **stdio** transport is opt-in (`--stdio`, or `transport = ["stdio", "http"]` in `tylluan.toml`), because a kernel started without a terminal (a service, `nohup`, CI) exits as soon as stdio reads end-of-input. Create a config with either:

```bash
# Generate one for a profile (also starts the kernel; --dir picks the target directory)
tylluan-cli install --profile server

# Or copy the template (edit before first boot)
cp tylluan.example.toml tylluan.toml
```

Minimal working config:

```toml
[nexus]
host = "127.0.0.1"
port = 47004
dev_mode = false
transport = ["http", "sse"]   # no stdio: safe to run detached from a terminal
```

---

## Installation Profiles

Pick a profile depending on your hardware. These are the exact descriptions printed by `tylluan-cli install --help`:

| Profile | Embeddings | Use case |
|---------|------------|----------|
| `portable` | none (BM25-only), no model downloads | Smallest footprint; zero downloads |
| `clinic` | BGE-Small (67 MB, 384-dim) | Light semantic search; the CLI sizes it for ~200K docs on 8 GB RAM |
| `server` | `mxbai-embed-large` (1024-dim) | Full semantic search, production grade |

```bash
tylluan-cli install --profile portable
tylluan-cli install --profile clinic
tylluan-cli install --profile server
```

Profiles can be changed later in `tylluan.toml` by setting `embedding_model` (and the matching `vector_dimensions`) to one of `"none"`, `"mxbai-embed-large"`, `"mxbai-q"`, `"bge-m3"`, `"bge-small"`, `"minilm"`, `"nomic-embed-text"`, `"snowflake-arctic-embed-l"` or `"multilingual-e5-large"` (the list the kernel prints when it rejects an unknown value). Dimensions: mxbai, bge-m3, arctic and e5 are 1024, nomic is 768, bge-small and minilm are 384.

For reference, a measured data point on the `portable` profile (published v0.17.1 Linux x86_64 binary, empty database, WSL2 with 6 CPUs and 8 GB RAM): healthy in 0.9–3.7 s and about 34 MB resident at idle. Other profiles were not measured.

---

## Auth

On first boot, a bearer token is written to `.tylluan-token` in the kernel's working directory.

| Mode | Auth required | Use case |
|------|---------------|----------|
| `dev_mode = true` | None | Local development only |
| `dev_mode = false` (default) | Bearer token | Single-user production |
| External network | Bearer token + HTTPS reverse proxy | Multi-user / LAN |

Pass the token as `?token=...` on the SSE URL or as an `Authorization: Bearer ...` header.

```json
{ "mcpServers": { "tylluan": { "type": "sse", "url": "http://127.0.0.1:47004/sse?token=YOUR_TOKEN" } } }
```

---

## Python Guilds

The kernel auto-discovers guilds in `guilds/` on startup (45 are routable by default; `sandbox` is experimental). Install dependencies:

```bash
# Linux / macOS
python -m venv .venv
source .venv/bin/activate

# Windows (PowerShell)
# python -m venv .venv
# .venv\Scripts\activate

pip install -r guilds/requirements.txt
```

Then restart with `tylluan-cli start` (or `cargo run -p tylluan-cli -- start` from source). Guilds appear as tools under `tylluan_do`.

---

## CLI Reference

The commands printed by `tylluan-cli --help`:

```text
tylluan-cli
  start            Start the TylluanNexus kernel
  stop             Stop the kernel (resolves the exact instance via its PID file)
  status           Check the status of the hub
  doctor           Full diagnostic scan (guilds, storage, system resources, config)
  logs             Stream kernel logs
  download-models  Download missing models
  connect          Connect to a remote Tylluan instance via SSE endpoint
  audit            Audit chain verification (tamper detection)
  install          Generate a tylluan.toml for an installation profile
  update           Check for updates and auto-update the Tylluan binary
  backup           Hot-consistent backup of the hub databases (kernel may keep running)
  restore          Restore from a `backup` directory (refuses to run while the kernel is up)
  new              Create a new guild or other scaffold
  sandbox          Manage the sandbox capability policy
  resume           Retrieve the last session summary from memory
  eval             Run benchmarks or list past results
  export-examples  Export CoherenceGate decision examples to JSONL
```

---

## Troubleshooting

### "Connection refused"

1. Is the kernel running? `curl http://127.0.0.1:47004/health`
2. Did it exit right after starting? A kernel started without a terminal and with the stdio transport in its config shuts down on end-of-input; set `transport = ["http", "sse"]`.
3. Is the port correct? Default is 47004. Check `tylluan.toml` if you changed it.
4. Is the URL correct? SSE clients need `/sse`, HTTP Streamable needs `/messages`.

### "Unauthorized"

1. `dev_mode = false` requires a bearer token.
2. Find it: `cat .tylluan-token` (file in the kernel working directory).
3. Append `?token=...` to the SSE URL, or set the `Authorization: Bearer ...` header.

### Embedding model download fails

- The first boot with a semantic profile downloads the model, so it needs a stable internet connection; `portable` downloads nothing.
- To fetch models ahead of time: `tylluan-cli download-models`.
- Models are cached in the Hugging Face cache (`~/.cache/huggingface/hub`, or `$HF_HOME`) and in a `.fastembed_cache` directory relative to the kernel's working directory.

### Guild not responding

1. Are the guild dependencies installed? Check with `pip list | grep fastmcp`.
2. Is the guild enabled in `tylluan.toml`? Search for `enabled = false`.
3. Restart the kernel — guilds are detected at startup only.
4. Run `tylluan-cli doctor` for a diagnostic of guilds, storage and config.

### High CPU / memory

- Disable unused guilds in `tylluan.toml`.
- Run `tylluan-cli doctor`.
- Look at recent friction events: `sqlite3 data/audit.db "SELECT id, event_type, timestamp, description FROM friction_events ORDER BY id DESC LIMIT 10;"`

---

## Quick Reference

| Command | Purpose |
|---------|---------|
| `curl http://127.0.0.1:47004/health` | Health check (`warming_up` for a few seconds, then `ok`) |
| `curl http://127.0.0.1:47004/api/v1/silva/stats` | Memory graph stats |
| `tylluan-cli logs` | Stream kernel logs |
| `tylluan-cli doctor` | Full diagnostic scan |
| `sqlite3 data/audit.db ".tables"` | List the tables of the friction/audit database |
| `cargo test -p tylluan-kernel --lib` | Run kernel unit tests |
| `cargo test -p tylluan-link --lib` | Run tylluan-link unit tests |

---

## Security

- `host = "127.0.0.1"` — localhost only. Default. Safe.
- `dev_mode = true` — disables auth. Never on a shared network.
- **Never** combine `host = "0.0.0.0"` + `dev_mode = true` (LAN RCE).
- Federation peers authenticate with a bearer `auth_token`, and payloads are encrypted with Noise NK once the peer's public key is known (ChaCha20-Poly1305 keyed by the shared secret otherwise). Protect the token and the secret like passwords.

See [SECURITY.md](../../SECURITY.md) and [docs/concepts/SECURITY.md](../concepts/SECURITY.md) for the full threat model.
