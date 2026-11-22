"""H-HER-M-05 y H-HER-M-06 (Hermes, 2026-11-22).

M-05: el guild `pdf` no podia abrir NINGUN PDF en produccion: el spawn de guilds solo hereda una
lista blanca de variables y ni `TYLLUAN_PDF_ROOTS` ni `TYLLUAN_DATA_DIR` estan en ella.
M-06: `find_files` devolvia los NOMBRES de `.tylluan-token`/`.env` (18 llamadas ok en
guild_audit.db); `file_search` y `file_list` ya los filtraban.
"""
import asyncio
import importlib
import os
import sys

import pytest

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if RAIZ not in sys.path:
    sys.path.insert(0, RAIZ)


# ── M-05: la raiz del pdf no puede depender de variables que el spawn no pasa ──────────────
def test_pdf_encuentra_raiz_sin_variables_de_entorno(monkeypatch):
    """MUTACION: quitar la rama por modulo deja `[]` y este test se pone rojo.

    Simula el spawn real: sin TYLLUAN_PDF_ROOTS ni TYLLUAN_DATA_DIR en el entorno.
    """
    monkeypatch.delenv("TYLLUAN_PDF_ROOTS", raising=False)
    monkeypatch.delenv("TYLLUAN_DATA_DIR", raising=False)
    import guilds.scholars.plugins.pdf as g

    importlib.reload(g)
    raices = g._raices_permitidas()
    assert raices, "sin variables el guild tiene que encontrar una raiz (sino no abre NINGUN PDF)"
    assert any(r.name == "data" for r in raices), "se esperaba <repo>/data, salio %s" % raices


def test_pdf_la_raiz_por_modulo_apunta_al_data_del_repo(monkeypatch):
    """La raiz derivada del modulo es `<repo>/data` (exista o no en este checkout)."""
    monkeypatch.delenv("TYLLUAN_ROOT", raising=False)
    monkeypatch.delenv("TYLLUAN_PDF_ROOTS", raising=False)
    monkeypatch.delenv("TYLLUAN_DATA_DIR", raising=False)
    import guilds.scholars.plugins.pdf as g

    raices = g._raices_permitidas()
    assert len(raices) == 1
    assert raices[0].name == "data"
    assert (raices[0].parent / "guilds").is_dir(), "el padre de la raiz tiene que ser el repo"


# ── H-HER-A-01: el prefijo extendido de Windows (\\?\) no puede llegar a la raiz ───────────
def test_sin_prefijo_extendido_recorta_local_y_unc():
    """MUTACION: si el helper devuelve la ruta tal cual, estos tres asserts caen."""
    import guilds.scholars.plugins.pdf as g

    assert g._sin_prefijo_extendido("\\\\?\\E:\\tylluan\\data") == "E:\\tylluan\\data"
    assert g._sin_prefijo_extendido("\\\\?\\UNC\\srv\\share\\data") == "\\\\srv\\share\\data"
    # lo que NO es prefijo verbatim se deja intacto
    assert g._sin_prefijo_extendido("E:\\tylluan\\data") == "E:\\tylluan\\data"
    assert g._sin_prefijo_extendido("\\\\srv\\share") == "\\\\srv\\share"


@pytest.mark.skipif(sys.platform != "win32", reason="arma una ruta con separadores de Windows y el prefijo extendido, que solo existen alli")
def test_raiz_desde_modulo_con_prefijo_no_devuelve_prefijo(tmp_path):
    """`__file__` llega prefijado cuando PYTHONPATH lleva \\\\?\\; la raiz no puede heredarlo."""
    import guilds.scholars.plugins.pdf as g

    falso_modulo = "\\\\?\\" + str(tmp_path) + "\\guilds\\scholars\\plugins\\pdf.py"
    raiz = g._raiz_desde_modulo(falso_modulo)
    assert raiz is not None, "un modulo prefijado tiene que dar raiz"
    assert not str(raiz).startswith("\\\\?\\"), "la raiz no puede llevar el prefijo: %s" % raiz
    assert raiz == (tmp_path / "data").resolve()


def test_raiz_prefijada_deja_pasar_una_ruta_absoluta(tmp_path, monkeypatch):
    """La prueba que cierra H-HER-A-01: con la raiz prefijada, `validate_path` denegaba TODO.

    MUTACION: sin el recorte en `_raices_permitidas`, la raiz sale `\\\\?\\...` y el assert rojo.
    """
    from guilds.core.utils import validate_path

    (tmp_path / "data").mkdir()
    monkeypatch.delenv("TYLLUAN_PDF_ROOTS", raising=False)
    monkeypatch.delenv("TYLLUAN_DATA_DIR", raising=False)
    monkeypatch.setenv("TYLLUAN_ROOT", "\\\\?\\" + str(tmp_path))
    import guilds.scholars.plugins.pdf as g

    importlib.reload(g)
    raiz = g._raices_permitidas()[0]
    assert not str(raiz).startswith("\\\\?\\"), "la raiz conservo el prefijo: %s" % raiz
    assert validate_path(str(raiz), str(tmp_path / "data" / "un.pdf")) is True, (
        "con el prefijo, validate_path deniega una ruta absoluta DENTRO de la raiz"
    )
    # y el traversal sigue denegado: el recorte no relaja la comprobacion
    assert validate_path(str(raiz), str(tmp_path / "fuera.txt")) is False



# ── M-06: find_files no revela nombres de secretos ────────────────────────────────────────
@pytest.fixture()
def fs(tmp_path, monkeypatch):
    (tmp_path / ".tylluan-token").write_text("x", encoding="utf-8")
    (tmp_path / ".env").write_text("A=1", encoding="utf-8")
    (tmp_path / "data").mkdir()
    (tmp_path / "data" / "silva.db").write_bytes(b"SQLite")
    (tmp_path / "nota.md").write_text("# hola", encoding="utf-8")
    monkeypatch.setenv("TYLLUAN_ROOT", str(tmp_path))
    import guilds.builders.plugins.filesystem as m

    importlib.reload(m)
    return m, tmp_path


def test_find_files_no_lista_el_token(fs):
    """MUTACION: quitar el filtro de nombres devuelve `.tylluan-token` y el test cae."""
    m, tmp = fs
    salida = str(asyncio.run(m.find_files("*", str(tmp))))
    assert ".tylluan-token" not in salida, "no puede listar el token: %r" % salida[:200]
    assert ".env" not in salida, "no puede listar .env: %r" % salida[:200]
    assert "silva.db" not in salida, "no puede listar bases de datos: %r" % salida[:200]


def test_find_files_sigue_listando_lo_normal(fs):
    """Control: los ficheros legítimos siguen apareciendo."""
    m, tmp = fs
    salida = str(asyncio.run(m.find_files("*", str(tmp))))
    assert "nota.md" in salida, "se perdio el fichero normal: %r" % salida[:200]
