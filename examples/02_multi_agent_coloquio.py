"""
Example 2: Multi-Agent Communication via Coloquio

Demonstrates how multiple agents communicate through Tylluan's
coloquio channels — shared message boards with persistent history.

  1. Create a channel
  2. Agent A posts a message
  3. Agent B reads and replies
  4. Both agents see the full conversation

Prerequisites:
  - Tylluan kernel running (default port resolved from data/active_port.json, env TYLLUAN_PORT, or 3030)

Usage:
  python examples/02_multi_agent_coloquio.py
  python examples/02_multi_agent_coloquio.py --host 127.0.0.1 --port 4000
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

def do(host: str, port: int, intent: str, agent_id: str = "unknown", guild: str = None) -> dict:
    payload = {"intent": intent, "agent_id": agent_id}
    if guild:
        payload["guild"] = guild
    url = f"http://{host}:{port}/api/v1/do"
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except Exception as e:
        return {"error": str(e)}

def main():
    parser = argparse.ArgumentParser(description="Tylluan Multi-Agent Coloquio Example")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=None, help="Kernel port (default: auto-detected or 3030)")
    args = parser.parse_args()

    port = resolve_port(args.host, args.port)
    print(f"Connecting to Tylluan at http://{args.host}:{port}...\n")

    # Step 1: Create a channel
    print("[Coloquio] Creating channel #example-chat...")
    result = do(args.host, port,
        "publica en coloquio example-chat: Channel created for demo",
        agent_id="setup", guild="coloquio")
    print(f"  -> {json.dumps(result)[:100]}...\n")

    # Step 2: Agent A posts
    print("[Agent A] (researcher) posts a finding...")
    result = do(args.host, port,
        "publica en coloquio example-chat: BGE-M3 embeddings (1024D) perform well on CPU for hybrid search.",
        agent_id="researcher", guild="coloquio")
    print(f"  -> Posted\n")

    # Step 3: Agent B reads and replies
    print("[Agent B] (engineer) reads the channel...")
    result = do(args.host, port,
        "lee los mensajes de coloquio example-chat",
        agent_id="engineer", guild="coloquio")
    print(f"  -> Read channel\n")

    print("[Agent B] (engineer) replies...")
    result = do(args.host, port,
        "publica en coloquio example-chat: Confirmed. We benchmarked INT8 ONNX models and throughput is solid.",
        agent_id="engineer", guild="coloquio")
    print(f"  -> Posted\n")

    # Step 4: Agent A reads the full conversation
    print("[Agent A] reads the full conversation...")
    result = do(args.host, port,
        "lee los mensajes de coloquio example-chat",
        agent_id="researcher", guild="coloquio")
    response_text = result.get("response", json.dumps(result))
    safe_text = str(response_text).encode("ascii", errors="backslashreplace").decode("ascii")
    print(f"  -> {safe_text[:300]}...\n")

    print("[SUCCESS] Both agents communicated through a persistent channel.")
    print("          Messages survive kernel restarts. Any MCP client can join the conversation.")

if __name__ == "__main__":
    main()
