import logging
import os
import sys
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-biome-warden")


def _biome_cmd() -> str:
    local = Path("node_modules/.bin/biome")
    if local.exists():
        return str(local.resolve())
    return "npx @biomejs/biome"


@mcp.tool()
async def biome_format_file(file_path: str) -> str:
    """Format a file instantly using BiomeJS."""
    fp = Path(file_path).resolve()
    if not fp.exists():
        return f"File not found: {file_path}"
    rc, out, err = await utils.run_command([_biome_cmd(), "format", "--write", str(fp)], timeout_secs=30)
    if rc != 0:
        return f"Biome format failed on {fp.name}.\n{err}\n{out}"
    return f"Biome format successful for {fp.name}.\n{out}"


@mcp.tool()
async def biome_lint_file(file_path: str) -> str:
    """Lint and apply safe automatic fixes to a file using BiomeJS."""
    fp = Path(file_path).resolve()
    if not fp.exists():
        return f"File not found: {file_path}"
    rc, out, err = await utils.run_command([_biome_cmd(), "check", "--write", str(fp)], timeout_secs=30)
    if rc != 0:
        return f"Biome lint/check found unresolved issues in {fp.name}.\n{err}\n{out}"
    return f"Biome lint/check successful for {fp.name}.\n{out}"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
