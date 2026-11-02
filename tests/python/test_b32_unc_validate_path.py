"""B-32 H2 (2026-11-02): `validate_path` no debe esperar a la red.

`validate_path` comprobaba contención con `Path.resolve()`, que en Windows abre
el recurso. Con un destino UNC (`\\\\host\\share\\x`, `//host/x`) eso resuelve el
host por SMB: medido, ~5,5 s por hostname nuevo y 2 ms si el host se repite
(la búsqueda negativa queda cacheada). El resultado siempre es el correcto
(`False`), así que el coste no compra nada: un llamante que cicla hostnames
bloquea un worker del guild en cada petición.

El arreglo rechaza el caso "destino UNC, base local" antes de tocar el disco, así
que estos tests exigen dos cosas a la vez: la respuesta sigue siendo `False` y
llega en menos de 100 ms.
"""
import pathlib
import sys
import time

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.utils import validate_path


UNC_TARGETS = [
    "//zz1fuzz/x",
    "//zz2fuzz/share/deep/file.txt",
    r"\\zz3fuzz\share\x",
    "//ñ/|",
]


@pytest.mark.parametrize("target", UNC_TARGETS)
def test_unc_target_is_rejected_fast(tmp_path, target):
    started = time.perf_counter()
    assert validate_path(str(tmp_path), target) is False
    elapsed = time.perf_counter() - started
    assert elapsed < 0.1, f"{target!r} tardó {elapsed * 1000:.0f} ms; debe resolverse sin red"


def test_unc_target_with_unc_base_is_not_rejected_by_the_shortcut():
    """Si la base también es UNC no aplicamos el atajo: la contención se decide
    con el camino normal (y puede ser legítima dentro del mismo share)."""
    started = time.perf_counter()
    # No debe lanzar y debe devolver un bool; el coste de red aquí es del propio
    # llamante, que pidió trabajar sobre un share.
    result = validate_path(r"\\zz4fuzz\share", r"\\zz4fuzz\share\sub\file.txt")
    assert isinstance(result, bool)
    assert time.perf_counter() - started < 60


def test_local_paths_still_validate(tmp_path):
    inside = tmp_path / "sub" / "file.txt"
    inside.parent.mkdir()
    inside.write_text("ok", encoding="utf-8")

    assert validate_path(str(tmp_path), str(inside)) is True
    assert validate_path(str(tmp_path), str(tmp_path)) is True


def test_traversal_and_outside_paths_still_rejected(tmp_path):
    outside = tmp_path.parent / "b32-outside.txt"

    assert validate_path(str(tmp_path), "..") is False
    assert validate_path(str(tmp_path), "../b32-outside.txt") is False
    assert validate_path(str(tmp_path), str(outside)) is False


def test_windows_device_still_rejected_before_the_unc_gate(tmp_path):
    # B-30 sigue mandando: un dispositivo no se acepta ni disfrazado de UNC.
    assert validate_path(str(tmp_path), "NUL") is False
    assert validate_path(str(tmp_path), "con.txt") is False
