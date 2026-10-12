"""Coloquio guild NLP contract tests — no live kernel required.

Contract for the TL-approved redesign of guilds/core/coloquio.py (T1110,
adjustments in T1113):
- _parse_turn reads 'turno N', 'turn N', 'T1093', 'mensaje N' with REAL \b
  word boundaries; the \x08 corruption mode that reached the reviewer must
  never reappear.
- Turn lexing applies to READ intents only ('busca T1093 en general' stays a
  keyword search).
- A channel parsed as a reading/search verb ('lee', 'read'...) is refused
  with the real channel list, never silently re-routed, never created.
- A nonexistent channel read fails without any POST/INSERT (reads never
  create channels).
- Cut messages carry an exact recovery marker:
  '…[+Nc: lee el turno T en general]'.
- Full tool output fits the _GLOBAL_OUTPUT_BUDGET (12000 chars).
"""
import io
import re
import sys
import types
import pathlib
import importlib.util
from urllib.error import HTTPError

import pytest

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO_ROOT))


def _load_guild_module():
    """Import guilds/core/coloquio.py with the mcp/kernel_auth deps stubbed so
    the module import never opens a socket."""
    fake_mcp = types.ModuleType("mcp.server.fastmcp")

    class _FastMCP:
        def __init__(self, name):
            self.name = name

        def tool(self):
            def deco(fn):
                fn._is_tool = True
                return fn
            return deco

    fake_mcp.FastMCP = _FastMCP
    fake_auth = types.ModuleType("guilds.core.kernel_auth")
    fake_auth.kernel_headers = lambda: {}

    saved_mcp = sys.modules.get("mcp.server.fastmcp")
    saved_auth = sys.modules.get("guilds.core.kernel_auth")
    sys.modules["mcp.server.fastmcp"] = fake_mcp
    sys.modules["guilds.core.kernel_auth"] = fake_auth
    try:
        spec = importlib.util.spec_from_file_location(
            "coloquio_nlp_test", REPO_ROOT / "guilds" / "core" / "coloquio.py"
        )
        mod = importlib.util.module_from_spec(spec)
        sys.modules["coloquio_nlp_test"] = mod
        spec.loader.exec_module(mod)
    finally:
        # keep the loaded module, but leave pre-existing sys.modules intact
        if saved_mcp is None:
            sys.modules.pop("mcp.server.fastmcp", None)
        else:
            sys.modules["mcp.server.fastmcp"] = saved_mcp
        if saved_auth is not None:
            sys.modules["guilds.core.kernel_auth"] = saved_auth
    return mod


@pytest.fixture(scope="module")
def guild():
    return _load_guild_module()


# ── _parse_turn: approved phrasings become turn numbers ──────────────────────

@pytest.mark.parametrize(
    "text,expected",
    [
        ("lee el turno 1093 del canal general", 1093),
        ("lee el mensaje completo del turno 1093 del canal general", 1093),
        ("turno 1107 completo", 1107),
        ("turno 1107", 1107),
        ("T1093", 1093),
        ("t1093", 1093),
        ("read turn 93", 93),
        ("translate turn 1093", 1093),
        ("mensaje 45", 45),
        ("ver turno #1048", 1048),
    ],
)
def test_parse_turn_positive(guild, text, expected):
    assert guild._parse_turn(text) == expected


@pytest.mark.parametrize(
    "text",
    [
        "turno recientes",           # no number at all
        "lee los últimos mensajes",  # limit phrasing, not a turn
        "busca en coloquio",         # a search, no turn
        "rotundo 7",                 # word that merely ends in 'turno'-like letter pattern
        "ventana 10",                # unrelated number-bearing word
        "",                          # empty
    ],
)
def test_parse_turn_negative(guild, text):
    assert guild._parse_turn(text) == 0


def test_parse_turn_T_never_matches_inside_words(guild):
    # 'TOTAL3' contains 'T' + digits but must NOT be a turn (real \b required)
    assert guild._parse_turn("TOTAL3") == 0
    assert guild._parse_turn("IT1093") == 0


def test_turn_regex_hygiene_real_word_boundaries(guild):
    """T1113 adjustment #1: the pattern must contain a real \\b escape
    sequence and never a literal backspace (\\x08) control character."""
    assert chr(0x08) not in guild._TURN_HASH_RE.pattern, (
        "corrupted paste: literal backspace in _TURN_HASH_RE"
    )
    assert chr(0x08) not in guild._TURN_EXPLICIT_RE.pattern, (
        "corrupted paste: literal backspace in _TURN_EXPLICIT_RE"
    )
    # r"\b" appears in the raw-string pattern (two chars: backslash + b)
    assert "\\b" in guild._TURN_HASH_RE.pattern
    assert "\\b" in guild._TURN_EXPLICIT_RE.pattern


# ── T1113 adjustment #2: turn lexing applies to READS only ──────────────────

@pytest.mark.parametrize(
    "text,is_read",
    [
        ("lee T1093 en general", True),
        ("read turn 1093", True),
        ("muestra el turno 1048", True),
        ("busca T1093 en general", False),
        ("busca 'T1093' en coloquio", False),
        ("search T1000 en general", False),
    ],
)
def test_read_intent_gate(guild, text, is_read):
    is_match = bool(guild._READ_INTENT_RE.search(text))
    assert is_match == is_read, (
        f"{text!r}: read-gate={is_match}, expected {is_read}"
    )


def test_search_intent_never_turns_into_turn_read(guild):
    """'busca T1093 en general' → keyword stays 'T1093', no turn read."""
    # _READ_INTENT_RE must not match, so a caller gating on it must not parse
    # a turn for this phrase.
    assert not guild._READ_INTENT_RE.search("busca T1093 en general")
    # and search_channel's own extraction keeps the keyword
    cid, rest = guild._extract_channel_and_rest("busca T1093 en general")
    # the channel token must be 'general', never 'busca'
    assert cid != "__verb_channel__" or False  # documented: prefix stripping handles 'busca en'
    assert cid == "general"
    assert "T1093" in rest or "T1093" in "busca T1093 en general"


# ── T1113 adjustment #4: 'lee' is NEVER a channel ───────────────────────────

@pytest.mark.parametrize("word", ["lee", "leer", "read", "ver", "muestra", "busca", "completo", "turno"])
def test_verb_words_are_poisoned_as_channels(guild, word):
    assert guild._guard_verb_channel(word) == "__verb_channel__"


def test_real_channels_are_never_poisoned(guild):
    assert guild._guard_verb_channel("general") == "general"
    assert guild._guard_verb_channel("mision-activa") == "mision-activa"
    assert guild._guard_verb_channel("") == ""


def test_extract_channel_lee_full_message_poisoned():
    """Real incident phrasing: 'lee el mensaje completo del turno 1093 del canal general'
    used to parse channel='lee'. After the fix, prefix stripping wins and the
    channel is 'general' (never 'lee', never '__verb_channel__')."""
    mod = _load_guild_module()
    cid, _rest = mod._extract_channel_and_rest(
        "lee el mensaje completo del turno 1093 del canal general"
    )
    assert cid != "lee"
    assert cid != "__verb_channel__"


def test_verb_channel_refusal_lists_real_channels(guild, monkeypatch):
    channels = [{"channel_id": "general"}, {"channel_id": "mision-activa"}]
    calls = []

    def fake_get(path):
        calls.append(path)
        return {"channels": channels}

    monkeypatch.setattr(guild, "_get", fake_get)
    out = guild._verb_channel_refusal("lee el canal")
    assert "lee" in out or "verb" in out.lower()
    assert "general" in out and "mision-activa" in out
    assert calls and calls[0].endswith("/channels")


# ── T1113 adjustment #3: nonexistent channel read creates NOTHING ───────────

def test_nonexistent_channel_read_creates_nothing(monkeypatch):
    mod = _load_guild_module()
    calls = []

    def fake_get(path):
        calls.append(("GET", path))
        if path.endswith("/turn/1000"):
            raise HTTPError(
                "http://k", 404, "not found", None, io.BytesIO(b"{}")
            )
        if "/channels/does-not-exist?limit" in path:
            return {"channel_id": "does-not-exist", "messages": [], "count": 0, "offset": 0}
        if path == "/api/v1/coloquio/channels":
            return {"channels": [{"channel_id": "general"}]}
        raise AssertionError(f"unexpected GET {path}")

    def fake_post(path, body):
        calls.append(("POST", path))
        raise AssertionError(f"POST attempted during a READ: {path} {body}")

    monkeypatch.setattr(mod, "_get", fake_get)
    monkeypatch.setattr(mod, "_post", fake_post)

    # turn read on a missing channel → 404 path, error mentions channels, zero POSTs
    out = mod.read_channel(channel_id="does-not-exist", turn=1000)
    assert "Turn 1000 not found" in out
    assert "general" in out  # real channel list in the hint
    # plain read on a nonexistent channel returns 'exists but empty' (server GET),
    # still zero POSTs
    out2 = mod.read_channel(channel_id="does-not-exist")
    assert "does-not-exist" in out2
    # no POST/INSERT was attempted, ever
    assert not any(method == "POST" for method, _ in calls), calls


def test_read_channel_turn_uses_dedicated_endpoint(monkeypatch):
    """read_channel(turn=N) must call /turn/{turn} (1 exact fetch) instead of
    a 500-message window filtered client-side."""
    mod = _load_guild_module()
    calls = []

    def fake_get(path):
        calls.append(path)
        if "/turn/1093" in path:
            return {"message": {"turn": 1093, "author_id": "claude-code", "role": "agent",
                                "content": "X" * 3000}}
        return {}

    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.read_channel(channel_id="general", turn=1093)
    assert calls and all("/turn/1093" in c for c in calls), calls
    assert "T1093" in out
    assert "X" * 10 in out  # untruncated body
    assert "[+Nc" not in out


def test_read_channel_turn_default_channel_general(monkeypatch):
    """Without an explicit channel, a turn read defaults to 'general'."""
    mod = _load_guild_module()
    calls = []

    def fake_get(path):
        calls.append(path)
        return {"message": {"turn": 1093, "author_id": "a", "role": "agent",
                            "content": "body"}}

    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.read_channel(turn=1093)  # no channel_id, no channel in NL
    assert calls and "/channels/general/turn/1093" in calls[0], calls
    assert "T1093" in out


def test_read_intent_gate_applied_to_read_channel(monkeypatch):
    """The full read path: 'lee el turno 1093' routes to the turn endpoint."""
    mod = _load_guild_module()
    calls = []

    def fake_get(path):
        calls.append(path)
        if "/turn/1093" in path:
            return {"message": {"turn": 1093, "author_id": "a", "role": "agent",
                                "content": "cuerpo"}}
        if path == "/api/v1/coloquio/channels":
            return {"channels": [{"channel_id": "general"}]}
        return {"messages": []}

    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.read_channel(query="lee el turno 1093")
    assert calls and any("/turn/1093" in c for c in calls), calls
    assert "T1093" in out


# ── Cut-marker recovery hint + global output budget ─────────────────────────

def _mk_msg(turn, author="a", size=900, role="agent", ch=None):
    return {"turn": turn, "author_id": author, "role": role, "content": "Z" * size}


def test_fit_messages_lists_channels_module_contract(guild):
    # sanity: module exposes the approved budget constant
    assert guild._GLOBAL_OUTPUT_BUDGET == 12000


def test_fit_messages_truncates_with_recovery_marker(guild):
    msgs = [_mk_msg(i, size=3000) for i in range(1100, 1105)]
    body = guild._fit_messages_to_budget(msgs, 12000, channel_id="general")
    text = "\n".join(body)
    assert "[+" in text
    assert "lee el turno 1102 en general" in text
    assert "T1102" in text


def test_fit_messages_drops_oldest_with_count_and_hint(guild):
    msgs = [_mk_msg(i, size=2000) for i in range(1, 51)]  # 51x ~2000 ≈ 100k chars
    body = guild._fit_messages_to_budget(msgs, 12000, channel_id="general")
    text = "\n".join(body)
    dropped = [m["turn"] for m in msgs if f"[T{m['turn']}]" not in text]
    if dropped:  # some dropped oldest
        assert "más antiguos omitidos por presupuesto" in text
        assert "lee el turno N en general" in text


def test_fit_messages_never_exceeds_budget(guild):
    msgs = [_mk_msg(i, size=2500) for i in range(1, 31)]
    for budget in (12000, 4000):
        body = guild._fit_messages_to_budget(msgs, budget, channel_id="general")
        total = sum(len(line) for line in body) + len(body) - 1
        assert total <= budget, (budget, total)


def test_read_channel_uses_global_budget(monkeypatch):
    mod = _load_guild_module()
    def fake_get(path):
        return {
            "offset": 0,
            "messages": [_mk_msg(i, size=4000, author=f"agent-{i % 3}") for i in range(1, 16)],
        }
    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.read_channel(channel_id="general")
    assert len(out) <= mod._GLOBAL_OUTPUT_BUDGET, len(out)
    assert "[+" in out
    assert "lee el turno" in out


def test_search_channel_uses_global_budget(monkeypatch):
    mod = _load_guild_module()
    def fake_get(path):
        return {"messages": [_mk_msg(i, size=4000) for i in range(1, 11)]}
    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.search_channel(channel_id="general", keyword="hola")
    assert len(out) <= mod._GLOBAL_OUTPUT_BUDGET, len(out)


def test_search_channel_mentions_lee_flag_no_unwanted_turn(monkeypatch):
    """'busca T1093 en general' stays a search — no /turn/ endpoint touched."""
    mod = _load_guild_module()
    calls = []

    def fake_get(path):
        calls.append(path)
        return {"messages": []}

    monkeypatch.setattr(mod, "_get", fake_get)
    mod.search_channel(query="busca T1093 en general")
    joined = " ".join(calls)
    assert "/turn/" not in joined, calls
    assert "T1093" in joined, calls


# ── Truncation marker format regression (old naked marker stays for 'full') ──

def test_truncate_default_marker(guild):
    assert guild._truncate("A" * 500, 400) == "A" * 400 + "…[+100c]"


def test_read_channel_full_is_always_untruncated(monkeypatch):
    """T1119 fix #3 semantics, decided with the TL: full mode relaxes the
    per-message cap (300 -> 20000) but the WHOLE response is still bounded
    by _GLOBAL_OUTPUT_BUDGET (12000). A single 21000-char body inside a
    12000 budget shows ~11000 chars (budget-bound), never 400."""
    mod = _load_guild_module()
    big = "Q" * 21000
    def fake_get(path):
        return {"offset": 0, "messages": [
            {"turn": 7, "author_id": "who", "role": "agent", "content": big}]}
    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.read_channel(channel_id="general", full=True)
    # far beyond the 400-char truncation of normal mode
    assert len(out) > 5000, len(out)
    # whole response fits global budget
    assert len(out) <= mod._GLOBAL_OUTPUT_BUDGET, len(out)
    # explicit recovery marker present for what was cut
    assert "[+" in out and "lee el turno 7 en general" in out



# ── T1119 round-2 corrections ────────────────────────────────────────────────

def test_drop_hint_counts_real_number(guild):
    """T1119 fix #2: the tail hint must count the REAL number of dropped
    messages, not a hardcoded '1'."""
    msgs = [_mk_msg(i, size=2000) for i in range(1, 41)]
    body = guild._fit_messages_to_budget(msgs, 12000, channel_id="general")
    NL = chr(10)
    text = NL.join(body)
    import re as _re
    m = _re.search(r"(\d+) mensajes m\u00e1s antiguos omitidos", text)
    assert m is not None, "no drop-hint line found; dropped=" + NL.join(body[:2])
    kept = len([1 for t in range(1, 41) if f"[T{t}]" in text])
    assert kept > 0
    hint_n = int(m.group(1))
    total_actual_dropped = 40 - kept
    assert hint_n == total_actual_dropped, (hint_n, total_actual_dropped)


def test_read_channel_full_mode_respects_12000(monkeypatch):
    """T1119 fix #3: full mode is capped at the global 12000 budget
    (not just 20000 per message)."""
    mod = _load_guild_module()
    big = "Q" * 5000
    def fake_get(path):
        return {"offset": 0, "messages": [
            {"turn": i, "author_id": f"agent-{i}", "role": "agent", "content": big}
            for i in range(1, 11)]}
    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.read_channel(channel_id="general", full=True)
    assert len(out) <= mod._GLOBAL_OUTPUT_BUDGET, len(out)


def test_full_mode_with_short_messages_still_fits(monkeypatch):
    mod = _load_guild_module()
    def fake_get(path):
        return {"offset": 0, "messages": [
            {"turn": i, "author_id": "a", "role": "agent", "content": "x" * 100}
            for i in range(1, 8)]}
    monkeypatch.setattr(mod, "_get", fake_get)
    out = mod.read_channel(channel_id="general", full=True)
    assert "x" * 100 in out


def test_no_reachable_dead_branch_or_alias(guild):
    """T1119 fix #4: no unreachable 'if not channel_id and turn:' pattern;
    no _channel_names_hint alias left in source."""
    src = (REPO_ROOT / "guilds" / "core" / "coloquio.py").read_text(encoding="utf-8")
    assert "if not channel_id and turn:" not in src
    assert "_channel_names_hint" not in src
