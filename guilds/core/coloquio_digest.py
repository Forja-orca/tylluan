"""Coloquio Digest guild — on-demand and scheduled pipeline: coloquio → SilvaDB.

Exposes MCP tools so any agent can trigger digestion via tylluan_do guild=coloquio_digest.
For the periodic background runner use scripts/coloquio_summarizer.py instead.

Tools:
  digest_channel(channel_id, batch_size, force)  — digest one channel
  digest_all_channels(batch_size)                — digest every channel with new messages
  auto_reason_cycle(max_nodes, depth)            — zero-shot CoT over recent SilvaDB nodes
  digest_status()                                — show checkpoint offsets
"""
import hashlib
import json
import re
import sqlite3
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("coloquio_digest")

KERNEL_BASE = "http://127.0.0.1:3030"
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
CHECKPOINT_DB = _REPO_ROOT / "data" / "coloquio_digest.db"
_MIN_CONTENT_LENGTH = 15


# ── internal helpers ──────────────────────────────────────────────────────────

def _init_db():
    CHECKPOINT_DB.parent.mkdir(exist_ok=True)
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS checkpoints (
            channel_id   TEXT PRIMARY KEY,
            last_offset  INTEGER DEFAULT 0,
            digested_at  TEXT
        )
    """)
    conn.commit()
    conn.close()


def _get_checkpoint(channel_id: str) -> int:
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    row = conn.execute("SELECT last_offset FROM checkpoints WHERE channel_id=?", (channel_id,)).fetchone()
    conn.close()
    return row[0] if row else 0


def _set_checkpoint(channel_id: str, offset: int):
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    conn.execute("""
        INSERT INTO checkpoints(channel_id, last_offset, digested_at)
        VALUES(?,?,datetime('now'))
        ON CONFLICT(channel_id) DO UPDATE
          SET last_offset=excluded.last_offset, digested_at=excluded.digested_at
    """, (channel_id, offset))
    conn.commit()
    conn.close()


def _init_hash_db():
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS digest_hashes (
            hash_hex TEXT PRIMARY KEY,
            channel_id TEXT NOT NULL,
            stored_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
    """)
    conn.commit()
    conn.close()


def _hash_exists(hash_hex: str) -> bool:
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    row = conn.execute("SELECT 1 FROM digest_hashes WHERE hash_hex=?", (hash_hex,)).fetchone()
    conn.close()
    return row is not None


def _store_hash(hash_hex: str, channel_id: str):
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    conn.execute(
        "INSERT OR IGNORE INTO digest_hashes(hash_hex, channel_id) VALUES(?,?)",
        (hash_hex, channel_id),
    )
    conn.commit()
    conn.close()


def _is_noise(text: str) -> bool:
    if len(text.strip()) < _MIN_CONTENT_LENGTH:
        return True
    if not re.search(r'[a-zA-Z0-9\u00C0-\u024F\u0400-\u04FF]', text, re.IGNORECASE):
        return True
    stripped = text.strip()
    if re.match(r'^(https?://\S+)$', stripped, re.IGNORECASE):
        return True
    if any(stripped.startswith(p) for p in ('[CERRADO]', '[CANCELADO]', '[RESUELTO]', '> ', 'Run:', 'Command:', '```')):
        return True
    return False


def _get(path: str, timeout: int = 15) -> dict:
    with urllib.request.urlopen(f"{KERNEL_BASE}{path}", timeout=timeout) as r:
        return json.loads(r.read())


def _post(path: str, body: dict, timeout: int = 30) -> dict:
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        f"{KERNEL_BASE}{path}", data=data,
        headers={"Content-Type": "application/json"}, method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def _build_summary(channel_id: str, messages: list) -> str:
    """Deterministic summary — no LLM required, preserves author attribution."""
    if not messages:
        return ""
    filtered = []
    for m in messages:
        content = str(m.get("content") or m.get("message") or "").strip()
        if content and not _is_noise(content):
            filtered.append(m)
    if not filtered:
        return ""
    lines = [f"[coloquio_digest] #{channel_id} — {len(filtered)} messages (filtered from {len(messages)}):"]
    for m in filtered[:30]:
        author = m.get("author_id") or m.get("author") or "unknown"
        content = str(m.get("content") or m.get("message") or "").strip()
        preview = content[:280] + "…" if len(content) > 280 else content
        lines.append(f"  [{author}] {preview}")
    return "\n".join(lines)[:3500]


_HASH_CACHE: dict[str, bool] = {}

def _store(content: str, channel_id: str = "") -> bool:
    content_hash = hashlib.sha256(content.encode("utf-8")).hexdigest()
    if content_hash in _HASH_CACHE:
        return True
    _init_hash_db()
    if _hash_exists(content_hash):
        _HASH_CACHE[content_hash] = True
        return True
    try:
        _post("/api/v1/memory/write", {"content": content})
        _store_hash(content_hash, channel_id or "unknown")
        _HASH_CACHE[content_hash] = True
        return True
    except Exception as e:
        import sys
        print(f"[coloquio_digest] _store failed: {e}", file=sys.stderr)
        return False


# ── MCP tools ─────────────────────────────────────────────────────────────────

@mcp.tool()
def digest_channel(
    channel_id: str,
    batch_size: int = 50,
    force: bool = False,
) -> str:
    """Digest new messages from one coloquio channel and store a summary in SilvaDB.

    Args:
        channel_id: Slug of the channel (e.g. 'mision-activa').
        batch_size: Messages per batch (default 50).
        force:      If True, re-digest from offset 0 ignoring the checkpoint.
    """
    _init_db()
    offset = 0 if force else _get_checkpoint(channel_id)
    quoted = urllib.parse.quote(channel_id, safe="")
    try:
        data = _get(f"/api/v1/coloquio/channels/{quoted}?limit={batch_size}&offset={offset}")
    except urllib.error.HTTPError as e:
        return f"❌ Channel '{channel_id}' not found (HTTP {e.code})." if e.code == 404 else f"❌ HTTP {e.code}: {e}"
    except Exception as e:
        return f"❌ Error reading channel: {e}"

    messages = data.get("messages", [])
    if not messages:
        return f"⏭ No new messages in '{channel_id}' since offset {offset}."

    summary = _build_summary(channel_id, messages)
    if not summary:
        return f"⏭ All {len(messages)} messages in '{channel_id}' filtered as noise or duplicates."

    stored = _store(summary, channel_id)
    new_offset = offset + len(messages)
    _set_checkpoint(channel_id, new_offset)

    icon = "✅" if stored else "⚠️ (storage failed)"
    return f"{icon} Digested {len(messages)} msgs from '{channel_id}' (offset {offset}→{new_offset})."


@mcp.tool()
def digest_all_channels(batch_size: int = 30) -> str:
    """Digest all coloquio channels with messages newer than their last checkpoint.

    Args:
        batch_size: Messages per channel per run.
    """
    _init_db()
    try:
        data = _get("/api/v1/coloquio/channels")
    except Exception as e:
        return f"❌ Cannot list channels: {e}"

    channels = data.get("channels", [])
    if not channels:
        return "ℹ No channels found."

    results = []
    for ch in channels:
        cid = ch.get("channel_id") or ch.get("id") or ""
        if not cid:
            continue
        try:
            result = digest_channel(cid, batch_size)
        except Exception as ex:
            result = f"❌ {ex}"
        results.append(f"• {cid}: {result}")

    return f"Digest complete ({len(channels)} channels):\n" + "\n".join(results)


def _run_reasoning_background(node_ids: list[str], depth: int):
    """Background task: call deep_analysis with a long timeout, store result."""
    seeds_preview = ", ".join(node_ids[:5])
    try:
        result = _post("/api/v1/do", {
            "intent": f"synthesize and extract insights from these recent knowledge nodes: {seeds_preview}",
            "guild": "deep_analysis",
            "remember": True,
        }, timeout=600)
        output = ""
        if isinstance(result, dict):
            output = result.get("result") or result.get("content") or str(result)
        else:
            output = str(result)
        if output:
            synthesis = f"[auto_reason_cycle] Over {len(node_ids)} nodes (depth={depth}):\n{str(output)[:2000]}"
            _store(synthesis)
            import sys
            print(f"[auto_reason_cycle] ✅ Stored synthesis from {len(node_ids)} nodes", file=sys.stderr)
        else:
            import sys
            print(f"[auto_reason_cycle] ⚠ deep_analysis returned no output", file=sys.stderr)
    except Exception as e:
        import sys
        print(f"[auto_reason_cycle] ❌ Background reasoning failed: {e}", file=sys.stderr)


@mcp.tool()
def auto_reason_cycle(max_nodes: int = 10, depth: int = 2) -> str:
    """Zero-shot CoT reasoning cycle over the most recent SilvaDB nodes.

    Runs as a background thread (returns immediately). The result is stored
    in SilvaDB when ready — check digest_status or silva/recent for results.

    Args:
        max_nodes: Number of recent nodes to use as seeds (default 10).
        depth:     PPR reasoning depth 1-3 (default 2).
    """
    try:
        recent = _get(f"/api/v1/silva/recent?limit={max_nodes}")
    except Exception as e:
        return f"❌ Cannot fetch recent nodes: {e}"

    node_ids = [n.get("id") or n.get("node_id") for n in recent if n.get("id") or n.get("node_id")]
    if not node_ids:
        return "ℹ No recent nodes to reason over — seed the graph first."

    thread = threading.Thread(
        target=_run_reasoning_background,
        args=(node_ids, depth),
        daemon=True,
    )
    thread.start()
    return f"⏳ Reasoning started in background over {len(node_ids)} nodes (depth={depth}). Check silva/recent or digest status for the synthesis result."


@mcp.tool()
def digest_status() -> str:
    """Show checkpoint state for all digested channels (offset + last run time)."""
    _init_db()
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    rows = conn.execute(
        "SELECT channel_id, last_offset, digested_at FROM checkpoints ORDER BY digested_at DESC"
    ).fetchall()
    conn.close()

    if not rows:
        return "ℹ No channels digested yet. Run digest_all_channels() to start."

    lines = ["📋 Coloquio Digest — checkpoint status:"]
    for cid, offset, ts in rows:
        lines.append(f"  • {cid:30s}  offset={offset:<6d}  last={ts}")
    return "\n".join(lines)


if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
