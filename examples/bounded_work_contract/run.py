#!/usr/bin/env python3
"""
Tylluan M10 -- Bounded Work Contract demo

Three agents solve a problem in max 5 cycles. No external API needed.
Shows the full lifecycle: open -> in_progress -> extension request -> done.

Usage:
    python run.py                              # echo mode, no LLM
    python run.py --kernel http://127.0.0.1:3033
    python run.py --budget 3                   # force extension request
"""

import argparse
import json
import sys
import threading
import time
import urllib.error
import urllib.request
from typing import Optional


# -- REST helpers ------------------------------------------------------------

def _req(url: str, method: str = "GET", body: Optional[dict] = None,
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
        return {"_http_error": e.code, "detail": e.read().decode(errors="replace")}
    except Exception as exc:
        return {"_error": str(exc)}


def create_contract(kernel: str, task: str, budget: int,
                    team: list, consolidator: str, channel: str,
                    token: Optional[str]) -> dict:
    return _req(f"{kernel}/api/v1/work-contracts", "POST", {
        "task": task, "budget": budget, "team": team,
        "consolidator": consolidator, "channel_id": channel,
    }, token=token)


def tick(kernel: str, contract_id: str, agent_id: str,
         token: Optional[str]) -> dict:
    return _req(
        f"{kernel}/api/v1/work-contracts/{contract_id}/tick", "POST",
        {"agent_id": agent_id}, token=token,
    )


def deliver(kernel: str, contract_id: str, agent_id: str,
            summary: str, token: Optional[str]) -> dict:
    return _req(
        f"{kernel}/api/v1/work-contracts/{contract_id}/deliver", "POST",
        {"agent_id": agent_id, "summary": summary}, token=token,
    )


def vote_extension(kernel: str, contract_id: str, agent_id: str,
                   cycles: int, token: Optional[str]) -> dict:
    return _req(
        f"{kernel}/api/v1/work-contracts/{contract_id}/vote", "POST",
        {"agent_id": agent_id, "vote": "approve", "cycles": cycles}, token=token,
    )


def close_contract(kernel: str, contract_id: str, agent_id: str,
                   summary: str, token: Optional[str]) -> dict:
    return _req(
        f"{kernel}/api/v1/work-contracts/{contract_id}/close", "POST",
        {"agent_id": agent_id, "summary": summary}, token=token,
    )


def post_message(kernel: str, channel: str, content: str,
                 author: str, token: Optional[str]) -> dict:
    return _req(
        f"{kernel}/api/v1/coloquio/channels/{channel}/post", "POST",
        {"content": content, "author_id": author, "role": "agent"}, token=token,
    )


def read_thread(kernel: str, channel: str, token: Optional[str]) -> list:
    r = _req(f"{kernel}/api/v1/coloquio/channels/{channel}/messages", token=token)
    return r.get("messages", r.get("turns", []))


# -- Mock tick (used when kernel does not have M10 endpoints yet) ------------

class MockBudget:
    """Local budget gate for demo until kernel M10 endpoints land."""
    def __init__(self, budget: int):
        self._lock = threading.Lock()
        self._remaining = budget
        self._budget = budget

    def tick(self) -> dict:
        with self._lock:
            if self._remaining <= 0:
                return {"remaining": 0, "action": "request_extension"}
            self._remaining -= 1
            return {"remaining": self._remaining}

    def extend(self, cycles: int) -> None:
        with self._lock:
            self._remaining += cycles

    @property
    def remaining(self) -> int:
        with self._lock:
            return self._remaining


# -- Agent watcher -----------------------------------------------------------

class ContractAgent(threading.Thread):
    """
    Agent that works under a Bounded Work Contract.
    Respects budget gate: stops and requests extension when cycles run out.
    SSE via urllib readline() -- no asyncio buffering issue on Windows.
    """

    def __init__(self, kernel: str, agent_id: str, channel: str,
                 budget: MockBudget, role: str, next_agent: Optional[str],
                 stop_event: threading.Event, print_lock: threading.Lock,
                 token: Optional[str] = None):
        super().__init__(daemon=True, name=f"agent-{agent_id}")
        self.kernel = kernel
        self.agent_id = agent_id
        self.channel = channel
        self.budget = budget
        self.role = role
        self.next_agent = next_agent  # agent to hand off to after delivery
        self.stop_event = stop_event
        self.print_lock = print_lock
        self.token = token
        self._last_turn = 0
        self.delivered = False

    def run(self) -> None:
        headers = {}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        req = urllib.request.Request(f"{self.kernel}/api/v1/events", headers=headers)
        try:
            with urllib.request.urlopen(req) as response:
                for line_bytes in response:
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
                    print(f"  [{self.agent_id}] stream ended: {exc}", flush=True)

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

        # Budget gate
        result = self.budget.tick()
        remaining = result["remaining"]
        n = self.budget._budget - remaining

        with self.print_lock:
            print(f"  [{self.agent_id}] mention at T{turn} -- cycle {n}, budget left: {remaining}",
                  flush=True)

        if result.get("action") == "request_extension":
            msg = (
                f"[SOLICITUD-EXTENSION: +5 ciclos. "
                f"Razon: budget agotado, entrega pendiente ({self.role})]"
            )
            post_message(self.kernel, self.channel, msg, self.agent_id, self.token)
            with self.print_lock:
                print(f"  [{self.agent_id}] budget exhausted -- extension requested", flush=True)
            return

        reply = self._work(content, n, remaining)
        post_message(self.kernel, self.channel, reply, self.agent_id, self.token)

        with self.print_lock:
            print(f"  [{self.agent_id}] -> {reply[:90]}", flush=True)

    def _work(self, incoming: str, cycle: int, remaining: int) -> str:
        prefix = f"[CICLO-{cycle}]"
        if remaining <= 2:
            prefix = f"[CICLO-{cycle}/WARNING:budget_low:{remaining}]"

        if not self.delivered and cycle >= 2:
            self.delivered = True
            suffix = ""
            if self.next_agent:
                suffix = f" @{self.next_agent} your turn."
            return (
                f"{prefix} [ENTREGA] {self.role} complete. "
                f"Findings integrated and tested.{suffix}"
            )

        handoff = f" @{self.next_agent} take note." if self.next_agent else ""
        return f"{prefix} Working on {self.role}. Progress nominal.{handoff}"


# -- Main --------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(description="M10 Bounded Work Contract demo")
    parser.add_argument("--kernel", default="http://127.0.0.1:3033")
    parser.add_argument("--channel", default="demo-bwc")
    parser.add_argument("--budget", type=int, default=5,
                        help="Total cycle budget (try --budget 3 to force extension)")
    parser.add_argument("--token", default=None)
    parser.add_argument("--wait", type=int, default=30)
    args = parser.parse_args()

    print()
    print("Tylluan M10 -- Bounded Work Contract")
    print(f"  kernel : {args.kernel}")
    print(f"  channel: #{args.channel}")
    print(f"  budget : {args.budget} cycles across 3 agents")
    print()

    # Verify kernel
    h = _req(f"{args.kernel}/health", token=args.token)
    if "_error" in h or "_http_error" in h:
        print(f"ERROR: kernel not reachable at {args.kernel}")
        print("  Start with: tylluan.bat  or  docker compose up -d")
        sys.exit(1)
    print(f"  kernel {h.get('version', '?')} OK")

    budget = MockBudget(args.budget)
    stop = threading.Event()
    lock = threading.Lock()

    agents = [
        ContractAgent(args.kernel, "researcher", args.channel,
                      budget, "data research", "engineer",
                      stop, lock, args.token),
        ContractAgent(args.kernel, "engineer", args.channel,
                      budget, "implementation", "reviewer",
                      stop, lock, args.token),
        ContractAgent(args.kernel, "reviewer", args.channel,
                      budget, "code review", None,
                      stop, lock, args.token),
    ]

    print(f"\n  Starting {len(agents)} agents on shared budget of {args.budget} cycles...")
    for a in agents:
        a.start()

    time.sleep(0.8)

    # Fire the contract
    task = (
        f"CONTRACT OPEN — budget:{args.budget} cycles — team:researcher,engineer,reviewer. "
        "@researcher begin: analyze the autonomous coordination pattern."
    )
    print(f"\n  -> #{args.channel}: {task[:80]}...\n")
    post_message(args.kernel, args.channel, task, "user", args.token)

    # Wait
    deadline = time.time() + args.wait
    while time.time() < deadline:
        delivered = sum(1 for a in agents if a.delivered)
        exhausted = budget.remaining == 0
        if delivered >= 2 or exhausted:
            break
        time.sleep(0.5)

    stop.set()

    # Show result
    print(f"\n{'-' * 60}")
    print(f"  #{args.channel} -- final state")
    print(f"  budget remaining: {budget.remaining}/{args.budget}")
    print(f"{'-' * 60}")
    for msg in read_thread(args.kernel, args.channel, args.token)[-8:]:
        author = msg.get("author_id", "?")
        content = msg.get("content", "")
        turn = msg.get("turn", "?")
        tag = "[EXT]" if "SOLICITUD-EXTENSION" in content else ""
        print(f"  [T{turn:>3}] {author:<12} {tag}{content[:80]}")

    delivered = sum(1 for a in agents if a.delivered)
    print(f"\n  {delivered}/3 deliveries -- {args.budget - budget.remaining} cycles used")
    if budget.remaining == 0:
        print("  Budget exhausted -- extension protocol triggered (check coloquio)")
    else:
        print("  Contract completed within budget")


if __name__ == "__main__":
    main()
