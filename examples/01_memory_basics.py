"""
Example 1: Memory Basics — Remember, Recall, Think

Demonstrates Tylluan's core memory loop using the sovereign REST API:
  1. Store knowledge with tylluan_remember (/api/v1/memory/write or /api/v1/do)
  2. Retrieve it with tylluan_recall (/api/v1/memory/search or /api/v1/do)
  3. Reason over it with tylluan_think (/api/v1/do)

Prerequisites:
  - Tylluan kernel running (default port resolved from data/active_port.json, env TYLLUAN_PORT, or 3030)

Usage:
  python examples/01_memory_basics.py
  python examples/01_memory_basics.py --host 127.0.0.1 --port 4000
"""

import argparse
import json
import os
import sys
import urllib.request
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")

def resolve_port(host: str, user_port: int = None) -> int:
    if user_port:
        return user_port
    env_port = os.environ.get("TYLLUAN_PORT")
    if env_port:
        return int(env_port)
    port_file = Path("data/active_port.json")
    if port_file.exists():
        try:
            data = json.loads(port_file.read_text())
            return int(data.get("port", 3030))
        except Exception:
            pass
    return 3030

def api_post(url: str, payload: dict):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except Exception as e:
        return {"error": str(e)}

def main():
    parser = argparse.ArgumentParser(description="Tylluan Memory Basics Example")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=None, help="Kernel port (default: auto-detected or 3030)")
    args = parser.parse_args()

    port = resolve_port(args.host, args.port)
    base_url = f"http://{args.host}:{port}"
    print(f"Connecting to Tylluan at {base_url}...")

    # Step 1: Health check
    try:
        with urllib.request.urlopen(f"{base_url}/health", timeout=5) as resp:
            health = json.loads(resp.read())
        print(f"[OK] Kernel healthy: {health.get('status')} (v{health.get('version')})\n")
    except Exception as e:
        print(f"[ERROR] Could not connect to kernel at {base_url}: {e}")
        sys.exit(1)

    # Step 2: Remember some facts
    facts = [
        "Tylluan is a sovereign cognitive substrate for AI agents",
        "The kernel is written in Rust using tokio and axum",
        "SilvaDB stores memories as a knowledge graph with BGE-M3 vector embeddings (1024D)",
        "There are 42 Python guilds that provide tools like bash, git, and filesystem",
        "The memory pipeline uses BM25 + FTS5 + BGE-M3 vector hybrid search with RRF fusion",
    ]

    print("Storing 5 facts in memory...")
    for fact in facts:
        res = api_post(f"{base_url}/api/v1/memory/write", {
            "content": fact,
            "agent_id": "example-script",
            "topic": "system-architecture"
        })
        status = res.get("status", "ok") if isinstance(res, dict) else "ok"
        print(f"  -> Stored [{status}]: {fact[:60]}...")
    print()

    # Step 3: Recall a specific fact
    print("Searching memory: 'What language is the kernel written in?'")
    search_res = api_post(f"{base_url}/api/v1/memory/search", {
        "query": "What language is the kernel written in?",
        "limit": 3
    })
    results = search_res if isinstance(search_res, list) else search_res.get("results", [])
    print(f"  -> Found {len(results)} results")
    for r in results[:3]:
        score = r.get("score", r.get("rrf_score", 0.0)) if isinstance(r, dict) else 0.0
        content = r.get("content", str(r))[:80] if isinstance(r, dict) else str(r)[:80]
        print(f"     [{score:.3f}] {content}...")
    print()

    # Step 4: Reason via tylluan_think tool
    print("Reasoning via tylluan_think: 'How does Tylluan store and retrieve knowledge?'")
    think_res = api_post(f"{base_url}/api/v1/do", {
        "intent": "How does Tylluan store and retrieve knowledge?",
        "agent_id": "example-script",
        "guild": "memory"
    })
    resp_text = think_res.get("response", json.dumps(think_res)) if isinstance(think_res, dict) else str(think_res)
    safe_text = str(resp_text).encode("ascii", errors="backslashreplace").decode("ascii")
    print(f"  -> Response: {safe_text[:200]}...\n")

    print("[SUCCESS] Your memories are persistent in SilvaDB.")
    print("          They will survive kernel restarts and be accessible to all MCP clients.")

if __name__ == "__main__":
    main()
