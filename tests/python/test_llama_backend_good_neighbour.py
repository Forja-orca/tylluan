"""llama-server propio: hilos acotados y --mlock solo en ARM (maquina compartida).

Antes `threads=0` significaba os.cpu_count() (56 en la estacion de José) y --mlock
se anadia siempre que n_gpu_layers == 0; el servidor huerfano llego a rachas de
13-26 nucleos. Ahora el auto tiene tope y mlock queda para placas ARM.
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core import llama_backend  # noqa: E402


def test_auto_threads_capped(monkeypatch):
    monkeypatch.setattr(llama_backend.os, "cpu_count", lambda: 56)
    assert llama_backend._resolve_threads(0) == llama_backend.AUTO_THREADS_CAP


def test_auto_threads_never_exceeds_cpu_count(monkeypatch):
    monkeypatch.setattr(llama_backend.os, "cpu_count", lambda: 2)
    assert llama_backend._resolve_threads(0) == 2


def test_explicit_threads_wins(monkeypatch):
    monkeypatch.setattr(llama_backend.os, "cpu_count", lambda: 56)
    assert llama_backend._resolve_threads(12) == 12


def test_mlock_only_on_arm(monkeypatch):
    import platform
    monkeypatch.setattr(platform, "machine", lambda: "AMD64")
    assert llama_backend._needs_mlock() is False
    monkeypatch.setattr(platform, "machine", lambda: "aarch64")
    assert llama_backend._needs_mlock() is True
