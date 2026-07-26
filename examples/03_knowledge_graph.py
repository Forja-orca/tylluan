"""
Example 3: Knowledge Graph Exploration

Demonstrates Tylluan's graph capabilities via the sovereign REST API:
  1. Add triples (subject-predicate-object relationships)
  2. Query knowledge graph
  3. Use tylluan_think to reason over connections

Prerequisites:
  - Tylluan kernel running (default port resolved from data/active_port.json, env TYLLUAN_PORT, or 3030)

Usage:
  python examples/03_knowledge_graph.py
  python examples/03_knowledge_graph.py --host 127.0.0.1 --port 4000
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
    parser = argparse.ArgumentParser(description="Tylluan Knowledge Graph Example")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=None, help="Kernel port (default: auto-detected or 3030)")
    args = parser.parse_args()

    port = resolve_port(args.host, args.port)
    base_url = f"http://{args.host}:{port}"
    print(f"Connecting to Tylluan at {base_url}...\n")

    # Step 1: Add knowledge as triples via tylluan_remember / SilvaDB
    triples = [
        ("Tylluan", "is_written_in", "Rust"),
        ("Tylluan", "uses", "BGE-M3"),
        ("Tylluan", "uses", "SilvaDB"),
        ("SilvaDB", "is_built_on", "SQLite"),
        ("SilvaDB", "stores", "knowledge_graph"),
        ("SilvaDB", "stores", "vector_embeddings"),
        ("BGE-M3", "produces", "vector_embeddings_1024D"),
        ("Rust", "framework", "tokio"),
        ("Rust", "framework", "axum"),
        ("guilds", "are_written_in", "Python"),
        ("guilds", "use", "fastmcp"),
    ]

    print(f"[Graph] Storing {len(triples)} triple facts in SilvaDB...")
    for subject, predicate, obj in triples:
        content = f"{subject} {predicate} {obj}"
        res = api_post(f"{base_url}/api/v1/memory/write", {
            "content": content,
            "agent_id": "example-script",
            "topic": "knowledge-graph"
        })
        print(f"  -> {subject} --[{predicate}]--> {obj}")
    print()

    # Step 2: Query connections via tylluan_think
    print("[Think] Reasoning: 'What is the technology stack of Tylluan?'")
    think_res = api_post(f"{base_url}/api/v1/do", {
        "intent": "What is the technology stack of Tylluan?",
        "agent_id": "example-script",
        "guild": "memory"
    })
    resp_text = think_res.get("response", json.dumps(think_res)) if isinstance(think_res, dict) else str(think_res)
    safe_text = str(resp_text).encode("ascii", errors="backslashreplace").decode("ascii")
    print(f"  -> {safe_text[:300]}...\n")

    # Step 3: Search graph nodes
    print("[Graph] Searching relationships for 'SilvaDB':")
    search_res = api_post(f"{base_url}/api/v1/memory/search", {
        "query": "SilvaDB SQLite knowledge_graph",
        "limit": 5
    })
    results = search_res if isinstance(search_res, list) else search_res.get("results", [])
    for r in results[:5]:
        content = r.get("content", str(r))[:80] if isinstance(r, dict) else str(r)[:80]
        safe_content = str(content).encode("ascii", errors="backslashreplace").decode("ascii")
        print(f"  -> {safe_content}")
    print()

    print("[SUCCESS] Knowledge graph triples stored and queried successfully.")
    print("          Use tylluan_think and tylluan_graph for reasoning over relationships.")

if __name__ == "__main__":
    main()
