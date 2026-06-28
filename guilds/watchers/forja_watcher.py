"""
ForjaWatcher — base class for autonomous agent watchers.

Sleep mode (default): polls /api/v1/agents/session every 30s.
  Cost: 0 LLM tokens, 0 inference.
Active mode: opens SSE stream to /api/v1/events, filters coloquio:new_turn,
  calls respond() only when @mention detected and gate is open.
"""
import asyncio
import json
import logging
import os
from abc import ABC, abstractmethod

import httpx

FORJA_BASE = os.getenv("FORJA_URL", "http://127.0.0.1:3033")
log = logging.getLogger(__name__)


class ForjaWatcher(ABC):
    def __init__(self, agent_id: str, channel_id: str = "mision-activa"):
        self.agent_id = agent_id
        self.channel_id = channel_id
        self._last_turn: int = 0

    async def run(self) -> None:
        log.info("[%s] Watcher iniciado (dormido hasta sesión activa)", self.agent_id)
        await asyncio.gather(self._poll_and_listen(), self._heartbeat_loop())

    # ── heartbeat loop ──────────────────────────────────────────────────────

    async def _heartbeat_loop(self) -> None:
        async with httpx.AsyncClient() as client:
            while True:
                try:
                    r = await client.post(
                        f"{FORJA_BASE}/api/v1/agents/heartbeat",
                        json={"agent_id": self.agent_id},
                        timeout=5,
                    )
                    data = r.json()
                    if r.status_code not in (200, 201):
                        log.debug("[%s] Heartbeat: %s", self.agent_id, data)
                except Exception as exc:
                    log.debug("[%s] Heartbeat error: %s", self.agent_id, exc)
                await asyncio.sleep(30)

    # ── main poll / listen loop ─────────────────────────────────────────────

    async def _poll_and_listen(self) -> None:
        while True:
            if await self._is_session_active():
                log.info("[%s] Sesión activa — abriendo SSE", self.agent_id)
                try:
                    await self._sse_listen()
                except Exception as exc:
                    log.warning("[%s] SSE interrumpido: %s", self.agent_id, exc)
            await asyncio.sleep(30)

    async def _sse_listen(self) -> None:
        async with httpx.AsyncClient(timeout=None) as client:
            async with client.stream(
                "GET",
                f"{FORJA_BASE}/api/v1/events",
            ) as resp:
                async for raw_line in resp.aiter_lines():
                    if not await self._is_session_active():
                        log.info("[%s] Sesión desactivada — volviendo a dormir", self.agent_id)
                        return
                    if not raw_line.startswith("data:"):
                        continue
                    payload = raw_line[5:].strip()
                    if not payload:
                        continue
                    try:
                        event = json.loads(payload)
                    except json.JSONDecodeError:
                        continue
                    await self._handle_event(event)

    # ── event handling ──────────────────────────────────────────────────────

    async def _handle_event(self, event: dict) -> None:
        # Only process coloquio messages from the watched channel
        if event.get("type") != "coloquio:new_turn":
            return
        if event.get("channel_id") != self.channel_id:
            return
        # Ignore own messages
        if event.get("author_id") == self.agent_id:
            return
        # Must contain an @mention of this agent
        content: str = event.get("content", "")
        if f"@{self.agent_id}" not in content:
            return
        turn: int = event.get("turn", 0)
        if turn <= self._last_turn:
            return
        # Check budget gate before calling LLM
        gate = await self._check_gate()
        if gate != "open":
            log.info("[%s] Gate cerrado (%s) — ignorando turno %d", self.agent_id, gate, turn)
            return
        self._last_turn = turn
        response = await self.respond(content, event)
        if response:
            await self._post(response)

    # ── helper calls ────────────────────────────────────────────────────────

    async def _is_session_active(self) -> bool:
        try:
            async with httpx.AsyncClient(timeout=3) as client:
                r = await client.get(f"{FORJA_BASE}/api/v1/agents/session")
                sessions = r.json().get("sessions", [])
                return any(
                    s.get("agent_id") == self.agent_id and s.get("status") == "active"
                    for s in sessions
                )
        except Exception:
            return False

    async def _check_gate(self) -> str:
        """Returns 'open' if the agent may send a response, otherwise a reason string."""
        try:
            async with httpx.AsyncClient(timeout=3) as client:
                r = await client.post(
                    f"{FORJA_BASE}/api/v1/agents/heartbeat",
                    json={"agent_id": self.agent_id},
                )
                if r.status_code == 200:
                    return "open"
                return r.json().get("error", "closed")
        except Exception:
            return "closed"

    async def _post(self, message: str) -> None:
        try:
            async with httpx.AsyncClient(timeout=10) as client:
                await client.post(
                    f"{FORJA_BASE}/api/v1/coloquio/channels/{self.channel_id}/post",
                    json={"content": message, "author_id": self.agent_id, "role": "agent"},
                )
        except Exception as exc:
            log.error("[%s] Error publicando respuesta: %s", self.agent_id, exc)

    # ── subclass contract ───────────────────────────────────────────────────

    @abstractmethod
    async def respond(self, content: str, event: dict) -> str | None:
        """Generate a response to content. Return None to skip."""
        ...
