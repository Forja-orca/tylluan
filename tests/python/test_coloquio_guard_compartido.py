"""D233b v3 (T4830) — un guard, cuatro escritores del Coloquio, con la LISTA de canales.

Sin el 404 de D233 (aparcada). La decisión: un GET a `/api/v1/coloquio/channels`; `None` si no
se pudo leer (kernel caído, 401, timeout) y lista vacía ⇒ **no se postea** (fail-closed).

**Mutación por escritor**: si se quita la comprobación en cualquiera de los cuatro, un test de
aquí se pone rojo.
"""
import asyncio
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core import colo_guard  # noqa: E402


# ── la decisión, pura ───────────────────────────────────────────────────────
def test_estado_desde_lista():
    assert colo_guard.estado_desde_lista("general", ["general", "dev"]) == colo_guard.EXISTE
    assert colo_guard.estado_desde_lista("General", ["general"]) == colo_guard.EXISTE
    assert colo_guard.estado_desde_lista("zz", ["general"]) == colo_guard.NO_EXISTE
    assert colo_guard.estado_desde_lista("general", []) == colo_guard.NO_EXISTE
    assert colo_guard.estado_desde_lista("general", None) == colo_guard.DESCONOCIDO


def test_motivo_de_rechazo():
    assert colo_guard.motivo_de_rechazo("general", colo_guard.EXISTE) is None
    assert "no existe" in colo_guard.motivo_de_rechazo("zz", colo_guard.NO_EXISTE)
    assert "No pude comprobar" in colo_guard.motivo_de_rechazo("g", colo_guard.DESCONOCIDO, "HTTP 401")


# ── escritor 1: post_to_channel ────────────────────────────────────────────
def test_escritor_post_to_channel_usa_el_guard(monkeypatch):
    from guilds.core import coloquio

    posts = []
    monkeypatch.setattr(colo_guard, "consulta_con", lambda leer, cid: (colo_guard.NO_EXISTE, ""))
    monkeypatch.setattr(coloquio, "_post", lambda p, b: posts.append(p) or {"turn": 1})
    salida = coloquio.post_to_channel(channel_id="zz-basura", content="x", author_id="a")
    assert "no existe" in salida, salida
    assert posts == []


def test_escritor_post_to_channel_posta_si_existe(monkeypatch):
    from guilds.core import coloquio

    posts = []
    monkeypatch.setattr(colo_guard, "consulta_con", lambda leer, cid: (colo_guard.EXISTE, ""))
    monkeypatch.setattr(coloquio, "_post", lambda p, b: posts.append(p) or {"turn": 3})
    salida = coloquio.post_to_channel(channel_id="general", content="x", author_id="a")
    assert "Posted to 'general'" in salida, salida
    assert len(posts) == 1


# ── escritor 2: post_to_coloquio ───────────────────────────────────────────
def test_escritor_post_to_coloquio_usa_el_guard(monkeypatch):
    from guilds.core import coloquio

    posts = []
    monkeypatch.setattr(colo_guard, "consulta_con", lambda leer, cid: (colo_guard.DESCONOCIDO, "HTTP 401"))
    monkeypatch.setattr(coloquio, "_post", lambda p, b: posts.append(p) or {"turn": 1})
    salida = coloquio.post_to_coloquio(channel_id="general", message="x", agent_id="a")
    assert "No pude comprobar" in salida, salida
    assert posts == []


# ── escritor 3: scheduler ──────────────────────────────────────────────────
def test_escritor_scheduler_usa_el_guard(monkeypatch):
    from guilds.core import scheduler

    enviados = []
    monkeypatch.setattr(colo_guard, "consulta_sync", lambda *a, **k: (colo_guard.NO_EXISTE, ""))
    monkeypatch.setattr(scheduler.urllib.request, "urlopen", lambda *a, **k: enviados.append(a))
    assert scheduler._post_to_coloquio("zz-basura", "hola") is False
    assert enviados == []


def test_escritor_scheduler_posta_si_el_canal_existe(monkeypatch):
    from guilds.core import scheduler

    class Resp:
        status = 201

        def __enter__(self):
            return self

        def __exit__(self, *a):
            return False

    enviados = []
    monkeypatch.setattr(colo_guard, "consulta_sync", lambda *a, **k: (colo_guard.EXISTE, ""))
    monkeypatch.setattr(scheduler.urllib.request, "urlopen",
                        lambda req, timeout=10: enviados.append(req.full_url) or Resp())
    assert scheduler._post_to_coloquio("general", "hola") is True
    assert enviados and enviados[0].endswith("/general/post")


# ── escritor 4: forja_watcher ──────────────────────────────────────────────
def test_escritor_forja_usa_el_guard(monkeypatch):
    from guilds.watchers import forja_watcher

    enviados = []

    class ClienteFalso:
        def __init__(self, *a, **k):
            pass

        async def __aenter__(self):
            return self

        async def __aexit__(self, *a):
            return False

        async def post(self, url, json=None):
            enviados.append(url)

            class R:
                status_code = 200

            return R()

    async def consulta(*a, **k):
        return colo_guard.NO_EXISTE, ""

    monkeypatch.setattr(colo_guard, "consulta_async", consulta)
    monkeypatch.setattr(forja_watcher.httpx, "AsyncClient", ClienteFalso)

    class WatcherDePrueba(forja_watcher.ForjaWatcher):
        async def respond(self, content, event):
            return None

    inst = WatcherDePrueba.__new__(WatcherDePrueba)
    inst.agent_id = "a"
    inst.channel_id = "zz-basura"
    asyncio.run(WatcherDePrueba._post(inst, "hola"))
    assert enviados == [], "forja_watcher no puede postear sin canal"
