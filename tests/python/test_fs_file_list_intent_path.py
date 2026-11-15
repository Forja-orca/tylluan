"""FS-FIND-FILES-IGNORA-RUTA-DEL-INTENT, parte `file_list` (encargo TL T4183).

Hermes-Linux reporto que `file_list directory=/ruta` reventaba con «Field
required». En la base, `file_list(directory: str, depth: int = 2)` NO tiene
`intent` y `directory` es obligatorio: si el llamante solo nombra la ruta en el
texto, la tool ni se ejecuta; y si manda ".", el arbol sale del CWD del kernel
(que en produccion es la raiz de Tylluan: `data/`, `bin/`, nombres de ficheros
sensibles). Son los dos fallos que fija este fichero, mas el control negativo de
`scripts/verify_portable_guild.py:102` (`directory` explicito relativo).
"""

import asyncio
from pathlib import Path

import pytest

CANARY = "CANARIO-FUERA-DEL-ROOT"
SECRET_NAME = "secreto.key"


def _load_fs(monkeypatch, root: Path):
    """Igual que `test_b24_guild_path_traversal.py`: `_FS_ROOT` es de modulo."""
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
    (docs / "guia.md").write_text("hola\n", encoding="utf-8")
    (root / "raiz.txt").write_text("x\n", encoding="utf-8")
    outside = tmp_path / "fuera"
    outside.mkdir()
    (outside / SECRET_NAME).write_text(CANARY + "\n", encoding="utf-8")
    return _load_fs(monkeypatch, root), root, docs, outside


def test_intent_absolute_path_lists_that_directory(tree):
    """Criterion pedido: «extraer la ruta del intent igual que find_files»."""
    fs, root, docs, _outside = tree
    out = asyncio.run(fs.file_list(intent=f"lista el arbol de {docs}"))
    assert "guia.md" in out, out
    assert "raiz.txt" not in out, out


def test_no_field_required_when_only_the_intent_names_the_path(tree):
    """El bug de Hermes-Linux: sin `directory` la tool ni se ejecutaba.

    Sin el arreglo esto es un TypeError («missing required positional
    argument: 'directory'»), o sea: rojo sin el fix, exactamente lo que pidio
    el TL.
    """
    fs, _root, docs, _outside = tree
    out = asyncio.run(fs.file_list(intent=f"file_list directory={docs}"))
    assert "guia.md" in out, out


def test_absolute_path_outside_root_is_blocked_not_cwd(tree):
    fs, _root, _docs, outside = tree
    out = asyncio.run(fs.file_list(intent=f"lista el arbol de {outside}"))
    assert "BLOCKED" in out, out
    assert CANARY not in out and SECRET_NAME not in out, out
    assert "did NOT fall back" in out, out


def test_missing_named_directory_is_not_replaced_by_cwd(tree):
    fs, root, _docs, _outside = tree
    ghost = root / "no-existe"
    out = asyncio.run(fs.file_list(intent=f"lista el arbol de {ghost}"))
    assert "Not a directory" in out, out
    assert str(ghost) in out, out
    assert "working directory" in out, out


def test_explicit_directory_parameter_still_wins(tree):
    fs, root, docs, _outside = tree
    out = asyncio.run(fs.file_list(directory=str(docs), intent=f"de {root}"))
    assert "guia.md" in out, out
    assert "raiz.txt" not in out, out


def test_relative_explicit_directory_still_works(tree, monkeypatch):
    """Control negativo del contrato portable
    (`scripts/verify_portable_guild.py:98-105`): `directory` explicito
    relativo tiene que seguir funcionando como siempre."""
    fs, root, _docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(fs.file_list(directory="docs", depth=1))
    assert "guia.md" in out, out
    assert str(root / "docs") in out, out


def test_intent_without_a_path_keeps_the_cwd_default(tree, monkeypatch):
    """Control negativo: sin ruta en el texto se mantiene el default, pero la
    cabecera dice que arbol se listo (el cwd nunca es una sorpresa)."""
    fs, root, _docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(fs.file_list(intent="lista el arbol"))
    assert str(root) in out, out
    assert "guia.md" in out, out


def test_url_in_intent_is_not_treated_as_a_directory(tree, monkeypatch):
    fs, root, _docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(fs.file_list(intent="lista el arbol de https://example.com/docs"))
    assert "example.com" not in out, out
    assert "guia.md" in out, out
