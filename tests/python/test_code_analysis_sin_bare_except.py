"""D-145 (2026-11-07): `code_analysis.py` sin `except:` desnudos, con prueba de que no traga BaseException.

El fallo tal cual: el fichero tenia CUATRO `except:` desnudos (lineas 208, 248, 266, 282 sobre
abe6ebf9) que se comian KeyboardInterrupt y SystemExit (BaseException) junto con todo lo demas:
un Ctrl+C dentro del bucle de ficheros o un sys.exit() del interprete quedaban en `continue`/`pass`
y la herramienta seguia como si nada. Qedan sustituidos por excepciones concretas
(OSError / subprocess.SubprocessError / SyntaxError / ValueError / UnicodeDecodeError), cada una
justificada en su sitio.

Que vigila este test:
1) AST: 0 ExceptHandler sin tipo en TODO el fichero (falla si alguien reintroduce un `except:`).
2) Runtime: si el `open` de la herramienta lanza KeyboardInterrupt o SystemExit, la excepcion
   PROPAGA. Con los `except:` viejos estas dos pruebas daban rojo (quedaban tragadas).
3) Comportamiento preservado: un fichero ilegible (OSError al abrir) sigue saltandose en silencio
   sin tumbar la herramienta.

Las dos de runtime se skippean si el interprete no tiene mcp 1.x (el modulo define las tools solo
si `from mcp.server.fastmcp import FastMCP` funciona). El pin del repo es `mcp>=1.27.0,<2`
(guilds/requirements.txt:14) y CI lo instala via guilds/requirements-hashes.txt: ahi corren de
verdad. En interpretes con mcp 2.x (sin FastMCP) se skippean y solo valen las dos primeras.
"""

import ast
import asyncio
import builtins
import pathlib
import subprocess
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))
ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
TARGET = ROOT / "guilds" / "builders" / "plugins" / "code_analysis.py"


def _tree():
    return ast.parse(TARGET.read_text(encoding="utf-8"))


def test_d145_cero_excepts_desnudos_en_code_analysis():
    """Regresion directa: ningún ExceptHandler sin tipo en el fichero entero."""
    bare = [n.lineno for n in ast.walk(_tree())
            if isinstance(n, ast.ExceptHandler) and n.type is None]
    assert bare == [], f"`except:` desnudo reaparece en las lineas {bare}"


def test_d145_ningun_handler_caza_BaseException():
    """Ni `except Exception` ni nombres concretos: ningun handler del fichero puede tragarse
    KeyboardInterrupt/SystemExit porque caza BaseException explicitamente."""
    for h in (n for n in ast.walk(_tree()) if isinstance(n, ast.ExceptHandler)):
        names, stack = [], [h.type]
        while stack:
            e = stack.pop()
            if isinstance(e, ast.Name):
                names.append(e.id)
            elif isinstance(e, ast.Tuple):
                stack.extend(e.elts)
        assert "BaseException" not in names, f"handler linea {h.lineno} caza BaseException explicitamente"


def _load_guild():
    sys.path.insert(0, str(ROOT))
    from guilds.builders.plugins import code_analysis as ca
    return ca


_ca = _load_guild()
_need_mcp = pytest.mark.skipif(
    not hasattr(_ca, "search_codebase"),
    reason="las tools solo se definen con mcp 1.x (FastMCP); en interpretes con mcp 2.x se skippean (ver docstring)",
)


def _sin_rg(monkeypatch):
    """search_codebase intenta primero ripgrep (`rg` esta en el PATH local y en los
    runners de CI, y ahi NO toca el fallback con el try parcheado). Forzar el camino
    del fallback como si rg no existiera: el propio codigo trata FileNotFoundError con
    `except FileNotFoundError: pass` (linea ~180) y cae al bucle de ficheros."""
    def fake_run(*a, **k):
        raise FileNotFoundError(2, "rg no instalado")
    monkeypatch.setattr(subprocess, "run", fake_run)


@_need_mcp
def test_d145_keyboardinterrupt_no_queda_tragada_en_search_codebase(tmp_path, monkeypatch):
    _sin_rg(monkeypatch)
    (tmp_path / "a.py").write_text("x = 1\n", encoding="utf-8")
    real_open = builtins.open

    def boom(path, *a, **k):
        if str(path).endswith("a.py"):
            raise KeyboardInterrupt
        return real_open(path, *a, **k)

    monkeypatch.setattr(builtins, "open", boom)
    with pytest.raises(KeyboardInterrupt):
        asyncio.run(_ca.search_codebase("x", str(tmp_path)))


@_need_mcp
def test_d145_systemexit_no_queda_tragada_en_search_codebase(tmp_path, monkeypatch):
    _sin_rg(monkeypatch)
    (tmp_path / "a.py").write_text("x = 1\n", encoding="utf-8")
    real_open = builtins.open

    def boom(path, *a, **k):
        if str(path).endswith("a.py"):
            raise SystemExit(2)
        return real_open(path, *a, **k)

    monkeypatch.setattr(builtins, "open", boom)
    with pytest.raises(SystemExit):
        asyncio.run(_ca.search_codebase("x", str(tmp_path)))


@_need_mcp
def test_d145_fichero_ilegible_sigue_saltandose_sin_tumbar_la_herramienta(tmp_path, monkeypatch):
    _sin_rg(monkeypatch)
    # a.py legible y con coincidencia; b.py lanza OSError al abrirse -> sigue saltandose (comportamiento viejo)
    (tmp_path / "a.py").write_text("x = 1\n", encoding="utf-8")
    (tmp_path / "b.py").write_text("y = 2\n", encoding="utf-8")
    real_open = builtins.open

    def sometimes(path, *a, **k):
        if str(path).endswith("b.py"):
            raise OSError(13, "Permission denied")
        return real_open(path, *a, **k)

    monkeypatch.setattr(builtins, "open", sometimes)
    out = asyncio.run(_ca.search_codebase("x", str(tmp_path)))
    assert 'a.py"' in out  # el legible se encuentra (ruta rematada por a.py")
    assert "b.py" not in out  # el ilegible se salta sin error ni crash
