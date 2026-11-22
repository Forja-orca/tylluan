r"""Raiz permitida de los guilds que tocan el sistema de ficheros (tarea R1, TL 21-nov).

**Que se midio antes de escribir esto** (kernel de prueba en 47949, sin tocar produccion):
dentro de un guild vivo, `TYLLUAN_ROOT` **no falta**: el kernel la inyecta con **su propio
cwd, canonicalizado** (`\\?\D:\Temp\guild-root-probe`), y `os.getcwd()` del guild da
exactamente el mismo valor. O sea: el fallback a `os.getcwd()` **no se ejerce en el spawn
normal**, pero la raiz permitida **es la raiz del repo** (en produccion, la de Tylluan).

Por eso este helper:
1. **Comportamiento identico al de hoy por defecto**: devuelve `TYLLUAN_ROOT` si esta, y si
   no, `os.getcwd()`. Nada cambia si no se toca la variable de abajo.
2. **Fallo cerrado solo si se activa explicitamente** con `TYLLUAN_ROOT_REQUIRED=1`: en ese
   caso, si no hay raiz declarada, se niega a operar en vez de asumir el cwd. Es la opcion
   que deja preparada la decision de Jose sobre cual debe ser la raiz en produccion.
3. Un solo sitio donde se decide, para que `filesystem`, `bash` y `code` no tengan tres
   politicas distintas (hoy repetian `os.environ.get("TYLLUAN_ROOT", os.getcwd())` en 7 sitios).
"""

from __future__ import annotations

import os

VARIABLE = "TYLLUAN_ROOT"
EXIGIR_DECLARADA = "TYLLUAN_ROOT_REQUIRED"


class RaizNoDeclarada(RuntimeError):
    """Se pidio fallo cerrado y no hay raiz declarada."""


PREFIJO_EXTENDIDO = "\\\\?\\"


def normaliza_raiz(raiz: str) -> str:
    """Quita el prefijo extendido de Windows (`\\\\?\\`) de una raiz.

    H-HER-A-01 (confirmado en vivo por Hermes, 2026-11-22): `workspace_root` nace de
    `guilds_dir.canonicalize()` (`guild_process.rs:776`), y en Windows `canonicalize()`
    devuelve la ruta con el prefijo `\\\\?\\`. Ese prefijo llega al guild por
    `TYLLUAN_ROOT`; si la raiz lo lleva y el destino que valida `validate_path` no, la
    comparacion falla y el guild `filesystem` **deniega absolutamente todo**, incluido '.'.
    El prefijo no aporta nada a la validacion: se quita aqui, en un solo sitio.
    """
    if raiz.startswith(PREFIJO_EXTENDIDO):
        resto = raiz[len(PREFIJO_EXTENDIDO):]
        if resto.upper().startswith("UNC\\"):
            return "\\\\" + resto[4:]
        return resto
    return raiz


def raiz_permitida() -> str:
    """Raiz contra la que se valida cualquier ruta. Mismo valor que antes por defecto."""
    declarada = os.environ.get(VARIABLE)
    if declarada:
        return normaliza_raiz(declarada)
    if os.environ.get(EXIGIR_DECLARADA) == "1":
        raise RaizNoDeclarada(
            f"No hay raiz declarada: {VARIABLE} no esta definida y {EXIGIR_DECLARADA}=1. "
            f"Sin una raiz explicita no se asume el directorio de trabajo del kernel "
            f"({os.getcwd()}), porque en produccion es la raiz del repo (con data/, "
            f".tylluan-token e identity.key). Define {VARIABLE} o quita {EXIGIR_DECLARADA}."
        )
    return normaliza_raiz(os.getcwd())


def raiz_o_deniega() -> tuple[str | None, str | None]:
    """Raiz, o el mensaje de denegacion. Para usar DENTRO de cada llamada.

    Resolver esto por llamada (y no al importar) es lo que evita que el guild entero se
    caiga: se deniega la peticion concreta y el resto del guild sigue vivo.
    """
    try:
        return raiz_permitida(), None
    except RaizNoDeclarada as exc:
        return None, f"BLOCKED: {exc}"


def mensaje_raiz(raiz: str) -> str:
    """Texto para los BLOCKED: deja claro CUAL es la raiz que se esta aplicando."""
    return f"the allowed root '{raiz}' (from {VARIABLE} or the kernel's cwd)"


__all__ = [
    "VARIABLE",
    "EXIGIR_DECLARADA",
    "PREFIJO_EXTENDIDO",
    "RaizNoDeclarada",
    "normaliza_raiz",
    "raiz_permitida",
    "raiz_o_deniega",
    "mensaje_raiz",
]
