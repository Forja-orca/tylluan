"""D-141 (auditoría de Buffy H2, 2026-11-06): `audio_tools` usaba `asyncio` sin importarlo.

El fallo era invisible en CI y solo aparecía en tiempo de ejecución, en el bucle de
espera de ComfyUI (`audio_tools.py`, `await asyncio.sleep(2)`): `NameError: name
'asyncio' is not defined` **siempre que ComfyUI aceptaba el trabajo**, no a veces.

Por eso el test no se queda en "el módulo importa" —importar NO fallaba, ese es el
detalle que hizo el bug escurridizo— sino en que el módulo tenga **enlazado** el
`asyncio` real: es la condición exacta que faltaba y la que hace imposible el
`NameError` en esa línea.
"""

import asyncio
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins import audio_tools


def test_d141_audio_tools_enlaza_asyncio():
    """Sin `import asyncio`, `audio_tools.asyncio` no existe y el bucle revienta."""
    assert hasattr(audio_tools, "asyncio"), (
        "audio_tools usa asyncio.sleep en el bucle de espera de ComfyUI pero no "
        "importa asyncio: la guild muere con NameError en cuanto ComfyUI acepta "
        "el trabajo (D-141 / H2 de la auditoría del 2026-11-06)"
    )
    assert audio_tools.asyncio is asyncio, (
        "audio_tools.asyncio no es el módulo asyncio real (¿se ha sustituido o "
        "shadowed en vez de importarlo?)"
    )


def test_d141_el_bucle_de_espera_usa_ese_asyncio():
    """El nombre enlazado tiene que ser el que usa el bucle, no uno decorativo.

    Se comprueba sobre el código fuente de la función que hace la espera: si
    alguien mueve el `asyncio.sleep` a otro sitio sin el import, esto lo delata.
    """
    fuente = pathlib.Path(audio_tools.__file__).read_text(encoding="utf-8")
    assert "await asyncio.sleep(" in fuente, (
        "el bucle de espera de ComfyUI ya no usa asyncio.sleep; si se cambió de "
        "mecanismo, este test y el import deben revisarse juntos"
    )
    assert "import asyncio" in fuente, "falta el import que da sentido al asyncio.sleep"
