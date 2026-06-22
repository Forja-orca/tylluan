"""Coloquio guild — read and write group chat channels in TylluanNexus."""
import json
import re
import urllib.request
import urllib.error
import urllib.parse
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("coloquio")

KERNEL_BASE = "http://127.0.0.1:3030"

# ── HTTP helpers ──────────────────────────────────────────────────────────────

def _get(path: str) -> dict:
    with urllib.request.urlopen(f"{KERNEL_BASE}{path}", timeout=10) as r:
        return json.loads(r.read())


def _post(path: str, body: dict) -> dict:
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        f"{KERNEL_BASE}{path}",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())


# ── Channel-ID extraction helpers ────────────────────────────────────────────

# Prefixes to strip before extracting channel name from natural language
_CHANNEL_PREFIXES = re.compile(
    r'^(?:lee\s+el\s+(?:canal\s+)?coloquio|leer\s+(?:canal\s+)?coloquio|'
    r'ver\s+(?:canal\s+)?coloquio|read\s+(?:coloquio\s+)?channel|'
    r'mostrar\s+(?:canal\s+)?coloquio|muestra\s+(?:el\s+)?coloquio|'
    r'historial\s+coloquio|canal\s+coloquio|busca\s+en\s+(?:canal\s+)?coloquio|'
    r'search\s+(?:coloquio\s+)?channel|publica\s+en\s+(?:canal\s+)?coloquio|'
    r'post\s+to\s+(?:coloquio\s+)?channel|escribe\s+en\s+(?:canal\s+)?coloquio|'
    r'responde\s+en\s+(?:canal\s+)?coloquio|publicar\s+en\s+(?:canal\s+)?coloquio|'
    r'send\s+(?:message\s+)?to\s+(?:(?:coloquio\s+)?channel\s+)?|'
    r'envia\s+al\s+canal|envía\s+al\s+canal|envia\s+a\s+canal|'
    r'message\s+(?:coloquio\s+)?|mision-activa\s+)\s*',
    re.IGNORECASE,
)

# Words that signal the end of the channel slug in a natural-language string
_STOP_WORDS = re.compile(
    r'\s+(?:ultimos?|últimos?|last|limit|offset|desde|desde\s+turno|author|content|'
    r'con\s+mensaje|\d+\s+(?:turnos?|mensajes?|msgs?))',
    re.IGNORECASE,
)

def _extract_channel_and_rest(raw: str) -> tuple[str, str]:
    """Extract (channel_id_slug, rest_of_string) from a natural-language string.

    Strategy:
    1. Strip known action prefixes.
    2. The channel slug is the first token(s) matching [a-z0-9_-]+ (slug-safe).
    3. Everything after the first non-slug token is 'rest'.
    """
    s = _CHANNEL_PREFIXES.sub("", raw).strip()
    if not s:
        return "", ""
    # Stop at first stop-word
    m = _STOP_WORDS.search(s)
    if m:
        return s[:m.start()].strip(), s[m.start():].strip()
    # Otherwise take only slug-safe leading tokens
    # Delimiters: whitespace+non-ws, OR any non-slug char (:, ., !, etc.), OR end-of-string
    slug_match = re.match(r'^([\w][\w\-]*(?:\s+[\w][\w\-]*)*?)(?=\s+\S|[^\w\-]|$)', s)
    if slug_match:
        candidate = slug_match.group(1).strip()
        rest = s[len(candidate):].strip()
        return candidate, rest
    # Fallback: everything is the channel
    return s.strip(), ""


def _parse_limit(text: str, default: int) -> int:
    """Extract limit N from patterns like 'ultimos 5', 'last 10', 'limit 20'."""
    m = re.search(r'(?:ultimos?|últimos?|last|limit|top)\s+(\d+)', text, re.IGNORECASE)
    if m:
        return min(int(m.group(1)), 500)
    m = re.search(r'(\d+)\s+(?:turnos?|mensajes?|msgs?)', text, re.IGNORECASE)
    if m:
        return min(int(m.group(1)), 500)
    return default


def _parse_offset(text: str) -> int:
    """Extract offset from 'desde turno N' / 'offset N'."""
    m = re.search(r'(?:offset|desde\s+turno|from\s+turn)\s+(\d+)', text, re.IGNORECASE)
    return int(m.group(1)) if m else 0


def _parse_author(text: str) -> str:
    """Extract 'author X' or 'as X' from intent string."""
    m = re.search(r'(?:author|as|autor)\s+([\w][\w\-]*)', text, re.IGNORECASE)
    return m.group(1) if m else "agent"


def _truncate(text: str, max_chars: int = 400) -> str:
    if len(text) <= max_chars:
        return text
    return text[:max_chars] + f"…[+{len(text)-max_chars}c]"


# ── Tools ─────────────────────────────────────────────────────────────────────

@mcp.tool()
def read_channel(channel_id: str = "", query: str = "", intent: str = "",
                 command: str = "", limit: int = 0, offset: int = 0) -> str:
    """Read messages from a Coloquio channel.
    Use for: lee el canal coloquio, leer coloquio, ver mensajes canal, read coloquio channel,
    mostrar hilo coloquio, get thread, ver conversacion grupal, historial coloquio,
    lee el coloquio, muestra el coloquio, ver canal.
    Supports: limit N, offset N, 'ultimos N', 'last N'.
    """
    raw = query or intent or command or ""
    if not channel_id:
        channel_id, rest = _extract_channel_and_rest(raw)
        if limit == 0:
            limit = _parse_limit(rest, 50)
        if offset == 0:
            offset = _parse_offset(rest)
    elif limit == 0:
        limit = _parse_limit(raw, 50)
    if limit == 0:
        limit = 50

    if not channel_id:
        return "❌ Specify a channel name: 'read the coloquio <channel-name>'"
    try:
        quoted_id = urllib.parse.quote(channel_id, safe="")
        url = f"/api/v1/coloquio/channels/{quoted_id}?limit={limit}&offset={offset}"
        data = _get(url)
        msgs = data.get("messages", [])
        if not msgs:
            return f"Channel '{channel_id}' exists but has no messages yet (offset={offset})."
        lines = [f"=== Coloquio: {channel_id} ({len(msgs)} messages, offset={offset}) ==="]
        for m in msgs:
            role_icon = "👤" if m.get("role") == "human" else "🤖"
            lines.append(
                f"[T{m['turn']}] {role_icon} {m['author_id']}: {_truncate(m['content'])}"
            )
        return "\n".join(lines)
    except urllib.error.HTTPError as e:
        return f"❌ Channel '{channel_id}' not found (HTTP {e.code}). Create it first from the dashboard."
    except Exception as e:
        return f"❌ Error reading channel: {e}"


@mcp.tool()
def post_to_channel(channel_id: str = "", content: str = "", author_id: str = "",
                    role: str = "agent", message: str = "", query: str = "",
                    intent: str = "", command: str = "") -> str:
    """Post a message to a Coloquio channel.
    Use for: post to channel, write to channel, respond in colloquio, add to thread.
    Format: 'post to channel <channel>: <message>' or supply channel_id + content directly.
    MENTIONS: writing @agent (e.g. @agent-1, @agent-2) will automatically deliver a
    notification to their inbox in the kernel — the mentioned agent can view it with tylluan_recall '@inbox'.
    """
    raw = query or intent or command or ""

    if not channel_id and raw:
        channel_id, rest = _extract_channel_and_rest(raw)
        # Extract content from 'content:' or ':' separator
        m_content = re.search(r'(?:content\s*[:\-]\s*|:\s*)(.+)$', rest, re.IGNORECASE | re.DOTALL)
        if m_content and not content:
            content = m_content.group(1).strip()
        elif not content:
            # Try '<action> <canal>: <content>'
            m2 = re.search(r':\s*(.+)$', raw, re.DOTALL)
            if m2:
                content = m2.group(1).strip()
        # Extract author_id from 'author X'
        if not author_id:
            author_id = _parse_author(rest) or _parse_author(raw)

    content = content or message or ""
    if not author_id:
        author_id = "agent"

    if not channel_id or not content:
        return "❌ Specify channel and content: 'post to coloquio <channel>: <message>'"
    try:
        quoted_id = urllib.parse.quote(channel_id, safe="")
        result = _post(
            f"/api/v1/coloquio/channels/{quoted_id}/post",
            {"author_id": author_id, "role": role, "content": content, "metadata": "{}"},
        )
        return (f"✅ Posted to '{channel_id}' as @{author_id} — "
                f"turn {result.get('turn', '?')} (msg_id: {result.get('msg_id', '?')[:8]}…)")
    except urllib.error.HTTPError as e:
        return f"❌ Error posting (HTTP {e.code}): {e.read().decode()}"
    except Exception as e:
        return f"❌ Error posting: {e}"


@mcp.tool()
def search_channel(channel_id: str = "", query: str = "", intent: str = "",
                   command: str = "", keyword: str = "", limit: int = 20) -> str:
    """Search messages in a Coloquio channel by keyword.
    Use for: busca en coloquio, search channel, buscar mensaje coloquio,
    find message in channel, busca '...' en canal.
    """
    raw = query or intent or command or ""
    if not channel_id:
        channel_id, rest = _extract_channel_and_rest(raw)
        if not keyword:
            # Extract quoted keyword or last significant token
            m = re.search(r'["\'](.+?)["\']', rest or raw)
            if m:
                keyword = m.group(1)
            else:
                # try 'busca X en' or 'search X'
                m2 = re.search(r'(?:busca|search|find)\s+(.+?)(?:\s+en\s+|$)', raw, re.IGNORECASE)
                if m2:
                    keyword = m2.group(1).strip()

    if not channel_id or not keyword:
        return "❌ Specify channel and keyword: 'search coloquio <channel>: <keyword>'"
    try:
        quoted_id = urllib.parse.quote(channel_id, safe="")
        quoted_q = urllib.parse.quote(keyword)
        data = _get(f"/api/v1/coloquio/channels/{quoted_id}/search?q={quoted_q}&limit={limit}")
        msgs = data.get("messages", [])
        if not msgs:
            return f"No results for '{keyword}' in channel '{channel_id}'."
        lines = [f"=== Search '{keyword}' in {channel_id} ({len(msgs)} hits) ==="]
        for m in msgs:
            lines.append(f"[T{m['turn']}] {m['author_id']}: {_truncate(m['content'], 300)}")
        return "\n".join(lines)
    except urllib.error.HTTPError as e:
        return f"❌ Search error (HTTP {e.code}): {e.read().decode()}"
    except Exception as e:
        return f"❌ Search error: {e}"


@mcp.tool()
def get_turn(channel_id: str, turn: int) -> str:
    """Get a specific turn (message) from a Coloquio channel by turn number.
    Use for: ver turno N, get turn N, muestra el turno N de coloquio.
    """
    try:
        quoted_id = urllib.parse.quote(channel_id, safe="")
        data = _get(f"/api/v1/coloquio/channels/{quoted_id}/turn/{turn}")
        msg = data.get("message") or data
        if not msg:
            return f"Turn {turn} not found in channel '{channel_id}'."
        return (f"[T{msg['turn']}] @{msg['author_id']} ({msg['role']}):\n{msg['content']}")
    except urllib.error.HTTPError as e:
        return f"❌ Turn {turn} not found in '{channel_id}' (HTTP {e.code})."
    except Exception as e:
        return f"❌ Error: {e}"


@mcp.tool()
def whats_new(agent_id: str = "", query: str = "", intent: str = "",
              command: str = "", limit: int = 20) -> str:
    """Catch up on ALL Coloquio channels: returns only unread messages for this agent
    and advances its read cursor. THE daily-work tool — call it at session start.
    Use for: que hay de nuevo, ponme al dia, novedades coloquio, what's new,
    catch up, mensajes sin leer, unread messages, ponte al dia en el coloquio.
    Always pass your agent_id so the cursor is yours.
    """
    raw = query or intent or command or ""
    if not agent_id:
        m = re.search(r'(?:agent_id|para|as|soy)\s+([\w][\w\-]*)', raw, re.IGNORECASE)
        agent_id = m.group(1) if m else "agent"

    try:
        quoted_agent = urllib.parse.quote(agent_id, safe="")
        summary = _get(f"/api/v1/coloquio/unread?reader={quoted_agent}")
        channels = summary.get("channels", [])
        total = summary.get("total_unread", 0)
        if total == 0:
            return f"✅ @{agent_id} está al día — 0 mensajes sin leer en {len(channels)} canales."

        lines = [f"=== Novedades para @{agent_id} ({total} sin leer) ==="]
        shown_channels = 0
        for ch in channels:
            if ch.get("unread_count", 0) <= 0:
                continue
            if shown_channels >= 5:
                lines.append(f"…y más canales con mensajes sin leer. Vuelve a llamar whats_new.")
                break
            shown_channels += 1
            cid = ch["channel_id"]
            quoted_id = urllib.parse.quote(cid, safe="")
            data = _get(
                f"/api/v1/coloquio/channels/{quoted_id}/new"
                f"?reader={quoted_agent}&limit={min(limit, 50)}&mark_read=true"
            )
            msgs = data.get("messages", [])
            lines.append(f"\n## #{cid} ({ch['unread_count']} nuevos)")
            for m_ in msgs:
                role_icon = "👤" if m_.get("role") == "human" else "🤖"
                mention_flag = " ⚠️TE MENCIONAN" if f"@{agent_id}" in m_.get("content", "") else ""
                lines.append(f"[T{m_['turn']}] {role_icon} {m_['author_id']}{mention_flag}: {_truncate(m_['content'])}")
            if ch["unread_count"] > len(msgs):
                lines.append(f"  …{ch['unread_count'] - len(msgs)} mensajes más sin mostrar (cursor avanzado solo hasta T{msgs[-1]['turn'] if msgs else '?'})")
        lines.append(f"\n(Cursores avanzados. Revisa también tu inbox: tylluan_recall '@inbox' agent_id={agent_id})")
        return "\n".join(lines)
    except Exception as e:
        return f"❌ Error in whats_new: {e}"


@mcp.tool()
def list_channels() -> str:
    """List all Coloquio channels.
    Use for: lista canales coloquio, ver canales, list coloquio channels, qué canales hay,
    canales disponibles, mostrar canales coloquio.
    """
    try:
        data = _get("/api/v1/coloquio/channels")
        channels = data if isinstance(data, list) else data.get("channels", [])
        if not channels:
            return "No Coloquio channels created yet. Create them from the dashboard (Team tab)."
        lines = ["=== Coloquio Channels ==="]
        for c in channels:
            lines.append(
                f"• {c['channel_id']} — \"{c['name']}\" "
                f"({c.get('message_count', 0)} msgs, last turn: {c.get('last_turn', 0)})"
            )
        return "\n".join(lines)
    except Exception as e:
        return f"❌ Error listing channels: {e}"


@mcp.tool()
def create_channel(channel_id: str, name: str) -> str:
    """Create a new Coloquio channel.
    Use for: crear canal coloquio, nuevo canal, create channel, crea un canal coloquio.
    """
    try:
        result = _post("/api/v1/coloquio/channels", {"channel_id": channel_id, "name": name})
        ch = result.get("channel", result)
        return f"✅ Canal '{ch.get('channel_id', channel_id)}' creado: \"{ch.get('name', name)}\""
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        if "UNIQUE" in body or e.code == 409:
            return f"ℹ️ El canal '{channel_id}' ya existe."
        return f"❌ Error creating channel (HTTP {e.code}): {body}"
    except Exception as e:
        return f"❌ Error: {e}"


@mcp.tool()
def post_to_coloquio(channel_id: str, message: str, agent_id: str) -> str:
    """Post a message to a Coloquio channel with explicit agent identity.
    Designed for agents that do not have direct file-system access.
    Use for: post to colloquio, write to channel, hello world colloquio.
    Parameters:
      channel_id: slug of the channel (e.g. 'general', 'active-mission', 'quick-test')
      message:    text content to post
      agent_id:   YOUR identifier (e.g. 'agent-1', 'agent-2') — REQUIRED
    Returns the turn number assigned by the kernel.
    """
    if not channel_id or not message or not agent_id:
        return "❌ All parameters are required: channel_id, message, agent_id"
    try:
        quoted_id = urllib.parse.quote(channel_id, safe="")
        result = _post(
            f"/api/v1/coloquio/channels/{quoted_id}/post",
            {"author_id": agent_id, "role": "agent", "content": message, "metadata": "{}"},
        )
        return (f"Posted to '#{channel_id}' as @{agent_id} — "
                f"turn {result.get('turn', '?')}")
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        return f"Error posting (HTTP {e.code}): {body}"
    except Exception as e:
        return f"Error posting: {e}"


from guilds.core import utils

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
