"""
TylluanNexus Deep Web Research Guild — Multi-source web research without API keys.

Uses:
    - DuckDuckGo Search (duckduckgo_search library, no API key required)
    - httpx for page fetching with HTML parsing
    - Research synthesis: search + fetch top results + structured summary

Status: operational
"""

import asyncio
import html.parser
import json
import logging
import os
import re
import sys
from typing import Optional

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("tylluan-deep-web-research")

_MAX_RESULTS = 20


# ── HTML text extractor ─────────────────────────────────────────────────────

class _TextExtractor(html.parser.HTMLParser):
    """Strip HTML tags, extract clean text."""

    def __init__(self):
        super().__init__()
        self._text: list[str] = []
        self._skip = False

    def handle_starttag(self, tag, attrs):
        if tag in ("script", "style", "noscript"):
            self._skip = True

    def handle_endtag(self, tag):
        if tag in ("script", "style", "noscript"):
            self._skip = False
        if tag in ("p", "br", "div", "h1", "h2", "h3", "h4", "li"):
            self._text.append("\n")

    def handle_data(self, data):
        if not self._skip:
            self._text.append(data)

    def text(self) -> str:
        raw = "".join(self._text)
        return re.sub(r" {2,}", " ", raw).strip()


def _extract_html(html_content: str, max_chars: int = 6000) -> str:
    """Extract and clean text from HTML content."""
    parser = _TextExtractor()
    try:
        parser.feed(html_content)
    except Exception:
        pass
    result = parser.text()
    # Collapse repeated newlines
    result = re.sub(r"\n{3,}", "\n\n", result)
    return result[:max_chars]


# ── Tools ───────────────────────────────────────────────────────────────────


@mcp.tool()
async def web_search(
    query: str,
    max_results: int = 8,
    region: str = "wt-wt",
    safesearch: str = "moderate",
    time_filter: str = "",
    intent: str = "",
) -> str:
    """Deep web search without API key using DuckDuckGo.
    Use for: search web, buscar en internet, web search, find online,
    research topic, buscar noticias, find articles, google alternative,
    search news, investigar tema, buscar información web, ddg search.

    Args:
        query: Search query.
        max_results: Maximum results to return (max 20).
        region: Region code (wt-wt = worldwide, es-es = Spain, etc.).
        safesearch: Safe search level (on, moderate, off).
        time_filter: Time filter (d=day, w=week, m=month, y=year).
        intent: Natural language search intent.
    """
    try:
        try:
            from ddgs import DDGS
        except ImportError:
            from duckduckgo_search import DDGS

        ddgs = DDGS()
        results = list(
            ddgs.text(
                query,
                region=region,
                safesearch=safesearch,
                timelimit=time_filter or None,
                max_results=min(max_results, _MAX_RESULTS),
            )
        )

        if not results:
            return f"📭 No results found for '{query}'."

        lines: list[str] = []
        for i, r in enumerate(results, 1):
            title = r.get("title", "").strip()
            url = r.get("href", r.get("url", "")).strip()
            snippet = r.get("body", r.get("snippet", "")).strip()
            lines.append(f"{i}. [{title}]({url})")
            if snippet:
                lines.append(f"   {snippet[:200]}")
                lines.append("")

        return "\n".join(lines).strip()

    except ImportError:
        return "❌ duckduckgo_search not installed. Run: pip install duckduckgo_search"
    except Exception as e:
        logging.warning("DDGS search failed: %s", e)
        # Fallback: try DuckDuckGo Lite via httpx
        return await _ddg_lite_fallback(query, max_results)


async def _ddg_lite_fallback(query: str, max_results: int = 8) -> str:
    """Fallback search using DuckDuckGo Lite HTML endpoint."""
    try:
        import httpx

        async with httpx.AsyncClient(timeout=15) as client:
            resp = await client.post(
                "https://lite.duckduckgo.com/lite/",
                data={"q": query},
                headers={"User-Agent": "Mozilla/5.0"},
            )
            resp.raise_for_status()
            # Parse href attributes from <a> tags in results
            urls: list[str] = re.findall(
                r'href="//duckduckgo\.com/l/\?uddg=([^&"]+)', resp.text
            )
            import urllib.parse

            decoded = [urllib.parse.unquote(u) for u in urls]
            if not decoded:
                return "📭 No results found via fallback."
            lines = [f"1. [{decoded[0]}]({decoded[0]})"]
            for u in decoded[1:max_results]:
                lines.append(f"   • {u}")
            return "\n".join(lines[:max_results * 2])
    except ImportError:
        return "❌ httpx not available for fallback search."
    except Exception as e2:
        return f"❌ Search failed (DDGS + fallback): {e2}"


@mcp.tool()
async def fetch_page(
    url: str,
    max_chars: int = 6000,
    intent: str = "",
) -> str:
    """Fetch and extract clean text from a URL.
    Use for: fetch page, read url, get content from link, extract text from website,
    leer página web, obtener contenido de url, descargar página, scrape page.

    Args:
        url: Full URL to fetch.
        max_chars: Maximum characters to return (default 6000).
        intent: Natural language description of what to find.
    """
    if not url.startswith(("http://", "https://")):
        # Try to prepend https:// if missing
        url = "https://" + url

    try:
        import httpx

        async with httpx.AsyncClient(
            timeout=20,
            follow_redirects=True,
            headers={
                "User-Agent": (
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                    "AppleWebKit/537.36 (KHTML, like Gecko) "
                    "Chrome/124.0.0.0 Safari/537.36"
                ),
                "Accept": "text/html,application/xhtml+xml",
            },
        ) as client:
            resp = await client.get(url)
            resp.raise_for_status()

            content_type = resp.headers.get("content-type", "")
            if "text/html" not in content_type and "application/xhtml" not in content_type:
                return (
                    f"⏭️ URL returned {content_type or 'unknown content type'} — "
                    f"not HTML: {url[:100]}"
                )

            text = _extract_html(resp.text, max_chars)

            if not text.strip():
                return f"📭 Page fetched but no readable text found: {url}"

            return (
                f"📄 **Content from:** {url}\n\n"
                f"{text}\n\n"
                f"---\n_Truncated to {max_chars} characters._"
            )

    except httpx.TimeoutException:
        return f"⏱️ Timeout fetching {url[:100]}"
    except httpx.HTTPStatusError as e:
        return f"❌ HTTP {e.response.status_code} for {url[:100]}"
    except ImportError:
        return "❌ httpx not installed. Run: pip install httpx"
    except Exception as e:
        return f"❌ Failed to fetch {url[:100]}: {e}"


@mcp.tool()
async def research_topic(
    topic: str = "",
    depth: int = 3,
    include_content: bool = True,
    intent: str = "",
    query: str = "",
) -> str:
    """Research a topic: search + fetch top pages + return structured summary.
    Use for: research topic, investigar tema, deep research, multi-source research,
    gather information about, recopilar información sobre, investigate, analizar tema,
    understand topic, comprehensive research.

    Returns structured JSON with sources and combined text ready for LLM synthesis.

    Args:
        topic: Topic to research.
        depth: Number of top results to fetch fully (1-5).
        include_content: Whether to include full fetched content in output.
        intent: Natural language description of the research goal.
    """
    # Accept intent/query as fallback when topic is empty (tylluan_do routing)
    effective_topic = topic or intent or query
    if not effective_topic:
        return json.dumps({"error": "topic, intent, or query required"})

    try:
        # Phase 1: search
        from duckduckgo_search import DDGS

        ddgs = DDGS()
        results = list(
            ddgs.text(
                effective_topic,
                region="wt-wt",
                safesearch="moderate",
                max_results=depth * 2,
            )
        )

        if not results:
            return json.dumps({
                "topic": effective_topic,
                "error": "No search results found",
                "sources": [],
                "combined_text": "",
            }, ensure_ascii=False, indent=2)

        # Phase 2: fetch top `depth` pages
        sources: list[dict] = []
        texts: list[str] = []
        for r in results[:depth]:
            url = r.get("href", r.get("url", "")).strip()
            title = r.get("title", "").strip()
            snippet = r.get("body", r.get("snippet", "")).strip()

            content_text = ""
            if include_content and url:
                try:
                    content_text = await fetch_page(url, max_chars=4000, intent="")
                    # Parse the response format to extract just the body
                    if content_text.startswith("📄"):
                        body_start = content_text.find("\n\n")
                        if body_start > 0:
                            content_text = content_text[body_start + 2:]
                        # Remove truncation marker
                        trunc_marker = content_text.find("\n---\n_Truncated")
                        if trunc_marker > 0:
                            content_text = content_text[:trunc_marker]
                    else:
                        # fetch failed — use snippet
                        content_text = snippet
                except Exception:
                    content_text = snippet
            else:
                content_text = snippet

            sources.append({
                "url": url,
                "title": title,
                "content_preview": (content_text or snippet)[:500],
            })
            if content_text:
                texts.append(f"--- Source: {title} ({url}) ---\n{content_text}")

        result = {
            "topic": topic,
            "sources": sources,
            "combined_text": "\n\n".join(texts) if texts else "",
        }
        return json.dumps(result, ensure_ascii=False, indent=2)

    except ImportError:
        return json.dumps({
            "topic": topic,
            "error": "duckduckgo_search not installed. Run: pip install duckduckgo_search",
            "sources": [],
            "combined_text": "",
        }, ensure_ascii=False)
    except Exception as e:
        return json.dumps({
            "topic": topic,
            "error": str(e),
            "sources": [],
            "combined_text": "",
        }, ensure_ascii=False)


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, stream=sys.stderr)
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
