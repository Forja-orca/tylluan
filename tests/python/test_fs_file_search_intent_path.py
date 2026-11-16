"""FS-FIND-FILES-IGNORA-RUTA-DEL-INTENT, parte `file_search` (propuesta (c),
en marcha tras «sigue con tu cola» del TL en T4210; el hueco lo anota
Antigravity en T4182: `file_search` seguia usando `directory="."` si no se pasa).

En la base, `file_search(directory: str, ...)` (filesystem.py:328) tiene
`directory` OBLIGATORIO y en `:373` cae en `os.path.abspath(".")`: un intent que
solo nombra la ruta ni ejecuta la tool, y si el llamante manda "." se recurre el
CWD del kernel (la raiz de Tylluan en produccion) en silencio.
"""

import asyncio
from pathlib import Path

import pytest

CANARY = "CANARIO-FUERA-DEL-ROOT"
SECRET_NAME = "secreto.key"
WORD = "marcador"


def _load_fs(monkeypatch, root: Path):
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    monkeypatch.setattr(
        "guilds.builders.plugins.filesystem._FS_ROOT", str(root), raising=False
    )
    from guilds.builders.plugins import filesystem

    return filesystem


@pytest.fixture
def tree(tmp_path, monkeypatch):
    root = tmp_path / "repo"
    docs = root / "docs"
    docs.mkdir(parents=True)
    (docs / "a.py").write_text(f"# {WORD} dentro de docs\n", encoding="utf-8")
    (root / "raiz.py").write_text(f"# {WORD} en la raiz\n", encoding="utf-8")
    outside = tmp_path / "fuera"
    outside.mkdir()
    (outside / SECRET_NAME).write_text(f"{WORD} {CANARY}\n", encoding="utf-8")
    return _load_fs(monkeypatch, root), root, docs, outside


def test_intent_absolute_path_searches_that_directory(tree):
    fs, _root, docs, _outside = tree
    out = asyncio.run(fs.file_search(intent=f'busca "{WORD}" en {docs}'))
    assert "a.py:1:" in out, out
    assert "raiz.py" not in out, out
    assert str(docs) in out, out


def test_no_error_when_only_the_intent_names_the_path(tree):
    """Sin el arreglo esto es un TypeError: `directory` era obligatorio y la
    tool ni se ejecutaba (el mismo fallo de schema que en `file_list`)."""
    fs, _root, docs, _outside = tree
    out = asyncio.run(fs.file_search(pattern=WORD, intent=f"en {docs}"))
    assert "a.py:1:" in out, out


def test_path_outside_root_is_blocked_not_cwd(tree):
    fs, _root, _docs, outside = tree
    out = asyncio.run(fs.file_search(intent=f'busca "{WORD}" en {outside}'))
    assert "BLOCKED" in out, out
    assert CANARY not in out and SECRET_NAME not in out, out
    assert "did NOT fall back" in out, out


def test_missing_named_directory_is_not_replaced_by_cwd(tree):
    fs, root, _docs, _outside = tree
    ghost = root / "no-existe"
    out = asyncio.run(fs.file_search(intent=f'busca "{WORD}" en {ghost}'))
    assert "Not a directory" in out, out
    assert str(ghost) in out, out
    assert "working directory" in out, out


def test_explicit_directory_parameter_still_wins(tree):
    fs, root, docs, _outside = tree
    out = asyncio.run(fs.file_search(directory=str(docs), pattern=WORD, intent=f"en {root}"))
    assert "a.py:1:" in out, out
    assert "raiz.py" not in out, out


def test_pattern_and_directory_are_echoed_when_nothing_matches(tree):
    """La rama «sin coincidencias» devuelve el patron y el directorio usado: ahi
    se ve si la ruta del intent acabo convertida en un glob o en otra palabra."""
    fs, _root, docs, _outside = tree
    out = asyncio.run(fs.file_search(intent=f'busca "zqk-no-existe" dentro de {docs}'))
    assert "No matches for 'zqk-no-existe'" in out, out
    assert str(docs) in out, out
    assert "*.docs" not in out and "*.cache" not in out, out


def test_windows_path_is_not_eaten_as_an_extension(tree):
    """Una ruta con un segmento con punto (`docs/v2.3`) no debe acabar como
    patron `*.3` o `*.v2`: el regex de glob de `file_search` (base :360) lo
    permitia antes de `_strip_path_tokens`."""
    fs, _root, docs, _outside = tree
    nested = docs / "v2.3"
    nested.mkdir()
    (nested / "b.py").write_text("# marcador anidado\n", encoding="utf-8")
    out = asyncio.run(fs.file_search(intent=f'busca "marcador" en {nested}'))
    assert str(nested) in out, out
    assert "*.3" not in out and "*.v2" not in out, out
    assert "b.py:1:" in out, out


def test_relative_explicit_directory_still_works(tree, monkeypatch):
    fs, root, _docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(fs.file_search(directory="docs", pattern=WORD))
    assert "a.py:1:" in out, out
    assert str(root / "docs") in out, out


def test_url_in_intent_is_not_treated_as_a_directory(tree, monkeypatch):
    fs, root, _docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(
        fs.file_search(pattern=WORD, intent="busca en https://example.com/docs")
    )
    assert "example.com" not in out, out
    assert "raiz.py" in out, out


def test_intent_without_a_path_keeps_the_cwd_default(tree, monkeypatch):
    """Control negativo: sin ruta en el texto sigue valiendo el default, pero la
    cabecera dice donde se busco."""
    fs, root, _docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(fs.file_search(pattern=WORD, intent="busca en todos los ficheros"))
    assert str(root) in out, out
    assert "raiz.py" in out and "a.py" in out, out
