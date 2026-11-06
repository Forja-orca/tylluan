"""D-142 (auditoría de Buffy H3, 2026-11-06): inyección de comandos en el plugin de git.

**El fallo**: `_run_git_shell` unía los argumentos con `shlex.quote` (quoting POSIX)
y los ejecutaba con `asyncio.create_subprocess_shell`. En Windows eso es `cmd.exe`,
que **no** entiende comillas simples: el payload `a & echo MARCADOR` dentro de un
mensaje de commit (o de una ruta) se ejecutaba como comando aparte. Alcanzable
desde `git_commit(message=...)` / `git_add(path=...)` en el escenario exacto para
el que existía ese fallback: dos timeouts encadenados por antivirus agresivo.

**Lo que se comprueba aquí, y por qué de dos formas**:

1. `test_d142_el_payload_no_se_ejecuta` — comportamiento. Se llama al último
   intento con el payload real y se exige que `MARCADOR` no aparezca en la salida.
   Ojo: en Linux este test **también pasaba con el código viejo** (`/bin/sh` sí
   entiende las comillas simples de `shlex.quote`); el fallo era exclusivo de
   Windows. Por eso solo no basta.
2. `test_d142_no_hay_apis_de_shell` — estructural y multiplataforma. Recorre el
   **AST** del módulo y falla si vuelve a aparecer `create_subprocess_shell` o un
   `import shlex`. Es el que impide la reaparición del patrón, que es lo que este
   repo ya hace con otros guards estructurales (p. ej. el matcher exacto de
   procesos). Va por AST y no por grep para que la propia documentación del
   arreglo —que nombra las APIs viejas— no dé un falso positivo.
"""

import ast
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins import git as git_plugin

PAYLOAD = "a & echo MARCADOR"


async def test_d142_el_payload_no_se_ejecuta(tmp_path):
    """El `&` tiene que llegar a git como texto, no al intérprete como separador."""
    salida = await git_plugin._run_git_ultimo_intento(
        ["rev-parse", "--verify", PAYLOAD], cwd=str(tmp_path)
    )
    assert "MARCADOR" not in salida, (
        "el payload se ejecutó: el plugin está pasando los argumentos por un "
        f"intérprete de comandos. Salida:\n{salida}"
    )


def test_d142_no_hay_apis_de_shell():
    """Guard estructural: el plugin no puede volver a construir líneas de comandos."""
    fuente = pathlib.Path(git_plugin.__file__).read_text(encoding="utf-8")
    arbol = ast.parse(fuente)

    llamadas_a_shell = [
        nodo.attr
        for nodo in ast.walk(arbol)
        if isinstance(nodo, ast.Attribute)
        and nodo.attr in {"create_subprocess_shell", "getoutput", "system"}
    ]
    assert not llamadas_a_shell, (
        "el plugin de git vuelve a usar una API de shell "
        f"({sorted(set(llamadas_a_shell))}): los argumentos dejarían de ser una "
        "lista y volverían a pasar por cmd.exe / sh (D-142 / H3)"
    )

    imports_shlex = [
        nodo
        for nodo in ast.walk(arbol)
        if isinstance(nodo, ast.Import) and any(a.name == "shlex" for a in nodo.names)
    ] + [
        nodo
        for nodo in ast.walk(arbol)
        if isinstance(nodo, ast.ImportFrom) and nodo.module == "shlex"
    ]
    assert not imports_shlex, (
        "el plugin de git vuelve a importar `shlex`: su quoting es POSIX y en "
        "Windows no protege nada (D-142 / H3)"
    )
