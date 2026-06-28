"""Web Search guild — SearXNG meta-search for autonomous agent research."""
import json
import urllib.parse
import urllib.request
import urllib.error
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("websearch")

SEARXNG_BASE = "http://127.0.0.1:32700"


def _search(query: str, num_results: int = 5) -> list[dict]:
    params = urllib.parse.urlencode({
        "q": query,
        "format": "json",
        "language": "en",
        "categories": "general",
        "pageno": 1,
    })
    url = f"{SEARXNG_BASE}/search?{params}"
    req = urllib.request.Request(url, headers={"User-Agent": "TylluanNexus/3.0"})
    with urllib.request.urlopen(req, timeout=30) as r:
        data = json.loads(r.read())
    results = data.get("results", [])
    return results[:num_results]


@mcp.tool()
def web_search(query: str = "", num_results: int = 5, intent: str = "",
               command: str = "") -> str:
    """Search the web using SearXNG meta-search engine.
    Use for: busca en internet, search web, busca informacion sobre, web search,
    buscar online, buscar en la web, look up online, research topic, find online,
    internet search, fact check, buscar informacion, google search.
    """
    q = query or intent or command or ""
    if not q.strip():
        return "❌ Specify what to search: 'search the web: <query>'"
    try:
        results = _search(q.strip(), max(1, min(num_results, 20)))
    except urllib.error.HTTPError as e:
        if e.code == 502:
            return "❌ SearXNG is not available. Run: docker compose -f docker-compose.searxng.yml up -d"
        return f"❌ Search error (HTTP {e.code})"
    except Exception as e:
        return f"❌ Error connecting to SearXNG: {e}. Is it running? docker compose -f docker-compose.searxng.yml up -d"
    if not results:
        return f"No results found for: {q}"
    lines = [f"=== Search results: {q} ({len(results)} results) ==="]
    for i, r in enumerate(results, 1):
        title = r.get("title", "Untitled")
        url = r.get("url", "")
        snippet = r.get("content", r.get("snippet", ""))
        snippet_clean = snippet[:300].replace("\n", " ")
        lines.append(f"\n{i}. {title}")
        lines.append(f"   {url}")
        if snippet_clean:
            lines.append(f"   {snippet_clean}")
    return "\n".join(lines)


from guilds.core import utils

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
