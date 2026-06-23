"""
Example 2: Multi-Agent Communication via Coloquio

Demonstrates how multiple agents communicate through Tylluan's
coloquio channels — shared message boards with persistent history.

  1. Create a channel
  2. Agent A posts a message
  3. Agent B reads and replies
  4. Both agents see the full conversation

Prerequisites:
  - Tylluan kernel running at http://127.0.0.1:3033 (or :3030)

Usage:
  python examples/02_multi_agent_coloquio.py
"""

import argparse
import json
import urllib.request

def do(host: str, port: int, intent: str, agent_id: str = "unknown", guild: str = None) -> dict:
    payload = {"intent": intent, "agent_id": agent_id}
    if guild:
        payload["guild"] = guild
    url = f"http://{host}:{port}/api/v1/do"
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())

def main():
    parser = argparse.ArgumentParser(description="Tylluan Multi-Agent Coloquio Example")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=3033)
    args = parser.parse_args()

    print(f"Connecting to Tylluan at {args.host}:{args.port}...\n")

    # Step 1: Create a channel
    print("📢 Creating channel #example-chat...")
    result = do(args.host, args.port,
        "publica en coloquio example-chat: Channel created for demo",
        agent_id="setup", guild="coloquio")
    print(f"  → {json.dumps(result)[:100]}...\n")

    # Step 2: Agent A posts
    print("🤖 Agent A (researcher) posts a finding...")
    result = do(args.host, args.port,
        "publica en coloquio example-chat: I found that BGE-M3 embeddings perform well on CPU with 768-dim vectors. Latency is acceptable for local use.",
        agent_id="researcher", guild="coloquio")
    print(f"  → Posted\n")

    # Step 3: Agent B reads and replies
    print("🤖 Agent B (engineer) reads the channel...")
    result = do(args.host, args.port,
        "lee los mensajes de coloquio example-chat",
        agent_id="engineer", guild="coloquio")
    print(f"  → Read channel\n")

    print("🤖 Agent B (engineer) replies...")
    result = do(args.host, args.port,
        "publica en coloquio example-chat: Good finding. We should benchmark with INT8 quantization next. I can set up the eval harness.",
        agent_id="engineer", guild="coloquio")
    print(f"  → Posted\n")

    # Step 4: Agent A reads the full conversation
    print("🤖 Agent A reads the full conversation...")
    result = do(args.host, args.port,
        "lee los mensajes de coloquio example-chat",
        agent_id="researcher", guild="coloquio")
    response_text = result.get("response", json.dumps(result))
    print(f"  → {str(response_text)[:300]}...\n")

    print("✅ Done! Both agents communicated through a persistent channel.")
    print("   Messages survive kernel restarts. Any MCP client can join the conversation.")

if __name__ == "__main__":
    main()
