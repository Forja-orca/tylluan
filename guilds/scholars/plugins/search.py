"""
TylluanNexus Search Guild — Local content search without API keys.

This guild provides:
    - Full-text search across files (ripgrep/grep)
    - Find files by name
    - Search within archives
    - Code-aware search (functions, classes)

No external services required - all local.

Status: operational (degraded: requires mcp fastmcp installed)
Last verified: 2026-05-11
"""

import os
import asyncio
import json
import sys
from pathlib import Path
from typing import Optional

_MCP_AVAILABLE = False
try:
    from mcp.server.fastmcp import FastMCP
    from guilds.core import utils
    from guilds.core._security import SKIP_DIRS, SKIP_FILES, SKIP_EXTENSIONS, rg_exclude_flags
    from guilds.core.silva_utils import add_node
    _MCP_AVAILABLE = True
except ImportError as e:
    _MCP_AVAILABLE = False
    print(f"⚠️ Search Guild: MCP dependencies not available: {e}", file=sys.stderr)

if _MCP_AVAILABLE:
    mcp = FastMCP("tylluan-search")
else:
    class StubMCP:
        @staticmethod
        def tool(func):
            return func
    mcp = StubMCP()


@mcp.tool()
async def search_content(
    query: str,
    path: str = ".",
    file_pattern: str = "*",
    case_sensitive: bool = False,
    max_results: int = 50,
    intent: str = "",
) -> str:
    """Search inside file contents for text patterns using ripgrep or grep.

    Use for: search for text in files, find TODO comments, grep for pattern,
    search inside files, find string in code, look for keyword in files,
    find all occurrences, search comments, find imports, content search,
    search code for text, find function calls, buscar en archivos, buscar texto.

    For finding files by NAME or PATTERN, use find_files instead.

    Args:
        query: Text pattern to search for
        path: Directory to search in (default: current)
        file_pattern: Glob pattern for files (e.g., "*.py", "*.{js,ts}")
        case_sensitive: Whether search is case-sensitive
        max_results: Maximum number of matches to return
        intent: Natural language intent for fallback query extraction
    """
    # Extract actual search term from query if natural language
    if query and len(query.split()) > 3:
        import re as _re
        m = _re.search(
            r'(?:search for|find|grep|look for|buscar)\s+["\']?([^\s"\']+)["\']?',
            query, _re.IGNORECASE
        )
        if m:
            query = m.group(1)
        else:
            stopwords = {"search", "find", "for", "the", "in", "all", "files", "content", "pattern", "grep", "look"}
            words = [w for w in query.split() if w.lower() not in stopwords]
            if words:
                query = words[0]

    rg_path = utils.find_executable("rg") or utils.find_executable("ripgrep")
    grep_path = utils.find_executable("grep")

    if not os.path.isdir(path):
        return f"❌ Directory not found: {path}"

    # Pure-Python fallback when neither rg nor grep is available (e.g. Windows)
    if not rg_path and not grep_path:
        return _python_search(query, path, file_pattern, case_sensitive, max_results)

    # Use ripgrep if available, otherwise grep
    if rg_path:
        args = [rg_path, "--json", "--max-count", str(max_results), "-e", query]
        if not case_sensitive:
            args.append("-i")
        # Use centralized security exclusions for ripgrep
        args.extend(rg_exclude_flags())
        if file_pattern != "*":
            args.extend(["-g", file_pattern])
        args.append(path)
    else:
        args = [grep_path, "-n", "-r"]
        if not case_sensitive:
            args.append("-i")
        # Use centralized security exclusions for grep
        for name in SKIP_FILES:
            args.append(f"--exclude={name}")
        for ext in SKIP_EXTENSIONS:
            args.append(f"--exclude=*{ext}")
        for excl_dir in SKIP_DIRS:
            args.append(f"--exclude-dir={excl_dir}")
        if file_pattern != "*":
            args.extend(["--include", file_pattern])
        args.extend([query, path])

    try:
        returncode, stdout, stderr = await utils.run_command(args, timeout_secs=30)

        if returncode not in [0, 1]:
            return f"❌ Search error: {stderr}"

        if not stdout.strip():
            return f"🔍 No matches found for '{query}' in {path}"

        # Parse ripgrep JSON output
        if rg_path:
            return parse_rg_output(stdout, max_results)
        else:
            return stdout[:10000]

    except asyncio.TimeoutError:
        return "⏱️ Search timed out after 30 seconds"
    except Exception as e:
        return f"❌ Search failed: {str(e)}"


def _python_search(query: str, path: str, file_pattern: str, case_sensitive: bool, max_results: int) -> str:
    """Pure-Python content search fallback (no external tools needed)."""
    import fnmatch
    results = []
    needle = query if case_sensitive else query.lower()
    root = Path(path)
    
    # Use centralized security exclusions from guilds.core._security
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for filename in filenames:
            # Skip sensitive files and extensions
            if filename in SKIP_FILES:
                continue
            ext = Path(filename).suffix.lower()
            if ext in SKIP_EXTENSIONS:
                continue
            if filename.startswith(".") and filename not in (".gitignore", ".dockerignore"):
                continue
            if file_pattern != "*" and not fnmatch.fnmatch(filename, file_pattern):
                continue
            filepath = Path(dirpath) / filename
            try:
                text = filepath.read_text(encoding="utf-8", errors="ignore")
                lines = text.splitlines()
                for lineno, line in enumerate(lines, 1):
                    hay = line if case_sensitive else line.lower()
                    if needle in hay:
                        rel = str(filepath.relative_to(root))
                        results.append(f"📄 {rel}:{lineno}\n   {line.strip()}")
                        if len(results) >= max_results:
                            break
            except Exception:
                continue
        if len(results) >= max_results:
            break

    if not results:
        return f"🔍 No matches found for '{query}' in {path}"
    return f"🔍 Found {len(results)} matches:\n\n" + "\n\n".join(results[:max_results])


def parse_rg_output(output: str, max_results: int) -> str:
    """Parse ripgrep JSON output into readable format."""
    lines = output.strip().split("\n")
    results = []
    
    for line in lines:
        if len(results) >= max_results:
            break
        if not line.strip():
            continue
        try:
            data = json.loads(line)
            if data.get("type") == "match":
                match = data.get("data", {})
                file_path = match.get("path", {}).get("text", "unknown")
                line_num = match.get("line_number", 0)
                match_text = match.get("lines", {}).get("text", "").strip()
                results.append(f"📄 {file_path}:{line_num}\n   {match_text}")
        except Exception:
            continue
    
    if not results:
        return "🔍 No matches found"
    
    return "\n\n".join(results[:max_results])


@mcp.tool()
async def find_files(
    name_pattern: str = "",
    path: str = ".",
    intent: str = "",
    query: str = "",
    file_type: Optional[str] = None,
    max_results: int = 30,
) -> str:
    """Find files by name pattern or extension. Use this for locating files by name.

    Use for: find files, list files named, find all .py files, find files with extension,
    locate file, show files matching pattern, find *.toml files, search for file by name,
    find file named, which files exist, list all python files, files with extension json,
    find all rust files, find typescript files, find configuration files, list markdown files,
    buscar archivos, encontrar archivos, listar archivos, archivos con extension.

    For searching text INSIDE file contents, use search_content instead.

    Args:
        name_pattern: Filename pattern (supports wildcards like *.py, *.{js,ts})
        path: Directory to search in
        intent: Natural language intent for fallback pattern extraction
        query: Alternative query string for pattern extraction
        file_type: Filter by extension (e.g., "py", "js", "md")
        max_results: Maximum files to return

    Returns:
        List of matching file paths
    """
    # Resolve name_pattern from intent or query if not provided
    if not name_pattern:
        source = intent or query or ""
        import re as _re
        m = _re.search(r'["\']([^"\']+)["\']', source)
        if m:
            name_pattern = m.group(1)
        else:
            m = _re.search(r'(\*\.[a-zA-Z0-9]+|\*\.?\{[^}]+\})', source)
            if m:
                name_pattern = m.group(1)
            else:
                m = _re.search(r'\b([\w\-]+\.(?:py|rs|ts|tsx|js|jsx|go|toml|json|md|yaml|yml|c|cpp|h))\b', source)
                if m:
                    name_pattern = m.group(1)
                else:
                    name_pattern = "*"

    path = os.path.abspath(path) if path and path not in (".", "") else path
    
    if not os.path.isdir(path):
        return f"❌ Directory not found: {path}"
    
    pattern = name_pattern
    if file_type and not name_pattern.endswith(f".{file_type}"):
        pattern = f"*.{file_type}"
    
    try:
        root = Path(path).resolve()
        matches = list(root.rglob(pattern))
        
        # Filter and limit
        files = []
        for m in matches:
            if m.is_file():
                try:
                    files.append(str(m.relative_to(root)))
                except ValueError:
                    files.append(str(m))
            if len(files) >= max_results:
                break
        
        if not files:
            return f"🔍 Found {len(matches)} matches but no files"
        
        return f"📁 Found {len(files)} files:\n\n" + "\n".join(files[:max_results])
        
    except Exception as e:
        return f"❌ Find failed: {str(e)}"


@mcp.tool()
async def search_code_structure(
    query: str,
    path: str = ".",
    language: str = "py",
) -> str:
    """Search for code structures (functions, classes, imports).
    
    Args:
        query: Structure type to find (function, class, import, def, const)
        path: Directory to search
        language: Programming language (py, js, ts, rs, go)
    
    Returns:
        List of code definitions found
    """
    patterns = {
        "py": {
            "function": r"^def\s+(\w+)",
            "class": r"^class\s+(\w+)",
            "import": r"^(?:from|import)\s+",
        },
        "js": {
            "function": r"^(?:function\s+|const\s+|let\s+|var\s+)(\w+)\s*[=(",
            "class": r"^class\s+(\w+)",
            "const": r"^(?:const|let|var)\s+(\w+)",
        },
        "ts": {
            "function": r"^(?:function\s+|const\s+)(\w+)\s*[=(",
            "class": r"^class\s+(\w+)",
            "interface": r"^interface\s+(\w+)",
        },
        "rs": {
            "function": r"^fn\s+(\w+)",
            "struct": r"^struct\s+(\w+)",
            "impl": r"^impl\s+(\w+)",
        },
    }
    
    lang_patterns = patterns.get(language, patterns["py"])
    pattern = lang_patterns.get(query.lower(), lang_patterns.get("function", ""))
    
    if not pattern:
        return f"❌ Unknown query type: {query}"
    
    return await search_content(
        query=pattern,
        path=path,
        file_pattern=f"*.{language}",
        max_results=30,
    )


@mcp.tool()
async def web_search(query: str, max_results: int = 5) -> str:
    try:
        import httpx, re as re_, urllib.parse
        async with httpx.AsyncClient(timeout=15, follow_redirects=True) as client:
            resp = await client.get(
                "https://lite.duckduckgo.com/lite/",
                params={"q": query},
                headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"}
            )
        if resp.status_code not in (200, 202):
            return f"DDG returned HTTP {resp.status_code}"
        raw_urls = re_.findall(r'href="//duckduckgo\.com/l/\?uddg=([^&"]+)', resp.text)
        urls = [urllib.parse.unquote(u) for u in raw_urls[:max_results]]
        titles = re_.findall(r'<b>([^<]+)</b>', resp.text)[:max_results]
        tds = re_.findall(r'<td[^>]*>\s*([^<\s][^<]{15,}?)\s*</td>', resp.text)
        snippets = [re_.sub(r'<[^>]+>', '', t).strip() for t in tds if len(t.strip()) > 20][:max_results]
        if not urls and not titles:
            return f"No results for: {query}"
        parts = []
        for i in range(min(max_results, max(len(urls), len(titles), 1))):
            t = titles[i] if i < len(titles) else "-"
            u = urls[i] if i < len(urls) else ""
            s = snippets[i] if i < len(snippets) else ""
            parts.append(f"**{t}**\n{u}\n{s[:200]}")
        return f"Web results for '{query}':\n\n" + "\n\n---\n\n".join(parts)
    except ImportError:
        return "Install httpx: pip install httpx"
    except Exception as e:
        return f"Search error: {type(e).__name__}: {e}"


@mcp.tool()
async def search_and_remember(
    query: str,
    max_results: int = 5,
    agent_id: str = "anonymous",
    tags: str = "",
) -> str:
    """Search the web and store a summarized result in SilvaDB memory.

    Args:
        query: What to search for.
        max_results: Number of web results to fetch (default 5).
        agent_id: Agent storing the memory.
        tags: Comma-separated tags for the node.

    Returns:
        JSON with node_id, summary preview, and source URLs.
    """
    import time
    from datetime import datetime as _dt
    result_text = await web_search(query, max_results)
    if not result_text or result_text.startswith("No results") or result_text.startswith("Search error") or result_text.startswith("DDG returned"):
        return json.dumps({"status": "no_results", "query": query})
    first_line = f"[WEB] {query}"
    parts = [first_line]
    for section in result_text.split("---\n\n"):
        preview = section[:300].replace("\n", " ")
        parts.append(preview)
        if len("\n".join(parts)) >= 1500:
            break
    summary = "\n---\n".join(parts)[:1500]
    tag_list = [t.strip() for t in tags.split(",") if t.strip()] if tags else []
    now = int(time.time())
    meta = {
        "query": query,
        "agent_id": agent_id,
        "source": "web_search",
        "ts": now,
        "time": _dt.fromtimestamp(now).isoformat(),
    }
    node_id = add_node(content=summary, node_type="web_research", tags=tag_list, metadata=meta)
    if node_id:
        return json.dumps({"status": "remembered", "node_id": node_id, "query": query, "summary_length": len(summary), "agent_id": agent_id})
    return json.dumps({"status": "error", "query": query, "error": "Failed to store in SilvaDB"})


if __name__ == "__main__":
    if _MCP_AVAILABLE:
        utils.safe_mcp_run(mcp)
    else:
        print("Search Guild: MCP not available. Install: pip install mcp fastmcp")