"""Coloquio polling agent for Deep — checks unread messages and reads new turns.

Level B polling from coloquio_wake_scheduling.md: shell-polling universal
fallback. Run this at the start of each session cycle to catch messages
published since the last check. Does NOT commit, push, or modify repo state
— reads only, per the turn 367 rule (only Claude Code touches the remote).

Usage:
  python guilds/core/check_coloquio.py          # one-shot check
  python guilds/core/check_coloquio.py --watch  # continuous polling (Ctrl+C to stop)
"""
import json
import os
import sys
import time
import urllib.request
from pathlib import Path

READER_ID = "deep"
KERNEL_URL = "http://127.0.0.1:4000"
CHECK_INTERVAL = 120  # seconds between checks in watch mode

_last_turn = None


def resolve_kernel():
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    if port_file.exists():
        try:
            data = json.loads(port_file.read_text())
            port = data.get("port", 4000)
            return f"http://127.0.0.1:{port}"
        except Exception:
            pass
    if "KERNEL_BASE" in os.environ:
        return os.environ["KERNEL_BASE"]
    return KERNEL_URL


def api_get(path, params=None):
    url = f"{resolve_kernel()}{path}"
    if params:
        qs = "&".join(f"{k}={urllib.request.quote(str(v))}" for k, v in params.items())
        url = f"{url}?{qs}"
    req = urllib.request.Request(url, method="GET")
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read())


def check_unread():
    global _last_turn
    try:
        data = api_get("/api/v1/coloquio/unread", {"reader": READER_ID})
    except Exception as e:
        sys.stderr.write(f"[check_coloquio] Kernel unreachable: {e}\n")
        return

    channels = data if isinstance(data, list) else data.get("channels", [data])
    total_unread = sum(c.get("unread_count", 0) for c in channels)

    if total_unread == 0:
        if _last_turn is None:
            print(f"[{time.strftime('%H:%M:%S')}] No unread messages for {READER_ID}")
        return

    print(f"[{time.strftime('%H:%M:%S')}] {total_unread} unread messages for {READER_ID}")

    for ch in channels:
        unread = ch.get("unread_count", 0)
        if unread == 0:
            continue
        channel_id = ch.get("channel_id", ch.get("id", "?"))
        print(f"  {channel_id}: {unread} unread (last read turn: {ch.get('last_read_turn', '?')})")

        try:
            thread = api_get(f"/api/v1/coloquio/channels/{channel_id}")
            messages = thread.get("messages", []) if isinstance(thread, dict) else thread
            for msg in messages:
                if not isinstance(msg, dict):
                    continue
                turn = msg.get("turn", 0)
                if _last_turn and turn <= _last_turn:
                    continue
                agent = msg.get("agent_id", "?")
                content = msg.get("content", "")
                mention_me = "deep" in content.lower() or "equipo" in (channel_id or "").lower()

                prefix = ">>>" if mention_me else "   "
                print(f"{prefix} Turn {turn} by {agent}: {content[:120]}...")
                if mention_me and turn > (_last_turn or 0):
                    _last_turn = turn
        except Exception as e:
            sys.stderr.write(f"  Error reading {channel_id}: {e}\n")

    if _last_turn:
        print(f"  Last seen turn: {_last_turn}")


def watch():
    print(f"[check_coloquio] Watching for messages to {READER_ID} every {CHECK_INTERVAL}s...")
    print(f"[check_coloquio] Press Ctrl+C to stop.\n")
    while True:
        check_unread()
        time.sleep(CHECK_INTERVAL)


if __name__ == "__main__":
    if "--watch" in sys.argv or "-w" in sys.argv:
        try:
            watch()
        except KeyboardInterrupt:
            print("\n[check_coloquio] Stopped.")
    else:
        check_unread()
