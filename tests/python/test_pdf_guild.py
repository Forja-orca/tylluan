"""Tests del guild `pdf` (guilds/scholars/plugins/pdf.py), fusionado en su sitio.

No hay binarios en el repo: el PDF con texto se construye a mano (bytes minimos con un objeto de
texto) y el de render con `pypdf` (pagina en blanco).
"""
import importlib
import os
import sys

import pytest

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if RAIZ not in sys.path:
    sys.path.insert(0, RAIZ)


def _pdf_con_texto(texto: str = "Hola Tylluan") -> bytes:
    contenido = "BT /F1 24 Tf 72 720 Td (%s) Tj ET" % texto
    objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        "/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        "<< /Length %d >>\nstream\n%s\nendstream" % (len(contenido), contenido),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    salida = b"%PDF-1.4\n"
    offsets = []
    for i, cuerpo in enumerate(objs, start=1):
        offsets.append(len(salida))
        salida += ("%d 0 obj\n%s\nendobj\n" % (i, cuerpo)).encode("latin-1")
    xref = len(salida)
    salida += ("xref\n0 %d\n" % (len(objs) + 1)).encode()
    salida += b"0000000000 65535 f \n"
    for off in offsets:
        salida += ("%010d 00000 n \n" % off).encode()
    salida += ("trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
               % (len(objs) + 1, xref)).encode()
    return salida


@pytest.fixture()
def entorno(tmp_path, monkeypatch):
    pdf = tmp_path / "muestra.pdf"
    pdf.write_bytes(_pdf_con_texto())
    monkeypatch.setenv("TYLLUAN_PDF_ROOTS", str(tmp_path))
    monkeypatch.delenv("TYLLUAN_DATA_DIR", raising=False)
    import guilds.scholars.plugins.pdf as g

    importlib.reload(g)
    return g, tmp_path, pdf


def test_pdf_info_lee_paginas_y_bytes(entorno):
    g, _tmp, pdf = entorno
    info = g.pdf_info(str(pdf))
    assert info["paginas"] == 1
    assert info["bytes"] == pdf.stat().st_size
    assert info["cifrado"] is False


def test_pdf_text_devuelve_el_texto(entorno):
    g, _tmp, pdf = entorno
    r = g.pdf_text(str(pdf))
    assert "Hola Tylluan" in r["texto"]
    assert r["desde"] == 1 and r["hasta"] == 1


def test_pdf_extract_text_alias_compatible(entorno):
    g, _tmp, pdf = entorno
    r = g.pdf_extract_text(str(pdf), 1, 1)
    assert "Hola Tylluan" in r["texto"]


def test_pdf_search_encuentra_y_da_contexto(entorno):
    g, _tmp, pdf = entorno
    r = g.pdf_search(str(pdf), "tylluan")
    assert r["total"] >= 1
    assert r["hits"][0]["pagina"] == 1


def test_pdf_search_aguja_vacia_es_error(entorno):
    g, _tmp, pdf = entorno
    with pytest.raises(g.PdfError):
        g.pdf_search(str(pdf), "   ")


def test_ruta_fuera_de_raices_se_deniega(entorno, tmp_path):
    """MUTACION: si se desactiva la comprobacion de raices, este test falla."""
    g, _tmp, pdf = entorno
    fuera = tmp_path.parent / "fuera-del-permitido.pdf"
    fuera.write_bytes(pdf.read_bytes())
    try:
        with pytest.raises(g.PdfError) as exc:
            g.pdf_info(str(fuera))
        assert "fuera de las raices" in str(exc.value)
    finally:
        fuera.unlink(missing_ok=True)


def test_sin_raices_configuradas_no_se_autoriza_nada(tmp_path, monkeypatch):
    """Sin TYLLUAN_PDF_ROOTS ni DATA_DIR no se autoriza ni el cwd ni el HOME."""
    monkeypatch.delenv("TYLLUAN_PDF_ROOTS", raising=False)
    monkeypatch.delenv("TYLLUAN_DATA_DIR", raising=False)
    import guilds.scholars.plugins.pdf as g

    importlib.reload(g)
    pdf = tmp_path / "x.pdf"
    pdf.write_bytes(_pdf_con_texto())
    with pytest.raises(g.PdfError) as exc:
        g.pdf_info(str(pdf))
    assert "raices permitidas" in str(exc.value)


def test_pdf_demasiado_grande_es_error(entorno, monkeypatch):
    """MUTACION: con MAX_BYTES desactivado, este test falla."""
    g, _tmp, pdf = entorno
    monkeypatch.setattr(g, "MAX_BYTES", 10)
    with pytest.raises(g.PdfError) as exc:
        g.pdf_info(str(pdf))
    assert "demasiado grande" in str(exc.value)


def test_pdf_corrupto_da_error_explicito(entorno):
    g, tmp, _pdf = entorno
    roto = tmp / "roto.pdf"
    roto.write_bytes(b"esto no es un PDF")
    with pytest.raises(g.PdfError) as exc:
        g.pdf_info(str(roto))
    assert "corrupto o ilegible" in str(exc.value)


def test_rango_de_paginas_invalido_es_error(entorno):
    g, _tmp, pdf = entorno
    with pytest.raises(g.PdfError):
        g.pdf_text(str(pdf), desde=0)
    with pytest.raises(g.PdfError):
        g.pdf_text(str(pdf), desde=5, hasta=9)


def test_pdf_render_png_dentro_de_raices(entorno):
    g, tmp, _pdf = entorno
    from pypdf import PdfWriter

    w = PdfWriter()
    w.add_blank_page(width=200, height=120)
    origen = tmp / "blanco.pdf"
    with open(origen, "wb") as f:
        w.write(f)
    salida = tmp / "pagina1.png"
    r = g.pdf_render(str(origen), 1, str(salida), escala=1.0)
    assert salida.exists() and salida.stat().st_size > 0
    assert r["bytes"] > 0


def test_pdf_render_salida_fuera_de_raices_se_deniega(entorno, tmp_path):
    g, tmp, _pdf = entorno
    from pypdf import PdfWriter

    w = PdfWriter()
    w.add_blank_page(width=100, height=100)
    origen = tmp / "blanco2.pdf"
    with open(origen, "wb") as f:
        w.write(f)
    with pytest.raises(g.PdfError):
        g.pdf_render(str(origen), 1, str(tmp_path.parent / "fuera.png"))


def test_pdf_merge_valida_salida(entorno, tmp_path):
    g, tmp, pdf = entorno
    with pytest.raises(g.PdfError):
        g.pdf_merge([str(pdf)], str(tmp_path.parent / "fuera.pdf"))
