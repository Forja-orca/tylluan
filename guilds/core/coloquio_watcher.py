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
import uuid
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


PENDING_DIR = Path(os.environ.get("TYLLUAN_PENDING", str(Path.home() / ".tylluan" / "pending_actions")))


def queue_for_approval(action_id: str, channel: str, msg: dict, exec_cmd: list) -> Path:
    """Human-confirmation gate (José's decision 2026-09-17, WORK_PROTOCOL.md):
    the watcher NEVER launches --exec on its own. Trusted-author mentions are
    queued as pending actions; a human explicitly approves with
    `--approve <id>` (mirrors the kernel's grants/HITL approve_action flow).
    """
    PENDING_DIR.mkdir(parents=True, exist_ok=True)
    prompt = f"coloquio:{channel} T{msg.get('turn')} @{msg.get('author_id')}: {msg.get('content', '')[:800]}"
    action = {
        "id": action_id,
        "queued_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "channel": channel,
        "turn": msg.get("turn"),
        "author_id": msg.get("author_id"),
        "content": msg.get("content", ""),
        "command": exec_cmd + [prompt],
    }
    path = PENDING_DIR / f"{action_id}.json"
    with open(path, "w", encoding="utf-8") as f:
        json.dump(action, f, ensure_ascii=False, indent=2)
    return path


def approve_action(action_id: str) -> bool:
    """Execute a previously queued action (explicit human approval)."""
    path = PENDING_DIR / f"{action_id}.json"
    if not path.exists():
        print(f"[watcher] no pending action '{action_id}' (already approved or unknown).")
        return False
    action = json.loads(path.read_text(encoding="utf-8"))
    cmd = action.get("command", [])
    print(f"[watcher] APPROVING {action_id} ({action.get('author_id')} T{action.get('turn')}): {' '.join(cmd)[:120]}...")
    try:
        subprocess.Popen(cmd, creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0))
    except Exception as e:
        print(f"[watcher] exec failed: {e}")
        return False
    path.unlink(missing_ok=True)
    print(f"[watcher] approved and executed {action_id}.")
    return True


def main() -> None:
    ap = argparse.ArgumentParser(description="Coloquio autonomous watcher (long-poll based)")
    ap.add_argument("--agent-id", required=True, help="agent identity (mentions of @<id> wake the watcher)")
    ap.add_argument("--channels", default="general", help="comma-separated channels to watch (default: general)")
    ap.add_argument("--wait", type=int, default=DEFAULT_WAIT_SECS, help="long-poll timeout per cycle (5-300)")
    ap.add_argument("--exec", nargs="*", default=None, help="callback command run with the message as last arg")
    ap.add_argument("--trusted-authors", default=None,
                    help="REQUIRED with --exec: comma-separated allowlist of author_ids whose mentions may queue an action")
    ap.add_argument("--approve", default=None, help="explicitly approve and run a queued action by id (HITL)")
    ap.add_argument("--inbox", default=str(DEFAULT_INBOX), help="inbox file path (default: ~/.tylluan/inbox)")
    ap.add_argument("--kernel", default=DEFAULT_KERNEL)
    args = ap.parse_args()

    # Human-confirmation mode: run one queued action and exit.
    if args.approve:
        ok = approve_action(args.approve)
        sys.exit(0 if ok else 1)

    # Fail closed (José's decision 2026-09-17): --exec requires the trusted
    # authors allowlist. Never open by default.
    trusted = None
    if args.exec:
        if not args.trusted_authors:
            print("[watcher] ERROR: --exec requires --trusted-authors (fail-closed, WORK_PROTOCOL.md).")
            sys.exit(1)
        trusted = {a.strip().lower() for a in args.trusted_authors.split(",") if a.strip()}
        print(f"[watcher] trusted authors for --exec: {sorted(trusted)}")

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
                if not message_mentions(msg, args.agent_id):
                    continue
                append_inbox(inbox, channel, msg)
                if not args.exec:
                    continue
                author = (msg.get("author_id") or "").lower()
                if trusted is not None and author not in trusted:
                    print(f"[watcher] skipped exec: @{msg.get('author_id')} not in trusted authors (inbox only)")
                    continue
                action_id = uuid.uuid4().hex[:12]
                path = queue_for_approval(action_id, channel, msg, args.exec)
                print(f"[watcher] action {action_id} QUEUED for human approval -> {path}")


if __name__ == "__main__":
    main()