# Contributing to Tylluan

Thank you for your interest in contributing. Tylluan is built by humans and AI agents working together — contributions from both are welcome.

## Prerequisites

- Rust 1.75+ (`rustup` recommended)
- Python 3.11+ with pip
- Node.js 18+ (for dashboard development only)
- Git

## How to contribute

1. Fork the repository
2. Create a branch (`git checkout -b feat/your-feature`)
3. Make your changes with tests
4. Run `cargo check -p tylluan-kernel` and `cargo test -p tylluan-kernel --lib`
5. Run `cargo clippy -p tylluan-kernel -- -D warnings`
6. Submit a pull request with a clear description

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

- The kernel exposes exactly 5 sovereign MCP tools: `tylluan_do`, `tylluan_recall`, `tylluan_remember`, `tylluan_think`, `tylluan_graph`
- Guilds are Python subprocesses using `fastmcp` — each is self-contained
- No cloud dependencies in the critical path
- Always use `127.0.0.1`, never `localhost` (IPv6 trap on Windows)
- Never ship `host = "0.0.0.0"` with `dev_mode = true` (LAN RCE risk)

## Adding a guild

1. Create a Python file in `guilds/` using `fastmcp`
2. Register it in `crates/tylluan-kernel/src/router/catalog.rs`
3. Add trigger phrases and a short description (5-8 words)
4. Test: `cargo test -p tylluan-kernel --lib`

## Reporting issues

- Include: what you did, what you expected, what happened
- Include kernel version (`curl http://127.0.0.1:3030/health`)
- If possible, include the routing trace from `tylluan_do`

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
