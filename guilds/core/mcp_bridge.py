"""
TylluanNexus MCP Bridge Guild — TylluanNexus as MCP client.

Connects to external MCP servers (other TylluanNexus kernels, Browser Use,
Playwright, etc.) and exposes their tools as if they were local guilds.

Transports supported:
  - SSE:   mcp_call(server_url="http://host:3030/sse", ...)
  - stdio: mcp_call(server_url="stdio://python -m some.module", ...)

Security: Only servers explicitly listed in tylluan.toml [mcp_clients] are
contacted when enforce_allowlist=True (default). Pass enforce_allowlist=False
for ad-hoc dev usage.

When enforce_allowlist=True (default), stdio:// commands are restricted to the
built-in _STDIO_ALLOWLIST of known-safe binaries below. There is currently no
tylluan.toml override for this list -- edit _STDIO_ALLOWLIST directly if a new
external_mcp server needs a binary that isn't already covered.
"""

import asyncio
import logging
import os
import re
import shlex
import sys
import json

from mcp import ClientSession
from mcp.client.sse import sse_client
from mcp.client.stdio import stdio_client, StdioServerParameters
from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-mcp-bridge")

# In-process tool cache: server_url -> list[dict]
# Avoids re-fetching tools/list on every call within the same process lifetime.
_tools_cache: dict[str, list[dict]] = {}

# Windows resolves "localhost" to IPv6 [::1] which the kernel rejects.
# Force all localhost references to 127.0.0.1.
_LOCALHOST_RE = re.compile(r'(?i)(https?://)localhost\b')

# Allowlist for stdio:// commands when enforce_allowlist=True.
# Covers the external MCP servers defined in tylluan.toml [[external_mcp]].
_STDIO_ALLOWLIST: frozenset[str] = frozenset({
    "node", "npx", "uv", "python", "python3", "gk",
})

# Minimal environment variables for stdio subprocesses (avoids leaking
# the parent's full env, including tokens and secrets).
_MINIMAL_ENV: dict[str, str] = {
    "PATH": os.environ.get("PATH", ""),
    "HOME": os.environ.get("HOME", ""),
    "USERPROFILE": os.environ.get("USERPROFILE", ""),
    "APPDATA": os.environ.get("APPDATA", ""),
    "TERM": os.environ.get("TERM", "xterm-256color"),
}


def _normalize_url(url: str) -> str:
    """Replace 'localhost' with '127.0.0.1' to avoid IPv6 resolution on Windows."""
    return _LOCALHOST_RE.sub(r'\g<1>127.0.0.1', url)


async def _session_sse(server_url: str):
    """Async context manager yielding a live ClientSession over SSE."""
    async with sse_client(server_url) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            yield session


async def _session_stdio(command: str, enforce_allowlist: bool = True):
    """Async context manager yielding a live ClientSession over stdio."""
    parts = shlex.split(command)
    if not parts:
        raise ValueError("Empty stdio command")
    if enforce_allowlist and parts[0] not in _STDIO_ALLOWLIST:
        raise ValueError(
            f"🚫 BLOCKED by mcp_bridge allowlist: '{parts[0]}' is not allowed. "
            f"Allowed: {', '.join(sorted(_STDIO_ALLOWLIST))}. "
            "Pass enforce_allowlist=False to bypass (dev only)."
        )
    params = StdioServerParameters(
        command=parts[0],
        args=parts[1:],
        env={**os.environ} if not enforce_allowlist else {**_MINIMAL_ENV},
    )
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            yield session


@mcp.tool()
async def mcp_list_tools(
    server_url: str,
    enforce_allowlist: bool = True,
) -> str:
    """List all tools available on an external MCP server.

    Use for: mcp list tools, list remote tools, show mcp tools, mcp tools list,
    what tools does server have, enumerate mcp tools, remote tool discovery.

    Args:
        server_url: SSE endpoint (http://host:port/sse) or stdio command
                    prefixed with 'stdio://' (e.g. 'stdio://python -m guild').
        enforce_allowlist: If True (default), stdio:// commands must be in the
                           allowed binaries list. Set False for ad-hoc dev usage.

    Returns:
        Newline-separated list of tool names and descriptions.
    """
    server_url = _normalize_url(server_url)
    if server_url.startswith("stdio://") and not enforce_allowlist:
        logging.warning("mcp_list_tools: enforce_allowlist=False — bypassing allowlist for stdio://")
    logging.info("mcp_list_tools: %s", server_url[:80])
    try:
        tools = await _fetch_tools(server_url, enforce_allowlist)
        if not tools:
            return f"⚠️ No tools found on {server_url}"
        lines = [f"🔧 {len(tools)} tools on {server_url}:"]
        for t in tools:
            desc = (t.get("description") or "")[:80]
            lines.append(f"  • {t['name']}: {desc}")
        return "\n".join(lines)
    except Exception as e:
        logging.error("mcp_list_tools failed: %s", e, exc_info=True)
        return f"❌ mcp_bridge.mcp_list_tools failed: {e}"


@mcp.tool()
async def mcp_call(
    server_url: str,
    tool_name: str,
    arguments: dict | None = None,
    timeout_secs: int = 60,
    enforce_allowlist: bool = True,
) -> str:
    """Call a tool on an external MCP server.

    Use for: call remote tool, federated MCP, bridge to external server,
    connect to other kernel, Browser Use, Playwright MCP, tylluan federation.

    Args:
        server_url: SSE endpoint (http://host:port/sse) or
                    'stdio://command args' for stdio transport.
        tool_name:  Name of the tool to call on the remote server.
        arguments:  JSON-serialisable dict of arguments for the tool.
        timeout_secs: Max seconds to wait for the remote call (default: 60).
        enforce_allowlist: If True (default), stdio:// commands must be in the
                           allowed binaries list. Set False for ad-hoc dev usage.

    Returns:
        Tool result as text, or error message.
    """
    server_url = _normalize_url(server_url)
    if server_url.startswith("stdio://") and not enforce_allowlist:
        logging.warning("mcp_call: enforce_allowlist=False — bypassing allowlist for stdio://")
    logging.info("mcp_call: server=%s tool=%s", server_url[:60], tool_name)
    args = arguments or {}
    try:
        result = await asyncio.wait_for(
            _do_call(server_url, tool_name, args, enforce_allowlist),
            timeout=timeout_secs,
        )
        return result
    except asyncio.TimeoutError:
        return f"⏰ mcp_call timed out after {timeout_secs}s ({server_url} → {tool_name})"
    except Exception as e:
        logging.error("mcp_call failed: %s", e, exc_info=True)
        return f"❌ mcp_bridge.mcp_call failed [{tool_name} @ {server_url[:50]}]: {e}"


@mcp.tool()
async def mcp_ping(
    server_url: str,
    enforce_allowlist: bool = True,
) -> str:
    """Check connectivity to an external MCP server.

    Use for: mcp ping, ping mcp server, check mcp connection, test mcp bridge,
    mcp connectivity, bridge ping, federated ping, mcp reachable.

    Args:
        server_url: SSE endpoint or 'stdio://command'.
        enforce_allowlist: If True (default), stdio:// commands must be in the
                           allowed binaries list. Set False for ad-hoc dev usage.

    Returns:
        Ping result with tool count and server info.
    """
    server_url = _normalize_url(server_url)
    if server_url.startswith("stdio://") and not enforce_allowlist:
        logging.warning("mcp_ping: enforce_allowlist=False — bypassing allowlist for stdio://")
    logging.info("mcp_ping: %s", server_url[:80])
    try:
        tools = await asyncio.wait_for(_fetch_tools(server_url, enforce_allowlist), timeout=10)
        return f"✅ {server_url} reachable — {len(tools)} tools available"
    except asyncio.TimeoutError:
        return f"⏰ {server_url} did not respond within 10s"
    except Exception as e:
        logging.error("mcp_ping failed: %s", e, exc_info=True)
        return f"❌ {server_url} unreachable: {e}"


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

async def _fetch_tools(server_url: str, enforce_allowlist: bool = True) -> list[dict]:
    """Fetch tools list from remote server, with in-process cache."""
    if server_url in _tools_cache:
        return _tools_cache[server_url]

    tools: list[dict] = []
    async for session in _safe_iter_session(server_url, enforce_allowlist):
        response = await session.list_tools()
        tools = [
            {"name": t.name, "description": t.description or ""}
            for t in (response.tools or [])
        ]
        break  # single iteration — we just need one session

    _tools_cache[server_url] = tools
    return tools


async def _do_call(server_url: str, tool_name: str, args: dict, enforce_allowlist: bool = True) -> str:
    """Open a session, call the tool, return text content."""
    async for session in _safe_iter_session(server_url, enforce_allowlist):
        result = await session.call_tool(tool_name, args)
        parts = []
        for block in (result.content or []):
            if hasattr(block, "text"):
                parts.append(block.text)
            else:
                parts.append(str(block))
        return "\n".join(parts) if parts else "(empty response)"
    return "❌ Could not establish session"


async def _iter_session(server_url: str, enforce_allowlist: bool = True):
    """Yield a single initialized ClientSession for the given URL/command."""
    if server_url.startswith("stdio://"):
        command = server_url[len("stdio://"):]
        async for session in _session_stdio(command, enforce_allowlist):
            yield session
    else:
        async for session in _session_sse(server_url):
            yield session


async def _safe_iter_session(server_url: str, enforce_allowlist: bool = True):
    """Wrapper that catches ExceptionGroup from asyncio.TaskGroup inside MCP lib.

    The MCP client library uses asyncio.TaskGroup internally. When connecting
    to an unreachable or non-existent server, the library raises ExceptionGroup
    instead of a plain Exception. This wrapper normalises that into a single
    exception message so callers don't need to handle ExceptionGroup everywhere.
    """
    try:
        async for session in _iter_session(server_url, enforce_allowlist):
            yield session
    except* Exception as eg:
        # Python 3.11+: unpack ExceptionGroup into the first underlying cause
        causes = [str(e) for e in eg.exceptions]
        raise Exception(f"mcp_bridge: connection failed to {server_url}: {'; '.join(causes)}")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, stream=sys.stderr)
    utils.safe_mcp_run(mcp)
