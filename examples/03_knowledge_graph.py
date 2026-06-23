"""
Example 3: Knowledge Graph Exploration

Demonstrates Tylluan's graph capabilities:
  1. Add triples (subject-predicate-object relationships)
  2. Query graph stats
  3. Use tylluan_think to reason over connections
  4. Traverse paths between concepts

Prerequisites:
  - Tylluan kernel running at http://127.0.0.1:3033 (or :3030)

Usage:
  python examples/03_knowledge_graph.py
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
    parser = argparse.ArgumentParser(description="Tylluan Knowledge Graph Example")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=3033)
    args = parser.parse_args()

    print(f"Connecting to Tylluan at {args.host}:{args.port}...\n")

    # Step 1: Add knowledge as triples
    triples = [
        ("Tylluan", "is_written_in", "Rust"),
        ("Tylluan", "uses", "BGE-M3"),
        ("Tylluan", "uses", "SilvaDB"),
        ("SilvaDB", "is_built_on", "SQLite"),
        ("SilvaDB", "stores", "knowledge_graph"),
        ("SilvaDB", "stores", "vector_embeddings"),
        ("BGE-M3", "produces", "vector_embeddings"),
        ("Rust", "framework", "tokio"),
        ("Rust", "framework", "axum"),
        ("guilds", "are_written_in", "Python"),
        ("guilds", "use", "fastmcp"),
        ("Tylluan", "has", "guilds"),
    ]

    print(f"📊 Adding {len(triples)} relationships to the knowledge graph...")
    for subject, predicate, obj in triples:
        result = api(args.host, args.port, "/api/v1/graph", {
            "command": "add_triple",
            "subject": subject,
            "predicate": predicate,
            "object": obj,
        })
        print(f"  → {subject} --[{predicate}]--> {obj}")
    print()

    # Step 2: Check graph stats
    print("📈 Graph statistics:")
    stats = api(args.host, args.port, "/api/v1/graph", {"command": "stats"})
    print(f"  Nodes: {stats.get('node_count', '?')}")
    print(f"  Edges: {stats.get('edge_count', '?')}")
    print()

    # Step 3: Think about connections
    print("🧠 Reasoning: 'What is the technology stack of Tylluan?'")
    result = api(args.host, args.port, "/api/v1/think", {
        "query": "What is the technology stack of Tylluan?",
        "depth": 2,
    })
    if "knowledge" in result:
        print(f"  → Found {len(result['knowledge'])} related concepts")
        for item in result["knowledge"][:5]:
            content = item.get("content", str(item))[:80]
            print(f"     • {content}")
    else:
        print(f"  → {json.dumps(result)[:200]}")
    print()

    # Step 4: Query neighbors
    print("🔗 Neighbors of 'SilvaDB':")
    neighbors = api(args.host, args.port, "/api/v1/graph", {
        "command": "list_neighbors",
        "entity": "SilvaDB",
    })
    for n in neighbors.get("neighbors", [])[:10]:
        print(f"  → {n}")
    print()

    print("✅ Done! The knowledge graph now contains structured relationships.")
    print("   Use tylluan_think to reason over connections.")
    print("   Use tylluan_graph to query paths and neighbors.")

if __name__ == "__main__":
    main()
