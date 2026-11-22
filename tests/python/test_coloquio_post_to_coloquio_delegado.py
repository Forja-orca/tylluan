"""D4976 — `post_to_coloquio` y `post_to_channel` dejan de comportarse distinto.

Origen: el router elige una u otra por parecido y `post_to_coloquio` exigía tres
campos (`channel_id`, `message`, `agent_id`) que un intent en lenguaje natural no
puede rellenar. Eso, junto con que la compuerta del kernel validara contra el
esquema de la tool equivocada, producía el `MISSING_ARGS: agent_id` de D4975.
Ahora `post_to_coloquio` delega en `post_to_channel`.
"""
import os
import sys

import pytest

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if RAIZ not in sys.path:
    sys.path.insert(0, RAIZ)

import guilds.core.coloquio as coloquio  # noqa: E402


@pytest.fixture()
def capturado(monkeypatch):
    enviados = []

    def falso_post(ruta, datos):
        enviados.append((ruta, datos))
        return {"turn": 4242, "msg_id": "0123456789abcdef"}

    monkeypatch.setattr(coloquio, "_post", falso_post)
    monkeypatch.setattr(coloquio, "_guard_canal_existe", lambda canal: None)
    return enviados


def canal_de(enviados):
    return enviados[0][0].split("/channels/")[1].split("/")[0]


def test_post_to_coloquio_con_los_tres_parametros_sigue_funcionando(capturado):
    """Compatibilidad: la firma histórica (channel_id, message, agent_id) intacta."""
    out = coloquio.post_to_coloquio(channel_id="general", message="hola", agent_id="deep")
    assert out.startswith("✅"), out
    assert canal_de(capturado) == "general"
    assert capturado[0][1]["content"] == "hola"
    assert capturado[0][1]["author_id"] == "deep", "el agent_id tiene que firmar el mensaje"


def test_post_to_coloquio_acepta_intent_natural(capturado):
    """MUTACION: sin la delegación, esta llamada devuelve el viejo «All parameters are required».

    Se usa la forma «publica en coloquio general: …» a propósito: es la que el parser de
    `main` ya entiende sin D4973, así este parche se sostiene solo. La forma inglesa
    («post to coloquio …») la arregla D4973 y se prueba allí.
    """
    out = coloquio.post_to_coloquio(intent="publica en coloquio general: " + ("palabra " * 40).strip())
    assert out.startswith("✅"), out
    assert canal_de(capturado) == "general"
    assert len(capturado[0][1]["content"]) > 250


def test_las_dos_tools_firman_igual_y_publican_igual(capturado):
    """El mismo texto por las dos puertas deja el mismo cuerpo y el mismo autor."""
    texto = "mensaje de control D4976"
    a = coloquio.post_to_coloquio(channel_id="general", message=texto, agent_id="hermes")
    cuerpo_a = capturado[-1][1]["content"]
    autor_a = capturado[-1][1]["author_id"]
    b = coloquio.post_to_channel(channel_id="general", content=texto, author_id="hermes")
    cuerpo_b = capturado[-1][1]["content"]
    autor_b = capturado[-1][1]["author_id"]
    assert (a.startswith("✅"), b.startswith("✅")) == (True, True)
    assert (cuerpo_a, autor_a) == (cuerpo_b, autor_b) == (texto, "hermes")


def test_sin_nada_sigue_diciendo_que_falta(capturado):
    out = coloquio.post_to_coloquio()
    assert not out.startswith("✅"), out
    assert "Specify channel and content" in out, out
    assert not capturado
