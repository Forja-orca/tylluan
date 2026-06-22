"""
TylluanNexus Memory Guild — DEPRECATED (Duplicates)

In v3.0+, all memory operations (search, store, graph) are handled natively 
by the Rust kernel for maximum performance and sovereignty.

This guild is preserved only as a stub to avoid breaking legacy registrations,
but all tools have been migrated to the kernel.
"""

from mcp.server.fastmcp import FastMCP
from guilds.core import utils
from guilds.core.silva_utils import compress_for_storage

mcp = FastMCP("tylluan-memory")

@mcp.tool()
async def memory_status() -> str:
    """Check the status of the unified memory bridge."""
    return "✅ Unified Memory is active and managed natively by the TylluanNexus Kernel."

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
