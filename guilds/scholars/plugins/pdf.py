"""
TylluanNexus Scholar Guild — PDF Tools (nativo; sustituye al MCP externo `pdf-viewer`).

Orden de Jose (2026-11-21): fuera los servidores MCP de terceros. Esto es lo nativo.

**Alcance**: leer. Nada de firmar, rellenar ni anotar (acciones con efectos, fuera del MVP).
`pdf_merge` se conserva porque ya existia desde v0.1.0, pero es la unica tool que escribe, valida
la salida igual que la entrada y queda marcada como tal.

Reglas que este modulo NO se salta (D4959, 2026-11-21):
- **Rutas**: solo dentro de raices permitidas, resueltas con `utils.validate_path` (dispositivos de
  Windows y UNC sin tocar la red). **Nunca** `~` expandido ni cwd: el hallazgo abierto de
  `builders/plugins/database.py:46-47` (autoriza HOME y arbol de trabajo) no se repite aqui. Las
  raices se declaran en `TYLLUAN_PDF_ROOTS` (separadas por `;`); por defecto, solo el DATA_DIR.
- **Topes**: tamano, paginas por llamada, caracteres, pixeles al renderizar.
- **Errores explicitos**: se lanza `PdfError` (FastMCP lo convierte en error de tool). Antes se
  devolvia un string "❌ …", que es un fallo silencioso disfrazado de respuesta.

Tools: `pdf_info`, `pdf_text` (alias `pdf_extract_text`), `pdf_search`, `pdf_render`, `pdf_merge`.
"""
from __future__ import annotations

import os
from pathlib import Path
from typing import Optional

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-pdf")

# ── Topes (mutables desde los tests: desactivarlos tiene que romper uno) ───────────────────
MAX_BYTES = 25 * 1024 * 1024          # 25 MB por fichero
MAX_PAGES_PER_CALL = 50               # paginas por llamada de texto
MAX_CHARS = 200_000                   # caracteres devueltos
MAX_RENDER_PIXELS = 4_000_000         # ~2000x2000 px
MAX_PAGES_SEARCH = 500                # HALLAZGO 3 (TL, T4970): `pdf_search` recorria TODAS las
                                      # paginas sin tope; 25 MB pueden ser miles y el tiempo no
                                      # estaba acotado.
RENDER_SCALES = (1.0, 1.5, 2.0, 3.0)  # 1.0 = 72 dpi


class PdfError(Exception):
    """Error explicito con el motivo. Nunca se devuelve una respuesta vacia."""


def _sin_prefijo_extendido(ruta: str) -> str:
    r"""Quita el prefijo de ruta extendida de Windows (``\\?\``).

    H-HER-A-01 (Hermes, 2026-11-22; confirmado en vivo por el TL tras el reinicio): el kernel
    resuelve la raiz del gremio con `std::fs::canonicalize`, que en Windows devuelve
    ``\\?\E:\...``. Ese valor acaba en `TYLLUAN_ROOT` y en `PYTHONPATH` (y por tanto en
    `__file__`). `validate_path` compara con `is_relative_to` DESPUES de `resolve()`, y
    ``\\?\E:\x`` no es relativo a ``E:\x``: medido, raiz con prefijo + objetivo absoluto normal
    -> False, o sea **se denegaba toda ruta absoluta**.

    Se recorta solo lo que devuelve `canonicalize`: el prefijo verbatim de una ruta local
    (``\\?\X:``) y el de una UNC (``\\?\UNC\host\share`` -> ``\\host\share``).
    """
    if not ruta.startswith("\\\\?\\"):
        return ruta
    resto = ruta[4:]
    if resto.upper().startswith("UNC\\"):
        return "\\\\" + resto[4:]
    return resto


def _raiz_desde_modulo(ruta_modulo: str) -> Path | None:
    r"""`<repo>/data` a partir de la ruta del propio modulo, sin depender del entorno.

    Separada de `_raices_permitidas()` para poder probarla con una ruta de modulo **prefijada**
    tal cual la entrega Python cuando `PYTHONPATH` lleva ``\\?\``.
    """
    try:
        repo = Path(_sin_prefijo_extendido(ruta_modulo)).resolve().parents[3]
        return (repo / "data").resolve()
    except (IndexError, OSError):
        return None


def _raices_permitidas() -> list[Path]:
    """Raices explicitas. Sin `~`, sin cwd por defecto (ver database.py:46-47).

    H-HER-M-05 (Hermes, 2026-11-21): en produccion esta funcion devolvia `[]` y el gremio no podia
    abrir NINGUN PDF. Causa: el spawn de guilds (`guild_process.rs:113-132`) solo hereda una lista
    blanca de variables y **ni `TYLLUAN_PDF_ROOTS` ni `TYLLUAN_DATA_DIR` estan en ella**, asi que
    las dos ramas de arriba nunca se cumplian. Las dos ramas siguientes no dependen de variables
    que el spawn no pase: `TYLLUAN_ROOT` SI viaja (esta en la lista EXACT) y, si tampoco estuviera,
    la posicion del PROPIO MODULO (`<repo>/guilds/scholars/plugins/pdf.py` -> `<repo>/data`) sigue
    dando una raiz valida. Todas las rutas se normalizan con `_sin_prefijo_extendido` (H-HER-A-01).
    """
    crudo = os.environ.get("TYLLUAN_PDF_ROOTS", "").strip()
    if crudo:
        return [Path(_sin_prefijo_extendido(p)).resolve() for p in crudo.split(";") if p.strip()]
    data_dir = os.environ.get("TYLLUAN_DATA_DIR", "").strip()
    if data_dir:
        return [Path(_sin_prefijo_extendido(data_dir)).resolve()]
    # TYLLUAN_ROOT lo inyecta el kernel siempre (con `\\?\` hasta que se arregle H-HER-A-01).
    root = os.environ.get("TYLLUAN_ROOT", "").strip()
    if root:
        return [(Path(_sin_prefijo_extendido(root)) / "data").resolve()]
    # `<repo>/guilds/scholars/plugins/pdf.py` -> parents[0]=plugins [1]=scholars [2]=guilds [3]=repo
    # Se devuelve aunque `data/` no exista todavia: `validate_path` resuelve lexicamente y el
    # fallo correcto entonces es «no existe el fichero», no «no hay raices» (que es lo que hacia
    # el gremio inutilizable). En un checkout limpio `data/` esta gitignored y no existe.
    por_modulo = _raiz_desde_modulo(__file__)
    return [por_modulo] if por_modulo else []


def _permitido(ruta: str) -> Path:
    """Ruta resuelta si cae dentro de una raiz permitida; si no, PdfError."""
    if not ruta or not str(ruta).strip():
        raise PdfError("ruta vacia")
    candidata = Path(str(ruta).strip())
    raices = _raices_permitidas()
    if not raices:
        raise PdfError(
            "no hay raices permitidas: define TYLLUAN_PDF_ROOTS (separadas por ';') "
            "o arranca con TYLLUAN_DATA_DIR; no se autoriza el HOME ni el cwd"
        )
    for raiz in raices:
        if utils.validate_path(str(raiz), str(candidata)):
            return Path(os.path.abspath(str(candidata))).resolve()
    raise PdfError(
        "ruta fuera de las raices permitidas: %s (permitidas: %s)"
        % (candidata, ", ".join(str(r) for r in raices))
    )


def _abrir(ruta: str):
    from pypdf import PdfReader

    p = _permitido(ruta)
    if not p.is_file():
        raise PdfError("no existe o no es un fichero: %s" % p)
    tam = p.stat().st_size
    if tam > MAX_BYTES:
        raise PdfError("PDF demasiado grande: %d bytes > tope %d (MAX_BYTES)" % (tam, MAX_BYTES))
    try:
        lector = PdfReader(str(p))
    except Exception as exc:
        raise PdfError("PDF corrupto o ilegible (%s): %s" % (p.name, exc)) from exc
    if lector.is_encrypted:
        try:
            desbloqueado = lector.decrypt("")
        except Exception as exc:
            raise PdfError("PDF cifrado: no se puede leer sin contrasena (%s)" % exc) from exc
        if not desbloqueado:
            raise PdfError("PDF cifrado con contrasena: no se descifra sin ella")
    return p, lector


def _rango(total: int, desde: int, hasta: int) -> tuple[int, int]:
    if desde < 1 or hasta < 1:
        raise PdfError("las paginas empiezan en 1 (pedido %d..%d)" % (desde, hasta))
    if desde > total:
        raise PdfError("pagina inicial %d fuera del documento (%d paginas)" % (desde, total))
    hasta = min(hasta, total)
    if hasta < desde:
        raise PdfError("rango invalido: %d..%d" % (desde, hasta))
    if hasta - desde + 1 > MAX_PAGES_PER_CALL:
        hasta = desde + MAX_PAGES_PER_CALL - 1
    return desde, hasta


@mcp.tool()
def pdf_info(ruta: str) -> dict:
    """Paginas, bytes, metadatos y si esta cifrado. Un PDF ilegible da error."""
    p, lector = _abrir(ruta)
    meta = {}
    try:
        for k, v in (lector.metadata or {}).items():
            if v is not None:
                meta[str(k)] = str(v)
    except Exception:
        meta = {}
    return {
        "ruta": str(p),
        "bytes": p.stat().st_size,
        "paginas": len(lector.pages),
        "cifrado": bool(lector.is_encrypted),
        "metadatos": meta,
    }


@mcp.tool()
def pdf_text(ruta: str, desde: int = 1, hasta: int = 0) -> dict:
    """Texto de un rango (1-based, inclusivo) con tope de paginas y de caracteres."""
    _p, lector = _abrir(ruta)
    total = len(lector.pages)
    if total == 0:
        raise PdfError("el PDF no tiene paginas")
    if hasta == 0:
        hasta = total
    d, h = _rango(total, desde, hasta)
    trozos: list[str] = []
    usado = 0
    ultima = d - 1
    for n in range(d, h + 1):
        try:
            texto = lector.pages[n - 1].extract_text() or ""
        except Exception as exc:
            raise PdfError("no se pudo extraer texto de la pagina %d: %s" % (n, exc)) from exc
        if usado + len(texto) > MAX_CHARS:
            trozos.append(texto[: max(0, MAX_CHARS - usado)])
            ultima = n
            break
        trozos.append(texto)
        usado += len(texto)
        ultima = n
    return {
        "paginas_documento": total,
        "desde": d,
        "hasta": ultima,
        "truncado": ultima < h or usado >= MAX_CHARS,
        "texto": "\n".join(trozos)[:MAX_CHARS],
    }


@mcp.tool()
def pdf_search(ruta: str, aguja: str, max_hits: int = 20) -> dict:
    """Busca texto (sin distinguir mayusculas): pagina y contexto de cada coincidencia."""
    if not aguja or not aguja.strip():
        raise PdfError("aguja vacia")
    if max_hits < 1 or max_hits > 200:
        raise PdfError("max_hits debe estar entre 1 y 200 (pedido %d)" % max_hits)
    _p, lector = _abrir(ruta)
    objetivo = aguja.lower()
    hits: list[dict] = []
    exploradas = 0
    for n, pagina in enumerate(lector.pages, start=1):
        if n > MAX_PAGES_SEARCH:
            break
        exploradas = n
        try:
            texto = (pagina.extract_text() or "").lower()
        except Exception:
            continue
        pos = texto.find(objetivo)
        while pos >= 0 and len(hits) < max_hits:
            hits.append({
                "pagina": n,
                "contexto": texto[max(0, pos - 40):pos + len(objetivo) + 40].strip(),
            })
            pos = texto.find(objetivo, pos + len(objetivo))
        if len(hits) >= max_hits:
            break
    return {
        "aguja": aguja,
        "hits": hits,
        "total": len(hits),
        "tope_alcanzado": len(hits) >= max_hits,
        "paginas_exploradas": exploradas,
        "truncado": exploradas >= MAX_PAGES_SEARCH and len(hits) < max_hits,
    }


@mcp.tool()
def pdf_render(ruta: str, pagina: int, salida: str, escala: float = 1.5) -> dict:
    """Renderiza UNA pagina a PNG. La salida tambien tiene que caer dentro de las raices."""
    _p, lector = _abrir(ruta)
    total = len(lector.pages)
    if pagina < 1 or pagina > total:
        raise PdfError("pagina %d fuera del documento (%d paginas)" % (pagina, total))
    if escala not in RENDER_SCALES:
        raise PdfError("escala %s no permitida (usa %s)" % (escala, list(RENDER_SCALES)))
    destino = _permitido(salida)
    if destino.suffix.lower() != ".png":
        raise PdfError("la salida tiene que ser .png (pedido %s)" % destino.suffix)
    if destino.exists():
        raise PdfError("la salida ya existe (no se sobrescribe): %s" % destino)
    import pypdfium2 as pdfium

    doc = pdfium.PdfDocument(str(_permitido(ruta)))
    try:
        pag = doc[pagina - 1]
        # HALLAZGO 1 (TL, T4970): el tope se comprobaba DESPUES de renderizar -> un PDF de
        # 433 bytes con una pagina de 3000x3000 pt a escala 3 llegaba a 364 MB de pico (y
        # 14400 pt darian ~7,5 GB) antes de lanzar el error. Se decide con el TAMANO DE
        # PAGINA, que no asigna bitmap:
        ancho_pt, alto_pt = pag.get_size()
        px_w = int(ancho_pt * escala)
        px_h = int(alto_pt * escala)
        if px_w * px_h > MAX_RENDER_PIXELS:
            raise PdfError(
                "render demasiado grande: %dx%d px > tope %d (MAX_RENDER_PIXELS); "
                "pagina de %.0fx%.0f pt a escala %s -- se rechaza ANTES de reservar memoria"
                % (px_w, px_h, MAX_RENDER_PIXELS, ancho_pt, alto_pt, escala)
            )
        bitmap = pag.render(scale=escala)
        bitmap.to_pil().save(str(destino), format="PNG")
    finally:
        doc.close()
    return {"png": str(destino), "pagina": pagina, "escala": escala, "bytes": destino.stat().st_size}


@mcp.tool()
def pdf_merge(rutas: list[str], salida: str) -> dict:
    """Une varios PDF en uno. **Unica tool que escribe**: valida entrada y salida."""
    from pypdf import PdfWriter

    if not rutas:
        raise PdfError("lista de PDF vacia")
    destinos = [_permitido(r) for r in rutas]
    out = _permitido(salida)
    if out.suffix.lower() != ".pdf":
        raise PdfError("la salida tiene que ser .pdf (pedido %s)" % out.suffix)
    if out.exists():
        raise PdfError("la salida ya existe (no se sobrescribe): %s" % out)
    w = PdfWriter()
    for d in destinos:
        if not d.is_file():
            raise PdfError("no existe: %s" % d)
        try:
            w.append(str(d))
        except Exception as exc:
            raise PdfError("no se pudo leer %s: %s" % (d.name, exc)) from exc
    with open(out, "wb") as f:
        w.write(f)
    return {"pdf": str(out), "origenes": len(destinos), "bytes": out.stat().st_size}


# Compatibilidad: el nombre viejo (v0.1.0) apunta a la version 1-based.
@mcp.tool()
def pdf_extract_text(ruta: str, desde: int = 1, hasta: int = 0) -> dict:
    """Alias de `pdf_text` (mismo contrato: 1-based, inclusivo)."""
    return pdf_text(ruta, desde, hasta)


if __name__ == "__main__":  # pragma: no cover
    utils.safe_mcp_run(mcp)
