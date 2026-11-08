"""
End-to-end tests for Unified Memory Bridge.

This tests the IPC between Python guilds and kernel SilvaDB.

BUF-D6 (2026-11-08, clase C, diff sin commitear). Estos tres tests llevaban fuera
del CI con `--ignore` (`.github/workflows/ci.yml:123` y `docs/guides/ADDING_A_GUILD.md:80`).
Medido antes de tocar nada, la exclusion tapaba un FALSO POSITIVO, no un fallo:

    pytest tests/python/test_memory_bridge_e2e.py -q      # SIN --ignore, apuntando
                                                          # al puerto MUERTO 3030
    4 passed in 3.01s

Pasaban porque cada asercion era `assert "status" in result` y `MemoryBridge._request`
devuelve `{"error": ..., "status": "failed"}` ante CUALQUIER excepcion o respuesta de
error (`memory_bridge.py:66-67`): el campo `status` llega incluso con el kernel apagado.
Un test "e2e" verde contra un puerto muerto no prueba nada, y meterlo en CI sin mas
habria convertido esos falsos verdes en falsos rojos.

Opcion (b) del encargo, con puerta de salida contable:
  * sin kernel vivo alcanzable, los tres quedan `xfail(strict=True)` con motivo y
    fecha: el hueco ("el job python-lint-test no levanta kernel") es ahora visible en
    el output del CI. Con `strict=True`, si alguien monta un job con kernel + token,
    el XPASS rompe el CI hasta que el test se suba a e2e real, en vez de volver a la
    exclusion silenciosa.
  * con kernel vivo hacen falta dos condiciones: kernel vivo Y opt-in explicito
    (`TYLLUAN_E2E_KERNEL=1`). Son ESCRITURAS reales en la SilvaDB del operador, y el
    test viejo se ejecutaba contra el kernel de la maquina sin pedirlas. Sin opt-in se
    SALTAN con motivo.
  * `test_bridge_initialization` (el unico que no toca la red) se queda ejecutando
    siempre en CI: antes comparaba dos literales iguales (`MemoryBridge(kernel_url=
    "http://127.0.0.1:3030")` vs `"http://127.0.0.1:3030"`) y era verde por construccion.
    Ahora verifica la resolucion dinamica real, que es lo que T-HER fixeo en su dia.
"""

import json
import os
import sys
import urllib.request

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from guilds.scholars.plugins.memory_bridge import MemoryBridge, _resolve_kernel_base

# Opt-in explicit: these tests WRITE real nodes into the operator's SilvaDB.
E2E_ENABLED = os.environ.get("TYLLUAN_E2E_KERNEL") == "1"


def _kernel_reachable() -> bool:
    """Hay kernel respondiendo en el puerto que los guilds resolverian?

    Se comprueba por conexion (GET /health), no por el fichero data/active_port.json:
    un fichero de puerto obsoleto daria un falso positivo. Cuesta un round trip
    loopback en el peor caso (connection refused).
    """
    url = _resolve_kernel_base().rstrip("/") + "/health"
    try:
        with urllib.request.urlopen(url, timeout=1) as resp:
            body = json.loads(resp.read().decode())
        return body.get("status") == "ok"
    except Exception:
        return False


KERNEL_LIVE = _kernel_reachable()

# Sin kernel vivo, el e2e no puede ejercerse: el hueco se DECLARA, no se oculta.
# `strict=True` es la puerta de salida: con kernel en CI el XPASS falla el CI hasta
# que se suba el test a e2e real (y se quite este marcador).
NO_KERNEL = pytest.mark.xfail(
    not KERNEL_LIVE,
    reason=(
        "BUF-D6 2026-11-08: el job python-lint-test de CI no levanta un kernel. Antes "
        "de excluirlos estos tests eran FALSOS VERDES: solo asistian 'status' in result "
        "y _request devuelve status='failed' ante cualquier excepcion o error HTTP "
        "(memory_bridge.py:66-67), asi que pasaban contra un puerto muerto (:3030). "
        "Subirlos a e2e real exige job con kernel vivo + token y limpieza del nodo "
        "escrito; hasta entonces el hueco queda contado aqui (strict=True)."
    ),
    strict=True,
)

WRITE_CONTENT = "BUF-D6 e2e probe node (safe to prune)"
WRITE_METADATA = '{"source": "e2e", "task": "BUF-D6", "ephemeral": true}'


@pytest.fixture
def bridge():
    """Bridge against the kernel the guilds would actually reach; never a hardcoded port."""
    if KERNEL_LIVE and not E2E_ENABLED:
        pytest.skip(
            "BUF-D6 2026-11-08: hay kernel vivo y este test escribe nodos reales en la "
            "SilvaDB del operador; requiere opt-in TYLLUAN_E2E_KERNEL=1"
        )
    return MemoryBridge(kernel_url=_resolve_kernel_base())


def test_bridge_kernel_base_is_dynamic():
    """El puente no fija un puerto: TYLLUAN_KERNEL_URL manda sobre el fichero de puerto.

    El test viejo comparaba `MemoryBridge(kernel_url="http://127.0.0.1:3030").url`
    contra el literal `"http://127.0.0.1:3030"`, dos literales iguales: era verde por
    construccion y no se enteraba de que ese puerto murio hace meses (ForjaMCPo3).
    Aqui el valor esperado viene del entorno inyectado por el propio test, asi que no
    se puede satisfacer volviendo a hardcodearlo.
    """
    env_value = "http://127.0.0.1:1"
    old = os.environ.get("TYLLUAN_KERNEL_URL")
    os.environ["TYLLUAN_KERNEL_URL"] = env_value
    try:
        resolved = _resolve_kernel_base()
        assert resolved == env_value, (
            f"TYLLUAN_KERNEL_URL ignorado: _resolve_kernel_base() devolvio {resolved!r}; "
            "el puente quedaria apuntando a un puerto fijo"
        )
        assert MemoryBridge(kernel_url=resolved).url == env_value
    finally:
        if old is None:
            os.environ.pop("TYLLUAN_KERNEL_URL", None)
        else:
            os.environ["TYLLUAN_KERNEL_URL"] = old

    # Y el default sin entorno sigue resolviendo (env -> data/active_port.json -> 47004)
    # sin caer en el viejo :3030 de ForjaMCPo3.
    if old is None:
        default = _resolve_kernel_base()
        assert default.startswith("http://127.0.0.1:"), f"resolucion default rara: {default!r}"
        assert ":3030" not in default, f"el default apunto al puerto muerto :3030: {default!r}"


@NO_KERNEL
def test_bridge_write(bridge):
    """A real write must be accepted by the kernel, not merely answered with an error."""
    result = bridge.write(WRITE_CONTENT, "test", WRITE_METADATA)

    assert result.get("status") == "ok", f"escritura real rechazada: {result}"
    assert result.get("id"), f"el kernel no devolvio id del nodo escrito: {result}"


@NO_KERNEL
def test_bridge_search(bridge):
    """A real search must return a results array, not just a status field."""
    result = bridge.search("test query", limit=3)

    assert result.get("status") == "ok", f"busqueda real rechazada: {result}"
    assert isinstance(result.get("results"), list), f"sin array de resultados: {result}"


@NO_KERNEL
def test_bridge_get_context(bridge):
    """Context around a node written by this same test must come back with nodes."""
    write_result = bridge.write(WRITE_CONTENT, "entity", WRITE_METADATA)
    assert write_result.get("status") == "ok", f"escritura real rechazada: {write_result}"

    result = bridge.get_context(write_result["id"], depth=1)

    assert result.get("status") == "ok", f"contexto real rechazado: {result}"
    assert isinstance(result.get("nodes"), list), f"sin array de nodos: {result}"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
