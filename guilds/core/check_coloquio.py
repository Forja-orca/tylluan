"""Coloquio polling agent for Deep â€” checks unread messages and reads new turns.

Level B polling from coloquio_wake_scheduling.md. Run at start of each
session cycle. Does NOT commit, push, or modify repo state â€” reads only,
per turn 367 rule.

Defaults to 'equipo' channel (team coordination). Other channels on request.

Usage:
  python guilds/core/check_coloquio.py              # equipo only, latest turns
  python guilds/core/check_coloquio.py --all         # all channels
  python guilds/core/check_coloquio.py --watch       # continuous polling
  python guilds/core/check_coloquio.py equipo mision-activa  # specific channels
"""
import json
import os
import sys
import time
import urllib.request
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


def resolve_reader_id(argv=None) -> str:
    """Agent identity used for unread/mention queries.

    Explicit ``--agent-id <id>`` wins, then ``TYLLUAN_AGENT_ID``. There is no
    silent default: an unread poll for the wrong agent silently returns empty,
    so a missing identity is a hard error (contract bwc-b0523fcc, turn 523 â€”
    the old hardcoded ``deep`` was removed).
    """
    argv = sys.argv[1:] if argv is None else argv
    for i, a in enumerate(argv):
        if a == "--agent-id" and i + 1 < len(argv):
            return argv[i + 1]
    env = os.environ.get("TYLLUAN_AGENT_ID", "").strip()
    if env:
        return env
    raise SystemExit(
        "[check_coloquio] ERROR: no reader identity. Pass --agent-id <id> "
        "or set TYLLUAN_AGENT_ID. (The old silent default 'deep' was removed "
        "so a poll never reads the wrong agent's inbox.)"
    )


CHECK_INTERVAL = 120  # seconds between checks in watch mode

# Only show these channels by default (team coordination).
# Use --all or pass channel names to override.
DEFAULT_CHANNELS = ["equipo"]

# Messages matching these patterns are noise â€” don't show.
NOISE_PATTERNS = [
    "ðŸ”„ Starting scheduled auto-sync",
    "Auto-sync: push to",
    "failed: error sending request",
    "ðŸ§¹ Running periodic SQLite maintenance",
    "ðŸ©º System diagnostic started",
    "kernel restarted",
    "shutdown_initiated",
]

# Where to persist last-read state across sessions
STATE_FILE = Path.home() / ".cache" / "tylluan" / "coloquio_last_read.json"


def resolve_kernel():
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    if port_file.exists():
        try:
            data = json.loads(port_file.read_text())
            return f"http://127.0.0.1:{data.get('port', 47004)}"
        except Exception:
            pass
    if "KERNEL_BASE" in os.environ:
        return os.environ["KERNEL_BASE"]
    return "http://127.0.0.1:47004"


def api_get(path, params=None):
    url = f"{resolve_kernel()}{path}"
    if params:
        qs = "&".join(f"{k}={urllib.request.quote(str(v))}" for k, v in params.items())
        url = f"{url}?{qs}"
    req = urllib.request.Request(url, method="GET")
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read())


def load_state():
    if STATE_FILE.exists():
        try:
            return json.loads(STATE_FILE.read_text())
        except Exception:
            pass
    return {}


def save_state(state):
    STATE_FILE.parent.mkdir(parents=True, exist_ok=True)
    STATE_FILE.write_text(json.dumps(state, indent=2))


def is_noise(content):
    return any(p in content for p in NOISE_PATTERNS)


def check_unread(channels_to_check, show_all=False):
    state = load_state()

    try:
        data = api_get("/api/v1/coloquio/unread", {"reader": READER_ID})
    except Exception as e:
        sys.stderr.write(f"[check_coloquio] Kernel unreachable: {e}\n")
        return

    channels = data if isinstance(data, list) else data.get("channels", [data])

    # Filter to requested channels
    if not show_all:
        channels = [c for c in channels
                     if c.get("channel_id", c.get("id", "")) in channels_to_check]

    total_unread = sum(c.get("unread_count", 0) for c in channels)
    if total_unread == 0:
        print(f"[{time.strftime('%H:%M:%S')}] No new messages in: {', '.join(channels_to_check)}")
        return

    print(f"\n{'='*60}")
    print(f"[{time.strftime('%H:%M:%S')}] {total_unread} unread messages")
    print(f"{'='*60}")

    new_turns = 0
    for ch in channels:
        unread = ch.get("unread_count", 0)
        if unread == 0:
            continue
        channel_id = ch.get("channel_id", ch.get("id", "?"))
        last_read = state.get(channel_id, 0)

        try:
            thread = api_get(f"/api/v1/coloquio/channels/{channel_id}")
            messages = thread.get("messages", []) if isinstance(thread, dict) else thread

            # Only show messages newer than last_read
            shown = 0
            max_turn = last_read
            for msg in reversed(messages):  # newest first
                if not isinstance(msg, dict):
                    continue
                turn = msg.get("turn", 0)
                if turn <= last_read:
                    break
                content = msg.get("content", "")
                agent = msg.get("agent_id", "?") or "?"

                if is_noise(content):
                    continue

                shown += 1
                new_turns += 1
                max_turn = max(max_turn, turn)

                mention = READER_ID in content.lower()
                prefix = "[>>>]" if mention else "     "
                preview = content[:150].replace("\n", " ")
                print(f"{prefix} T{turn} {agent}: {preview}...")

            if max_turn > last_read:
                state[channel_id] = max_turn

        except Exception as e:
            sys.stderr.write(f"  Error reading {channel_id}: {e}\n")

    if new_turns:
        save_state(state)
        print(f"\n  {new_turns} new turns since last check. State saved.")

    # Quick summary: any mentions of me?
    mentions = [m for m in channels_to_check if state.get(m, 0) > (load_state().get(m, 0) or 0)]
    if mentions:
        print(f"\n  Channels with new activity: {', '.join(mentions)}")


def watch(channels_to_check):
    print(f"[check_coloquio] Watching {', '.join(channels_to_check)} every {CHECK_INTERVAL}s for {READER_ID}")
    print("[check_coloquio] Ctrl+C to stop.\n")
    while True:
        check_unread(channels_to_check)
        time.sleep(CHECK_INTERVAL)


if __name__ == "__main__":
    argv = sys.argv[1:]
    # Resolve the reader identity from the ORIGINAL argv (so --agent-id works
    # in every mode), then rebuild the positional args without the
    # --agent-id <value> pair â€” otherwise the value would be mistaken for a
    # channel name by the positional filter below.
    READER_ID = resolve_reader_id(argv)
    cleaned = []
    skip_next = False
    for a in argv:
        if a == "--agent-id":
            skip_next = True
            continue
        if skip_next:
            skip_next = False
            continue
        cleaned.append(a)

    args = [a for a in cleaned if not a.startswith("-")]

    if "--watch" in cleaned or "-w" in cleaned:
        channels = args if args else DEFAULT_CHANNELS
        try:
            watch(channels)
        except KeyboardInterrupt:
            print("\n[check_coloquio] Stopped.")
    elif "--all" in cleaned or "-a" in cleaned:
        check_unread([], show_all=True)
    else:
        channels = args if args else DEFAULT_CHANNELS
        check_unread(channels)
