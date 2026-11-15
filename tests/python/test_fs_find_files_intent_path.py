"""FS-FIND-FILES-IGNORA-RUTA-DEL-INTENT (encargo TL T4147).

Hermes-Linux reporto that the `filesystem` guild, given an intent that names an
absolute path ("lista los ficheros de /ruta/exacta"), listed the kernel's CWD
instead of that path. In production the kernel CWD is the Tylluan root, so the
listing exposed `data/`, `bin/` and the file NAMES of `.tylluan-token` and
`identity.key` (the guild filters directories, not sensitive file names).

These tests pin the new contract:

1. an absolute path inside the allowed root, named in the intent, is the
   directory that gets listed;
2. an absolute path outside the allowed root is refused with a clear message
   and is NOT replaced by the CWD;
3. the result always names the directory it searched, so the CWD can never be
   a silent surprise;
4. negative controls: an explicit `directory` parameter still wins, and an
   intent with no path keeps the old default behaviour.
"""

import asyncio
from pathlib import Path

import pytest

CANARY = "CANARIO-FUERA-DEL-ROOT"
SECRET_NAME = "secreto.key"


def _load_fs(monkeypatch, root: Path):
    """Import the guild with its allowed root pointing at `root`.

    `_FS_ROOT` is a module-level constant (`filesystem.py:23`), so the test has
    to set both the environment variable (for a fresh import) and the attribute
    (for an already-imported module), same as `test_b24_guild_path_traversal.py`.
    """
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
    """Criterion 1: the path named in the intent is the one searched."""
    fs, root, docs, _outside = tree
    out = asyncio.run(fs.find_files(intent=f"lista los ficheros de {docs}"))
    assert "guia.md" in out, out
    assert "raiz.txt" not in out, out


def test_result_names_the_directory_it_searched(tree):
    """Criterion 3: the header says WHERE it looked — this is what fails on main,
    where the header had no directory at all and the listing came from the CWD."""
    fs, _root, docs, _outside = tree
    out = asyncio.run(fs.find_files(intent=f"lista los ficheros de {docs}"))
    assert str(docs) in out, out


def test_path_in_intent_is_not_mistaken_for_an_extension(tree):
    """A Windows-style path containing a dot segment must not become `*.segment`.

    Before the fix the extension regex (`filesystem.py`, now `find_files`) could
    read `.docs`/`.cache` out of the path and produce a bogus glob.
    """
    fs, _root, docs, _outside = tree
    out = asyncio.run(fs.find_files(intent=f"enséñame los .md de {docs}"))
    assert "guia.md" in out, out
    assert "*.docs" not in out and "*.cache" not in out, out


def test_absolute_path_outside_root_is_blocked_not_cwd(tree):
    """Criterion 2: outside the allowed root → clear refusal, never the CWD."""
    fs, _root, _docs, outside = tree
    out = asyncio.run(fs.find_files(intent=f"lista los ficheros de {outside}"))
    assert "BLOCKED" in out, out
    assert CANARY not in out and SECRET_NAME not in out, out
    assert "did NOT fall back" in out, out


def test_missing_named_directory_is_not_replaced_by_cwd(tree):
    """A path in the request that does not exist says so, instead of quietly
    listing the kernel's working directory."""
    fs, root, _docs, _outside = tree
    ghost = root / "no-existe"
    out = asyncio.run(fs.find_files(intent=f"lista los ficheros de {ghost}"))
    assert "not found" in out, out
    assert str(ghost) in out, out
    assert "working directory" in out, out


def test_explicit_directory_parameter_still_wins(tree):
    """Negative control: an explicit `directory` is the caller's decision and the
    intent cannot override it."""
    fs, root, docs, _outside = tree
    out = asyncio.run(fs.find_files(directory=str(docs), intent=f"de {root}"))
    assert "guia.md" in out, out
    assert "raiz.txt" not in out, out


def test_intent_without_a_path_keeps_the_cwd_default(tree, monkeypatch):
    """Negative control: no path in the intent → default behaviour stands, but
    the header now names the directory that was searched."""
    fs, root, _docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(fs.find_files(intent="busca archivos markdown"))
    assert "guia.md" in out, out
    assert str(root) in out, out


def test_url_in_intent_is_not_treated_as_a_directory(tree, monkeypatch):
    """`https://host/path` must not be harvested as a local directory: that would
    turn a web-ish intent into a bogus filesystem listing."""
    fs, root, docs, _outside = tree
    monkeypatch.chdir(root)
    out = asyncio.run(
        fs.find_files(intent="busca archivos md en https://example.com/docs")
    )
    assert "example.com" not in out, out
    assert "guia.md" in out, out
