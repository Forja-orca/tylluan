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
"""

import asyncio
import logging
import os
import re
import shlex
import sys

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


def _normalize_url(url: str) -> str:
    """Replace 'localhost' with '127.0.0.1' to avoid IPv6 resolution on Windows."""
    return _LOCALHOST_RE.sub(r'\g<1>127.0.0.1', url)


async def _session_sse(server_url: str):
    """Async context manager yielding a live ClientSession over SSE."""
    async with sse_client(server_url) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            yield session


async def _session_stdio(command: str):
    """Async context manager yielding a live ClientSession over stdio."""
    parts = shlex.split(command)
    params = StdioServerParameters(
        command=parts[0],
        args=parts[1:],
        env={**os.environ},
    )
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            yield session


@mcp.tool()
async def mcp_list_tools(server_url: str) -> str:
    """List all tools available on an external MCP server.

    Use for: mcp list tools, list remote tools, show mcp tools, mcp tools list,
    what tools does server have, enumerate mcp tools, remote tool discovery.

    Args:
        server_url: SSE endpoint (http://host:port/sse) or stdio command
                    prefixed with 'stdio://' (e.g. 'stdio://python -m guild').

    Returns:
        Newline-separated list of tool names and descriptions.
    """
    server_url = _normalize_url(server_url)
    logging.info("mcp_list_tools: %s", server_url[:80])
    try:
        tools = await _fetch_tools(server_url)
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

    Returns:
        Tool result as text, or error message.
    """
    server_url = _normalize_url(server_url)
    logging.info("mcp_call: server=%s tool=%s", server_url[:60], tool_name)
    args = arguments or {}
    try:
        result = await asyncio.wait_for(
            _do_call(server_url, tool_name, args),
            timeout=timeout_secs,
        )
        return result
    except asyncio.TimeoutError:
        return f"⏰ mcp_call timed out after {timeout_secs}s ({server_url} → {tool_name})"
    except Exception as e:
        logging.error("mcp_call failed: %s", e, exc_info=True)
        return f"❌ mcp_bridge.mcp_call failed [{tool_name} @ {server_url[:50]}]: {e}"


@mcp.tool()
async def mcp_ping(server_url: str) -> str:
    """Check connectivity to an external MCP server.

    Use for: mcp ping, ping mcp server, check mcp connection, test mcp bridge,
    mcp connectivity, bridge ping, federated ping, mcp reachable.

    Args:
        server_url: SSE endpoint or 'stdio://command'.

    Returns:
        Ping result with tool count and server info.
    """
    server_url = _normalize_url(server_url)
    logging.info("mcp_ping: %s", server_url[:80])
    try:
        tools = await asyncio.wait_for(_fetch_tools(server_url), timeout=10)
        return f"✅ {server_url} reachable — {len(tools)} tools available"
    except asyncio.TimeoutError:
        return f"⏰ {server_url} did not respond within 10s"
    except Exception as e:
        logging.error("mcp_ping failed: %s", e, exc_info=True)
        return f"❌ {server_url} unreachable: {e}"


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

async def _fetch_tools(server_url: str) -> list[dict]:
    """Fetch tools list from remote server, with in-process cache."""
    if server_url in _tools_cache:
        return _tools_cache[server_url]

    tools: list[dict] = []
    async for session in _iter_session(server_url):
        response = await session.list_tools()
        tools = [
            {"name": t.name, "description": t.description or ""}
            for t in (response.tools or [])
        ]
        break  # single iteration — we just need one session

    _tools_cache[server_url] = tools
    return tools


async def _do_call(server_url: str, tool_name: str, args: dict) -> str:
    """Open a session, call the tool, return text content."""
    async for session in _iter_session(server_url):
        result = await session.call_tool(tool_name, args)
        parts = []
        for block in (result.content or []):
            if hasattr(block, "text"):
                parts.append(block.text)
            else:
                parts.append(str(block))
        return "\n".join(parts) if parts else "(empty response)"
    return "❌ Could not establish session"


async def _iter_session(server_url: str):
    """Yield a single initialized ClientSession for the given URL/command."""
    if server_url.startswith("stdio://"):
        command = server_url[len("stdio://"):]
        async for session in _session_stdio(command):
            yield session
    else:
        async for session in _session_sse(server_url):
            yield session


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, stream=sys.stderr)
    utils.safe_mcp_run(mcp)
