# Adding a Guild to Tylluan

Guilds are Tylluan's extension point: each guild is a small Python FastMCP server that the Rust kernel starts on demand or keeps always on. This guide uses a minimal word-count guild so every step can be copied and adapted.

## 1. Create the guild file

Put the file in a category directory. For a builder-style plugin, use:

```text
guilds/builders/plugins/word_count.py
```

The directory determines the catalog category (`builders` → `Builder`). The file must expose a `FastMCP` instance and use the repository's safe runner so stdout remains available for MCP JSON-RPC:

```python
"""Word-count guild.

Use for: count words, count tokens, measure text length, word statistics.
"""
from mcp.server.fastmcp import FastMCP

from guilds.builders.plugins import utils

mcp = FastMCP("word_count")


@mcp.tool()
def word_count(text: str) -> str:
    """Return the number of whitespace-separated words in text."""
    words = text.split()
    return f"{len(words)} words"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
```

Important details:

- Keep the module-level `FastMCP(...)` declaration. The catalog scanner uses it to recognize a real guild.
- Put routing examples after `Use for:` in the module docstring. The scanner extracts comma-separated phrases as `trigger_phrases`.
- Use `@mcp.tool()` for every function intended to be callable through the guild.
- Keep logs and diagnostics on stderr. Do not print to stdout before `safe_mcp_run(mcp)` starts the MCP server.
- Validate inputs and avoid embedding secrets or machine-specific absolute paths in the guild.

The example is intentionally not added to the repository as a live guild. If you add it as `guilds/builders/plugins/word_count.py`, complete the registration steps below so the catalog and runtime registry tests remain accurate.

## 2. How discovery and registration work

Tylluan has two related registration layers:

1. **Catalog discovery** — `crates/tylluan-kernel/src/router/catalog.rs` scans Python files below `guilds/`, detects `FastMCP(...)`, derives the module path and category, and extracts `Use for:` phrases. A normal guild does **not** need a hand-written `GuildDescriptor`.
2. **Runtime reachability** — `crates/tylluan-kernel/src/registry/guild_list.rs` contains `LAZY_GUILDS`, the on-demand registrations used by `tylluan_do`. A guild can also be listed in `tylluan.toml`'s `[guilds.core] always_on` configuration when it must run continuously.

For the `word_count` example, add the runtime entry:

```rust
("word_count", "guilds.builders.plugins.word_count", false),
```

The third field indicates whether the guild is CPU-bound under the current registry contract. Use `true` only when the guild should receive the CPU-bound scheduling behavior used by existing entries.

### When to edit `catalog.rs`

Edit `catalog.rs` only when the automatic values are not sufficient:

- Add a `description_override()` arm when the generated name-based description is too vague for semantic routing.
- Add a `guild_overrides()` entry when the guild needs a non-default timeout weight or required arguments.
- Add the guild name to the `KNOWN_GUILDS` test list when you add a real Python MCP file. This is an executable anti-regression inventory, not the runtime registry itself.
- Do not duplicate the whole `GuildDescriptor` by hand for a normally auto-discovered Python guild.

The category comes from the directory (`core`, `builders`, `scholars`, `watchers`). The module path comes from the directory and filename, so `guilds/builders/plugins/word_count.py` becomes `guilds.builders.plugins.word_count`.

## 3. Test the guild

Run the kernel catalog and Python checks from the repository root:

```bash
cargo test -p tylluan-kernel --lib
python -m pytest tests/python/ --ignore=tests/python/test_memory_bridge_e2e.py -q
```

The Rust suite checks that the guild is discoverable, has a unique name, has a description, has a valid module path, appears in the known-guild inventory, and is reachable through either `LAZY_GUILDS` or the always-on configuration. The Python suite checks the surrounding guild tooling.

For a quick syntax/import check before starting the kernel, run:

```bash
python -m py_compile guilds/builders/plugins/word_count.py
```

## 4. Invoke it through `tylluan_do`

Start Tylluan first. The default local endpoint is `http://127.0.0.1:4000`; use the bearer token when authentication is enabled.

The REST intent endpoint can force the guild while keeping the public `tylluan_do` contract:

```bash
curl -X POST http://127.0.0.1:4000/api/v1/do \
  -H "Content-Type: application/json" \
  -d '{"intent":"count the words in this text","guild":"word_count","agent_id":"guild-tutorial","arguments":{"text":"one two three"}}'
```

You can also invoke the same public tool through MCP Streamable HTTP:

```bash
curl -X POST http://127.0.0.1:4000/messages \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2026-07-28" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"tylluan_do","arguments":{"intent":"count the words in this text","guild":"word_count","agent_id":"guild-tutorial","text":"one two three"}}}'
```

If the request is not forced to a guild, make the natural-language intent specific enough for the router, for example:

```text
count the words in this text using the word_count guild
```

Check the result and the runtime registry if routing fails:

```bash
curl http://127.0.0.1:4000/health
curl http://127.0.0.1:4000/api/v1/guilds
```

If the catalog contains the guild but `tylluan_do` reports `Unknown guild`, the usual cause is a missing `LAZY_GUILDS`/always-on registration entry, not a missing `GuildDescriptor`.

## 5. Contributor checklist

- [ ] The file contains `FastMCP(...)`, at least one `@mcp.tool()`, and the `safe_mcp_run` entry point.
- [ ] The module docstring has useful `Use for:` phrases.
- [ ] The runtime module path is in `LAZY_GUILDS` or the guild is intentionally always-on.
- [ ] `KNOWN_GUILDS` is updated if the file is a real repository guild.
- [ ] `description_override`/`guild_overrides` are added only when automatic discovery is insufficient.
- [ ] `cargo test -p tylluan-kernel --lib` passes.
- [ ] The Python tests and `py_compile` check pass.
- [ ] Manual `tylluan_do` invocation succeeds without leaking secrets or writing protocol data to stdout.
