"""Tarea R1 (v2): helper unico de raiz, resuelto EN CADA LLAMADA.

Lo que se prueba:

(a) comportamiento **identico al de hoy** por defecto (`TYLLUAN_ROOT` si esta; si no, cwd);
(b) **fallo cerrado solo si se activa** con `TYLLUAN_ROOT_REQUIRED=1`;
(c) con el fallo cerrado activo y sin raiz, **el modulo IMPORTA bien y la herramienta
    responde BLOCKED**: no se queda el guild caido (cambio pedido por el TL en T4983);
(d) los tres guilds usan el helper y no les queda el patron viejo.
"""

import asyncio
import importlib
import os
import pathlib

import pytest

RAIZ = pathlib.Path(__file__).resolve().parents[2]

from guilds.core import _root  # noqa: E402


# --- (a) identico a hoy ------------------------------------------------------

def test_con_variable_devuelve_la_variable(monkeypatch):
    monkeypatch.setenv("TYLLUAN_ROOT", "D:/espacio-de-trabajo")
    monkeypatch.delenv("TYLLUAN_ROOT_REQUIRED", raising=False)
    assert _root.raiz_permitida() == "D:/espacio-de-trabajo"


def test_sin_variable_devuelve_el_cwd_como_antes(monkeypatch):
    monkeypatch.delenv("TYLLUAN_ROOT", raising=False)
    monkeypatch.delenv("TYLLUAN_ROOT_REQUIRED", raising=False)
    assert _root.raiz_permitida() == os.getcwd()


def test_raiz_o_deniega_no_deniega_por_defecto(monkeypatch):
    monkeypatch.delenv("TYLLUAN_ROOT", raising=False)
    monkeypatch.delenv("TYLLUAN_ROOT_REQUIRED", raising=False)
    raiz, error = _root.raiz_o_deniega()
    assert raiz == os.getcwd() and error is None


# --- (b) fallo cerrado solo si se activa ------------------------------------

def test_fallo_cerrado_activado_sin_raiz_deniega(monkeypatch):
    monkeypatch.delenv("TYLLUAN_ROOT", raising=False)
    monkeypatch.setenv("TYLLUAN_ROOT_REQUIRED", "1")
    with pytest.raises(_root.RaizNoDeclarada) as exc:
        _root.raiz_permitida()
    mensaje = str(exc.value)
    assert "TYLLUAN_ROOT" in mensaje, "tiene que decir QUE variable falta"
    assert os.getcwd() in mensaje, "y que NO se esta asumiendo el cwd"


def test_raiz_o_deniega_devuelve_mensaje_en_vez_de_lanzar(monkeypatch):
    monkeypatch.delenv("TYLLUAN_ROOT", raising=False)
    monkeypatch.setenv("TYLLUAN_ROOT_REQUIRED", "1")
    raiz, error = _root.raiz_o_deniega()
    assert raiz is None
    assert error and "BLOCKED" in error and "TYLLUAN_ROOT" in error


def test_fallo_cerrado_activado_con_raiz_funciona(monkeypatch):
    monkeypatch.setenv("TYLLUAN_ROOT", "D:/espacio-de-trabajo")
    monkeypatch.setenv("TYLLUAN_ROOT_REQUIRED", "1")
    assert _root.raiz_permitida() == "D:/espacio-de-trabajo"


# --- (c) import sano + BLOCKED por llamada (el cambio de T4983) -------------

def test_con_fallo_cerrado_el_modulo_importa_y_la_tool_deniega(monkeypatch):
    monkeypatch.delenv("TYLLUAN_ROOT", raising=False)
    monkeypatch.setenv("TYLLUAN_ROOT_REQUIRED", "1")

    modulo = importlib.import_module("guilds.builders.plugins.filesystem")
    importlib.reload(modulo)  # tiene que importar SIN lanzar con el fallo cerrado activo

    salida = asyncio.run(modulo.file_list())
    assert isinstance(salida, str)
    assert "BLOCKED" in salida, f"esperaba BLOCKED, llego: {salida[:200]!r}"
    assert "TYLLUAN_ROOT" in salida, "el BLOCKED tiene que decir que variable falta"


def test_con_fallo_cerrado_desactivado_la_tool_lista(monkeypatch, tmp_path):
    monkeypatch.setenv("TYLLUAN_ROOT", str(tmp_path))
    monkeypatch.delenv("TYLLUAN_ROOT_REQUIRED", raising=False)
    (tmp_path / "un-fichero.txt").write_text("x", encoding="utf-8")

    modulo = importlib.import_module("guilds.builders.plugins.filesystem")
    importlib.reload(modulo)

    salida = asyncio.run(modulo.file_list(directory=str(tmp_path)))
    assert "un-fichero.txt" in salida, f"deberia listar el fichero, llego: {salida[:200]!r}"


# --- (d) los tres guilds usan el helper (y mutacion) ------------------------

def test_los_tres_guilds_usan_el_helper():
    patron_viejo = 'os.environ.get("TYLLUAN_ROOT", os.getcwd())'
    pendientes = []
    for rel in (
        "guilds/builders/plugins/filesystem.py",
        "guilds/builders/plugins/bash.py",
        "guilds/builders/plugins/code.py",
    ):
        texto = (RAIZ / rel).read_text(encoding="utf-8", errors="replace")
        if patron_viejo in texto:
            pendientes.append(f"{rel}: sigue con el patron viejo")
        if "raiz_permitida()" not in texto and "raiz_o_deniega()" not in texto:
            pendientes.append(f"{rel}: no usa el helper")
        if "from guilds.core._root import" not in texto:
            pendientes.append(f"{rel}: no importa el helper")
    assert not pendientes, pendientes


def test_filesystem_no_resuelve_la_raiz_al_importar():
    """El constante module-level era lo que dejaba el guild caido; no puede volver."""
    texto = (RAIZ / "guilds/builders/plugins/filesystem.py").read_text(encoding="utf-8")
    assert "_FS_ROOT = " not in texto, "la raiz no puede evaluarse al importar"


def test_el_mensaje_de_raiz_nombra_la_raiz_real():
    assert "D:/sitio" in _root.mensaje_raiz("D:/sitio")
    assert "TYLLUAN_ROOT" in _root.mensaje_raiz("D:/sitio")


# --- (e) H-HER-A-01: el prefijo extendido de Windows ------------------------

def test_normaliza_quita_el_prefijo_extendido():
    assert _root.normaliza_raiz("\\\\?\\D:\\Temp\\x") == "D:\\Temp\\x"


def test_normaliza_quita_el_prefijo_de_una_ruta_unc():
    assert _root.normaliza_raiz("\\\\?\\UNC\\srv\\share") == "\\\\srv\\share"


def test_normaliza_deja_igual_una_ruta_normal():
    assert _root.normaliza_raiz("D:\\Temp\\x") == "D:\\Temp\\x"


def test_raiz_permitida_normaliza_la_variable(monkeypatch, tmp_path):
    monkeypatch.setenv("TYLLUAN_ROOT", "\\\\?\\" + str(tmp_path))
    monkeypatch.delenv("TYLLUAN_ROOT_REQUIRED", raising=False)
    assert _root.raiz_permitida() == str(tmp_path)


def test_h_her_a_01_filesystem_lista_con_la_raiz_prefijada(monkeypatch, tmp_path):
    """El sintoma que confirmo Hermes en vivo: con la raiz con `\\\\?\\`, el guild
    denegaba TODO, incluido '.'. Aqui se reproduce y se exige que liste."""
    (tmp_path / "visible.txt").write_text("x", encoding="utf-8")
    monkeypatch.setenv("TYLLUAN_ROOT", "\\\\?\\" + str(tmp_path))
    monkeypatch.delenv("TYLLUAN_ROOT_REQUIRED", raising=False)

    modulo = importlib.import_module("guilds.builders.plugins.filesystem")
    importlib.reload(modulo)

    salida = asyncio.run(modulo.file_list(directory=str(tmp_path)))
    assert "visible.txt" in salida, f"deberia listar el fichero, llego: {salida[:200]!r}"
    assert "BLOCKED" not in salida, f"no deberia denegar su propia raiz: {salida[:200]!r}"
