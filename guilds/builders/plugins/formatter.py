"""
TylluanNexus Builder Guild — Code Formatter.

Provides tools for formatting code in multiple languages using
industry-standard formatters (Ruff, Prettier, Rustfmt, Gofmt).

Status: operational (degraded: requires mcp fastmcp installed)
Last verified: 2026-05-11
"""

import subprocess
import os
import sys

_MCP_AVAILABLE = False
try:
    from mcp.server.fastmcp import FastMCP
    from guilds.core import utils
    _MCP_AVAILABLE = True
except ImportError as e:
    _MCP_AVAILABLE = False
    print(f"⚠️ Formatter Guild: MCP dependencies not available: {e}", file=sys.stderr)

if _MCP_AVAILABLE:
    mcp = FastMCP("tylluan-formatter")
    
    @mcp.tool()
    async def format_code(path: str, lang: str) -> str:
        """Format a source code file.

        Args:
            path: Absolute path to the file.
            lang: Language of the file (python, javascript, typescript, rust, go).
        """
        if not os.path.exists(path):
            return f"❌ Error: File not found: {path}"

        cmd = []
        if lang.lower() == "python":
            cmd = ["ruff", "format", path]
        elif lang.lower() in ["javascript", "typescript", "json", "css"]:
            cmd = ["npx", "prettier", "--write", path]
        elif lang.lower() == "rust":
            cmd = ["rustfmt", path]
        elif lang.lower() == "go":
            cmd = ["gofmt", "-w", path]
        else:
            return f"❓ Error: Unsupported language: {lang}"

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            return f"✨ Successfully formatted {path} using {cmd[0]}.\n{result.stdout}"
        except subprocess.CalledProcessError as e:
            return f"❌ Formatting Error: {e.stderr or str(e)}"
        except FileNotFoundError:
            return f"❌ Error: {cmd[0]} not found in PATH."

    if __name__ == "__main__":
        utils.safe_mcp_run(mcp)
else:
    # Stub when MCP not available
    class StubMCP:
        @staticmethod
        def tool(func):
            return func
    
    mcp = StubMCP()
    
    async def format_code(path: str, lang: str) -> str:
        return "❌ Formatter guild unavailable. Install dependencies: pip install mcp fastmcp"
    
    if __name__ == "__main__":
        print("Formatter Guild: MCP not available. Install: pip install mcp fastmcp")