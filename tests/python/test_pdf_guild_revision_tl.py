"""Tests añadidos tras la revisión del TL (T4970): hallazgos 1, 2 y 3."""
import importlib
import os
import sys

import pytest

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if RAIZ not in sys.path:
    sys.path.insert(0, RAIZ)


def _pdf_pagina(pt: int, texto: str = "Hola") -> bytes:
    """PDF minimo con UNA pagina de `pt` x `pt` puntos (y poco peso: sirve de bomba)."""
    contenido = "BT /F1 24 Tf 10 10 Td (%s) Tj ET" % texto
    objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %d %d] "
        "/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>" % (pt, pt),
        "<< /Length %d >>\nstream\n%s\nendstream" % (len(contenido), contenido),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    salida = b"%PDF-1.4\n"
    offs = []
    for i, c in enumerate(objs, start=1):
        offs.append(len(salida))
        salida += ("%d 0 obj\n%s\nendobj\n" % (i, c)).encode("latin-1")
    xref = len(salida)
    salida += ("xref\n0 %d\n" % (len(objs) + 1)).encode() + b"0000000000 65535 f \n"
    for o in offs:
        salida += ("%010d 00000 n \n" % o).encode()
    salida += ("trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
               % (len(objs) + 1, xref)).encode()
    return salida


@pytest.fixture()
def entorno(tmp_path, monkeypatch):
    monkeypatch.setenv("TYLLUAN_PDF_ROOTS", str(tmp_path))
    monkeypatch.delenv("TYLLUAN_DATA_DIR", raising=False)
    import guilds.scholars.plugins.pdf as g

    importlib.reload(g)
    return g, tmp_path


# ── HALLAZGO 1: el tope de pixeles se decide ANTES de renderizar ──────────────────────────
def test_render_rechaza_pagina_gigante_sin_reservar_memoria(entorno):
    """433 bytes con MediaBox 3000x3000 a escala 3 = 9000x9000 px: debe rechazarse ANTES.

    MUTACION: si se quita la comprobacion previa (y la posterior ya no esta), no hay error
    y este test se pone rojo. Con la version del TL (comprobar DESPUES) el pico era de 364 MB.
    """
    import time

    g, tmp = entorno
    origen = tmp / "bomba.pdf"
    origen.write_bytes(_pdf_pagina(3000))
    assert origen.stat().st_size < 1024, "el fichero tiene que ser diminuto para probar la bomba"
    salida = tmp / "bomba.png"
    t0 = time.time()
    with pytest.raises(g.PdfError) as exc:
        g.pdf_render(str(origen), 1, str(salida), escala=3.0)
    dt = time.time() - t0
    assert "MAX_RENDER_PIXELS" in str(exc.value)
    assert not salida.exists(), "no debe quedar PNG de un render rechazado"
    assert dt < 5, "el rechazo tiene que ser inmediato (sin reservar bitmap): %.1f s" % dt


def test_render_acepta_pagina_pequena(entorno):
    """Control: una pagina normal sigue renderizando."""
    g, tmp = entorno
    origen = tmp / "normal.pdf"
    origen.write_bytes(_pdf_pagina(100))
    salida = tmp / "normal.png"
    r = g.pdf_render(str(origen), 1, str(salida), escala=1.0)
    assert salida.exists() and r["bytes"] > 0


def test_render_gigante_no_sube_la_memoria_del_proceso(entorno):
    """Medido, no asumido: con la proteccion el PICO de RSS es ~17 MB; sin ella, ~544 MB.

    `MAX_RENDER_PIXELS` enorme simula la version anterior (comprobar despues de renderizar):
    el pico se dispara y este test se pone rojo por el umbral de memoria, no por la excepcion.
    """
    import threading
    import time

    import psutil

    g, tmp = entorno
    origen = tmp / "bomba-mem.pdf"
    origen.write_bytes(_pdf_pagina(3000))
    proc = psutil.Process()

    def pico_rss(fn):
        base = proc.memory_info().rss
        pico = [base]
        parar = threading.Event()

        def muestrear():
            while not parar.is_set():
                r = proc.memory_info().rss
                if r > pico[0]:
                    pico[0] = r
                time.sleep(0.005)

        hilo = threading.Thread(target=muestrear, daemon=True)
        hilo.start()
        try:
            fn()
        finally:
            parar.set()
            hilo.join(timeout=1)
        return (pico[0] - base) / 1048576

    def intento():
        try:
            g.pdf_render(str(origen), 1, str(tmp / "bomba-mem.png"), escala=3.0)
        except g.PdfError:
            pass

    subida = pico_rss(intento)
    assert subida < 120, (
        "el render rechazado subio el RSS %.0f MB (esperado <120): la comprobacion tiene que "
        "hacerse ANTES de reservar el bitmap" % subida
    )
    assert not (tmp / "bomba-mem.png").exists()


# ── HALLAZGO 2: un test por tope/regla que faltaba ───────────────────────────────────────
def test_pdf_cifrado_da_error_explicito(entorno):
    """MUTACION: si se quita la rama is_encrypted, un PDF con contrasena se lee como si nada."""
    from pypdf import PdfWriter

    g, tmp = entorno
    w = PdfWriter()
    w.add_blank_page(width=100, height=100)
    w.encrypt("secreto")
    cifrado = tmp / "cifrado.pdf"
    with open(cifrado, "wb") as f:
        w.write(f)
    with pytest.raises(g.PdfError) as exc:
        g.pdf_info(str(cifrado))
    assert "cifrado" in str(exc.value).lower()


def test_enlace_simbolico_fuera_de_raiz_se_deniega(entorno, tmp_path):
    """MUTACION: si se desactiva validate_path, el enlace deja leer fuera de la raiz.

    En Windows `os.symlink` pide privilegio; si no lo hay, se usa un **junction** de
    directorio (`mklink /J`), que no lo necesita y ejercita la misma resolucion de rutas.
    """
    import subprocess

    g, tmp = entorno
    fuera_dir = tmp_path.parent / "fuera-dir-tl"
    fuera_dir.mkdir(exist_ok=True)
    (fuera_dir / "secreto.pdf").write_bytes(_pdf_pagina(100))

    enlace = tmp / "enlace.pdf"
    creado = "symlink"
    try:
        os.symlink(fuera_dir / "secreto.pdf", enlace)
    except (OSError, NotImplementedError):
        creado = ""
        junction = tmp / "enlace-dir"
        r = subprocess.run(["cmd", "/c", "mklink", "/J", str(junction), str(fuera_dir)],
                           capture_output=True, text=True)
        if r.returncode == 0 and junction.exists():
            creado = "junction"
            enlace = junction / "secreto.pdf"
    if not creado:
        pytest.skip("ni symlink ni junction disponibles en este entorno")
    try:
        with pytest.raises(g.PdfError):
            g.pdf_info(str(enlace))
    finally:
        if creado == "junction":
            subprocess.run(["cmd", "/c", "rmdir", str(enlace.parent)], capture_output=True)
        else:
            enlace.unlink(missing_ok=True)
        (fuera_dir / "secreto.pdf").unlink(missing_ok=True)
        fuera_dir.rmdir() if fuera_dir.exists() and not any(fuera_dir.iterdir()) else None


def test_pdf_merge_salida_existente_se_deniega(entorno):
    """MUTACION: sin la comprobacion de existencia, sobrescribe un fichero del usuario."""
    g, tmp = entorno
    a = tmp / "a.pdf"
    a.write_bytes(_pdf_pagina(100))
    salida = tmp / "salida.pdf"
    salida.write_bytes(b"ya estoy aqui")
    with pytest.raises(g.PdfError) as exc:
        g.pdf_merge([str(a)], str(salida))
    assert "ya existe" in str(exc.value)
    assert salida.read_bytes() == b"ya estoy aqui", "no debe haberse tocado"


def test_pdf_merge_une_dos_pdfs(entorno):
    """Camino feliz de la unica tool que escribe."""
    from pypdf import PdfReader

    g, tmp = entorno
    a = tmp / "a.pdf"
    b = tmp / "b.pdf"
    a.write_bytes(_pdf_pagina(100, "uno"))
    b.write_bytes(_pdf_pagina(100, "dos"))
    salida = tmp / "unido.pdf"
    r = g.pdf_merge([str(a), str(b)], str(salida))
    assert salida.exists() and r["origenes"] == 2
    assert len(PdfReader(str(salida)).pages) == 2


# ── HALLAZGO 3: pdf_search con tope de paginas exploradas ────────────────────────────────
def test_pdf_search_tope_de_paginas_exploradas(entorno, monkeypatch):
    """MUTACION: sin MAX_PAGES_SEARCH explora todas y `truncado` nunca se pone."""
    from pypdf import PdfWriter

    g, tmp = entorno
    w = PdfWriter()
    for _ in range(4):
        w.add_blank_page(width=100, height=100)
    muchas = tmp / "cuatro.pdf"
    with open(muchas, "wb") as f:
        w.write(f)
    monkeypatch.setattr(g, "MAX_PAGES_SEARCH", 2)
    r = g.pdf_search(str(muchas), "no-existe-en-ninguna")
    assert r["paginas_exploradas"] == 2
    assert r["truncado"] is True
    assert r["total"] == 0
