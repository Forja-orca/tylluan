"""D233b v3 (T4830) — un solo guard para los cuatro escritores del Coloquio.

Hermes (H-HER-E-01) encontró dos escritores que posteaban sin comprobar el canal:
`guilds/core/scheduler.py:86` y `guilds/watchers/forja_watcher.py:194`. El kernel crea el canal
al postear (`memory/coloquio.rs:671-697`), así que un nombre mal escrito deja basura.

**Alcance de la v3**: sin el 404 de D233 (aparcada: cambia un contrato y tiene 3 consumidores que
no capturan no-2xx). Se usa **la lista de canales**, un GET, y el `None` fail-closed que ya está en
main: `None` = no se pudo comprobar, `[]` = no hay canales, y en ambos casos **no se postea**.

Este módulo es la única fuente de la decisión; cada guild pone su transporte.
"""
from __future__ import annotations

import json
import urllib.error
import urllib.request

RUTA_CANALES = "/api/v1/coloquio/channels"

EXISTE = "existe"
NO_EXISTE = "no_existe"
DESCONOCIDO = "desconocido"


def estado_desde_lista(channel_id: str, lista) -> str:
    """Decisión pura: `None` (no se pudo leer) o lista vacía ⇒ no se puede postear."""
    if lista is None:
        return DESCONOCIDO
    if not lista:
        return NO_EXISTE
    objetivo = channel_id.strip().lower()
    for c in lista:
        if str(c).strip().lower() == objetivo:
            return EXISTE
    return NO_EXISTE


def motivo_de_rechazo(channel_id: str, estado: str, detalle: str = "") -> str | None:
    """Mensaje de rechazo, o None si se puede postear. Puro: testeable sin red."""
    if estado == EXISTE:
        return None
    if estado == NO_EXISTE:
        return (
            f"❌ El canal '{channel_id}' no existe (o no hay ninguno); no lo creo por accidente. "
            f"Si de verdad quieres uno nuevo, create_channel('{channel_id}')."
        )
    return (
        f"❌ No pude comprobar el canal '{channel_id}'"
        + (f" ({detalle})" if detalle else "")
        + "; no posteo para no crear uno por accidente. Reintenta en unos segundos."
    )


def _ids(data) -> list:
    canales = data if isinstance(data, list) else (data or {}).get("channels", [])
    return [c.get("channel_id") for c in canales if isinstance(c, dict) and c.get("channel_id")]


def consulta_con(leer, channel_id: str):
    """(estado, detalle) usando el `leer(ruta)` del propio módulo (mockeable en tests)."""
    try:
        return estado_desde_lista(channel_id, _ids(leer(RUTA_CANALES))), ""
    except Exception as e:  # kernel caído, 401, timeout: fail-closed
        return DESCONOCIDO, str(e)[:120]


def consulta_sync(base: str, channel_id: str, headers: dict, timeout: float = 5.0):
    """(estado, detalle) con urllib. Nunca lanza."""
    url = base.rstrip("/") + RUTA_CANALES
    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req, timeout=timeout) as r:
            data = json.loads(r.read().decode("utf-8", "replace"))
        return estado_desde_lista(channel_id, _ids(data)), ""
    except urllib.error.HTTPError as e:
        return DESCONOCIDO, f"HTTP {e.code}"
    except Exception as e:
        return DESCONOCIDO, str(e)[:120]


async def consulta_async(base: str, channel_id: str, headers: dict, timeout: float = 5.0):
    """Igual que `consulta_sync`, con httpx. Nunca lanza."""
    url = base.rstrip("/") + RUTA_CANALES
    try:
        import httpx

        async with httpx.AsyncClient(timeout=timeout) as client:
            r = await client.get(url, headers=headers)
            if r.status_code != 200:
                return DESCONOCIDO, f"HTTP {r.status_code}"
            return estado_desde_lista(channel_id, _ids(r.json())), ""
    except Exception as e:
        return DESCONOCIDO, str(e)[:120]
