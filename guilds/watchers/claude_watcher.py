"""
ClaudeHaikuWatcher — autonomous watcher for the Haiku agent.

Sleeps until a session is activated via POST /api/v1/agents/session/start.
Responds only when @haiku is mentioned and the budget gate is open.
"""
import asyncio
import logging
import os

import anthropic

from forja_watcher import ForjaWatcher

SYSTEM = """Eres Haiku, agente rápido de la flota Forja/Tylluan.
Forja es un framework cognitivo multi-agente con memoria SilvaDB (grafos + embeddings BGE-M3 1024-dim),
retrieval híbrido (BM25 + vector + Jina reranker), IdleLab (optimización autónoma de parámetros),
y coloquio compartido entre agentes. Tu rol: subtareas acotadas, validaciones, tests, fixes menores,
implementación quirúrgica. Eres económico y preciso.
Escala a @claude si la tarea excede tu scope (decisiones arquitectónicas, contexto global).
Máximo 150 palabras por turno. Sé directo y conciso."""

log = logging.getLogger(__name__)


class ClaudeHaikuWatcher(ForjaWatcher):
    def __init__(self) -> None:
        super().__init__("haiku")
        self.client = anthropic.Anthropic(api_key=os.environ["ANTHROPIC_API_KEY"])

    async def respond(self, content: str, event: dict) -> str | None:
        author = event.get("author_id", "?")
        try:
            msg = self.client.messages.create(
                model="claude-haiku-4-5-20251001",
                max_tokens=300,
                system=SYSTEM,
                messages=[{"role": "user", "content": f"[{author}]: {content}"}],
            )
            text = msg.content[0].text if msg.content else ""
            return f"[haiku] {text}"
        except Exception as exc:
            log.error("[haiku] Error llamando API: %s", exc)
            return f"[haiku] ❌ {exc}"


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    asyncio.run(ClaudeHaikuWatcher().run())
