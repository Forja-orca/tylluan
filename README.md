<p align="center">
  <img src="assets/branding/logo.jpg" alt="Tylluan" width="140">
</p>

<h1 align="center">Tylluan</h1>

<p align="center">
  <strong>Local memory, a knowledge graph and guarded tool execution for AI agents.</strong><br>
  One kernel binary. Runs on your machine. Works with MCP clients.
</p>

<p align="center">
  <a href="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml"><img src="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="MIT License"></a>
  <img src="https://img.shields.io/badge/version-0.17.2-blue.svg" alt="v0.17.2">
  <img src="https://img.shields.io/badge/MCP-native-purple.svg" alt="MCP native">
</p>

<p align="center">
  <strong>English</strong> · <a href="README.es.md">Español</a> · <a href="README.zh.md">中文</a> <sub>(pending native review)</sub>
</p>

---

Tylluan is a local server that your AI tools connect to over [MCP](https://modelcontextprotocol.io) or plain REST. It gives them three things they do not have on their own:

- **Memory that outlives the session.** Stored in a SQLite file on your disk and searched by keyword and, with an embedding profile, by vector similarity and a knowledge graph.
- **Tools that actually run.** Shell, git, files, Docker, web search, databases and more, through [45 "guilds"](guilds/README.md) (separate tool programs). `tylluan_do` has a plan (dry-run) mode, and calls made through the MCP tools are audited.
- **One shared place for several agents.** Every client you connect reads and writes the same memory and can talk in shared channels.

You bring the model and the client (Claude Code, Cursor, VS Code, …). Tylluan brings no language model of its own; its embedding models (optional) run locally. It is not a chat app and not an agent framework.

> **Research software.** Tylluan runs real commands on your machine and has had no independent third-party security audit. Read [Security](#security-what-it-protects-and-what-it-does-not) and [DISCLAIMER.md](DISCLAIMER.md) before you put anything sensitive near it.

<p align="center">
  <img src="docs/assets/how-it-fits.svg" alt="Process map: MCP clients reach the HTTP transport of one Rust process, tylluan-nexus, on 127.0.0.1 port 47004; REST and A2A clients enter the same transport. Every request passes auth, trust and grants, then the six sovereign tools. Tools route to the guild registry, which talks to 45 Python guild processes over stdio JSON-RPC, and read and write SilvaDB (SQLite with FTS5, graph and vectors). The judge model runs in llama-server through the llama_backend guild and is opt-in. Federation with other approved Tylluan nodes goes through the mesh over Noise." width="100%">
</p>

## Install

| Platform | Release binary |
|----------|----------------|
| Linux x86_64 | `tylluan-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `tylluan-aarch64-unknown-linux-gnu.tar.gz` |
| macOS, Apple Silicon | `tylluan-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `tylluan-x86_64-pc-windows-msvc.tar.gz` |

There is no macOS Intel build. Binaries are on the [releases page](https://github.com/forja-orca/tylluan/releases).

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex
```

This puts `tylluan-nexus` (the kernel) and `tylluan-cli` in `~/.tylluan/bin/` and adds them to your `PATH`. Open a new terminal afterwards.

The kernel needs no Rust, Python or Node. The 45 guilds behind `tylluan_do` are Python programs and need **Python 3.12 and FastMCP** installed separately (`pip install -r guilds/requirements.txt`; the file ships in the release archive and in this repository); without them those guilds fail to start and `/api/v1/doctor` reports `degraded`. Memory and recall work without Python.

To build from source, see [docs/getting-started/QUICKSTART.md](docs/getting-started/QUICKSTART.md).

## Start and connect

```bash
tylluan-cli start
curl -s http://127.0.0.1:47004/health
```

Run `tylluan-cli install` first: it writes the configuration. Starting without one makes the first start load the default embedding model, which took 52 s in our test, longer than the 30 s the CLI waits, and the CLI then stops the kernel. The kernel listens on `127.0.0.1:47004`. Authentication is on by default: on first start it creates a bearer token in a file named `.tylluan-token` **in the directory the kernel was started from**. Keep that file private; whoever holds the token can run tools on your machine.

Connect a client by sending the token in an `Authorization` header. For Claude Code:

```bash
claude mcp add --transport http tylluan http://127.0.0.1:47004/messages \
  --header "Authorization: Bearer $(cat .tylluan-token)"
```

On Windows PowerShell:

```powershell
claude mcp add --transport http tylluan http://127.0.0.1:47004/messages --header "Authorization: Bearer $(Get-Content .tylluan-token)"
```

Other clients take the same URL and header. For Cursor, VS Code and Claude Desktop see [integrations/](integrations/) for sample configs and [docs/guides/DEV_MODE_OFF_GUIDE.md](docs/guides/DEV_MODE_OFF_GUIDE.md) for where each client takes the token. Use `127.0.0.1`, not `localhost`: on Windows `localhost` can resolve to IPv6 first and miss the kernel.

## Try it

```bash
TOKEN=$(cat .tylluan-token)

# Store a memory
curl -X POST http://127.0.0.1:47004/api/v1/memory/write \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"content": "Tylluan keeps my notes in a local SQLite file."}'

# Retrieve it (the default `portable` profile matches keywords only, and every word must appear)
curl "http://127.0.0.1:47004/api/v1/memory/search?q=notes" \
  -H "Authorization: Bearer $TOKEN"
```

From an MCP client you do not call these endpoints. You just ask your agent to remember or recall something, or to run a task ("show me the last 10 commits in this repo"), and it uses the tools below.

## The six tools

With the default configuration of the current `main` branch, every connected client sees exactly these six tools, however many guilds run underneath. The published v0.17.1 release exposes five: `tylluan_decide` arrived after that tag.

| Tool | What it does |
|------|--------------|
| `tylluan_do` | Runs a task described in natural language by routing it to a guild |
| `tylluan_remember` | Stores knowledge |
| `tylluan_recall` | Searches long-term memory (keyword; vector and graph with an embedding profile) |
| `tylluan_think` | Reasons over the knowledge graph |
| `tylluan_graph` | Direct graph operations: triples, paths, PageRank |
| `tylluan_decide` | Answers a bounded question (choice, score or yes/no) or abstains. Today it applies deterministic rules and logs every decision; by default no model is connected to it |

<p align="center">
  <img src="docs/assets/memory-flow.svg" alt="Write path: tylluan_remember passes the L1 injection-pattern filter, is embedded with mxbai-embed-large (1024 dimensions, ONNX) and stored in SilvaDB; an asynchronous L2 judge classifies the new node as SAFE or SUSPICIOUS and sets a quarantine flag. Recall path: tylluan_recall picks a retrieval tier (fast, balanced or deep), runs FTS5 BM25 plus cosine search fused with RRF at k equal to 60, then the coherence gate (L1 patterns, L2 provenance, L3 cosine below 0.85, optional L4 judge) filters candidates before ranked memories and a gate-stats header return to the agent." width="100%">
</p>

## Profiles

One kernel binary, three configurations that trade retrieval quality for footprint.

| Profile | Embeddings | Extra model download | Measured |
|---------|-----------|----------|----------|
| `portable` (default install profile) | none, BM25 keyword search only | 0 MB | Windows start time and memory, below |
| `clinic` | BGE-Small, 384 dimensions | about 67 MB | not measured |
| `server` | mxbai-embed-large, 1024 dimensions | not verified | not measured |

`portable` was measured on 2026-10-26 with the published v0.17.1 Windows x86_64 release (a 15.4 MB download), four cold starts with an empty database: healthy after 12.8 to 16.8 s, 38.7 to 40 MB resident, and no outbound network connection or non-local UDP socket in the 10 seconds after it became healthy. That machine was not idle (a production kernel and development tools were running), so read the start time as an upper bound. The script is [scripts/measure_portable_start.ps1](scripts/measure_portable_start.ps1); the machine and the raw runs are in [portable_start_2026-10-26.json](docs/reference/measurements/portable_start_2026-10-26.json). Linux, macOS, ARM and a physical Raspberry Pi have **not** been measured; CI compiles the ARM64 build but does not boot it.

To change profile, stop the kernel first, then run `tylluan-cli install --profile server --force`. It overwrites `tylluan.toml` and starts a kernel on the default port, so do not leave another kernel running. `tylluan-cli start --setup` can pick a profile from your RAM and GPU.

## Why Tylluan, and why not

**Why**

- Your memory is a SQLite file you can open with any SQLite tool. No account, no API key, no service to keep alive.
- One compiled kernel binary, MCP-native, shared by every client you connect.
- Local by default: embeddings run in-process with ONNX Runtime, and routing a task to a guild uses local embeddings (keyword scoring in the `portable` profile), not a cloud model.

**Why not**

- Mem0, Letta, Zep, Cognee and others have years of production use and larger communities. Tylluan is young; v0.17.2 is its latest release.
- It is not a hosted service. There is no team dashboard, no managed backup and no support. A React dashboard exists in `dashboard/`, but the published binaries do not embed it: build from source with `--features bundled-dashboard` to serve it from the kernel.
- It is not hardened for hostile networks. Keep it on `127.0.0.1`.
- Several guilds need Python 3.12, and some (web search, for example) reach the internet when you ask them to.

**Retrieval quality.** On LongMemEval-S (50 questions) with the BGE-M3 model, Tylluan scored Recall@5 82%, Recall@10 90%, median latency 12.9 ms on CPU ([raw results](benchmarks/longmemeval_v0.12.0.json)). That was v0.12.0 on 2026-07-05, before the default model changed to `mxbai-embed-large`. It has not been re-run, it is a small set, and it is not a comparison against other systems.

## Security: what it protects and what it does not

- **Network.** By default the kernel binds to `127.0.0.1` only. Never combine `host = "0.0.0.0"` with `dev_mode = true`, which would be an unauthenticated remote shell on your network; if you do, the kernel logs a critical warning and forces the host back to `127.0.0.1`.
- **Authentication.** On by default (`dev_mode = false`). Every API and MCP call needs the bearer token, except a short public list: the health check, discovery, the dashboard's static files, and four endpoints under `/api/` (federation ping, federation sync send and receive, and the MCP probe). The exact list is `is_public_bypass_path` in `crates/tylluan-kernel/src/transport/http/auth.rs`.
- **Requests from web pages.** A page open in your browser could otherwise talk to the kernel on `127.0.0.1`. From v0.17.2 on the five MCP and SSE routes (`/messages`, `/mcp`, `/api/v1/mcp`, `/sse`, `/sse/download`) and every non-GET, non-HEAD route under `/api/v1/` reject requests with a non-local `Origin` or `Host` with 403, and the MCP endpoint requires `Content-Type: application/json`. If you run v0.17.1, keep `dev_mode = false`: a web page does not have your token, so it cannot call an authenticated kernel.
- **Memory poisoning.** A coherence gate filters what `tylluan_recall` returns, with deterministic pattern, provenance and drift checks. An optional model-based classifier for ambiguous cases is off by default; when you enable it, it can quarantine or down-weight a memory. The plain REST search endpoint is not behind this gate yet.
- **Tools.** Guilds run as normal operating-system processes with your user's rights. An opt-in Docker sandbox (`[security.sandbox]` in `tylluan.toml`, needs Docker) can isolate the bash and code guilds, or every guild in its `strict` profile. It is off by default, so out of the box a guild can do anything you can do. Tool calls made through the MCP tools are written to a hash-chained audit log; the direct REST endpoint for calling a guild is not audited yet, and an audit write that fails is logged as a warning rather than stopping the call.
- **Not covered.** No independent third-party security audit. Federation is meant for a local network or your own VPN, not the open internet.

How to report a vulnerability: [SECURITY.md](SECURITY.md). Threat model: [docs/concepts/SECURITY.md](docs/concepts/SECURITY.md).

## Federation

Two or more Tylluan instances can sync memories with peers you approve one by one: encrypted with Noise, with the source of every node recorded, and with per-peer and per-space permissions that deny by default. It is designed for a local network or a private VPN. It has not been tested over the open internet and no release ships a public bootstrap node. See [docs/concepts/FEDERATION.md](docs/concepts/FEDERATION.md).

## Stop, your data, uninstall

- **Stop:** `tylluan-cli stop`. With authentication on it needs the token: pass `--token`. If more than one kernel runs, also pass `--port` (or `--data-dir`) so it stops the right one.
- **Your data:** the SQLite database is `./data/tylluan.db` relative to the directory the kernel was started from, with the rest of its files in `data/` there. Back it up by copying that folder while the kernel is stopped.
- **Uninstall:** delete `~/.tylluan/` and remove the `PATH` entry the installer added. If you lose the token, delete `.tylluan-token` and restart: a new one is generated, and every client must be given it again.

## Status

1496 tests across Rust kernel (lib), `tylluan-link` and `tylluan-fsrs` pass; CI runs them on every push together with clippy, `cargo-deny`, Python lint and the security audit suite.

- **Latest release:** v0.17.2 (2026-10-27). Work since then lives on `main`; see [CHANGELOG.md](CHANGELOG.md).
- **What is planned and what is not:** [ROADMAP.md](ROADMAP.md). Current state of every claim: [STATUS.md](STATUS.md).
- **Platforms:** Linux x86_64 and ARM64, macOS Apple Silicon, Windows x86_64. CI runs the tests on Linux x86_64, and the release workflow builds all four binaries. The start-up measurement above was taken on Windows x86_64. The macOS and ARM64 builds have not been run on hardware by the maintainers (not verified).

## Documentation

| | |
|---|---|
| [Quickstart](docs/getting-started/QUICKSTART.md) | Configuration, authentication, troubleshooting, building from source |
| [Architecture](docs/concepts/ARCHITECTURE.md) | How the pieces fit |
| [Guilds](guilds/README.md) | The 45 tool programs behind `tylluan_do` |
| [Architecture decisions](docs/reference/adr/) | Why things are the way they are |
| [Contributing](CONTRIBUTING.md) | How to help |

## License

[MIT](LICENSE).
