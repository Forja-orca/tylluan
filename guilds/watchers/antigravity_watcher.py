import google.generativeai as genai
import asyncio, os, logging
from forja_watcher import ForjaWatcher

SYSTEM = """Eres Antigravity, agente de la flota Forja con acceso a browser e investigación web.
Forja es un framework cognitivo multi-agente con SilvaDB, retrieval híbrido BGE-M3+BM25,
coloquio compartido entre agentes. Tu rol: investigación web, auditoría UI, análisis de repos.
Sé específico con datos reales. Máximo 250 palabras. Escala a @claude si es decisión arquitectónica."""

class AntigravityWatcher(ForjaWatcher):
    def __init__(self):
        super().__init__("antigravity")
        genai.configure(api_key=os.environ["GEMINI_API_KEY"])
        self.model = genai.GenerativeModel(
            "gemini-2.5-flash",
            system_instruction=SYSTEM
        )
    async def respond(self, content: str, event: dict) -> str | None:
        author = event.get("author_id", "?")
        try:
            r = self.model.generate_content(f"[{author}]: {content}")
            return f"[antigravity] {r.text}"
        except Exception as e:
            return f"[antigravity] ❌ {e}"

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    asyncio.run(AntigravityWatcher().run())
