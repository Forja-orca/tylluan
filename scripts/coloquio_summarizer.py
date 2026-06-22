"""
coloquio_summarizer.py — Periodic Coloquio -> SilvaDB memory pipeline.

Pulls new messages from all Coloquio channels, batches them, and stores
structured summaries via tylluan_remember so the knowledge graph accumulates
conversation context automatically.

ARCHITECTURE (the flywheel):
  Coloquio channels ──► read via HTTP API ──► batch + summarize ──► tylluan_remember ──► SilvaDB
                                                                                          │
                                                                                    tylluan_recall
                                                                                     ▲  agents
                                                                                     │
                                                                              (next cycle richer)

Usage:
    python scripts/coloquio_summarizer.py                  # run once
    python scripts/coloquio_summarizer.py --interval 30    # loop every 30 min
    python scripts/coloquio_summarizer.py --once --channel mision-activa  # single channel

Checkpoint DB: data/summarizer_checkpoints.db (auto-created)
"""

import hashlib, json, os, re, sqlite3, time, urllib.request, urllib.error, urllib.parse, sys, argparse
from datetime import datetime, timezone
from pathlib import Path

KERNEL_BASE = "http://127.0.0.1:3030"
REPO_ROOT = Path(__file__).resolve().parent.parent
CHECKPOINT_DB = REPO_ROOT / "data" / "summarizer_checkpoints.db"
HASH_DB = REPO_ROOT / "data" / "summarizer_hashes.db"
BATCH_SIZE = 20
MAX_CHARS_PER_MEMORY = 1500
_MIN_CONTENT_LENGTH = 15

_CHECKPOINT_DB_INIT = """
CREATE TABLE IF NOT EXISTS checkpoints (
    channel_id TEXT PRIMARY KEY,
    last_offset INTEGER NOT NULL DEFAULT 0,
    last_turn INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    channels_checked INTEGER NOT NULL DEFAULT 0,
    channels_with_new INTEGER NOT NULL DEFAULT 0,
    total_messages INTEGER NOT NULL DEFAULT 0,
    memories_stored INTEGER NOT NULL DEFAULT 0,
    duration_sec REAL NOT NULL DEFAULT 0.0
);
"""


# ── HTTP helpers ──────────────────────────────────────────────────────────────

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


def _init_hash_db():
    HASH_DB.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(HASH_DB))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS content_hashes (
            hash_hex TEXT PRIMARY KEY,
            channel_id TEXT NOT NULL,
            stored_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
    """)
    conn.commit()
    conn.close()


def _hash_exists(hash_hex: str) -> bool:
    conn = sqlite3.connect(str(HASH_DB))
    row = conn.execute("SELECT 1 FROM content_hashes WHERE hash_hex=?", (hash_hex,)).fetchone()
    conn.close()
    return row is not None


def _store_hash(hash_hex: str, channel_id: str):
    conn = sqlite3.connect(str(HASH_DB))
    conn.execute(
        "INSERT OR IGNORE INTO content_hashes(hash_hex, channel_id) VALUES(?,?)",
        (hash_hex, channel_id),
    )
    conn.commit()
    conn.close()


def _get(path: str, timeout: int = 10) -> dict:
    with urllib.request.urlopen(f"{KERNEL_BASE}{path}", timeout=timeout) as r:
        return json.loads(r.read())


def _post_tylluan_remember(content: str, agent_id: str = "coloquio-summarizer",
                         metadata: dict | None = None) -> dict:
    """Store a memory via the kernel's /api/v1/do endpoint routed to tylluan_remember."""
    body = json.dumps({
        "tool": "tylluan_remember",
        "content": content,
        "agent_id": agent_id,
        "metadata": metadata or {},
    }).encode()
    req = urllib.request.Request(
        f"{KERNEL_BASE}/api/v1/do",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        body_text = e.read().decode()
        return {"status": "error", "code": e.code, "detail": body_text[:200]}
    except urllib.error.URLError as e:
        return {"status": "error", "detail": f"Kernel not reachable: {e.reason}"}


# ── Checkpoint DB ─────────────────────────────────────────────────────────────

def _init_db():
    CHECKPOINT_DB.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(CHECKPOINT_DB))
    conn.executescript(_CHECKPOINT_DB_INIT)
    conn.commit()
    return conn


def _get_checkpoint(conn: sqlite3.Connection, channel_id: str) -> int:
    cur = conn.execute("SELECT last_offset FROM checkpoints WHERE channel_id = ?", (channel_id,))
    row = cur.fetchone()
    return row[0] if row else 0


def _update_checkpoint(conn: sqlite3.Connection, channel_id: str, offset: int, turn: int):
    conn.execute(
        """INSERT INTO checkpoints (channel_id, last_offset, last_turn, updated_at)
           VALUES (?, ?, ?, datetime('now'))
           ON CONFLICT(channel_id) DO UPDATE SET
               last_offset = excluded.last_offset,
               last_turn = excluded.last_turn,
               updated_at = datetime('now')""",
        (channel_id, offset, turn),
    )
    conn.commit()


# ── Core logic ────────────────────────────────────────────────────────────────

def get_channels() -> list[dict]:
    """List all Coloquio channels."""
    data = _get("/api/v1/coloquio/channels")
    return data if isinstance(data, list) else data.get("channels", [])


def read_messages(channel_id: str, offset: int = 0, limit: int = 50) -> list[dict]:
    """Read messages from a channel starting at offset."""
    quoted = urllib.parse.quote(channel_id, safe="")
    data = _get(f"/api/v1/coloquio/channels/{quoted}?limit={limit}&offset={offset}")
    return data.get("messages", [])


def format_batch(channel_id: str, messages: list[dict]) -> str:
    """Format a batch of messages as a structured memory entry for SilvaDB."""
    if not messages:
        return ""
    filtered = [m for m in messages if not _is_noise(m.get("content", ""))]
    if not filtered:
        return ""
    t_start = filtered[0]["turn"]
    t_end = filtered[-1]["turn"]
    authors = sorted(set(m.get("author_id", "?") for m in filtered))
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    lines = [
        f"[Coloquio:{channel_id}] Turns {t_start}-{t_end} ({len(filtered)}/{len(messages)} msgs, {len(authors)} agents)",
        f"Timestamp: {ts}",
        f"Participants: {', '.join(authors)}",
        "---",
    ]
    for m in filtered:
        role = "H" if m.get("role") == "human" else "A"
        lines.append(f"[T{m['turn']}] ({role}) {m['author_id']}: {m['content']}")

    text = "\n".join(lines)
    if len(text) > MAX_CHARS_PER_MEMORY:
        text = text[:MAX_CHARS_PER_MEMORY] + f"\n...[+{len(text) - MAX_CHARS_PER_MEMORY}c truncated]"
    return text


_HASH_CACHE: dict[str, bool] = {}

def _store_with_dedup(content: str, channel_id: str) -> bool:
    content_hash = hashlib.sha256(content.encode("utf-8")).hexdigest()
    if content_hash in _HASH_CACHE:
        return True
    _init_hash_db()
    if _hash_exists(content_hash):
        _HASH_CACHE[content_hash] = True
        return True
    result = _post_tylluan_remember(
        content=content,
        agent_id="coloquio-summarizer",
        metadata={
            "source": "coloquio_summarizer",
            "channel": channel_id,
            "tags": ["coloquio", channel_id],
        },
    )
    if result.get("status") != "error":
        _store_hash(content_hash, channel_id)
        _HASH_CACHE[content_hash] = True
        return True
    return False


def process_channel(conn: sqlite3.Connection, channel: dict) -> tuple[int, int]:
    """Process one channel: read new messages, store memories, update checkpoint.
    Returns (messages_read, memories_stored).
    """
    cid = channel["channel_id"]
    offset = _get_checkpoint(conn, cid)

    messages = read_messages(cid, offset=offset, limit=100)
    if not messages:
        return (0, 0)

    total_read = len(messages)
    memories_stored = 0
    batch: list[dict] = []

    for i, msg in enumerate(messages):
        batch.append(msg)
        if len(batch) >= BATCH_SIZE or i == len(messages) - 1:
            content = format_batch(cid, batch)
            if content:
                if _store_with_dedup(content, cid):
                    memories_stored += 1
            batch = []

    last_turn = messages[-1]["turn"]
    _update_checkpoint(conn, cid, offset + total_read, last_turn)
    return (total_read, memories_stored)


def run_once(channel_filter: str | None = None) -> dict:
    """Single pass: process all channels (or one if filter set)."""
    conn = _init_db()
    try:
        channels = get_channels()
        if channel_filter:
            channels = [c for c in channels if c["channel_id"] == channel_filter]

        total_msgs = 0
        total_memories = 0
        checked = 0
        with_new = 0

        for ch in channels:
            checked += 1
            msgs, mems = process_channel(conn, ch)
            if msgs > 0:
                with_new += 1
            total_msgs += msgs
            total_memories += mems

        return {
            "checked": checked,
            "with_new": with_new,
            "messages": total_msgs,
            "memories": total_memories,
        }
    finally:
        conn.close()


# ── CLI ───────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Coloquio -> SilvaDB summarizer")
    parser.add_argument("--interval", type=int, default=0,
                        help="Run in loop every N minutes (0 = run once)")
    parser.add_argument("--channel", type=str, default=None,
                        help="Process only this channel")
    parser.add_argument("--once", action="store_true",
                        help="Run once and exit (default)")
    args = parser.parse_args()

    interval = args.interval
    channel = args.channel

    if interval > 0:
        print(f"Coloquio Summarizer — polling every {interval} min"
              + (f" (channel: {channel})" if channel else ""))
        while True:
            start = time.time()
            result = run_once(channel)
            elapsed = time.time() - start
            status = (
                f"[{datetime.now().strftime('%H:%M:%S')}] "
                f"checked={result['checked']} new={result['with_new']} "
                f"msgs={result['messages']} memories={result['memories']} "
                f"({elapsed:.1f}s)"
            )
            print(status)
            time.sleep(interval * 60)
    else:
        result = run_once(channel)
        print(f"Checked {result['checked']} channels, "
              f"{result['with_new']} with new messages, "
              f"{result['messages']} msgs -> {result['memories']} memories stored")


if __name__ == "__main__":
    main()
