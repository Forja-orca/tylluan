"""SEGURIDAD: `file_read` no puede devolver tokens ni bases de datos (Deep, 2026-11-21).

Hallazgo medido en produccion: con `_FS_ROOT = os.getcwd()` (= `E:\\tylluan`), `validate_path`
daba PERMITIDA a `.tylluan-token` (33 B, clase OPERADOR), `.forja-token`, `.env` y `data/*.db`.
`file_search` ya los filtraba con los SKIP compartidos; `file_read` no.
"""
import asyncio
import os
import sys

import pytest

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if RAIZ not in sys.path:
    sys.path.insert(0, RAIZ)


@pytest.fixture()
def fs(tmp_path, monkeypatch):
    """Guild apuntando a una raiz temporal con un secreto y un fichero normal dentro."""
    (tmp_path / ".tylluan-token").write_text("secreto-que-no-debe-salir", encoding="utf-8")
    (tmp_path / "data").mkdir()
    (tmp_path / "data" / "silva.db").write_bytes(b"SQLite format 3\x00")
    (tmp_path / "README.md").write_text("# hola\nlinea 2\n", encoding="utf-8")
    monkeypatch.setenv("TYLLUAN_ROOT", str(tmp_path))
    import importlib

    import guilds.builders.plugins.filesystem as m

    importlib.reload(m)
    return m, tmp_path


def _leer(m, ruta):
    return str(asyncio.run(m.file_read(str(ruta))))


def test_file_read_bloquea_el_token_maestro(fs):
    """MUTACION: sin el filtro de SKIP_FILES, este test devuelve el secreto y falla."""
    m, tmp = fs
    r = _leer(m, tmp / ".tylluan-token")
    assert "BLOCKED" in r, "el token maestro NO puede leerse: %r" % r[:80]
    assert "secreto-que-no-debe-salir" not in r


def test_file_read_bloquea_una_base_de_datos(fs):
    m, tmp = fs
    r = _leer(m, tmp / "data" / "silva.db")
    assert "BLOCKED" in r
    assert "SQLite format" not in r


def test_file_read_bloquea_env(fs):
    m, tmp = fs
    (tmp / ".env").write_text("ANTHROPIC_API_KEY=sk-no", encoding="utf-8")
    r = _leer(m, tmp / ".env")
    assert "BLOCKED" in r


def test_file_read_sigue_leyendo_un_fichero_normal(fs):
    """Control: no se ha roto la lectura legítima."""
    m, tmp = fs
    r = _leer(m, tmp / "README.md")
    assert "hola" in r
    assert "BLOCKED" not in r


def test_la_lista_compartida_incluye_los_dos_tokens():
    """El arreglo vive en `_security.SKIP_FILES`, asi que tambien protege a `file_search`."""
    from guilds.core import _security

    assert ".tylluan-token" in _security.SKIP_FILES
    assert ".forja-token" in _security.SKIP_FILES
    assert ".db" in _security.SKIP_EXTENSIONS
