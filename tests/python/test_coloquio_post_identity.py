"""Coloquio guild: who signs a post and which channel it goes to (TL, 2026-11-19).

Real incident: messages published with `tylluan_do("publica en coloquio general: ...")`
were signed with a word taken from the BODY of the message (@D213, @sin, @bajo,
@cifras...) because `_parse_author` matched `as <word>` at the end of any word
("tareas D213"), and a "canal X" inside the body could change the channel. The
channel and the author are read only from the header (before the first ':').
"""
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from test_coloquio_nlp import _load_guild_module  # noqa: E402


@pytest.fixture()
def guild():
    mod = _load_guild_module()
    posted = []

    def fake_post(path, body):
        posted.append((path, body))
        return {"turn": 7, "msg_id": "abcdef0123456789"}

    mod._post = fake_post
    mod._posted = posted
    return mod


def _last(guild):
    return guild._posted[-1]


@pytest.mark.parametrize(
    "body",
    [
        "Tareas D213 para todos",
        "tienes cuatro mias sin hacer",
        "Rutas bajo operator: state y unlock",
        "revisad las cifras de la tabla",
    ],
)
def test_body_words_never_become_the_author(guild, body):
    out = guild.post_to_channel(intent=f"publica en coloquio general: {body}")
    path, sent = _last(guild)
    assert sent["author_id"] == "agent", out
    assert sent["content"] == body
    assert path.endswith("/channels/general/post")


def test_agent_id_parameter_signs_the_post(guild):
    guild.post_to_channel(
        intent="publica en coloquio general: Tareas D213 para todos", agent_id="claude-code"
    )
    assert _last(guild)[1]["author_id"] == "claude-code"


def test_explicit_author_in_the_header_still_works(guild):
    guild.post_to_channel(intent="publica en coloquio general as deep: hola")
    assert _last(guild)[1]["author_id"] == "deep"


def test_channel_word_in_the_body_does_not_change_the_channel(guild):
    guild.post_to_channel(intent="publica en coloquio general: mira el canal foo y el channel bar")
    path, sent = _last(guild)
    assert path.endswith("/channels/general/post")
    assert sent["content"] == "mira el canal foo y el channel bar"


def test_header_may_not_be_confused_with_a_clock_in_the_body(guild):
    guild.post_to_channel(intent="publica en coloquio general: [TL 21:30] aviso")
    assert _last(guild)[1]["content"] == "[TL 21:30] aviso"
