"""BUF-D2: gate AST que exige marcar el silencio de los `except: pass` en guilds/.

Plantilla: `tests/python/test_git_sin_shell.py` (D-142), que recorre el AST en vez de grepear
para no confundir documentación con código.

Regla: si un `except` tiene como ÚNICO cuerpo un `pass`, la línea del `except` debe llevar
`# silent-ok: <razon>`. Así el silencio es una decisión escrita (con su porqué) y un `pass`
nuevo no puede colarse: la CI lo para.

Por qué: un `except: pass` esconde fallos reales. En este censo (2026-11-07) había 25 y uno
—cron_scheduler.py— tragaba el fallo de un comando y el job contaba como ejecutado.
"""

import ast
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

RAIZ = pathlib.Path(__file__).resolve().parent.parent.parent / "guilds"
MARCA = "# silent-ok:"


def _pass_sin_marcar(fichero: pathlib.Path) -> list[str]:
    """Devuelve las líneas `except ...: pass` sin marca, con el motivo."""
    try:
        fuente = fichero.read_text(encoding="utf-8", errors="replace")
        arbol = ast.parse(fuente)
    except SyntaxError as exc:  # no silencioso: el que no parsea lo dice aqui
        return [f"{fichero}: no parsea ({exc})"]

    lineas = fuente.splitlines()
    faltas = []
    for nodo in ast.walk(arbol):
        if not isinstance(nodo, ast.ExceptHandler):
            continue
        if len(nodo.body) != 1 or not isinstance(nodo.body[0], ast.Pass):
            continue
        linea = lineas[nodo.lineno - 1] if nodo.lineno - 1 < len(lineas) else ""
        if MARCA not in linea:
            tipo = ast.unparse(nodo.type) if nodo.type else "bare"
            try:
                donde = fichero.relative_to(RAIZ.parent).as_posix()
            except ValueError:  # fichero de un fixture, fuera del repo
                donde = str(fichero)
            faltas.append(
                f"{donde}:{nodo.lineno}: "
                f"`except {tipo}: pass` sin marca. Si el silencio es deliberado, "
                f"añade `{MARCA} <razon>`; si no, registra el fallo o estrecha la excepción."
            )
    return faltas


def test_guilds_except_pass_esta_marcado():
    faltas = []
    for fichero in sorted(RAIZ.rglob("*.py")):
        faltas.extend(_pass_sin_marcar(fichero))
    assert not faltas, (
        f"{len(faltas)} `except: pass` sin justificar en guilds/:\n  " + "\n  ".join(faltas)
    )


def test_el_gate_no_es_vacuo(tmp_path):
    """Un `except: pass` plantado a propósito DEBE hacer saltar el gate.

    Sin esto, el test podría pasar por no estar mirando nada (el patrón "detector vacío"
    que ya nos mordió con check_test_count).
    """
    malo = tmp_path / "malo.py"
    malo.write_text(
        "def f():\n    try:\n        pass\n    except Exception:\n        pass\n",
        encoding="utf-8",
    )
    assert _pass_sin_marcar(malo), "el detector no ve un except: pass plantado"

    bueno = tmp_path / "bueno.py"
    bueno.write_text(
        "def f():\n    try:\n        pass\n    except Exception:  # silent-ok: es una prueba\n        pass\n",
        encoding="utf-8",
    )
    assert not _pass_sin_marcar(bueno), "el detector rechaza un except ya justificado"

    con_cuerpo = tmp_path / "con_cuerpo.py"
    con_cuerpo.write_text(
        "def f():\n    try:\n        pass\n    except Exception:\n        print('x')\n",
        encoding="utf-8",
    )
    assert not _pass_sin_marcar(con_cuerpo), "un except con cuerpo no es silencio"
