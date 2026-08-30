#!/usr/bin/env python3
"""Verifies a guild runs for real, isolated, using ONLY a given portable
Python interpreter -- no fallback to whatever's on the host's PATH.

Portability point 4 (2026-08-29): Antigravity prototyped guilds-python/
manually via WSL and one-off throwaway scripts (deleted after use, per
their own report). This is the permanent, CI-runnable equivalent, so the
same proof doesn't need to be re-invented by hand every time the portable
build changes.

What "isolated" means here, concretely:
  1. sys.executable/sys.prefix inside the guild process point INTO the
     given portable interpreter's own tree, not the host's system Python.
  2. The guild boots as a real MCP server over stdio and answers a real
     tool call -- not just "python -c 'import mcp'", the actual guild
     entrypoint, actually serving a request.

Usage:
    <portable-python> scripts/verify_portable_guild.py <portable-python-path>

Exits 0 on success, 1 with a clear reason otherwise.
"""
import asyncio
import os
import sys
from pathlib import Path

# Self-heal the console encoding instead of requiring every caller to set
# PYTHONIOENCODING=utf-8 first. Real gap found by Buffy 2026-08-30 building
# the Windows portable prototype: this script's own checkmarks (U+2705)
# crash with UnicodeEncodeError on Windows' default cp1252 console before a
# single check even runs. reconfigure() is Python 3.7+; harmless no-op if
# the stream doesn't support it (e.g. already redirected to a file as utf-8).
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8")

REPO_ROOT = Path(__file__).resolve().parent.parent


def _fail(msg: str) -> None:
    print(f"❌ {msg}")
    sys.exit(1)


async def main() -> None:
    if len(sys.argv) != 2:
        _fail("usage: verify_portable_guild.py <portable-python-path>")
    portable_python = Path(sys.argv[1]).resolve()
    if not portable_python.exists():
        _fail(f"portable python not found at {portable_python}")

    # ── Check 1: this script's own interpreter must BE the portable one ──
    # (the caller is expected to invoke us with that interpreter directly --
    # this just confirms the invocation was done right, not by accident
    # with the host's python on PATH).
    if Path(sys.executable).resolve() != portable_python:
        _fail(
            f"running under {sys.executable}, expected {portable_python} -- "
            "invoke this script WITH the portable interpreter, not the host's."
        )
    print(f"✅ sys.executable is the portable interpreter: {sys.executable}")

    # ── Check 2: sys.prefix must be inside the portable tree, not a host
    # system path -- catches the "isolated in name only" failure mode. ──
    prefix = Path(sys.prefix).resolve()
    guilds_python_root = REPO_ROOT / "guilds-python"
    try:
        prefix.relative_to(guilds_python_root)
    except ValueError:
        _fail(f"sys.prefix ({prefix}) is not under guilds-python/ -- not isolated")
    print(f"✅ sys.prefix is inside guilds-python/: {prefix}")

    for bad in ("/usr/lib", "/usr/local/lib", "/usr/bin", str(Path.home() / ".local")):
        if any(str(Path(p).resolve()).startswith(bad) for p in sys.path if p):
            _fail(f"sys.path leaks a host system path matching '{bad}': {sys.path}")
    print("✅ sys.path contains no host system paths")

    # ── Check 3: actually boot a real guild over stdio and call a real tool. ──
    from mcp import ClientSession
    from mcp.client.stdio import stdio_client, StdioServerParameters

    guild_script = REPO_ROOT / "guilds" / "builders" / "plugins" / "filesystem.py"
    if not guild_script.exists():
        _fail(f"guild script not found: {guild_script}")

    params = StdioServerParameters(
        command=str(portable_python),
        args=[str(guild_script)],
        env={**os.environ, "PYTHONPATH": str(REPO_ROOT), "PYTHONNOUSERSITE": "1"},
        cwd=str(REPO_ROOT),
    )
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            tools = await session.list_tools()
            tool_names = {t.name for t in tools.tools}
            if "file_list" not in tool_names:
                _fail(f"expected tool 'file_list' not found, got: {tool_names}")
            print(f"✅ guild booted over stdio with the portable interpreter, tools: {sorted(tool_names)}")

            result = await session.call_tool("file_list", {"directory": "scripts", "depth": 1})
            text = "".join(c.text for c in result.content if hasattr(c, "text"))
            if "verify_portable_guild.py" not in text:
                _fail(f"file_list did not see its own directory correctly: {text[:300]}")
            print("✅ real tool call succeeded and returned real filesystem content")

    print("✅ ALL CHECKS PASSED — guild runs isolated under the portable interpreter")


if __name__ == "__main__":
    asyncio.run(main())
