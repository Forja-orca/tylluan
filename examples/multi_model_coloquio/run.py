#!/usr/bin/env python3
"""
Tylluan — Autonomous Multi-Model Coloquio Demo

Two agents (scout and analyst) coordinate via @mention pull, no orchestrator.
Works in echo mode with zero setup. Add --llm-url for real LLM responses.

Pattern demonstrated:
  user  → @scout "research something"
  scout ←  SSE stream detects mention → LLM/echo → posts reply @analyst
  analyst ← SSE stream detects mention → LLM/echo → posts reply

This is the inverse of AutoGen/LangGraph (push from orchestrator).
Here the protocol IS the coordination layer.

Usage:
    python run.py                                      # echo mode, no LLM
    python run.py --llm-url http://localhost:1234/v1   # LM Studio
    python run.py --llm-url http://localhost:11434/v1  # Ollama
    python run.py --kernel http://127.0.0.1:3030       # different port

Prerequisites:
    Tylluan kernel running. Start with: tylluan.bat  or  docker compose up -d
"""

import argparse
import json
import sys
import threading
import time
import urllib.error
import urllib.request
from typing import Optional


# ── REST helpers ────────────────────────────────────────────────────────────

def _request(url: str, method: str = "GET", body: Optional[dict] = None,
             token: Optional[str] = None, timeout: int = 10) -> dict:
    data = json.dumps(body).encode() if body is not None else None
    headers: dict = {}
    if data:
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        body_text = e.read().decode(errors="replace")
        return {"_http_error": e.code, "detail": body_text}
    except Exception as exc:
        return {"_error": str(exc)}


def health(kernel: str, token: Optional[str]) -> dict:
    return _request(f"{kernel}/health", token=token)


def start_session(kernel: str, agent_id: str, token: Optional[str]) -> dict:
    return _request(
        f"{kernel}/api/v1/agents/session/start", "POST",
        {"agent_id": agent_id, "ttl_secs": 600, "max_responses": 20},
        token=token,
    )


def stop_session(kernel: str, agent_id: str, token: Optional[str]) -> None:
    _request(
        f"{kernel}/api/v1/agents/session/stop", "POST",
        {"agent_id": agent_id}, token=token,
    )


def heartbeat(kernel: str, agent_id: str, token: Optional[str]) -> dict:
    return _request(
        f"{kernel}/api/v1/agents/heartbeat", "POST",
        {"agent_id": agent_id}, token=token,
    )


def post_message(kernel: str, channel: str, content: str,
                 author: str, token: Optional[str]) -> dict:
    return _request(
        f"{kernel}/api/v1/coloquio/channels/{channel}/post", "POST",
        {"content": content, "author_id": author, "role": "agent"},
        token=token,
    )


def read_thread(kernel: str, channel: str, token: Optional[str]) -> list:
    result = _request(f"{kernel}/api/v1/coloquio/channels/{channel}/messages", token=token)
    return result.get("messages", result.get("turns", []))


def llm_complete(llm_url: str, model: str, system: str, user: str) -> str:
    body = {
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_tokens": 120,
        "temperature": 0.7,
    }
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        f"{llm_url}/chat/completions",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            result = json.loads(r.read())
            return result["choices"][0]["message"]["content"].strip()
    except Exception as exc:
        return f"[LLM error: {exc}]"


# ── Agent watcher ────────────────────────────────────────────────────────────

class AgentWatcher(threading.Thread):
    """
    Subscribes to the kernel SSE stream via urllib (not httpx).
    urllib.request uses readline() internally — this bypasses the
    asyncio/ProactorEventLoop buffering bug that silently drops SSE
    chunks on Windows localhost connections.
    """

    def __init__(
        self,
        kernel: str,
        agent_id: str,
        channel: str,
        *,
        llm_url: Optional[str],
        llm_model: str,
        token: Optional[str],
        stop_event: threading.Event,
        print_lock: threading.Lock,
    ):
        super().__init__(daemon=True, name=f"watcher-{agent_id}")
        self.kernel = kernel
        self.agent_id = agent_id
        self.channel = channel
        self.llm_url = llm_url
        self.llm_model = llm_model
        self.token = token
        self.stop_event = stop_event
        self.print_lock = print_lock
        self._last_turn: int = 0
        self.responses_sent: int = 0

    def run(self) -> None:
        headers: dict = {}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        req = urllib.request.Request(f"{self.kernel}/api/v1/events", headers=headers)
        try:
            with urllib.request.urlopen(req) as response:  # no timeout = persistent stream
                for line_bytes in response:  # readline()-based — no buffering issue
                    if self.stop_event.is_set():
                        return
                    line = line_bytes.decode("utf-8", errors="ignore").strip()
                    if not line.startswith("data:"):
                        continue
                    payload = line[5:].strip()
                    if not payload:
                        continue
                    try:
                        event = json.loads(payload)
                    except json.JSONDecodeError:
                        continue
                    self._handle(event)
        except Exception as exc:
            if not self.stop_event.is_set():
                with self.print_lock:
                    print(f"  [{self.agent_id}] SSE stream ended: {exc}", flush=True)

    def _handle(self, event: dict) -> None:
        if event.get("type") != "coloquio:new_turn":
            return
        if event.get("channel_id") != self.channel:
            return
        if event.get("author_id") == self.agent_id:
            return
        content: str = event.get("content", "")
        if f"@{self.agent_id}" not in content:
            return
        turn: int = event.get("turn", 0)
        if turn <= self._last_turn:
            return
        self._last_turn = turn

        sender = event.get("author_id", "?")
        with self.print_lock:
            print(f"  [{self.agent_id}] got mention from @{sender} at turn {turn}", flush=True)

        reply = self._generate_reply(content, sender)
        if reply:
            post_message(self.kernel, self.channel, reply, self.agent_id, self.token)
            heartbeat(self.kernel, self.agent_id, self.token)
            self.responses_sent += 1
            with self.print_lock:
                preview = reply[:100] + ("..." if len(reply) > 100 else "")
                print(f"  [{self.agent_id}] -> {preview}", flush=True)

    def _generate_reply(self, incoming: str, sender: str) -> Optional[str]:
        if self.llm_url:
            system = (
                f"You are {self.agent_id}, a specialist agent in a multi-agent system. "
                "Respond concisely (2-3 sentences). If appropriate, mention the next agent "
                "in the chain using @agent-id syntax."
            )
            return llm_complete(self.llm_url, self.llm_model, system, incoming)

        # Echo mode: deterministic, no LLM needed
        if self.agent_id == "scout":
            return (
                f"@{sender} Acknowledged. I've run the preliminary scan. "
                "Key finding: autonomous @mention pull scales to any runtime without config changes. "
                "@analyst — your synthesis, please."
            )
        if self.agent_id == "analyst":
            return (
                f"@{sender} Synthesis complete. The pull pattern inverts AutoGen/LangGraph: "
                "coordination emerges from the protocol, not from an orchestrator. "
                "This chain ran across three runtimes with zero config."
            )
        return f"[@{self.agent_id}] Received from @{sender}: {incoming[:60]}"


# ── Main ─────────────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(
        description="Tylluan autonomous @mention chain demo",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--kernel", default="http://127.0.0.1:3033",
                        help="Tylluan kernel URL (default: http://127.0.0.1:3033)")
    parser.add_argument("--llm-url", default=None,
                        help="OpenAI-compatible endpoint, e.g. http://localhost:1234/v1")
    parser.add_argument("--model", default="local-model",
                        help="Model ID to request from the LLM endpoint")
    parser.add_argument("--channel", default="demo-chain",
                        help="Coloquio channel to use (created if absent)")
    parser.add_argument("--token", default=None,
                        help="Bearer token (leave empty when dev_mode = true)")
    parser.add_argument("--wait", type=int, default=25,
                        help="Seconds to wait for chain to complete (default: 25)")
    args = parser.parse_args()

    mode = f"LLM via {args.llm_url}" if args.llm_url else "echo mode (no LLM required)"

    print()
    print("Tylluan — Autonomous @mention Chain")
    print(f"  kernel : {args.kernel}")
    print(f"  channel: #{args.channel}")
    print(f"  mode   : {mode}")
    print()

    # Verify kernel
    h = health(args.kernel, args.token)
    if "_error" in h or "_http_error" in h:
        print(f"ERROR: kernel not reachable at {args.kernel}")
        print("  Start with:  tylluan.bat          (Windows)")
        print("               docker compose up -d  (any platform)")
        sys.exit(1)
    print(f"  kernel {h.get('version', '?')} OK\n")

    # Register agent sessions (409 = already exists, that's fine)
    for agent_id in ("scout", "analyst"):
        r = start_session(args.kernel, agent_id, args.token)
        if "_error" in r:
            print(f"  warning: could not register {agent_id}: {r}")
        else:
            status = r.get("status", "?")
            conflict = r.get("_http_error") == 409
            label = "already active" if conflict else status
            print(f"  registered {agent_id}: {label}")

    stop = threading.Event()
    lock = threading.Lock()

    watchers = [
        AgentWatcher(
            args.kernel, "scout", args.channel,
            llm_url=args.llm_url, llm_model=args.model,
            token=args.token, stop_event=stop, print_lock=lock,
        ),
        AgentWatcher(
            args.kernel, "analyst", args.channel,
            llm_url=args.llm_url, llm_model=args.model,
            token=args.token, stop_event=stop, print_lock=lock,
        ),
    ]

    for w in watchers:
        w.start()

    time.sleep(0.8)  # let SSE connections establish before posting

    # Fire the chain
    trigger = "@scout I need a quick analysis of autonomous agent coordination patterns. Go."
    print(f"\n  trigger -> #{args.channel}: {trigger}\n")
    post_message(args.kernel, args.channel, trigger, "user", args.token)

    # Wait for chain
    deadline = time.time() + args.wait
    while time.time() < deadline:
        total = sum(w.responses_sent for w in watchers)
        if total >= 2:
            break
        time.sleep(0.5)

    stop.set()

    # Final summary
    print(f"\n{'-' * 60}")
    print(f"  #{args.channel} -- final state")
    print(f"{'-' * 60}")
    messages = read_thread(args.kernel, args.channel, args.token)
    for msg in messages[-6:]:
        author = msg.get("author_id", "?")
        content = msg.get("content", "")
        turn = msg.get("turn", "?")
        print(f"  [T{turn:>3}] {author:<12} {content[:90]}")

    total_replies = sum(w.responses_sent for w in watchers)
    print(f"\n  {total_replies} autonomous replies — zero orchestrator config")

    # Cleanup
    for agent_id in ("scout", "analyst"):
        stop_session(args.kernel, agent_id, args.token)


if __name__ == "__main__":
    main()
