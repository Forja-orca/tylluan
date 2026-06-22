#!/usr/bin/env python3
"""
tylluan_stdio_proxy.py — Stdio MCP bridge for TylluanNexus kernel.

Translates stdio JSON-RPC (Qwen Desktop, LM Studio) → HTTP POST localhost:3030/messages.
Rules:
  - Requests (have "id")     → forward to kernel, write response to stdout
  - Notifications (no "id") → forward to kernel, NO response written (MCP spec)
  - Kernel unreachable       → respond with JSON-RPC error (requests only)
All tracing goes to stderr — stdout is pure JSON-RPC.
"""
import sys
import io
import json
import urllib.request
import urllib.error

# Force UTF-8 on all stdio — Windows defaults to cp1252 which can't encode
# emojis and unicode symbols that the kernel returns in response text.
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace', line_buffering=True)
sys.stdin  = io.TextIOWrapper(sys.stdin.buffer,  encoding='utf-8', errors='replace')

KERNEL_URL = "http://127.0.0.1:3030/messages"
TIMEOUT_S = 120


def send(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj, ensure_ascii=False, separators=(',', ':')) + "\n")
    sys.stdout.flush()


def error_response(msg_id, code: int, message: str) -> dict:
    return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}}


def http_post(payload: bytes) -> str:
    req = urllib.request.Request(
        KERNEL_URL,
        data=payload,
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "tylluan-stdio-proxy/1.0",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT_S) as resp:
        return resp.read().decode("utf-8")


def parse_response(body: str) -> dict:
    """Handle both plain JSON and SSE-wrapped responses."""
    body = body.strip()
    if body.startswith("data:"):
        for line in body.splitlines():
            if line.startswith("data:"):
                return json.loads(line[5:].strip())
    return json.loads(body)


def main() -> None:
    print("TylluanNexus stdio proxy ready — kernel at " + KERNEL_URL, file=sys.stderr, flush=True)

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue

        # Parse incoming message
        try:
            msg = json.loads(raw_line)
        except json.JSONDecodeError as e:
            send(error_response(None, -32700, f"Parse error: {e}"))
            continue

        is_notification = "id" not in msg  # JSON-RPC notifications have no id

        try:
            body = http_post(raw_line.encode("utf-8"))
        except urllib.error.URLError as e:
            # Kernel unreachable — only respond if this was a request, not a notification
            if not is_notification:
                reason = getattr(e, 'reason', str(e))
                send(error_response(msg.get("id"), -32000,
                     f"TylluanNexus kernel unreachable at {KERNEL_URL}: {reason}"))
            continue
        except Exception as e:
            if not is_notification:
                send(error_response(msg.get("id"), -32603, str(e)))
            continue

        # Notifications: kernel may return 202 empty — don't write anything to stdout
        if is_notification:
            continue

        # Requests: parse and forward the response
        try:
            send(parse_response(body))
        except (json.JSONDecodeError, ValueError) as e:
            send(error_response(msg.get("id"), -32603, f"Bad kernel response: {e}"))


if __name__ == "__main__":
    if "--help" in sys.argv:
        print("tylluan_stdio_proxy — MCP stdio bridge for TylluanNexus")
        print(f"Forwards to: {KERNEL_URL}")
        print("Usage: python tylluan_stdio_proxy.py")
        sys.exit(0)
    main()
