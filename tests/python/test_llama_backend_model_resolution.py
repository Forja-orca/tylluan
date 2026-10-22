"""D-33: el juez del kernel manda `model` al backend y deja de morir en silencio.

El defecto medido (M-BUFFY-3): con un backend compatible con OpenAI que tiene
varios modelos cargados (LM Studio es el caso real), la peticion sin el campo
`model` recibe 400 y la capa 2 (puerta de escritura ASI06) -y la capa 4- quedan
inertes sin que nada lo muestre.

Este fichero NO necesita kernel ni modelo: levanta un backend falso que
reproduce ese contrato (400 sin `model`, 200 con el) y ejercita los dos lados
del arreglo en el guild: resolver el modelo (`_resolve_legacy_model`) y enviarlo
(`_post_chat_completion` / `query_model`).
"""
import asyncio
import json
import pathlib
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core import llama_backend  # noqa: E402


class _FakeOpenAIHandler(BaseHTTPRequestHandler):
    """Backend compatible con OpenAI con DOS modelos cargados.

    Contrato del caso real: sin `model` -> 400 `model is required`.
    Con `model` -> 200 con el texto pedido.
    """

    models = ["model-a", "model-b"]
    requests = []  # (path, body)

    def log_message(self, fmt, *args):  # silencio en la salida de pytest
        return

    def do_GET(self):
        if self.path.rstrip("/") != "/v1/models":
            self.send_error(404)
            return
        type(self).requests.append((self.path, None))
        payload = {"object": "list", "data": [{"id": m, "object": "model"} for m in self.models]}
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.path.rstrip("/") != "/v1/chat/completions":
            self.send_error(404)
            return
        raw = self.rfile.read(int(self.headers.get("Content-Length") or 0))
        try:
            body = json.loads(raw)
        except Exception:
            body = {}
        type(self).requests.append((self.path, body))
        if not body.get("model"):
            payload = {"error": {"message": "model is required", "type": "invalid_request_error"}}
            data = json.dumps(payload).encode()
            self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        payload = {"choices": [{"message": {"role": "assistant", "content": "SAFE"}}]}
        data = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


@pytest.fixture()
def fake_backend():
    _FakeOpenAIHandler.requests = []
    llama_backend._LEGACY_MODEL_CACHE.clear()
    handler = _FakeOpenAIHandler
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = f"http://127.0.0.1:{server.server_address[1]}/v1"
    try:
        yield base
    finally:
        server.shutdown()
        server.server_close()
        llama_backend._LEGACY_MODEL_CACHE.clear()


def test_backend_rejects_a_request_without_model(fake_backend):
    # El defecto original, reproducido: esto es lo que hacia el juez antes del
    # arreglo y lo que el kernel contaba (mal) como un `http` cualquiera.
    with pytest.raises(Exception) as excinfo:
        llama_backend._post_chat_completion(fake_backend, {"messages": [{"role": "user", "content": "hi"}]})
    assert "400" in str(excinfo.value)


def test_backend_accepts_the_same_request_with_model(fake_backend):
    text = llama_backend._post_chat_completion(
        fake_backend,
        {"model": "model-a", "messages": [{"role": "user", "content": "hi"}]},
    )
    assert text == "SAFE"


def test_resolve_legacy_model_picks_the_first_loaded_one(fake_backend):
    assert llama_backend._resolve_legacy_model(fake_backend) == "model-a"


def test_resolve_legacy_model_honours_the_requested_model_without_probing(fake_backend):
    assert llama_backend._resolve_legacy_model(fake_backend, "explicit-model") == "explicit-model"
    assert _FakeOpenAIHandler.requests == [], "con modelo pedido no hace falta /v1/models"


def test_resolve_legacy_model_is_cached(fake_backend):
    assert llama_backend._resolve_legacy_model(fake_backend) == "model-a"
    assert llama_backend._resolve_legacy_model(fake_backend) == "model-a"
    probes = [r for r in _FakeOpenAIHandler.requests if r[0].endswith("/models")]
    assert len(probes) == 1, f"la sonda debe cachearse 60 s, vi {len(probes)}"


def test_resolve_legacy_model_returns_empty_when_backend_has_no_models_route():
    # Sin pista no se inventa un modelo: se mantiene el comportamiento anterior
    # y el kernel lo cuenta como `backend_error` si el backend protesta.
    assert llama_backend._resolve_legacy_model("http://127.0.0.1:1/v1") == ""


def test_query_model_sends_the_resolved_model_on_the_legacy_path(fake_backend, monkeypatch):
    monkeypatch.setattr(llama_backend, "_get_backend_url", lambda: fake_backend)

    text = asyncio.run(
        llama_backend.query_model(prompt="hola", max_tokens=8, temperature=0.0)
    )
    assert text == "SAFE"

    posts = [b for p, b in _FakeOpenAIHandler.requests if p.endswith("/chat/completions")]
    assert posts, "deberia haber una peticion de chat"
    assert posts[-1].get("model") == "model-a", f"el cuerpo no llevo model: {posts[-1]}"


def test_query_model_passes_an_explicit_model_through(monkeypatch, fake_backend):
    # Antes, un `model` que no casaba con ningun proveedor externo se
    # DESCARTABA: el backend seguia contestando 400 y el juez quedaba mudo.
    monkeypatch.setattr(llama_backend, "_get_backend_url", lambda: fake_backend)
    monkeypatch.setattr(llama_backend, "_get_provider_for_model", lambda name: None)

    text = asyncio.run(
        llama_backend.query_model(
            prompt="hola", model="explicit-model", max_tokens=8, temperature=0.0
        )
    )
    assert text == "SAFE"
    posts = [b for p, b in _FakeOpenAIHandler.requests if p.endswith("/chat/completions")]
    assert posts[-1].get("model") == "explicit-model"
