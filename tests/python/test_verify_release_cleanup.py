"""Tests de `scripts/verify_release.py` (T4960).

Cubren lo que cambio el parche y que la revision del TL pidio probar:

1. Politica de limpieza del temporal: se borra si la verificacion pasa, se conserva si
   hay algun FAIL o si se pide con `conservar=True`, y un fallo de borrado solo avisa.
2. Extraccion del tarball: un tarball con una entrada `../escape.txt` tiene que ser
   RECHAZADO y no debe escribir nada fuera del destino.

Nota de metodo (me paso en la v5 y lo dejo escrito): el primer test de la extraccion
llamaba a `tf.extractall(..., filter="data")` el mismo, sin pasar por el script, asi que
seguia verde al quitar el filtro. Ahora llama a `verify_release.extrae()`, que es el codigo
que se despliega.
"""

import importlib.util
import pathlib
import tarfile

import pytest

RAIZ = pathlib.Path(__file__).resolve().parents[2]
_RUTA = RAIZ / "scripts" / "verify_release.py"


def _cargar_modulo():
    spec = importlib.util.spec_from_file_location("verify_release", _RUTA)
    modulo = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(modulo)
    return modulo


verify_release = _cargar_modulo()


# --- 1) politica de limpieza -------------------------------------------------

def test_sin_fallos_borra_el_temporal(tmp_path):
    trabajo = tmp_path / "tylluan-verify-falso"
    trabajo.mkdir()
    (trabajo / "resto.txt").write_text("x", encoding="utf-8")

    verify_release.cerrar_temporal(trabajo, fallos=0, conservar=False)

    assert not trabajo.exists(), "sin FAIL el temporal tiene que desaparecer"


def test_con_un_fallo_conserva_el_temporal(tmp_path):
    trabajo = tmp_path / "tylluan-verify-fallo"
    trabajo.mkdir()
    (trabajo / "kernel-salida.log").write_text("traza", encoding="utf-8")

    verify_release.cerrar_temporal(trabajo, fallos=1, conservar=False)

    assert trabajo.exists(), "con FAIL hay que conservarlo para poder inspeccionarlo"
    assert (trabajo / "kernel-salida.log").is_file(), "y su contenido tiene que seguir ahi"


def test_con_conservar_no_borra_aunque_pase(tmp_path):
    trabajo = tmp_path / "tylluan-verify-conservar"
    trabajo.mkdir()

    verify_release.cerrar_temporal(trabajo, fallos=0, conservar=True)

    assert trabajo.exists(), "--conservar manda sobre el resultado"


def test_si_el_borrado_falla_avisa_y_no_lanza(tmp_path, monkeypatch, capsys):
    trabajo = tmp_path / "tylluan-verify-bloqueado"
    trabajo.mkdir()

    def rmtree_que_falla(*_a, **_k):
        raise OSError(32, "The process cannot access the file")

    monkeypatch.setattr(verify_release.shutil, "rmtree", rmtree_que_falla)
    # que no tarde 5 reintentos con espera real
    monkeypatch.setattr(verify_release.time, "sleep", lambda _s: None)

    verify_release.cerrar_temporal(trabajo, fallos=0, conservar=False)

    salida = capsys.readouterr().out
    assert "WARN" in salida, "un borrado imposible tiene que avisarse"
    assert trabajo.exists(), "si no se pudo borrar, la carpeta sigue (no se finge limpieza)"


# --- 2) extraccion con filter="data" ----------------------------------------

def _tarball_con_escape(destino_tar: pathlib.Path) -> pathlib.Path:
    """Construye un .tar.gz a mano con una entrada que intenta salir del destino."""
    dentro = destino_tar.parent / "contenido"
    dentro.mkdir(exist_ok=True)
    malicioso = dentro / "escape.txt"
    malicioso.write_text("fuera", encoding="utf-8")
    with tarfile.open(destino_tar, "w:gz") as tf:
        info = tarfile.TarInfo(name="../escape.txt")
        info.size = malicioso.stat().st_size
        with malicioso.open("rb") as fh:
            tf.addfile(info, fh)
    return destino_tar


def test_extrae_rechaza_entrada_que_escapa(tmp_path):
    trabajo = tmp_path / "verif"
    trabajo.mkdir()
    destino = trabajo / "instalado"
    destino.mkdir()
    tar = _tarball_con_escape(trabajo / "malicioso.tar.gz")

    with pytest.raises(tarfile.TarError):
        verify_release.extrae(tar, destino)

    assert not (tmp_path / "escape.txt").exists(), "no puede escribir fuera del destino"
    assert not (trabajo / "escape.txt").exists()


def test_extrae_acepta_un_tarball_normal(tmp_path):
    """Control: el filtro no puede rechazar un tarball legitimo."""
    origen = tmp_path / "normal"
    origen.mkdir()
    (origen / "f.txt").write_text("contenido", encoding="utf-8")
    tar = tmp_path / "normal.tar.gz"
    with tarfile.open(tar, "w:gz") as tf:
        tf.add(origen / "f.txt", arcname="f.txt")

    destino = tmp_path / "destino"
    destino.mkdir()
    verify_release.extrae(tar, destino)

    assert (destino / "f.txt").read_text(encoding="utf-8") == "contenido"
