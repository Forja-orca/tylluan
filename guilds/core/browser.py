"""
TylluanNexus Browser Guild — Local browser automation using Edge/Chrome CDP.

Uses the browser's remote debugging protocol to control the browser
without external dependencies (msedge/chrome come with Windows).

No external services required - all local.

Status: operational (degraded: requires mcp fastmcp installed)
Last verified: 2026-05-11
"""
# ⚠️ DEPRECATED — not registered in tylluan.toml. Keep for reference only.
# To reactivate: add to [guilds.core] lazy list in tylluan.toml

import asyncio
import json
import os
import re
import socket
import subprocess
import sys
from typing import Optional

_MCP_AVAILABLE = False
try:
    from mcp.server.fastmcp import FastMCP
    from guilds.core import utils
    _MCP_AVAILABLE = True
except ImportError as e:
    _MCP_AVAILABLE = False
    print(f"⚠️ Browser Guild: MCP dependencies not available: {e}", file=sys.stderr)

if _MCP_AVAILABLE:
    mcp = FastMCP("tylluan-browser")
else:
    class StubMCP:
        @staticmethod
        def tool(func):
            return func
    mcp = StubMCP()

# Cross-platform startup info (Windows needs STARTUPINFO for hidden window)
if sys.platform == "win32":
    _STARTUPINFO = subprocess.STARTUPINFO(dwFlags=subprocess.STARTF_USESHOWWINDOW)
else:
    _STARTUPINFO = None

BROWSER_PORT = 9222
CHROME_PATHS = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
]


def find_browser() -> Optional[str]:
    """Find available browser executable."""
    for path in CHROME_PATHS:
        if os.path.exists(path):
            return path
    return None


def find_free_port() -> int:
    """Find a free port for remote debugging."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('', 0))
        return s.getsockname()[1]


async def ensure_browser_running() -> Optional[subprocess.Popen]:
    """Ensure browser is running with remote debugging enabled."""
    browser_path = find_browser()
    if not browser_path:
        return None
    
    port = find_free_port()
    
    # Launch browser with remote debugging
    cmd = [
        browser_path,
        f"--remote-debugging-port={port}",
        "--no-first-run",
        "--no-default-browser-check",
        "--new-window",
        "about:blank",
    ]
    
    try:
        process = subprocess.Popen(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            startupinfo=_STARTUPINFO,
        )
        await asyncio.sleep(2)
        return process
    except Exception:
        return None


async def send_cdp_command(port: int, method: str, params: dict = None) -> dict:
    """Send a Chrome DevTools Protocol command."""
    try:
        import urllib.request
        import urllib.parse
        
        url = f"http://localhost:{port}/json"
        response = urllib.request.urlopen(url, timeout=5)
        tabs = json.loads(response.read().decode())
        
        if not tabs:
            return {"error": "No tabs found"}
        
        ws_url = tabs[0].get("webSocketDebuggerUrl")
        if not ws_url:
            return {"error": "No websocket URL"}
        
        # For now, return basic info - full CDP needs websocket client
        return {
            "success": True,
            "url": tabs[0].get("url"),
            "title": tabs[0].get("title"),
        }
    except Exception as e:
        return {"error": str(e)}


@mcp.tool()
async def browser_navigate(url: str) -> str:
    """Navigate browser to a URL.
    
    Args:
        url: The URL to navigate to.
    
    Returns:
        Confirmation or error message.
    """
    browser_path = find_browser()
    if not browser_path:
        return "No browser found. Please install Edge or Chrome."
    
    try:
        cmd = [browser_path, "--new-window", url]
        result = subprocess.run(
            cmd,
            capture_output=True,
            timeout=10,
            startupinfo=_STARTUPINFO,
        )
        
        if result.returncode == 0:
            return f"Opened {url} in new browser window"
        else:
            stderr = result.stderr.decode(errors="replace")
            return f"Failed to open browser: {stderr}"
            
    except subprocess.TimeoutExpired:
        return "Timeout: Browser took too long to respond"
    except Exception as e:
        return f"Error: {str(e)}"


@mcp.tool()
async def browser_screenshot(path: str = "screenshot.png") -> str:
    """Take a screenshot of the current browser window.
    
    Args:
        path: Path to save the screenshot.
    
    Returns:
        Path to saved screenshot or error.
    """
    browser_path = find_browser()
    if not browser_path:
        return "No browser found on system"
    
    # Basic screenshot using mss or PIL would be needed
    # For now, return placeholder with guidance
    return f"Screenshot feature requires remote debugging. Use browser_navigate first to open browser with debugging enabled."


@mcp.tool()
async def browser_status() -> str:
    """Check browser availability and status.
    
    Returns:
        Browser status information.
    """
    browser_path = find_browser()
    if not browser_path:
        return "Browser not found: Neither Edge nor Chrome is installed"
    
    browser_name = os.path.basename(os.path.dirname(browser_path))
    return f"OK: Browser available ({browser_name})\nPath: {browser_path}"


@mcp.tool()
async def search_web(query: str) -> str:
    """Search the web using the system's default browser.
    
    Args:
        query: Search query.
    
    Returns:
        Confirmation that browser opened with search.
    """
    # Use DuckDuckGo as default search engine (no tracking)
    search_url = f"https://duckduckgo.com/?q={query.replace(' ', '+')}"
    return await browser_navigate(search_url)


@mcp.tool()
async def browser_tabs() -> str:
    """Get list of open browser tabs (requires remote debugging).
    
    Returns:
        List of open tabs or availability message.
    """
    browser_path = find_browser()
    if not browser_path:
        return "Browser not running"
    return "Tab listing requires browser with remote debugging enabled. Use browser_navigate first to start browser in debugging mode."


if __name__ == "__main__":
    if _MCP_AVAILABLE:
        utils.safe_mcp_run(mcp)
    else:
        print("Browser Guild: MCP not available. Install: pip install mcp fastmcp")