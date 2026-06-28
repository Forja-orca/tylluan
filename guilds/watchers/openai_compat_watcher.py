"""
OpenAICompatWatcher — autonomous watcher for any OpenAI-compatible inference endpoint.

Provider-agnostic: works with LM Studio, Ollama, Hermes gateway, vLLM,
llama.cpp, OpenAI, Anthropic (via compat layer), or any /v1/chat/completions endpoint.

Configure via environment variables:
  WATCHER_LLM_URL    Base URL of the inference endpoint (no /v1 suffix)
                     Default: http://127.0.0.1:1234  (LM Studio)
  WATCHER_LLM_MODEL  Model name to request
                     Default: google/gemma-4-12b
  WATCHER_LLM_KEY    API key (use "lm-studio" or "ollama" for local endpoints
                     that require a non-empty key but don't validate it)
                     Default: lm-studio
  WATCHER_AGENT_ID   Agent identity in the coloquio (default: assistant)
  FORJA_URL          Tylluan kernel URL (default: http://127.0.0.1:3033)

Examples:
  # LM Studio (default)
  python openai_compat_watcher.py haiku

  # Ollama
  WATCHER_LLM_URL=http://127.0.0.1:11434 WATCHER_LLM_MODEL=llama3.2 \\
    python openai_compat_watcher.py haiku

  # Hermes gateway (once port is known)
  WATCHER_LLM_URL=http://127.0.0.1:9119 WATCHER_LLM_MODEL=hermes-3 \\
    python openai_compat_watcher.py haiku

  # OpenAI
  WATCHER_LLM_URL=https://api.openai.com WATCHER_LLM_MODEL=gpt-4o-mini \\
    WATCHER_LLM_KEY=sk-... python openai_compat_watcher.py haiku

  # Anthropic (via compat proxy)
  WATCHER_LLM_URL=https://api.anthropic.com/v1/... WATCHER_LLM_MODEL=claude-haiku-4-5 \\
    WATCHER_LLM_KEY=sk-ant-... python openai_compat_watcher.py haiku
"""
import asyncio
import logging
import os
import sys

import httpx

from forja_watcher import ForjaWatcher

LLM_BASE = os.getenv("WATCHER_LLM_URL", "http://127.0.0.1:1234")
LLM_MODEL = os.getenv("WATCHER_LLM_MODEL", "google/gemma-4-12b")
LLM_KEY = os.getenv("WATCHER_LLM_KEY", "lm-studio")

SYSTEM = """Eres un agente autónomo de la flota Forja/Tylluan.
Forja es un framework cognitivo multi-agente con memoria SilvaDB (grafos + embeddings
BGE-M3 1024-dim), retrieval híbrido (BM25 + vector), IdleLab y coloquio compartido.
Responde de forma concisa y útil. Máximo 150 palabras por turno.
Escala decisiones arquitectónicas a @claude."""

log = logging.getLogger(__name__)


class OpenAICompatWatcher(ForjaWatcher):
    """Watcher backed by any OpenAI-compatible /v1/chat/completions endpoint."""

    def __init__(self, agent_id: str = "assistant") -> None:
        super().__init__(agent_id)
        self._llm_headers = {"Authorization": f"Bearer {LLM_KEY}"}

    async def respond(self, content: str, event: dict) -> str | None:
        author = event.get("author_id", "?")
        payload = {
            "model": LLM_MODEL,
            "messages": [
                {"role": "system", "content": SYSTEM},
                {"role": "user", "content": f"[{author}]: {content}"},
            ],
            "max_tokens": 300,
            "temperature": 0.7,
            "stream": False,
        }
        try:
            async with httpx.AsyncClient(
                headers=self._llm_headers, timeout=60
            ) as client:
                r = await client.post(
                    f"{LLM_BASE}/v1/chat/completions",
                    json=payload,
                )
                r.raise_for_status()
                text = r.json()["choices"][0]["message"]["content"].strip()
                return f"[{self.agent_id}] {text}"
        except Exception as exc:
            log.error("[%s] LLM error (%s): %s", self.agent_id, LLM_BASE, exc)
            return f"[{self.agent_id}] ❌ inference error: {exc}"


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
    )
    agent_id = sys.argv[1] if len(sys.argv) > 1 else os.getenv("WATCHER_AGENT_ID", "assistant")
    log.info(
        "OpenAICompatWatcher | agent=%s | model=%s | endpoint=%s",
        agent_id, LLM_MODEL, LLM_BASE,
    )
    asyncio.run(OpenAICompatWatcher(agent_id=agent_id).run())
