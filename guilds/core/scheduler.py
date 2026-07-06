"""Scheduler guild — async timer service for agent wake-up calls.

Architecture:
  SQLite persistence + background polling thread + coloquio callback.
  Survives kernel restarts (schedules restored from DB on next boot).

Flow:
  1. Agent calls schedule() → stored in SQLite with run_at timestamp
  2. Background thread polls every 30s for due schedules
  3. When due → POST /api/v1/coloquio/channels/{channel}/post with @agent_id mention
  4. Agent discovers wake-up on next tylluan_do / whats_new / inbox check

No external deps — uses stdlib only (sqlite3, threading, urllib.request, time).
"""
from mcp.server.fastmcp import FastMCP
import sqlite3
import threading
import time
import json
import urllib.request
import urllib.error
import uuid
import os
from datetime import datetime, timedelta
from pathlib import Path

mcp = FastMCP("scheduler")

DB_PATH = os.environ.get(
    "SCHEDULER_DB",
    str(Path(__file__).resolve().parent.parent.parent / "data" / "scheduler.db"),
)
def _resolve_kernel_base() -> str:
    if "KERNEL_BASE" in os.environ:
        return os.environ["KERNEL_BASE"]
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    try:
        import json as _json
        data = _json.loads(port_file.read_text())
        port = data.get("port", 3032)
        return f"http://127.0.0.1:{port}"
    except Exception:
        return "http://127.0.0.1:3032"

KERNEL_BASE = _resolve_kernel_base()
POLL_INTERVAL_SECS = int(os.environ.get("SCHEDULER_POLL_SECS", "30"))



def _get_conn() -> sqlite3.Connection:
    os.makedirs(os.path.dirname(DB_PATH), exist_ok=True)
    conn = sqlite3.connect(DB_PATH, check_same_thread=False)
    conn.row_factory = sqlite3.Row
    conn.execute("""
        CREATE TABLE IF NOT EXISTS schedules (
            id TEXT PRIMARY KEY,
            intent TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            channel TEXT DEFAULT 'schedules',
            run_at TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now')),
            fired INTEGER DEFAULT 0,
            error TEXT
        )
    """)
    conn.execute("""
        CREATE INDEX IF NOT EXISTS idx_schedules_pending
        ON schedules(fired, run_at)
    """)
    conn.commit()
    return conn


def _post_to_coloquio(channel: str, message: str, author_id: str = "scheduler") -> bool:
    url = f"{KERNEL_BASE}/api/v1/coloquio/channels/{channel}/post"
    body = json.dumps({
        "author_id": author_id,
        "role": "agent",
        "content": message,
        "metadata": "{}",
    }).encode()
    try:
        req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            return resp.status == 201
    except (urllib.error.URLError, urllib.error.HTTPError, OSError) as exc:
        return False


def _scheduler_loop() -> None:
    conn = _get_conn()
    while True:
        try:
            due = conn.execute(
                "SELECT id, intent, agent_id, channel FROM schedules "
                "WHERE fired = 0 AND run_at <= datetime('now')"
            ).fetchall()
            for row in due:
                sid, intent, agent_id, channel = row["id"], row["intent"], row["agent_id"], row["channel"]
                msg = f"@{agent_id} ⏰ schedule `{sid}` fired: {intent}"
                ok = _post_to_coloquio(channel, msg)
                conn.execute(
                    "UPDATE schedules SET fired = 1, error = ? WHERE id = ?",
                    (None if ok else "post_failed", sid),
                )
            conn.commit()
        except sqlite3.Error:
            conn.rollback()
        time.sleep(POLL_INTERVAL_SECS)


@mcp.tool()
def schedule(
    intent: str,
    agent_id: str,
    delay_minutes: int = 60,
    channel: str = "schedules",
) -> str:
    """Schedule a wake-up call. When the timer fires, posts @agent_id to coloquio channel.

    Args:
        intent: The task description to deliver on wake-up.
        agent_id: Target agent (e.g. 'claude-code', 'antigravity'). Mentioned in coloquio post.
        delay_minutes: Minutes until wake-up (default 60).
        channel: Coloquio channel to post to (default 'schedules').

    Returns:
        Schedule ID for tracking or cancellation.
    """
    schedule_id = str(uuid.uuid4())
    run_at = (datetime.utcnow() + timedelta(minutes=delay_minutes)).isoformat()
    conn = _get_conn()
    try:
        conn.execute(
            "INSERT INTO schedules (id, intent, agent_id, channel, run_at) VALUES (?, ?, ?, ?, ?)",
            (schedule_id, intent, agent_id, channel, run_at),
        )
        conn.commit()
    except sqlite3.Error as exc:
        conn.close()
        return f"error: {exc}"
    conn.close()
    return schedule_id


@mcp.tool()
def cancel_schedule(schedule_id: str) -> bool:
    """Cancel a pending schedule by ID. Returns True if cancelled."""
    conn = _get_conn()
    try:
        cur = conn.execute(
            "UPDATE schedules SET fired = -1 WHERE id = ? AND fired = 0",
            (schedule_id,),
        )
        conn.commit()
        cancelled = cur.rowcount > 0
    except sqlite3.Error:
        conn.close()
        return False
    conn.close()
    return cancelled


@mcp.tool()
def list_pending(agent_id: str = "") -> list:
    """List pending (unfired) schedules, optionally filtered by agent_id."""
    conn = _get_conn()
    if agent_id:
        rows = conn.execute(
            "SELECT id, intent, agent_id, channel, run_at, created_at "
            "FROM schedules WHERE fired = 0 AND agent_id = ? "
            "ORDER BY run_at ASC",
            (agent_id,),
        ).fetchall()
    else:
        rows = conn.execute(
            "SELECT id, intent, agent_id, channel, run_at, created_at "
            "FROM schedules WHERE fired = 0 "
            "ORDER BY run_at ASC",
        ).fetchall()
    conn.close()
    return [dict(r) for r in rows]


# Start background scheduler loop on import
_thread = threading.Thread(target=_scheduler_loop, daemon=True)
_thread.start()
