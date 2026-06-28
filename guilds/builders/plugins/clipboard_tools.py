import logging
import sys

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-clipboard-tools")


@mcp.tool()
async def clipboard_read() -> str:
    """Read the current contents of the system clipboard."""
    try:
        if sys.platform == "win32":
            rc, out, err = await utils.run_command(["powershell", "-command", "Get-Clipboard"], timeout_secs=5)
        elif sys.platform == "darwin":
            rc, out, err = await utils.run_command(["pbpaste"], timeout_secs=5)
        else:
            rc, out, err = await utils.run_command(["xclip", "-selection", "clipboard", "-o"], timeout_secs=5)
        if rc != 0:
            return f"Clipboard read failed: {err}"
        return out or "(empty clipboard)"
    except Exception as e:
        return f"Clipboard read failed: {e}"


@mcp.tool()
async def clipboard_write(text: str) -> str:
    """Write text to the system clipboard."""
    try:
        if sys.platform == "win32":
            escaped = text.replace("'", "''")
            rc, out, err = await utils.run_command(["powershell", "-command", f"Set-Clipboard -Value '{escaped}'"], timeout_secs=5)
        elif sys.platform == "darwin":
            import json
            rc, out, err = await utils.run_command(["bash", "-c", f"echo {json.dumps(text)} | pbcopy"], timeout_secs=5)
        else:
            import json
            rc, out, err = await utils.run_command(["bash", "-c", f"echo {json.dumps(text)} | xclip -selection clipboard"], timeout_secs=5)
        if rc != 0:
            return f"Clipboard write failed: {err}"
        return f"Written {len(text)} chars to clipboard."
    except Exception as e:
        return f"Clipboard write failed: {e}"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
