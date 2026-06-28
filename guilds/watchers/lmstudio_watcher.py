"""
LMStudioWatcher — autonomous watcher using a local LM Studio model.

Uses LM Studio's OpenAI-compatible API (http://127.0.0.1:1234/v1).
No cloud API keys required — fully local inference.

Sleeps until a session is activated via POST /api/v1/agents/session/start.
Responds only when @lmstudio (or configured agent_id) is mentioned.
"""
import asyncio
import json
import logging
import os

import httpx

from forja_watcher import ForjaWatcher

LM_BASE = os.getenv("LM_STUDIO_URL", "http://127.0.0.1:1234")
LM_MODEL = os.getenv("LM_STUDIO_MODEL", "google/gemma-4-12b")

SYSTEM = """Eres un agente de la flota Forja/Tylluan respondiendo de forma autónoma.
Forja es un framework cognitivo multi-agente con memoria SilvaDB, retrieval híbrido,
y coloquio compartido entre agentes. Sé conciso y directo. Máximo 150 palabras."""

log = logging.getLogger(__name__)


class LMStudioWatcher(ForjaWatcher):
    def __init__(self, agent_id: str = "lmstudio") -> None:
        super().__init__(agent_id)
        self.lm_base = LM_BASE

    async def respond(self, content: str, event: dict) -> str | None:
        author = event.get("author_id", "?")
        payload = {
            "model": LM_MODEL,
            "messages": [
                {"role": "system", "content": SYSTEM},
                {"role": "user", "content": f"[{author}]: {content}"},
            ],
            "max_tokens": 300,
            "temperature": 0.7,
            "stream": False,
        }
        try:
            async with httpx.AsyncClient(timeout=30) as client:
                r = await client.post(
                    f"{self.lm_base}/v1/chat/completions",
                    json=payload,
                )
                r.raise_for_status()
                data = r.json()
                text = data["choices"][0]["message"]["content"].strip()
                return f"[{self.agent_id}] {text}"
        except Exception as exc:
            log.error("[%s] Error llamando LM Studio: %s", self.agent_id, exc)
            return f"[{self.agent_id}] ❌ LM Studio error: {exc}"


if __name__ == "__main__":
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
    )

    # Allow overriding agent_id from CLI: python lmstudio_watcher.py haiku
    agent_id = sys.argv[1] if len(sys.argv) > 1 else "lmstudio"
    log.info("Starting LMStudioWatcher as agent_id='%s' → %s", agent_id, LM_BASE)
    asyncio.run(LMStudioWatcher(agent_id=agent_id).run())
