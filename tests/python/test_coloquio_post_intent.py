"""H-20261121-COLOQUIO-POST (P2): «post to coloquio» tomaba «post» como canal.

Reproducido el 22-nov sobre `c6eaeb73` llamando a `post_to_channel` **en proceso**
(POST y guard de canal interceptados: no se publica nada):

```
publica en coloquio general: <320 chars>   -> OK      canal=general
post to coloquio general: <320 chars>      -> OK*     canal=post   <-- el bug
publica en coloquio general <320 chars>    -> FALLO   «Specify channel and content»
```

`*` el guard de canal existente rechazaba después, así que el mensaje no entraba y el
usuario volvía a intentarlo con otra redacción: de ahí la correlación con «mensajes
largos» (los largos se escriben con la forma inglesa o sin «:», no por la longitud).
"""
import os
import re
import sys

import pytest

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if RAIZ not in sys.path:
    sys.path.insert(0, RAIZ)

import guilds.core.coloquio as coloquio  # noqa: E402


@pytest.fixture()
def capturado(monkeypatch):
    """Intercepta el POST y el guard de canal: nada sale a la red ni a producción."""
    enviados = []

    def falso_post(ruta, datos):
        enviados.append((ruta, datos))
        return {"turn": 4242, "msg_id": "0123456789abcdef"}

    monkeypatch.setattr(coloquio, "_post", falso_post)
    monkeypatch.setattr(coloquio, "_guard_canal_existe", lambda canal: None)
    return enviados


def canal_de(enviados):
    return enviados[0][0].split("/channels/")[1].split("/")[0]


MSG = "palabra " * 40  # ~320 caracteres, el tamaño que fallaba


def test_post_to_coloquio_no_toma_post_como_canal(capturado):
    """MUTACION: sin «post\\s+to\\s+coloquio» en _CHANNEL_PREFIXES, el canal sale «post»."""
    out = coloquio.post_to_channel(intent=f"post to coloquio general: {MSG}")
    assert out.startswith("✅"), out
    assert canal_de(capturado) == "general", "el canal no puede ser «post»: %r" % out
    assert capturado[0][1]["content"] == MSG.strip()


def test_post_sin_dos_puntos_usa_el_resto_como_mensaje(capturado):
    """MUTACION: sin el fallback de _POST_INTENT vuelve «Specify channel and content»."""
    out = coloquio.post_to_channel(intent=f"publica en coloquio general {MSG}")
    assert out.startswith("✅"), out
    assert canal_de(capturado) == "general"
    assert MSG.strip() in capturado[0][1]["content"]


@pytest.mark.parametrize("intent", [
    f"publica en el canal general: {MSG}",
    f"manda un mensaje al coloquio general: {MSG}",
    f"post to coloquio general: {MSG}",
    f"write a message to the coloquio general: {MSG}",
])
def test_las_formas_reales_de_la_flota_publican_en_el_canal_correcto(capturado, intent):
    out = coloquio.post_to_channel(intent=intent)
    assert out.startswith("✅"), out
    assert canal_de(capturado) == "general"
    assert capturado[0][1]["content"] == MSG.strip()


def test_una_lectura_sigue_siendo_una_lectura(capturado):
    """Control: el cambio no convierte una lectura en un post.

    «lee el canal general» no lleva verbo de escritura, así que no hay contenido
    y se rechaza con el mensaje de siempre (no se inventa un mensaje).
    """
    out = coloquio.post_to_channel(intent="lee el canal general")
    assert not out.startswith("✅"), out
    assert not capturado, "una lectura no puede publicar nada"
