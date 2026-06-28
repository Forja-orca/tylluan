import logging
import os
import sys
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-screenshot-tools")

SCREENSHOTS_DIR = Path(os.environ.get("SCREENSHOTS_DIR") or "data/screenshots")


@mcp.tool()
async def screenshot_capture(filename: str = "") -> str:
    """Capture a screenshot of the current screen. Returns the file path and image data."""
    SCREENSHOTS_DIR.mkdir(parents=True, exist_ok=True)
    fname = filename or f"screenshot_{int(__import__('time').time() * 1000)}.png"
    output = str(SCREENSHOTS_DIR / fname)

    platform = sys.platform
    try:
        if platform == "win32":
            ps_cmd = (
                'Add-Type -AssemblyName System.Windows.Forms; '
                '$bmp = New-Object System.Drawing.Bitmap([System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width, '
                '[System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height); '
                '$g = [System.Drawing.Graphics]::FromImage($bmp); '
                '$g.CopyFromScreen([System.Drawing.Point]::Empty, [System.Drawing.Point]::Empty, $bmp.Size); '
                f'$bmp.Save("{output}"); $g.Dispose(); $bmp.Dispose()'
            )
            rc, out, err = await utils.run_command(["powershell", "-command", ps_cmd], timeout_secs=10)
        elif platform == "darwin":
            rc, out, err = await utils.run_command(["screencapture", "-x", output], timeout_secs=10)
        else:
            rc, out, err = await utils.run_command(["import", "-window", "root", output], timeout_secs=10)

        if rc != 0:
            return f"Screenshot failed: {err}"

        try:
            import base64
            data = Path(output).read_bytes()
            b64 = base64.b64encode(data).decode()
            return f'Screenshot: {output}\n![screenshot](data:image/png;base64,{b64})'
        except Exception:
            return f"Screenshot saved to: {output}"
    except Exception as e:
        return f"Screenshot failed: {e}"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
