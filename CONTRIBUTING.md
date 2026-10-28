# Contributing to Tylluan

<!-- RUTAS-DE-LECTURA:BEGIN (bloque identico en todos los ficheros agenticos; lo vigila scripts/check_read_routes.sh) -->
## Rutas de lectura (orden obligatorio, rutas desde la raiz del repo)

| Puesto | Fichero | Que te da | Quien |
|---|---|---|---|
| 1 | tu fichero de entrada: `CLAUDE.md`, `AGENTS.md` o `.agents/AGENTS.md` | reglas de tu runtime | Claude Code lee `CLAUDE.md`; Antigravity lee `.agents/AGENTS.md`; cualquier otro agente (Codex, OpenCode, Deep, Mimo, DSH, Hermes, Qwen) lee `AGENTS.md` |
| 2 | `STATUS.md` | estado real verificado: que funciona hoy | todos |
| 3 | `REVIEW.md` | como se revisa y que cuenta como hallazgo, antes de tocar o entregar nada | todos |
| 4 | `docs/internal/WORK_PROTOCOL.md` | ciclo de trabajo, entregas y reglas de flota | todos |
| 5 | segun la tarea: `SPEC.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `SECURITY.md`, `docs/reference/adr/ARCHITECTURE_DECISIONS.md` | contrato, plan, contribucion, seguridad, decisiones | quien lo necesite |

`REVIEW.md` es siempre el tercer fichero que se lee. `CLAUDE.md` lo importa con una linea `@REVIEW.md`; el resto entra por esta tabla.
<!-- RUTAS-DE-LECTURA:END -->

Thank you for your interest in contributing. Tylluan is built by humans and AI agents working together — contributions from both are welcome.

## Prerequisites

- Rust 1.88+ minimum (`rust-toolchain.toml` pins 1.99.1; `rustup` recommended)
- Python 3.12+ with pip
- Node.js 20+ (for dashboard development only)
- Git

## How to contribute

1. Fork the repository
2. Create a branch (`git checkout -b feat/your-feature`)
3. Make your changes with tests
4. Run `cargo check -p tylluan-kernel` and `cargo test -p tylluan-kernel --lib` (add `--features encryption` to verify SQLCipher build)
5. Run `cargo clippy -p tylluan-kernel -- -D warnings`
6. Submit a pull request with a clear description

## Your first PR in 30 minutes

A step-by-step guide to clone, build, verify, and open your first Pull Request with zero cloud dependencies or heavy model downloads:

### Minutes 0–5: Clone and prepare a clean branch
```bash
git clone https://github.com/forja-orca/tylluan.git
cd tylluan
git checkout -b feat/my-first-pr
cp tylluan.example.toml tylluan.toml
```

### Minutes 5–15: Build and run the lightweight unit test suite (`--lib`)
Verify that the kernel and link protocol compile and pass their unit tests without spawning external processes:
```bash
cargo check -p tylluan-kernel
cargo test -p tylluan-kernel --lib
cargo test -p tylluan-link --all-targets
```

### Minutes 15–25: Make a focused change with a test
Pick a small, self-contained improvement:
- **A Rust fix or unit test:** Add or update a unit test inside the `#[cfg(test)]` module of the file you touch under `crates/tylluan-kernel/src/` and run it in isolation:
  ```bash
  cargo test -p tylluan-kernel --lib your_test_name
  ```
- **A new Python guild:** Follow [`docs/guides/ADDING_A_GUILD.md`](docs/guides/ADDING_A_GUILD.md) — create the file in `guilds/`, register its descriptor in `crates/tylluan-kernel/src/router/catalog.rs` and its module path in `LAZY_GUILDS` (`crates/tylluan-kernel/src/registry/guild_list.rs`), then verify:
  ```bash
  cargo test -p tylluan-kernel --lib test_every_guild_file_is_in_catalog
  ```
- **A documentation fix:** If you update file paths, ports, or test counts in Markdown docs, run the documentation reality gates:
  ```bash
  bash scripts/check_docs_reality.sh
  python scripts/check_doc_links.py
  ```

### Minutes 25–30: Run Clippy and open the PR
CI enforces `clippy` with `-D warnings` (note: `cargo fmt --check` is not currently a CI gate and the existing tree has unformatted files, so format only the lines you touch rather than running a repo-wide `cargo fmt --check`):
```bash
cargo clippy -p tylluan-kernel -- -D warnings
```
If your commit touches `transport/`, `guilds/`, `integrations/`, or `tylluan*.toml`, include an `## Impact` section in the commit message explaining which flows or contracts are affected (checked by `scripts/check_no_predation.sh`). Open the Pull Request using the repository template.

## What to NEVER include in a pull request

- Personal data (names, emails, local usernames)
- Secrets, tokens, or API keys
- Absolute file paths (e.g., `C:\Users\...`, `/home/...`)
- Database files (`.db`, `.fjv1`)
- Model weights or cache files
- Screenshots or logs containing personal information

If your PR touches security-sensitive code (auth, guild execution, token handling), flag it in the description.

## Code style

- Rust: `cargo fmt` + `cargo clippy -- -D warnings`
- Python: PEP 8, type hints where practical
- TypeScript: Prettier + ESLint
- Comments: only when the WHY is not obvious

## Architecture rules

- The kernel exposes exactly 6 sovereign MCP tools: `tylluan_do`, `tylluan_recall`, `tylluan_remember`, `tylluan_think`, `tylluan_graph`, `tylluan_decide`
- Guilds are Python subprocesses using `fastmcp` — each is self-contained
- No cloud dependencies in the critical path
- Always use `127.0.0.1`, never `localhost` (IPv6 trap on Windows)
- Never ship `host = "0.0.0.0"` with `dev_mode = true` (LAN RCE risk)

## Adding a guild

Follow the complete [Adding a Guild guide](docs/guides/ADDING_A_GUILD.md) for a copy-paste scaffold, registration details, tests, and a manual `tylluan_do` invocation.

1. Create a Python file in `guilds/` using `FastMCP`, `@mcp.tool()`, and `utils.safe_mcp_run(mcp)`.
2. Register it: add its descriptor to `router/catalog.rs` and its module path to `LAZY_GUILDS` (or configure it as always-on). Auto-discovery alone leaves it unroutable ("Unknown guild").
3. Add useful `Use for:` trigger phrases and a short description.
4. Test: `cargo test -p tylluan-kernel --lib`.

## Reporting issues

- Include: what you did, what you expected, what happened
- Include kernel version (`curl http://127.0.0.1:47004/health`)
- If possible, include the routing trace from `tylluan_do`

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
