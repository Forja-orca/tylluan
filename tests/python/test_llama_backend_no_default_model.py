"""D-158: Tylluan no trae ni descarga modelo de chat.

Criterio medible: sin `[inference.llama] model_path` (ni `models_dir`, que
activa el modo router) el guild **falla con la ruta exacta a configurar** y
**no** intenta bajar nada de HuggingFace. Con `model_path` del usuario, se usa
tal cual. Y `backend_health` no puede anunciar un modelo que no existe.

Este fichero no necesita kernel, ni modelo, ni red: monkeypatchea `_get_config`
y prohíbe `hf_hub_download` para que una recaída explote.
"""
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core import llama_backend  # noqa: E402


def _config(model_path="", models_dir=""):
    return {"model_path": model_path, "models_dir": models_dir}


@pytest.fixture()
def sin_modelo(monkeypatch):
    monkeypatch.setattr(llama_backend, "_get_config", lambda: _config())


def test_sin_modelo_falla_diciendo_que_configurar(sin_modelo):
    with pytest.raises(RuntimeError) as excinfo:
        llama_backend._resolve_model_path()
    msg = str(excinfo.value)
    assert "[inference.llama] model_path" in msg, msg
    assert "models_dir" in msg, msg
    assert "no empaqueta ni descarga" in msg, msg


def test_no_se_descarga_nada_de_huggingface(sin_modelo, monkeypatch):
    huggingface_hub = pytest.importorskip("huggingface_hub")

    def _prohibido(*args, **kwargs):
        raise AssertionError("D-158: llama_backend no puede descargar modelos de chat")

    monkeypatch.setattr(huggingface_hub, "hf_hub_download", _prohibido)
    with pytest.raises(RuntimeError):
        llama_backend._resolve_model_path()


def test_sin_modelo_no_toca_la_cache_de_huggingface(sin_modelo):
    # La descarga vieja usaba hf_hub_download con cache_dir en ~/.cache. Si
    # vuelve, esto lo delata sin depender de que haya red.
    source = pathlib.Path(llama_backend.__file__).read_text(encoding="utf-8")
    assert "hf_hub_download" not in source
    assert "cache_dir=" not in source


def test_model_path_del_usuario_se_usa_tal_cual(tmp_path, monkeypatch):
    gguf = tmp_path / "mi-modelo.gguf"
    gguf.write_bytes(b"GGUF")
    monkeypatch.setattr(llama_backend, "_get_config", lambda: _config(model_path=str(gguf)))
    assert llama_backend._resolve_model_path() == str(gguf)


def test_model_path_inexistente_no_intenta_descargar(tmp_path, monkeypatch):
    monkeypatch.setattr(
        llama_backend, "_get_config", lambda: _config(model_path=str(tmp_path / "no-existe.gguf"))
    )
    with pytest.raises(RuntimeError) as excinfo:
        llama_backend._resolve_model_path()
    assert "no existe" in str(excinfo.value)


def test_health_dice_la_verdad_cuando_no_hay_modelo(sin_modelo):
    assert llama_backend._configured_model_label() == "none"


def test_health_etiqueta_el_modelo_del_usuario(monkeypatch):
    monkeypatch.setattr(
        llama_backend,
        "_get_config",
        lambda: _config(model_path=r"C:\modelos\mio.gguf"),
    )
    assert llama_backend._configured_model_label() == r"C:\modelos\mio.gguf"


def test_health_prefiere_models_dir_en_modo_router(monkeypatch):
    monkeypatch.setattr(
        llama_backend,
        "_get_config",
        lambda: _config(model_path=r"C:\modelos\mio.gguf", models_dir=r"C:\modelos"),
    )
    assert llama_backend._configured_model_label() == r"C:\modelos"
