"""
Example 1: Memory Basics — Remember, Recall, Think

Demonstrates Tylluan's core memory loop:
  1. Store knowledge with tylluan_remember (via REST API)
  2. Retrieve it with tylluan_recall
  3. Reason over it with tylluan_think

Prerequisites:
  - Tylluan kernel running at http://127.0.0.1:3033 (or :3030)
  - No authentication needed if dev_mode = true

Usage:
  python examples/01_memory_basics.py
  python examples/01_memory_basics.py --host 127.0.0.1 --port 3030
"""

import argparse
import json
import urllib.request

def api(host: str, port: int, endpoint: str, payload: dict) -> dict:
    url = f"http://{host}:{port}{endpoint}"
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())

def main():
    parser = argparse.ArgumentParser(description="Tylluan Memory Basics Example")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=3033)
    args = parser.parse_args()

    print(f"Connecting to Tylluan at {args.host}:{args.port}...")

    # Step 1: Check health
    health_url = f"http://{args.host}:{args.port}/health"
    with urllib.request.urlopen(health_url, timeout=5) as resp:
        health = json.loads(resp.read())
    print(f"✅ Kernel healthy: {health['status']} (v{health['version']})\n")

    # Step 2: Remember some facts
    facts = [
        "Tylluan is a sovereign cognitive substrate for AI agents",
        "The kernel is written in Rust using tokio and axum",
        "SilvaDB stores memories as a knowledge graph with vector embeddings",
        "There are 47 Python guilds that provide tools like bash, git, and filesystem",
        "The project uses BGE-M3 for embeddings and Jina Reranker for search quality",
    ]

    print("📝 Storing 5 facts in memory...")
    for fact in facts:
        result = api(args.host, args.port, "/api/v1/remember", {
            "content": fact,
            "agent_id": "example-script",
        })
        print(f"  → Stored: {fact[:60]}...")
    print()

    # Step 3: Recall a specific fact
    print("🔍 Searching memory: 'What language is the kernel written in?'")
    result = api(args.host, args.port, "/api/v1/recall", {
        "query": "What language is the kernel written in?",
        "limit": 3,
    })
    print(f"  → Found {len(result.get('results', []))} results")
    for r in result.get("results", [])[:3]:
        score = r.get("score", 0)
        content = r.get("content", "")[:80]
        print(f"     [{score:.2f}] {content}...")
    print()

    # Step 4: Think about relationships
    print("🧠 Reasoning: 'How does Tylluan store and retrieve knowledge?'")
    result = api(args.host, args.port, "/api/v1/think", {
        "query": "How does Tylluan store and retrieve knowledge?",
        "depth": 2,
    })
    if "analysis" in result:
        print(f"  → Analysis: {result['analysis'][:200]}...")
    elif "knowledge" in result:
        print(f"  → Found {len(result['knowledge'])} related nodes")
    else:
        print(f"  → Response: {json.dumps(result)[:200]}...")
    print()

    print("✅ Done! Your memories are now persistent in SilvaDB.")
    print("   They will survive kernel restarts and be available to any MCP client.")

if __name__ == "__main__":
    main()
