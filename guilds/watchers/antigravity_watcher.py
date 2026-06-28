"""
AntigravityWatcher — autonomous watcher for the Antigravity agent (Gemini).

Sleeps until a session is activated via POST /api/v1/agents/session/start.
Responds only when @antigravity is mentioned and the budget gate is open.
"""
import asyncio
import logging
import os

import google.generativeai as genai

from forja_watcher import ForjaWatcher

SYSTEM = """Eres Antigravity, agente de la flota Forja/Tylluan especializado en investigación web y auditoría.
Forja es un framework cognitivo multi-agente con memoria SilvaDB (grafos + embeddings BGE-M3 1024-dim),
retrieval híbrido (BM25 + vector + Jina reranker), IdleLab (optimización autónoma de parámetros),
y coloquio compartido entre agentes. Tu rol: investigación web, auditoría UI/UX, análisis de repos,
benchmarks, comparativas técnicas. Sé específico con datos reales.
Escala a @claude si es una decisión arquitectónica que excede tu scope.
Máximo 250 palabras por turno."""

log = logging.getLogger(__name__)


class AntigravityWatcher(ForjaWatcher):
    def __init__(self) -> None:
        super().__init__("antigravity")
        genai.configure(api_key=os.environ["GEMINI_API_KEY"])
        self.model = genai.GenerativeModel(
            "gemini-2.5-flash",
            system_instruction=SYSTEM,
        )

    async def respond(self, content: str, event: dict) -> str | None:
        author = event.get("author_id", "?")
        try:
            # generate_content is blocking — run in executor to avoid blocking event loop
            loop = asyncio.get_event_loop()
            r = await loop.run_in_executor(
                None,
                lambda: self.model.generate_content(f"[{author}]: {content}"),
            )
            text = r.text if r.text else ""
            return f"[antigravity] {text}"
        except Exception as exc:
            log.error("[antigravity] Error llamando API: %s", exc)
            return f"[antigravity] ❌ {exc}"


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    asyncio.run(AntigravityWatcher().run())
