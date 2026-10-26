"""La auto-deteccion de backends en puertos comunes (11434, 1234, 8000) es opt-in.

Antes, sin `[[external_providers]]` ni variables de entorno, el guild
`llama_backend` consultaba esos puertos y se enganchaba al primer servidor que
respondiera (p. ej. el LM Studio del usuario en 1234), sondeandolo y enviandole
las peticiones del juez del kernel. Ahora solo ocurre con TYLLUAN_LLM_AUTODISCOVER=1.
"""
import pathlib
import sys
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core import llama_backend  # noqa: E402


class _Resp:
    status = 200

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False


def _reset(monkeypatch):
    for var in ("OPENAI_BASE_URL", "LITELLM_API_BASE", "OLLAMA_HOST"):
        monkeypatch.delenv(var, raising=False)
    monkeypatch.setattr(llama_backend, "_read_external_providers", lambda: [])
    monkeypatch.setattr(llama_backend, "_EXTERNAL_PROVIDERS", {})
    monkeypatch.setattr(llama_backend, "_EXTERNAL_API_BASE", None)
    monkeypatch.setattr(llama_backend, "_EXTERNAL_PROVIDERS_LOADED_TS", 0.0)


def test_default_does_not_probe_common_ports(monkeypatch):
    _reset(monkeypatch)
    monkeypatch.delenv("TYLLUAN_LLM_AUTODISCOVER", raising=False)
    calls = []

    def fake_urlopen(req, timeout=None):
        calls.append(getattr(req, "full_url", req))
        return _Resp()

    monkeypatch.setattr(urllib.request, "urlopen", fake_urlopen)
    llama_backend._reload_external_providers()
    assert calls == [], f"no debe sondear puertos ajenos por defecto: {calls}"
    assert llama_backend._EXTERNAL_API_BASE is None


def test_opt_in_probes_and_adopts_first_responder(monkeypatch):
    _reset(monkeypatch)
    monkeypatch.setenv("TYLLUAN_LLM_AUTODISCOVER", "1")
    calls = []

    def fake_urlopen(req, timeout=None):
        calls.append(getattr(req, "full_url", req))
        return _Resp()

    monkeypatch.setattr(urllib.request, "urlopen", fake_urlopen)
    llama_backend._reload_external_providers()
    assert calls, "con el opt-in debe sondear"
    assert llama_backend._EXTERNAL_API_BASE == "http://127.0.0.1:11434/v1"
