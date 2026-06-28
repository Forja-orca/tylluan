import json
import logging
import os

import httpx
from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-local-llm-proxy")

BASE_URL = (os.environ.get("LOCAL_LLM_URL") or "http://localhost:11434").rstrip("/")
PROVIDER = os.environ.get("LOCAL_LLM_PROVIDER") or "ollama"


@mcp.tool()
async def llm_chat(prompt: str, model: str = "", system: str = "", temperature: float = 0.7, max_tokens: int = 1024) -> str:
    """Send a chat completion request to a local LLM (Ollama, LM Studio, or OpenAI-compatible)."""
    mdl = model or ("llama3" if PROVIDER == "ollama" else "default")
    msgs = []
    if system:
        msgs.append({"role": "system", "content": system})
    msgs.append({"role": "user", "content": prompt})

    try:
        async with httpx.AsyncClient(timeout=120) as client:
            if PROVIDER == "ollama":
                r = await client.post(f"{BASE_URL}/api/chat", json={
                    "model": mdl, "messages": msgs, "stream": False,
                    "options": {"temperature": temperature, "num_predict": max_tokens},
                })
                r.raise_for_status()
                result = r.json()
                return result.get("message", {}).get("content", json.dumps(result))
            else:
                r = await client.post(f"{BASE_URL}/v1/chat/completions", json={
                    "model": mdl, "messages": msgs, "temperature": temperature, "max_tokens": max_tokens,
                })
                r.raise_for_status()
                result = r.json()
                return result.get("choices", [{}])[0].get("message", {}).get("content", json.dumps(result))
    except Exception as e:
        return f"LLM request failed: {e}\nURL: {BASE_URL}\nProvider: {PROVIDER}"


@mcp.tool()
async def llm_list_models() -> str:
    """List all available models on the local LLM instance."""
    try:
        async with httpx.AsyncClient(timeout=10) as client:
            if PROVIDER == "ollama":
                r = await client.get(f"{BASE_URL}/api/tags")
                r.raise_for_status()
                data = r.json()
                models = data.get("models", [])
                items = [f"  - {m['name']} ({round(m.get('size', 0) / 1e9, 1)}GB)" for m in models]
            else:
                r = await client.get(f"{BASE_URL}/v1/models")
                r.raise_for_status()
                data = r.json()
                items = [f"  - {m['id']}" for m in data.get("data", [])]
        if items:
            return f"Available Models ({PROVIDER}):\n" + "\n".join(items)
        return "No models found."
    except Exception as e:
        return f"Cannot list models: {e}"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
