import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[2]))

from guilds.core.coloquio import _resolve_author   # noqa: E402


def test_la_credencial_gana_al_texto():
    """D5003 / H-20261118-COLOQUIO-POST-AUTOR: el kernel manda `agent_id=claude-code`
    y el mensaje dice «as D206»; antes firmaba @D206 (turnos 4606/4613/4624/4626/4627)."""
    assert _resolve_author("", "D206", "claude-code") == "claude-code"
    assert _resolve_author("", "cifras", "deep") == "deep"


def test_sin_credencial_se_usa_lo_que_diga_la_cabecera():
    """Uso manual (CLI sin credencial): se mantiene el comportamiento de siempre."""
    assert _resolve_author("", "D206", "") == "D206"


def test_el_autor_explicito_gana_a_todo():
    """Un `author_id` explícito no lo pisa ni la credencial. (El guard de autores
    protegidos —jose/admin/system— sigue aplicándose más abajo, en el post.)"""
    assert _resolve_author("jose", "D206", "claude-code") == "jose"


def test_sin_nada_queda_agent():
    assert _resolve_author("", "", "") == "agent"
