"""Coloquio guild — read and write group chat channels in TylluanNexus."""
import json
import os
import re
import urllib.request
import urllib.error
import urllib.parse
from pathlib import Path
from mcp.server.fastmcp import FastMCP

try:
    from guilds.core import kernel_auth
except ImportError:  # direct-script run: python guilds/core/coloquio.py
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
    from guilds.core import kernel_auth

mcp = FastMCP("coloquio")


def _resolve_kernel_base() -> str:
    if "KERNEL_BASE" in os.environ:
        return os.environ["KERNEL_BASE"]
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    try:
        data = json.loads(port_file.read_text())
        port = data.get("port", 47004)
        return f"http://127.0.0.1:{port}"
    except Exception:
        return "http://127.0.0.1:47004"


KERNEL_BASE = _resolve_kernel_base()

# ── HTTP helpers ──────────────────────────────────────────────────────────────

def _get(path: str) -> dict:
    req = urllib.request.Request(f"{KERNEL_BASE}{path}", headers=kernel_auth.kernel_headers())
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())


def _post(path: str, body: dict) -> dict:
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        f"{KERNEL_BASE}{path}",
        data=data,
        headers=kernel_auth.kernel_headers(),
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())


# ── Channel-ID extraction helpers ────────────────────────────────────────────

# Words that can NEVER be a channel slug: read/search verbs (+ parameters-ish
# filler words) that used to fall through the slug regex and get adopted as a
# channel name ('lee el mensaje completo del turno 1093 del canal general' ->
# channel "lee" -> "Channel lee exists but has no messages yet"). Approved in
# the coloquio design (T1110) as a closed list: if the parsed channel
# candidate lands here, the guild must not silently re-route to another
# channel (T1113 adjustment #4) -- it must refuse with the real channel list
# so the caller can re-state the request unambiguously.
_VERB_CHANNEL_BLACKLIST = frozenset({
    "lee", "leer", "read", "ver", "muestra", "mostrar",
    "busca", "buscar", "search", "find", "completo", "completa",
    "full", "turno", "turn", "mensaje", "message",
    "ultimos", "últimos", "last", "limit", "offset",
    "desde", "author", "autor", "hilo", "thread", "coloquio",
})

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

    Ordering inside this function matters for the read-by-turn path: the
    'del canal X' / ' in X' mentions and the stop-word cutoff are tried
    BEFORE the leading-slug regex, so that 'el turno 1093' (whose only
    slug-safe token is the verb 'lee') resolves to '' channel + the turn in
    rest, never to channel='lee'.
    """
    s = _CHANNEL_PREFIXES.sub("", raw).strip()
    if not s:
        return "", ""
    # Turn-scoped reads name their channel AFTER the turn phrase: 'lee el
    # turno 1093 del canal general'. The leftover leading token may be a
    # verb ('lee') + 'el' — both unusable as a channel. Detect and strip
    # leading 'el/la/the' + STALE verb remnants so the explicit channel
    # mention below can win; if the leftover leading slug is still a verb,
    # the ordinary read path refuses with the verb-channel guard anyway.
    s = re.sub(r'^(?:el|la|los|las|the)\s+', "", s, flags=re.IGNORECASE)
    # Explicit channel mention anywhere in the tail: 'del canal X', 'en el
    # canal X', 'del channel X', 'in channel X'. Wins over leading-token
    # extraction so verb-led phrasings ('lee el mensaje completo del turno
    # 1093 del canal general') resolve to the named channel, never to the
    # verb.
    m_canal = re.search(r'(?:\bdel\s+|\bde\s+|\ben\s+\bel\s+|\bin\s+|\bfrom\s+)?\b(?:canal|channel)\s+([\w][\w\-]*)', s, re.IGNORECASE)
    if m_canal:
        channel = _guard_verb_channel(m_canal.group(1))
        if channel == "__verb_channel__":
            return channel, s
        s_without = (s[:m_canal.start()] + " " + s[m_canal.end():]).strip()
        return channel, s_without.strip(" ,:;")
    # 'busca T1093 en general' / 'search foo in mision-activa': trailing
    # ' en X' / ' in X' names the channel while the keyword sits before it.
    m_en = re.search(r'\s+(?:en|in)\s+([\w][\w\-]*)\s*$', s, re.IGNORECASE)
    if m_en:
        channel = _guard_verb_channel(m_en.group(1))
        if channel == "__verb_channel__":
            return channel, s
        s_without = s[:m_en.start()].strip()
        return channel, s_without
    m = _STOP_WORDS.search(s)
    if m:
        return _guard_verb_channel(s[:m.start()].strip()), s[m.start():].strip()
    # Otherwise take only slug-safe leading tokens
    # Delimiters: whitespace+non-ws, OR any non-slug char (:, ., !, etc.), OR end-of-string
    slug_match = re.match(r'^([\w][\w\-]*(?:\s+[\w][\w\-]*)*?)(?=\s+\S|[^\w\-]|$)', s)
    if slug_match:
        candidate = slug_match.group(1).strip()
        rest = s[len(candidate):].strip()
        return _guard_verb_channel(candidate), rest
    # Fallback: everything is the channel
    return _guard_verb_channel(s.strip()), ""


import unicodedata


def _normalize_candidate_slug(word: str) -> str:
    """Lowercase NFKD-fold a candidate channel token so that 'Léeme' / 'LEE' /
    'léeme' all collapse to blacklist members ('leeme' not itself listed, but
    folded accents let 'Últimos' match 'ultimos' etc.). Spanish user phrasings
    arrive with accents; the slug regex accepts them; the blacklist must too."""
    return unicodedata.normalize("NFKD", word).encode("ascii", "ignore").decode("ascii").lower()


def _guard_verb_channel(candidate: str) -> str:
    """Return a poisoning marker if the channel candidate is a verb parameter
    word (closed list), else the candidate unchanged. Callers compare against
    this marker ('__verb_channel__') and refuse the read with the real list of
    channels instead of silently re-routing (T1113 adjustment #4)."""
    if not candidate:
        return candidate
    lowered = _normalize_candidate_slug(candidate)
    return "__verb_channel__" if lowered in _VERB_CHANNEL_BLACKLIST else candidate


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


# Global output budget for read_channel / search_channel (approved design):
# the FULL tool response must fit in this many characters, distributing per-
# message space proportionally and telling the caller exactly how to recover
# each cut message (its turn number is always in the row header).
_GLOBAL_OUTPUT_BUDGET = 12000


def _fit_messages_to_budget(msgs: list, budget: int, channel_id: str = "",
                            per_message_default_cap: int = 300) -> list:
    """Render body lines for `msgs` within `budget` total chars.

    Contract:
    - The most RECENT messages are kept in full first; older ones get less.
    - Bodies are truncated with a marker pointing at the exact recovery
      action: '…[+2057c: lee el turno 1048 en general]'.
    - When even the trimmed rendering doesn't fit, the OLDEST lines are
      dropped and counted in a tail hint with the REAL number dropped.
    - Joined output fits within `budget` (asserted by tests). Always
      terminates: each iteration of the shave loop strictly shrinks a body
      slice or removes a line.
    """
    if budget <= 0 or not msgs:
        return []
    ch = channel_id or "general"

    MARKER = "…"
    DROP_HINT_TPL = ("  " + MARKER + "{n} mensajes más antiguos omitidos por presupuesto;"
                     " recupéralos uno a uno con 'lee el turno N en " + ch + "'.")

    def render(m):
        role_icon = "👤" if m.get("role") == "human" else "🤖"
        header = f"[T{m['turn']}] {role_icon} {m.get('author_id', '?')}: "
        body = m.get("content", "") or ""
        return header, body

    def line(header, body_show, full_len, turn):
        if len(body_show) < full_len:
            return f"{header}{body_show}{MARKER}[+{full_len - len(body_show)}c: lee el turno {turn} en {ch}]"
        return header + body_show

    def hint_cost(n):
        return (len(DROP_HINT_TPL.format(n=n)) + 1) if n else 0

    rendered = [render(m) for m in msgs]
    n = len(rendered)

    # Phase 1: drop the oldest messages until the GUARANTEED minimum
    # rendering (header + floor body + marker + newline x keep_n + hint) fits.
    dropped = 0
    while n - dropped > 1:
        keep_n = n - dropped
        tail = rendered[dropped:]
        headers_len = sum(len(h) for h, _ in tail)
        need = headers_len + keep_n + hint_cost(dropped) + keep_n * 105  # 60 body + 45 marker
        if need <= budget:
            break
        dropped += 1

    keep_n = n - dropped
    tail = rendered[dropped:]
    turns_kept = [m["turn"] for m in msgs[dropped:]]
    headers_len = sum(len(h) for h, _ in tail)
    available = budget - headers_len - keep_n - hint_cost(dropped)

    def cost(lines):
        return sum(len(l) for l in lines) + len(lines) - 1

    # Phase 2: distribute body budget proportionally, but never below 30 and
    # never above what's actually there.
    shares = []
    for h, b in tail:
        cap = min(per_message_default_cap, len(b))
        shares.append(cap)
    total_bodies = sum(shares)
    if cost([h + b[:s] for (h, b), s in zip(tail, shares)]) + len(tail) - 1 > available:
        scale = max(available, 0) / max(total_bodies, 1)
        # Cautiously RESHARE from a copy of the current shares; the previous
        # version reassigned inside the comprehension source list, which made
        # `zip(shares, tail)` consume an already-empties list (real bug
        # caught by the TL review T1119 cycle).
        reshares = []
        for s, (h, b) in zip(shares, tail):
            new_s = max(min(int(s * scale), len(b)), min(30, len(b)))
            reshares.append(new_s)
        shares = reshares
    out = []
    for (h, b), t, s in zip(tail, turns_kept, shares):
        out.append(line(h, b[:s], len(b), t))

    # Phase 3: exact-fit shave. Each iteration either strictly reduces a
    # line's body slice or drops the oldest line entirely, so the loop
    # always terminates.
    while cost(out) > available and out:
        overshoot = cost(out) - available
        h, b = tail[0]
        t = turns_kept[0]
        shown_now = len(out[0]) - len(h)
        # Body slice actually visible in line[0]:
        marker_idx = out[0].find(MARKER)
        body_now = (marker_idx - len(h)) if marker_idx >= 0 else shown_now
        new_show = max(0, body_now - overshoot)
        if new_show <= 0 or new_show >= len(b):
            out.pop(0)
            tail.pop(0)
            turns_kept.pop(0)
            dropped += 1
            available = budget - sum(len(h) for h, _ in tail) - len(tail) - hint_cost(dropped)
        else:
            out[0] = line(h, b[:new_show], len(b), t)

    if dropped:
        out.append(DROP_HINT_TPL.format(n=dropped))
    return out


def _list_channel_ids() -> list:
    """Best-effort live list of existing Coloquio channel ids, for error hints.
    Never raises: on any failure returns []."""
    try:
        data = _get("/api/v1/coloquio/channels")
        channels = data if isinstance(data, list) else data.get("channels", [])
        return [c["channel_id"] for c in channels]
    except Exception:
        return []


def _verb_channel_refusal(raw: str) -> str:
    """Refuse a read whose parsed channel is a verb (closed list), telling the
    caller the real channel list so they can restate the request (T1113
    adjustment #4: no silent re-route to 'general')."""
    chans = _list_channel_ids()
    listing = ", ".join(chans) if chans else "(no channels found)"
    return ("❌ A channel name can never be a reading/search verb like "
            f"'lee'/'read'/'busca'/'search'. Restate with an explicit channel. "
            f"Available channels: {listing}")


def _truncate(text: str, max_chars: int = 400) -> str:
    if len(text) <= max_chars:
        return text
    return text[:max_chars] + f"…[+{len(text)-max_chars}c]"


def _parse_full(text: str) -> bool:
    """Detect an explicit request for untruncated message bodies    ('full', 'completo', 'sin truncar', 'sin resumir')."""
    return bool(re.search(r'\b(full|completo|sin\s+truncar|sin\s+resumir|untruncated)\b', text, re.IGNORECASE))


# ── Turn-number parsing (read-only lexing, approved T1113 adjustment #2) ────────────────────────

# Valid READ-intent anchors used to gate _parse_turn: turn-number lexing only
# applies in reading phrasings, never in search ones ('busca T1093 en general'
# must stay a keyword search for the literal string 'T1093', not silently become
# a turn read).
_READ_INTENT_RE = re.compile(
    r"\b(?:lee|leer|read|ver|muestra|mostrar|dame|dime|get|show)\b",
    re.IGNORECASE,
)

# Explicit word forms: 'turno N', 'turn N', 'mensaje N', optional '#', with
# word boundaries so 'ventana 10' / 'rotundo 3' never match.
_TURN_EXPLICIT_RE = re.compile(
    r"\b(?:turno|turn|mensaje|message)\s+#?(\d+)\b",
    re.IGNORECASE,
)

# 'T1093' compact form. T1113 adjustment #1: the word boundaries are REAL \b
# metacharacters inside this raw string (backslash + b). The previous design
# post reached the reviewer with \x08 backspace control characters in place of
# \b; guarded against here + explicitly asserted in test_coloquio_nlp.py so any
# future corrupt paste fails loudly in the test battery.
_TURN_HASH_RE = re.compile(r"\bT(\d{1,6})\b", re.IGNORECASE)

# Corruption canaries: a literal backspace control char must never appear
# inside any compiled turn pattern.
assert chr(0x08) not in _TURN_EXPLICIT_RE.pattern, "corrupted paste: literal backspace in _TURN_EXPLICIT_RE"
assert chr(0x08) not in _TURN_HASH_RE.pattern, "corrupted paste: literal backspace in _TURN_HASH_RE"
assert r"\b" in _TURN_HASH_RE.pattern and r"\b" in _TURN_EXPLICIT_RE.pattern


def _parse_turn(text: str) -> int:
    """Extract a turn number from a natural-language READ phrase:
    'turno N', 'turn N', 'T1234', 'mensaje N', optionally suffixed with
    modifiers ('turno 1071 completo', 'translate turn 1093', 'T1093 completo').
    Returns the turn number, or 0 when nothing matches (real kernel turn
    numbering is 1-indexed, so 0 is a safe 'no turn requested' sentinel).

    T1113 adjustment #2: callers apply this ONLY on read intents; search
    keyword phrasings ('busca T1093') pass through untouched.
    """
    m = _TURN_EXPLICIT_RE.search(text)
    if m:
        return int(m.group(1))
    m = _TURN_HASH_RE.search(text)
    if m:
        return int(m.group(1))
    return 0


# ── Tools ─────────────────────────────────────────────────────────────────────

@mcp.tool()
def read_channel(channel_id: str = "", query: str = "", intent: str = "",
                 command: str = "", limit: int = 0, offset: int = 0,
                 turn: int = 0, full: bool = False) -> str:
    """Read messages from a Coloquio channel.
    Use for: lee el canal coloquio, leer coloquio, ver mensajes canal, read coloquio channel,
    mostrar hilo coloquio, get thread, ver conversacion grupal, historial coloquio,
    lee el coloquio, muestra el coloquio, ver canal, lee canal, leer canal, lee el canal,
    read channel, lee mensajes del canal, leer mensajes del canal, lee los ultimos turnos,
    leer los ultimos mensajes, lee los turnos, lee el hilo, ver mensajes del canal.
    Supports: limit N, offset N, 'ultimos N', 'last N'.
    By default each message body is truncated to 400 chars ('...[+Nc]' marker) to keep
    multi-message reads compact. Pass full=true (or say 'completo'/'full'/'sin truncar'
    in the intent) to get untruncated bodies -- capped at 20000 chars per message as a
    DoS guard, matching the pattern used elsewhere in the security claims gate. Pass
    turn=N to fetch a single message by its turn number in full, regardless of `full`
    (a targeted turn read is always untruncated -- there would be no point otherwise).
    Without this, any MCP client without raw filesystem/DB access has no way to recover
    a message body beyond 400 chars -- confirmed as a real gap, not a design choice
    (found 2026-08-21, see Coloquio mision-activa).
    """
    raw = query or intent or command or ""
    # `offset` is ambiguous by construction: 0 means both "caller didn't ask
    # for one" AND "a real, explicit offset of zero". Track which one this
    # actually is -- it matters below (found 2026-08-23, real bug: read_channel
    # was always sending offset=0 to the REST API, which silently defeated
    # that API's own "give me the most recent N messages" fallback and made
    # every default read return the OLDEST 50 messages in the channel
    # instead, no matter how many turns the channel had accumulated).
    offset_explicit = offset != 0
    # Turn lexing is a READ-only feature (T1113 adjustment #2): parse the turn
    # number BEFORE _extract_channel_and_rest, straight off the original raw
    # string, so that 'turno'/'T...' tokens can never contaminate the channel
    # slug extraction (and the slug extraction can never eat the turn number).
    natural_turn = _parse_turn(raw) if raw else 0
    if turn == 0 and natural_turn:
        turn = natural_turn
    # A turn read without a named channel defaults to 'general' (approved
    # design; reads never create channels, so this cannot fragment
    # anything). Covers BOTH the explicit turn= kwarg and the NL-parsed
    # turn; resolved BEFORE the verb-refusal so the read routes cleanly.
    if turn and not channel_id:
        channel_id = "general"
        rest = ""

    if not channel_id:
        channel_id, rest = _extract_channel_and_rest(raw)
        if limit == 0:
            limit = _parse_limit(rest, 50)
        if offset == 0:
            parsed_offset = _parse_offset(rest)
            if parsed_offset != 0:
                offset = parsed_offset
                offset_explicit = True
    elif limit == 0:
        limit = _parse_limit(raw, 50)
    if limit == 0:
        limit = 50
    if not full:
        full = _parse_full(raw)

    if not channel_id:
        # T1113 adjustment #4: a channel that IS a reading/search verb is not a
        # channel. NO silent defaulting to 'general' here -- refuse with the
        # actual channel list so the caller can restate unambiguously.
        chans = _list_channel_ids()
        return ("❌ Specify a channel name: 'read the coloquio <channel-name>'. "
                f"Available channels: {', '.join(chans) if chans else '(none)'}")
    if channel_id == "__verb_channel__":
        return _verb_channel_refusal(raw)
    try:
        quoted_id = urllib.parse.quote(channel_id, safe="")
        # Turn-scoped read: call the dedicated REST endpoint instead of
        # downloading a 500-message window and filtering client-side (the
        # endpoint existed all along: /api/v1/coloquio/channels/{id}/turn/{turn}).
        # One exact row, no truncation, no window guessing, no 404-because-it-
        # was-older-than-the-window. Turn default channel is 'general' --
        # confirmed with the kernel (T1113): reads never create channels.
        if turn:
            turn_channel = channel_id or "general"
            quoted_turn_channel = urllib.parse.quote(turn_channel, safe="")
            try:
                data = _get(f"/api/v1/coloquio/channels/{quoted_turn_channel}/turn/{turn}")
            except urllib.error.HTTPError as e:
                if e.code == 404:
                    chans = _list_channel_ids()
                    return (f"Turn {turn} not found in channel '{turn_channel}'. "
                            f"Available channels: {', '.join(chans) if chans else '(none)'}")
                raise
            msg = data.get("message") or data
            if not msg:
                return f"Turn {turn} not found in channel '{turn_channel}'."
            role_icon = "👤" if msg.get("role") == "human" else "🤖"
            return (f"=== Coloquio: {turn_channel} — turn {msg['turn']} ===\n"
                    f"[T{msg['turn']}] {role_icon} {msg['author_id']}: {msg['content']}")
        # Non-turn read: normal thread listing (up to 20000 chars), no
        # pagination games. The turn/no-turn split means 'turn=N' is never
        # silently defeated by the REST offset default.
        url = f"/api/v1/coloquio/channels/{quoted_id}?limit={limit}"
        if offset_explicit:
            url += f"&offset={offset}"
        data = _get(url)
        msgs = data.get("messages", [])
        # Show the offset the SERVER actually used, not the local variable --
        # when offset_param was omitted above, the server computed its own
        # (the "most recent N" default), and echoing our stale local value
        # here would misreport it.
        real_offset = data.get("offset", offset)
        if not msgs:
            return f"Channel '{channel_id}' exists but has no messages yet (offset={real_offset})."
        header = f"=== Coloquio: {channel_id} ({len(msgs)} messages, offset={real_offset}) ==="
        lines = [header]
        # Global budget applies in BOTH modes (T1110 point 7, re-asserted in
        # T1119): 'full' is an untruncated-per-message mode, not an exempt
        # tool call. Older messages overflow the budget and get the same
        # recovery marker/hint as normal mode; each individual body is also
        # capped at 20000 chars (DoS guard).
        body = _fit_messages_to_budget(
            msgs,
            _GLOBAL_OUTPUT_BUDGET - len(header) - 1,
            channel_id=channel_id,
            per_message_default_cap=20000 if full else 300,
        )
        lines.extend(body)
        return "\n".join(lines)
    except urllib.error.HTTPError as e:
        if e.code == 401:
            return ("❌ Kernel auth failed (HTTP 401): no valid bearer token. "
                    "The kernel injects TYLLUAN_TOKEN into guild envs; check that .tylluan-token exists and matches.")
        if e.code == 404:
            # T1113 adjustment #3: channel names that don't exist are genuinely
            # typo'd beg -- refuse with the REAL channel list, never create one
            # (reads never create: verified against coloquio.rs -- only
            # post_message and create_channel write INSERT INTO coloquio*).
            chans = _list_channel_ids()
            return (f"❌ Channel '{channel_id}' not found (HTTP 404). Available: "
                    f"{', '.join(chans) if chans else '(none)'}")
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
    SECURITY: this tool is agent-facing and can NEVER post as role="human" --
    the `role` parameter is accepted for backward compatibility but is always
    overridden to "agent" server-side. Impersonating a protected author
    (jose/admin/system) by claiming role="human" was a real, reproducible
    vulnerability (found 2026-07-11: any agent could call this tool with
    role="human", author_id="jose" and it would render indistinguishably
    from a genuine human message, with zero server-side verification).
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
        # Never trust the caller-supplied `role` -- this tool is only ever
        # invoked by agents (via tylluan_do routing), so it must never be
        # able to grant itself the "human" role that bypasses the kernel's
        # protected-author impersonation guard.
        result = _post(
            f"/api/v1/coloquio/channels/{quoted_id}/post",
            {"author_id": author_id, "role": "agent", "content": content, "metadata": "{}"},
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
    # T1113 adjustment #2: a search intent is a SEARCH, never a turn read even
    # if the keyword looks like 'T1093'. No _parse_turn here by design.
    if not channel_id:
        channel_id, rest = _extract_channel_and_rest(raw)
        if channel_id == "__verb_channel__":
            return _verb_channel_refusal(raw)
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
    if channel_id == "__verb_channel__":
        return _verb_channel_refusal(raw)
    try:
        quoted_id = urllib.parse.quote(channel_id, safe="")
        quoted_q = urllib.parse.quote(keyword)
        data = _get(f"/api/v1/coloquio/channels/{quoted_id}/search?q={quoted_q}&limit={limit}")
        msgs = data.get("messages", [])
        if not msgs:
            return f"No results for '{keyword}' in channel '{channel_id}'."
        # Global output budget (approved design): whole response fits in 12000
        # chars. Chars are distributed proportionally to need, and each cut
        # message carries an exact recovery hint in its marker.
        lines = [f"=== Search '{keyword}' in {channel_id} ({len(msgs)} hits) ==="]
        budget = _GLOBAL_OUTPUT_BUDGET - len(lines[0])
        body = _fit_messages_to_budget(msgs, budget, channel_id=channel_id,
                                       per_message_default_cap=300)
        lines.extend(body)
        total = sum(len(l) for l in lines)
        if total > _GLOBAL_OUTPUT_BUDGET:
            # Header + hint fit is guaranteed by the _fit_messages_to_budget
            # contract nested-budget check; this branch is reachable only if
            # the number of cut messages outgrows the tail hint, in which case
            # drop the oldest lines until it fits.
            while len(lines) > 2 and sum(len(l) for l in lines) > _GLOBAL_OUTPUT_BUDGET:
                lines.pop(1)
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
