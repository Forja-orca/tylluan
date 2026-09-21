#!/usr/bin/env python3
"""Buffy's own supervised loop over Coloquio (Jose's 2026-09-21 pattern).

ONE controlled recurring instance — never a fresh process per mention, never
a swarm. Each wake: check /coloquio/unread for reader `buffy`, pull new
messages from the channels that have any, filter for work addressed to
Buffy, append real items to a JSONL inbox (dedup by msg_id), sleep, repeat.

STRICTLY SOLO-INBOX (WORK_PROTOCOL rule + Jose's decision, 2026-09-17):
this script NEVER executes anything. It reads Coloquio and writes a local
inbox file. Acting on inbox items is a separate, explicit, human-visible
step — never this loop.

Reads use the kernel's server-side cursor (`/new?reader=buffy`), optionally
advancing it with `--ack`. Local state is only the dedupe set of seen
msg_ids, so the script survives restarts without re-delivering.

Usage:
  python scripts/coloquio_loop_buffy.py --once              # one wake (cron/scheduled task)
  python scripts/coloquio_loop_buffy.py --interval 900      # self-looping, 15 min
  python scripts/coloquio_loop_buffy.py --once --ack        # also advance the kernel cursor
  python scripts/coloquio_loop_buffy.py --once --verbose    # print every checked message
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

READER_ID = "buffy"
DEFAULT_BASE_URL = "http://127.0.0.1:47004"
DEFAULT_LIMIT = 50
DEFAULT_INBOX = Path("logs/buffy_inbox.jsonl")
SELF_AUTHOR = READER_ID  # never treat my own posts as work


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def log(msg: str) -> None:
    """UTF-8-safe line to stdout (Windows console-proof)."""
    sys.stdout.buffer.write(f"{now_iso()} {msg}\n".encode("utf-8"))
    sys.stdout.buffer.flush()


def http_json(base_url: str, path: str, timeout: float = 10.0) -> dict:
    req = urllib.request.Request(f"{base_url}{path}", method="GET")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode("utf-8"))


def load_seen(inbox_path: Path) -> set[str]:
    seen: set[str] = set()
    if inbox_path.exists():
        with inbox_path.open("r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    seen.add(json.loads(line).get("msg_id", ""))
                except json.JSONDecodeError:
                    continue  # tolerate a torn tail line from a killed process
    return seen


def mentions_buffy(content: str) -> bool:
    """Work addressed to Buffy = DIRECT @mention only.

    A bare 'buffy' matches every meta-discussion ABOUT my work (Spanish
    fleet conversation talks about me constantly) — that is not work FOR
    me. The fleet convention for directing work is '@buffy' (see T688/T692).
    """
    return "@buffy" in content.lower()


def wake_once(base_url: str, inbox_path: Path, limit: int, ack: bool,
              verbose: bool) -> int:
    """One loop iteration. Returns number of NEW inbox items appended."""
    # 1) Kernel alive? (also records which build is serving us)
    try:
        health = http_json(base_url, "/health", timeout=5.0)
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        log(f"WAKE kernel unreachable ({e}) — nothing to do, will retry next wake")
        return 0
    log(f"WAKE kernel={health.get('commit', '?')} status={health.get('status', '?')}")

    # 2) Where is there unread traffic for me?
    try:
        summary = http_json(base_url, f"/api/v1/coloquio/unread?reader={READER_ID}")
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        log(f"WAKE unread summary failed ({e}) — will retry next wake")
        return 0
    channels = summary.get("channels", [])
    total = summary.get("total_unread", 0)
    if total == 0:
        log("WAKE total_unread=0 — nothing to do, sleeping")
        return 0

    seen = load_seen(inbox_path)
    appended = 0
    inbox_path.parent.mkdir(parents=True, exist_ok=True)

    # 3) Pull new messages per channel (most-unread first)
    ordered = sorted(channels, key=lambda c: c.get("unread_count", 0), reverse=True)
    for ch in ordered:
        n_unread = ch.get("unread_count", 0)
        if n_unread <= 0:
            continue
        ch_id = ch.get("channel_id", "")
        mark = "true" if ack else "false"
        try:
            data = http_json(
                base_url,
                f"/api/v1/coloquio/channels/{ch_id}/new?reader={READER_ID}"
                f"&limit={max(limit, n_unread)}&mark_read={mark}",
            )
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            log(f"  channel {ch_id}: fetch failed ({e}) — skipping, stays unread")
            continue
        msgs = data.get("messages", [])
        log(f"  channel {ch_id}: {len(msgs)} new (unread was {n_unread})")

        for m in msgs:
            author = m.get("author_id", "")
            turn = m.get("turn", 0)
            content = m.get("content", "")
            if verbose:
                log(f"    T{turn} [{author}] {content[:100]!r}")
            if author == SELF_AUTHOR:
                continue  # my own posts are never work
            if not mentions_buffy(content):
                continue
            msg_id = m.get("msg_id", "")
            if msg_id in seen:
                continue
            item = {
                "msg_id": msg_id,
                "turn": turn,
                "channel": ch_id,
                "author": author,
                "role": m.get("role", ""),
                "created_at": m.get("created_at"),
                "preview": content[:500],
                "collected_at": now_iso(),
            }
            with inbox_path.open("a", encoding="utf-8") as f:
                f.write(json.dumps(item, ensure_ascii=False) + "\n")
            seen.add(msg_id)
            appended += 1
            log(f"    INBOX+ T{turn} [{author}] {content[:80]!r}")

    log(f"WAKE done: inbox_items_appended={appended} ack={ack}")
    return appended


def main() -> int:
    ap = argparse.ArgumentParser(description="Buffy's supervised Coloquio loop (solo-inbox).")
    ap.add_argument("--once", action="store_true", help="single wake, then exit (for cron/scheduled task)")
    ap.add_argument("--interval", type=int, default=900, help="seconds between wakes in loop mode (default 900)")
    ap.add_argument("--base-url", default=DEFAULT_BASE_URL)
    ap.add_argument("--limit", type=int, default=DEFAULT_LIMIT, help="max messages per channel per wake")
    ap.add_argument("--ack", action="store_true", help="advance the kernel read cursor (mark_read=true)")
    ap.add_argument("--inbox", type=Path, default=DEFAULT_INBOX, help="inbox JSONL path")
    ap.add_argument("--verbose", action="store_true", help="log every checked message")
    args = ap.parse_args()

    if args.once:
        wake_once(args.base_url, args.inbox, args.limit, args.ack, args.verbose)
        return 0

    log(f"LOOP start: interval={args.interval}s base={args.base_url} inbox={args.inbox}")
    while True:
        try:
            wake_once(args.base_url, args.inbox, args.limit, args.ack, args.verbose)
        except Exception as e:  # never die on one bad iteration
            log(f"LOOP iteration error (suppressed): {e!r}")
        time.sleep(max(30, args.interval))


if __name__ == "__main__":
    sys.exit(main())
