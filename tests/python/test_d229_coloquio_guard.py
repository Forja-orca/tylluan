"""D229 v3 (T4799) — el guard del Coloquio no puede dejar que el kernel cree canales.

Hallazgo de Hermes (T4798): `_list_channel_ids()` se traga los fallos y devuelve `[]`, así
que un guard basado en "lista vacía" no protegía nada: con el kernel caído o un 401 se
seguía posteando y `memory/coloquio.rs:671-697` creaba el canal. Estos tests fijan los
cuatro caminos, y el de `post_to_coloquio` (segunda tool que hace el mismo POST).

No necesitan kernel: se monkeypatchean `_get` y `_post`.
"""
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core import coloquio  # noqa: E402


class Registro:
    """Sustituye a `_get`/`_post` y apunta si se llegó a postear."""

    def __init__(self, get_resultado=None, get_excepcion=None, post_resultado=None):
        self.get_resultado = get_resultado
        self.get_excepcion = get_excepcion
        self.post_resultado = post_resultado or {"turn": 7, "msg_id": "abc"}
        self.posts = []

    def get(self, path):
        if self.get_excepcion is not None:
            raise self.get_excepcion
        return self.get_resultado

    def post(self, path, body):
        self.posts.append((path, body))
        return self.post_resultado


def _con(monkeypatch, registro):
    monkeypatch.setattr(coloquio, "_get", registro.get)
    monkeypatch.setattr(coloquio, "_post", registro.post)


def test_kernel_caido_rechaza_y_no_posta(monkeypatch):
    reg = Registro(get_excepcion=ConnectionRefusedError("kernel caido"))
    _con(monkeypatch, reg)
    salida = coloquio.post_to_channel(channel_id="general", content="hola", author_id="a")
    assert "No pude comprobar" in salida, salida
    assert reg.posts == [], "no debe postear si no pudo comprobar los canales"


def test_401_rechaza_y_no_posta(monkeypatch):
    reg = Registro(get_excepcion=PermissionError("401"))
    _con(monkeypatch, reg)
    salida = coloquio.post_to_channel(channel_id="general", content="hola", author_id="a")
    assert "No pude comprobar" in salida, salida
    assert reg.posts == []


def test_sin_canales_rechaza_y_no_posta(monkeypatch):
    reg = Registro(get_resultado={"channels": []})
    _con(monkeypatch, reg)
    salida = coloquio.post_to_channel(channel_id="general", content="hola", author_id="a")
    # D233b v3: el mensaje es uno solo para "no hay canales" y "no existe".
    assert "no existe" in salida or "No hay ning" in salida, salida
    assert reg.posts == []


def test_canal_inexistente_rechaza_y_no_posta(monkeypatch):
    reg = Registro(get_resultado={"channels": [{"channel_id": "general"}]})
    _con(monkeypatch, reg)
    salida = coloquio.post_to_channel(channel_id="zz-basura", content="hola", author_id="a")
    assert "no existe" in salida and "zz-basura" in salida, salida
    assert reg.posts == [], "no debe crear el canal posteando"


def test_canal_existente_posta(monkeypatch):
    reg = Registro(get_resultado={"channels": [{"channel_id": "general"}]})
    _con(monkeypatch, reg)
    salida = coloquio.post_to_channel(channel_id="general", content="hola", author_id="a")
    assert "Posted to 'general'" in salida, salida
    assert len(reg.posts) == 1


def test_nombre_se_compara_sin_mayusculas(monkeypatch):
    reg = Registro(get_resultado={"channels": [{"channel_id": "General"}]})
    _con(monkeypatch, reg)
    salida = coloquio.post_to_channel(channel_id="general", content="hola", author_id="a")
    assert "Posted to 'general'" in salida, salida
    assert len(reg.posts) == 1


@pytest.mark.parametrize("excepcion", [ConnectionRefusedError("x"), PermissionError("401")])
def test_post_to_coloquio_tiene_el_mismo_guard(monkeypatch, excepcion):
    """La otra tool MCP registrada hacía el mismo POST sin guard (T4799)."""
    reg = Registro(get_excepcion=excepcion)
    _con(monkeypatch, reg)
    salida = coloquio.post_to_coloquio(channel_id="zz-basura", message="hola", agent_id="a")
    assert "No pude comprobar" in salida, salida
    assert reg.posts == [], "post_to_coloquio tampoco puede crear el canal"
