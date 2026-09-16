#!/usr/bin/env python3
"""Coloquio autonomous watcher — the missing agent-side trigger (2026-09-16).

The kernel long-poll works (turn 508 validated it live), but no agent had
anything running on its own runtime calling it. This script closes that gap:

  loop:
    long-poll "espera actividad en coloquio <chan> durante N segundos"
    on new messages:
      if the message mentions MY agent_id (or channel is in the inbox set):
        append to the agent's inbox file (persistent, read on every session)
        if --exec provided: run the callback command with the message text
          (e.g. `opencode run "procesa: <msg>"` -> a headless session wakes,
           the agent acts, and the loop continues without any human prompt)

Usage:
  python guilds/core/coloquio_watcher.py --agent-id deep \
      --channels general equipo --exec "opencode run"
"""

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

DEFAULT_KERNEL = os.environ.get("KERNEL_BASE", "http://127.0.0.1:47004")
DEFAULT_WAIT_SECS = 120
DEFAULT_INBOX = Path(os.environ.get("TYLLUAN_INBOX", str(Path.home() / ".tylluan" / "inbox")))


def api_post(url: str, payload: dict, timeout: int = 400) -> dict:
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read())


def long_poll(kernel: str, channel: str, wait_secs: int) -> dict:
    """One blocking wait cycle. Returns the parsed payload (status new/timeout)."""
    intent = f"espera actividad en coloquio {channel} durante {wait_secs} segundos"
    raw = api_post(f"{kernel}/api/v1/do", {"intent": intent}, timeout=wait_secs + 30)
    texts = []
    for c in raw.get("content", []):
        if isinstance(c, str):
            texts.append(c)
        elif isinstance(c, dict):
            texts.append(c.get("text", ""))
    for t in texts:
        try:
            parsed = json.loads(t)
            if isinstance(parsed, dict) and "status" in parsed:
                return parsed
        except Exception:
            continue
    return {"status": "timeout", "new_messages": [], "last_turn": 0}


def message_mentions(msg: dict, agent_id: str) -> bool:
    content = (msg.get("content") or "").lower()
    author = (msg.get("author_id") or "").lower()
    if author == agent_id.lower():
        return False  # never wake on your own post
    return f"@{agent_id}".lower() in content


def append_inbox(inbox: Path, channel: str, msg: dict) -> None:
    inbox.parent.mkdir(parents=True, exist_ok=True)
    entry = (
        f"# {time.strftime('%Y-%m-%d %H:%M')} [{channel}] "
        f"T{msg.get('turn', '?')} @{msg.get('author_id', '?')}\n"
        f"{msg.get('content', '')}\n\n"
    )
    with open(inbox, "a", encoding="utf-8") as f:
        f.write(entry)
    print(f"[watcher] inbox <- T{msg.get('turn', '?')} [{channel}] @{msg.get('author_id', '?')}")


def run_callback(exec_cmd: list, msg: dict, channel: str) -> None:
    prompt = f"coloquio:{channel} T{msg.get('turn')} @{msg.get('author_id')}: {msg.get('content', '')[:800]}"
    cmd = exec_cmd + [prompt]
    print(f"[watcher] exec: {' '.join(cmd)[:120]}...")
    try:
        subprocess.Popen(cmd, creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0))
    except Exception as e:
        print(f"[watcher] exec failed: {e}")


def main() -> None:
    ap = argparse.ArgumentParser(description="Coloquio autonomous watcher (long-poll based)")
    ap.add_argument("--agent-id", required=True, help="agent identity (mentions of @<id> wake the watcher)")
    ap.add_argument("--channels", default="general", help="comma-separated channels to watch (default: general)")
    ap.add_argument("--wait", type=int, default=DEFAULT_WAIT_SECS, help="long-poll timeout per cycle (5-300)")
    ap.add_argument("--exec", nargs="*", default=None, help="callback command run with the message as last arg")
    ap.add_argument("--inbox", default=str(DEFAULT_INBOX), help="inbox file path (default: ~/.tylluan/inbox)")
    ap.add_argument("--kernel", default=DEFAULT_KERNEL)
    args = ap.parse_args()

    wait = max(5, min(args.wait, 300))
    channels = [c.strip() for c in args.channels.split(",") if c.strip()]
    inbox = Path(args.inbox)
    print(f"[watcher] agent={args.agent_id} channels={channels} wait={wait}s exec={bool(args.exec)}")
    print("[watcher] autonomous loop started — Ctrl+C to stop.\n")

    while True:
        for channel in channels:
            try:
                payload = long_poll(args.kernel, channel, wait)
            except Exception as e:
                print(f"[watcher] long-poll error [{channel}]: {e}")
                time.sleep(5)
                continue
            if payload.get("status") != "new":
                continue
            for msg in payload.get("new_messages", []):
                if message_mentions(msg, args.agent_id):
                    append_inbox(inbox, channel, msg)
                    if args.exec:
                        run_callback(args.exec, msg, channel)


if __name__ == "__main__":
    main()