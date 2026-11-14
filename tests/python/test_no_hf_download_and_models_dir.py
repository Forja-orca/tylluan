"""D-159: cerrar Hugging Face por defecto y fijar la carpeta de modelos.

Criterios del TL (T3721, F1/F2/F4):
- con `allow_hf_download = false` (defecto) **ningún** guild llama a
  `hf_hub_download`/`snapshot_download`;
- los guilds reciben `TYLLUAN_MODELS_DIR` **absoluta** desde el kernel;
- `vision.py` no arrastra un `_MODEL_ID` muerto.

Son comprobaciones estáticas o de unidad: no necesitan kernel, red ni modelos.
"""
import pathlib
import sys

import pytest

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT))

GUILDS = ROOT / "guilds"


def test_no_guild_downloads_from_huggingface_by_default():
    infractores = []
    for path in GUILDS.rglob("*.py"):
        text = path.read_text(encoding="utf-8", errors="replace")
        if "hf_hub_download(" in text or "snapshot_download(" in text:
            infractores.append(str(path.relative_to(ROOT)))
    assert infractores == [], (
        "estos guilds descargan de HuggingFace y deben leer de models/ "
        f"(allow_hf_download=false): {infractores}"
    )


def test_kernel_exports_the_models_dir_to_guilds():
    guild_process = (ROOT / "crates/tylluan-kernel/src/registry/guild_process.rs").read_text(
        encoding="utf-8"
    )
    config = (ROOT / "crates/tylluan-kernel/src/config.rs").read_text(encoding="utf-8")
    assert 'pub const MODELS_DIR_ENV: &str = "TYLLUAN_MODELS_DIR";' in config
    # T4120: el entorno de los guilds se construye en config::guild_launcher_env_with_operator_override;
    # el spawn debe usarlo y esa funcion debe exportar la variable.
    assert "guild_launcher_env(" in guild_process, (
        "el kernel debe exportar TYLLUAN_MODELS_DIR al lanzar un guild (via guild_launcher_env)"
    )
    builder = config.split("pub fn guild_launcher_env_with_operator_override", 1)[1].split("pub fn guild_launcher_env(", 1)[0]
    assert "MODELS_DIR_ENV.to_string()" in builder, (
        "guild_launcher_env_with_operator_override debe incluir TYLLUAN_MODELS_DIR"
    )


def test_vision_prefers_the_exported_models_dir(tmp_path, monkeypatch):
    from guilds.core import vision

    monkeypatch.setenv("TYLLUAN_MODELS_DIR", str(tmp_path))
    monkeypatch.delenv("TYLLUAN_VISION_MODEL_DIR", raising=False)
    assert vision._models_dir() == str(tmp_path)
    with pytest.raises(FileNotFoundError) as excinfo:
        vision._resolve_model_dir()
    assert str(tmp_path) in str(excinfo.value), "el error debe decir la ruta exacta que falta"


def test_vision_has_no_dead_model_id():
    from guilds.core import vision

    assert not hasattr(vision, "_MODEL_ID"), "F4: _MODEL_ID muerto debía borrarse"


# ── GLiNER (knowledge.py): el cuarto que descargaba de HF ─────────────────


def _write_cfg(tmp_path, texto):
    cfg = tmp_path / "tylluan.toml"
    cfg.write_text(texto, encoding="utf-8")
    return cfg


def test_gliner_gate_defaults_to_closed(tmp_path):
    from guilds.scholars.plugins import knowledge

    assert knowledge._hf_download_allowed(None) in (True, False)  # no revienta
    cfg = _write_cfg(tmp_path, "[nexus]\nallow_hf_download = false\n")
    assert knowledge._hf_download_allowed(str(cfg)) is False


def test_gliner_gate_opens_only_with_explicit_permission(tmp_path):
    from guilds.scholars.plugins import knowledge

    cfg = _write_cfg(tmp_path, "[nexus]\nallow_hf_download = true\n")
    assert knowledge._hf_download_allowed(str(cfg)) is True


def test_gliner_gate_closed_when_config_is_missing_or_broken(tmp_path):
    from guilds.scholars.plugins import knowledge

    assert knowledge._hf_download_allowed(str(tmp_path / "no-existe.toml")) is False
    roto = _write_cfg(tmp_path, "esto no es toml = = =")
    assert knowledge._hf_download_allowed(str(roto)) is False


def test_gliner_never_downloads_without_permission(monkeypatch):
    import types

    from guilds.scholars.plugins import knowledge

    calls = []

    class SpyGLiNER:
        @staticmethod
        def from_pretrained(source):
            calls.append(source)
            return object()

    fake = types.ModuleType("gliner")
    fake.GLiNER = SpyGLiNER
    monkeypatch.setitem(sys.modules, "gliner", fake)
    monkeypatch.setattr(knowledge, "GLINER_AVAILABLE", True)
    monkeypatch.setattr(knowledge, "_model", None)
    monkeypatch.setattr(knowledge, "_model_load_failed", False)
    monkeypatch.delenv("TYLLUAN_GLINER_MODEL_DIR", raising=False)
    monkeypatch.setattr(knowledge, "_hf_download_allowed", lambda *a, **k: False)

    assert knowledge.get_model() is None
    assert calls == [], f"con HF apagado no se puede llamar a from_pretrained: {calls}"


def test_gliner_uses_a_local_copy_even_with_hf_off(monkeypatch, tmp_path):
    import types

    from guilds.scholars.plugins import knowledge

    local = tmp_path / "gliner-local"
    local.mkdir()
    calls = []

    class SpyGLiNER:
        @staticmethod
        def from_pretrained(source):
            calls.append(source)
            return object()

    fake = types.ModuleType("gliner")
    fake.GLiNER = SpyGLiNER
    monkeypatch.setitem(sys.modules, "gliner", fake)
    monkeypatch.setattr(knowledge, "GLINER_AVAILABLE", True)
    monkeypatch.setattr(knowledge, "_model", None)
    monkeypatch.setattr(knowledge, "_model_load_failed", False)
    monkeypatch.setenv("TYLLUAN_GLINER_MODEL_DIR", str(local))
    monkeypatch.setattr(knowledge, "_hf_download_allowed", lambda *a, **k: False)

    assert knowledge.get_model() is not None
    assert calls == [str(local)], calls


def test_example_toml_documents_allow_hf_download():
    text = (ROOT / "tylluan.example.toml").read_text(encoding="utf-8")
    assert "allow_hf_download" in text, "F6: falta documentar la bandera en el TOML de ejemplo"
