"""Scrapling Web Guild — sovereign web scraping using Scrapling library.

Scraping adaptativo que escala de una URL a crawls completos.
Usa Scrapling (BSD-3, D4Vinci/Scrapling) con fallback a requests+BeautifulSoup.
"""
import json
import re
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("scrapling")

# Try Scrapling (primary), fallback to requests+BeautifulSoup
HAS_SCRAPLING = False
HAS_FALLBACK = False
try:
    from scrapling.fetchers import StealthyFetcher, Fetcher
    HAS_SCRAPLING = True
except ImportError:
    try:
        import requests
        from bs4 import BeautifulSoup
        HAS_FALLBACK = True
    except ImportError:
        pass


def _fetch(url: str, dynamic: bool = False) -> str:
    """Fetch a URL using best available method."""
    if HAS_SCRAPLING:
        try:
            if dynamic:
                p = StealthyFetcher.fetch(url, headless=True, network_idle=True)
            else:
                p = Fetcher.fetch(url)
            if p and p.status == 200:
                return p.text
        except Exception:
            pass

    if HAS_FALLBACK:
        try:
            import requests
            resp = requests.get(url, timeout=30,
                                headers={"User-Agent": "TylluanNexus/3.0"})
            if resp.status_code == 200:
                return resp.text
        except Exception:
            pass

    return ""


def _extract_text(html: str) -> str:
    """Extract clean text from HTML using Scrapling or BeautifulSoup."""
    if not html:
        return ""

    if HAS_SCRAPLING:
        try:
            from scrapling.parser import ParsedElement
            parsed = ParsedElement(html)
            text = parsed.css("body::text")
            if text:
                lines = [t.strip() for t in text if t.strip()]
                return "\n".join(lines[:200])
        except Exception:
            pass

    if HAS_FALLBACK:
        try:
            from bs4 import BeautifulSoup
            soup = BeautifulSoup(html, "html.parser")
            for tag in soup(["script", "style", "nav", "footer", "header"]):
                tag.decompose()
            text = soup.get_text(separator="\n")
            lines = [l.strip() for l in text.split("\n") if l.strip()]
            return "\n".join(lines[:200])
        except Exception:
            pass

    # Last resort: basic regex
    text = re.sub(r"<[^>]+>", " ", html)
    text = re.sub(r"\s+", " ", text).strip()
    return text[:10000]


def _extract_title(html: str) -> str:
    """Extract page title."""
    m = re.search(r"<title[^>]*>(.*?)</title>", html, re.IGNORECASE | re.DOTALL)
    return m.group(1).strip() if m else ""


@mcp.tool()
def scrape_url(url: str = "", dynamic: bool = False, intent: str = "",
               command: str = "") -> str:
    """Scrape a URL and return clean text content.
    Use for: scrape url, raspa la pagina, extrae contenido de, fetch webpage,
    download page, scrape page, extract content from url, scrape website,
    get page content, fetch url, download content, fetch page, get webpage.
    """
    target = url or intent or command or ""
    if not target.strip():
        return "❌ Specify a URL to scrape: 'scrape url: https://...'"
    if not target.startswith(("http://", "https://")):
        target = "https://" + target

    if not HAS_SCRAPLING and not HAS_FALLBACK:
        return ("❌ No scraping engine available. "
                "Install: pip install scrapling requests beautifulsoup4")

    html = _fetch(target, dynamic=dynamic)
    if not html:
        return f"❌ Could not access URL: {target}"

    title = _extract_title(html)
    text = _extract_text(html)
    n_chars = len(html)
    n_text = len(text)

    lines = [f"=== {title or target} ==="]
    lines.append(f"({n_text} chars extraídos de {n_chars} totales)")
    lines.append("")
    lines.append(text)
    return "\n".join(lines)


@mcp.tool()
def scrape_search(query: str = "", engine: str = "google",
                 intent: str = "", command: str = "") -> str:
    """Search and scrape results from a search engine.
    Use for: search scrape, buscar y raspar, search and extract,
    google scrape, search results, scrapling search.
    """
    q = query or intent or command or ""
    if not q.strip():
        return "❌ Specify what to search: 'search scrape: <query>'"

    search_urls = {
        "google": f"https://www.google.com/search?q={q.replace(' ', '+')}",
        "bing": f"https://www.bing.com/search?q={q.replace(' ', '+')}",
        "duckduckgo": f"https://html.duckduckgo.com/html/?q={q.replace(' ', '+')}",
    }
    url = search_urls.get(engine, search_urls["google"])

    if not HAS_SCRAPLING and not HAS_FALLBACK:
        return ("❌ No scraping engine available. "
                "Install: pip install scrapling requests beautifulsoup4")

    html = _fetch(url, dynamic=False)
    if not html:
        return f"❌ Could not search on {engine}"

    text = _extract_text(html)
    lines = [f"=== Resultados de búsqueda: {q} ({engine}) ==="]
    lines.append("")
    lines.append(text[:5000])
    return "\n".join(lines)


@mcp.tool()
def extract_structured(url: str = "", schema_hint: str = "",
                       intent: str = "", command: str = "") -> str:
    """Extract structured content (links, headings, tables) from a URL.
    Use for: extract structure, extraer estructura, get links, get headings,
    get tables, structured extraction, page structure, url structure.
    """
    target = url or intent or command or ""
    if not target.strip():
        return "❌ Specify a URL to analyze: 'extract structure: https://...'"
    if not target.startswith(("http://", "https://")):
        target = "https://" + target

    if not HAS_SCRAPLING and not HAS_FALLBACK:
        return ("❌ No scraping engine available. "
                "Install: pip install scrapling requests beautifulsoup4")

    html = _fetch(target, dynamic=False)
    if not html:
        return f"❌ Could not access: {target}"

    title = _extract_title(html)

    if HAS_FALLBACK or not HAS_SCRAPLING:
        from bs4 import BeautifulSoup
        soup = BeautifulSoup(html, "html.parser")
    else:
        from scrapling.parser import ParsedElement
        soup = ParsedElement(html)

    result = {"url": target, "title": title}

    # Extract links
    if HAS_FALLBACK:
        links = []
        for a in soup.find_all("a", href=True)[:30]:
            href = a.get("href", "")
            text = a.get_text(strip=True)[:80]
            if href and not href.startswith(("#", "javascript:")):
                links.append({"text": text, "href": href})
        result["links"] = links
    else:
        links = []
        for a in soup.css("a[href]")[:30]:
            href = a.attrib.get("href", "")
            text = a.text[:80] if hasattr(a, 'text') else ""
            if href and not href.startswith(("#", "javascript:")):
                links.append({"text": text, "href": href})
        result["links"] = links

    # Extract headings
    headings = []
    for level in range(1, 4):
        if HAS_FALLBACK:
            for h in soup.find_all(f"h{level}")[:10]:
                headings.append(f"H{level}: {h.get_text(strip=True)[:100]}")
        else:
            for h in (soup.css(f"h{level}") or [])[:10]:
                headings.append(f"H{level}: {h.text[:100]}")
    result["headings"] = headings

    return json.dumps(result, indent=2, ensure_ascii=False)


if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
