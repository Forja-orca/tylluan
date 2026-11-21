#!/usr/bin/env python3
"""Verificacion POST-publicacion de una release de Tylluan (solo lectura).

Que hace, en orden, y todo dentro de un directorio temporal propio:

1. Resuelve los assets de la release en GitHub (API publica, sin token).
2. Descarga el tarball de la plataforma actual y, si existe, `checksums.txt`.
3. Comprueba la suma SHA-256 del tarball contra ese manifiesto.
4. Extrae el tarball en un HOME temporal.
5. Comprueba que el paquete trae lo que promete: `guilds/` en la raiz y
   `requirements-hashes.txt`.
6. Arranca el kernel del paquete con **puerto propio** y `dev_mode = false`, con
   `DATA_DIR` dentro del temporal (nunca el de produccion).
7. Mide: `/health` responde 200, la version del cuerpo es la esperada y
   `tylluan-cli --version` dice la misma version.
8. Cierra el kernel y deja el directorio temporal en disco para inspeccion.

Reglas de seguridad que el script aplica por si mismo:
- **Nunca** arranca en el 47004 ni en el puerto de una instancia declarada: si el puerto
  pedido es 47004, aborta.
- **Borra su temporal si la verificacion termina SIN FAIL** (T4960): conserva la carpeta
  solo cuando hay FAIL (para poder inspeccionarla) y con `--conservar` siempre. Antes se
  conservaba todo y se acumularon 10 carpetas / 1,57 GB en %TEMP%.
- **Extrae el tarball con `filter="data"`** (Python >=3.12): el paquete que se verifica
  podria estar comprometido, asi que se rechazan rutas absolutas, `..` que salgan del
  destino y enlaces que apunten fuera.
- No toca `E:/tylluan` ni escribe fuera del temporal.

Uso:
    python scripts/verify_release.py --tag v0.17.3
    python scripts/verify_release.py --tag v0.17.2 --port 47931

Salida: una tabla de comprobaciones con PASS/FAIL/WARN y codigo de salida 1 si algo
falla de verdad (un WARN no tumba el codigo, pero se ve).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO = "Forja-orca/tylluan"
API = f"https://api.github.com/repos/{REPO}/releases/tags/{{tag}}"
UA = {"User-Agent": "tylluan-verify-release"}


class Falla(Exception):
    """Un fallo que invalida la verificacion (no un aviso)."""


def plataforma() -> str:
    maquina = platform.machine().lower()
    if sys.platform.startswith("win"):
        return "x86_64-pc-windows-msvc" if maquina in ("amd64", "x86_64") else f"{maquina}-pc-windows-msvc"
    if sys.platform == "darwin":
        return "aarch64-apple-darwin" if maquina in ("arm64", "aarch64") else "x86_64-apple-darwin"
    if maquina in ("aarch64", "arm64"):
        return "aarch64-unknown-linux-gnu"
    return "x86_64-unknown-linux-gnu"


def pide_json(url: str) -> dict:
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read().decode("utf-8"))


def descarga(url: str, destino: Path) -> None:
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=300) as r, open(destino, "wb") as f:
        shutil.copyfileobj(r, f)


def sha256(ruta: Path) -> str:
    h = hashlib.sha256()
    with open(ruta, "rb") as f:
        for bloque in iter(lambda: f.read(1 << 20), b""):
            h.update(bloque)
    return h.hexdigest()


def puerto_libre(preferido: int) -> int:
    for p in [preferido] + list(range(preferido + 1, preferido + 40)):
        if p == 47004:
            continue
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            if s.connect_ex(("127.0.0.1", p)) != 0:
                return p
    raise Falla("no encontre un puerto libre cerca del preferido")


def espera_health(port: int, limite_s: int = 90) -> dict:
    url = f"http://127.0.0.1:{port}/health"
    fin = time.time() + limite_s
    ultimo = ""
    while time.time() < fin:
        try:
            with urllib.request.urlopen(url, timeout=3) as r:
                cuerpo = json.loads(r.read().decode("utf-8"))
                return {"status": r.status, "body": cuerpo}
        except Exception as e:  # el kernel aun no escucha
            ultimo = str(e)
            time.sleep(1.0)
    raise Falla(f"/health no respondio en {limite_s}s ({ultimo})")


def extrae(tarball: Path, destino: Path) -> None:
    """Extrae el tarball en `destino` rechazando entradas que se salgan.

    `filter="data"` (Python >=3.12) rechaza rutas absolutas, entradas `..` que salgan del
    destino y enlaces que apunten fuera. Este script verifica una release que PODRIA estar
    comprometida (el `checksums.txt` sale del mismo origen), asi que el paquete no es de
    fiar por definicion: sin filtro, un `../escape.txt` escribiria fuera de `instalado/`.
    """
    with tarfile.open(tarball, "r:gz") as tf:
        tf.extractall(destino, filter="data")


def cerrar_temporal(trabajo: Path, fallos: int, conservar: bool) -> None:
    """Borra el directorio temporal si la verificacion termino SIN fallos.

    Motivo (TL, T4960): cada corrida dejaba una carpeta en %TEMP% y se acumularon
    10 carpetas / 1,57 GB. Se conserva solo lo que puede hacer falta: lo que fallo.

    Regla deliberadamente conservadora: **cualquier FAIL conserva la carpeta** (para
    poder mirar el log del kernel y los artefactos), y `--conservar` la conserva siempre.
    Si el borrado falla, se avisa pero NO se convierte en un fallo de la verificacion.
    """
    if conservar or fallos:
        motivo = "--conservar" if conservar else f"{fallos} FAIL"
        print(f"   temporal conservado en: {trabajo}  ({motivo})")
        return
    ultimo: OSError | None = None
    for intento in range(5):
        try:
            shutil.rmtree(trabajo)
            print(f"   temporal borrado (verificacion sin FAIL): {trabajo}")
            return
        except OSError as e:
            ultimo = e
            time.sleep(0.4 * (intento + 1))
    print(f"   WARN: no pude borrar {trabajo} tras 5 intentos: {ultimo}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Verificacion post-publicacion de una release de Tylluan")
    ap.add_argument("--tag", default="v0.17.3", help="etiqueta a verificar (por defecto v0.17.3)")
    ap.add_argument("--port", type=int, default=47930, help="puerto para el kernel de prueba (nunca 47004)")
    ap.add_argument("--platform", default=None, help="target del tarball (por defecto, el de esta maquina)")
    ap.add_argument("--timeout", type=int, default=90, help="segundos de espera para /health")
    ap.add_argument("--conservar", action="store_true",
                    help="deja el directorio temporal aunque la verificacion pase (util para inspeccionar)")
    args = ap.parse_args()

    if args.port == 47004:
        print("ABORTADO: ese es el puerto de produccion; este script nunca lo usa")
        return 2

    version = args.tag.lstrip("v")
    target = args.platform or plataforma()
    archivo = f"tylluan-{target}.tar.gz"
    filas: list[tuple[str, str, str]] = []
    fallos = 0

    def anota(nombre: str, estado: str, detalle: str = "") -> None:
        nonlocal fallos
        if estado == "FAIL":
            fallos += 1
        filas.append((nombre, estado, detalle))
        print(f"[{estado:4}] {nombre}: {detalle}")

    print(f"== verificando {args.tag} para {target}")
    trabajo = Path(tempfile.mkdtemp(prefix=f"tylluan-verify-{version}-"))
    print(f"   temporal: {trabajo}")

    # 1) assets
    try:
        rel = pide_json(API.format(tag=args.tag))
    except urllib.error.HTTPError as e:
        print(f"FAIL: no pude leer la release {args.tag} en GitHub ({e.code})")
        fallos += 1
        cerrar_temporal(trabajo, fallos, args.conservar)
        return 1
    assets = {a["name"]: a for a in rel.get("assets", [])}
    anota("release existe", "PASS", f"{args.tag} con {len(assets)} assets")
    if archivo not in assets:
        anota("tarball de la plataforma", "FAIL", f"{archivo} no esta entre {sorted(assets)}")
        cerrar_temporal(trabajo, fallos, args.conservar)
        return 1
    anota("tarball de la plataforma", "PASS", archivo)

    # 2) descargas
    tarball = trabajo / archivo
    descarga(assets[archivo]["browser_download_url"], tarball)
    anota("descarga del tarball", "PASS", f"{tarball.stat().st_size} B")

    manifiesto = trabajo / "checksums.txt"
    if "checksums.txt" in assets:
        descarga(assets["checksums.txt"]["browser_download_url"], manifiesto)
        # 3) suma
        esperado = None
        for linea in manifiesto.read_text(encoding="utf-8", errors="replace").splitlines():
            partes = linea.split()
            if len(partes) >= 2 and partes[1].lstrip("*") == archivo:
                esperado = partes[0].lower()
                break
        real = sha256(tarball)
        if esperado is None:
            anota("suma del tarball", "FAIL", f"{archivo} no figura en checksums.txt")
        elif esperado == real:
            anota("suma del tarball", "PASS", f"sha256 {real}")
        else:
            anota("suma del tarball", "FAIL", f"esperado {esperado}, obtenido {real}")
    else:
        # Hallazgo real y citable: el asset de nivel superior lo empezo a publicar el
        # workflow con AUD-01 (2026-10-28), asi que las releases anteriores no lo tienen.
        anota(
            "checksums.txt de la release",
            "WARN",
            "no publicado para esta etiqueta (el asset de nivel superior existe desde AUD-01); "
            "la suma se puede comprobar contra el checksums.txt interno del tarball",
        )

    # 4) extraccion
    destino = trabajo / "instalado"
    destino.mkdir()
    extrae(tarball, destino)
    anota("extraccion", "PASS", str(destino))

    # 5) contenido prometido
    guilds = destino / "guilds"
    anota("guilds/ en la raiz", "PASS" if guilds.is_dir() else "FAIL", str(guilds))
    # El manifiesto viaja en ./guilds/requirements-hashes.txt, NO en la raiz del tarball
    # (correccion del TL en T4759 sobre mi falso positivo: en v0.17.3 esta en guilds/,
    # y en v0.17.2 no estaba en ninguna de las dos). Se aceptan las dos rutas y se dice
    # cual se encontro, para que el informe no dependa de donde lo ponga el empaquetado.
    hashes = next(
        (p for p in (destino / "guilds" / "requirements-hashes.txt", destino / "requirements-hashes.txt") if p.is_file()),
        None,
    )
    anota(
        "requirements-hashes.txt",
        "PASS" if hashes else "FAIL",
        f"encontrado en ./{hashes.relative_to(destino).as_posix()}" if hashes else "no esta ni en la raiz ni en guilds/",
    )
    interno = destino / "checksums.txt"
    if interno.is_file():
        cubre = sum(1 for linea in interno.read_text(encoding="utf-8", errors="replace").splitlines() if " guilds/" in linea or " *guilds/" in linea)
        anota("checksums.txt interno cubre guilds/", "PASS" if cubre > 0 else "FAIL", f"{cubre} ficheros de guilds/")
    else:
        anota("checksums.txt interno", "WARN", "el tarball no trae checksums.txt interno")
    if guilds.is_dir():
        anota("guilds tienen descriptor", "PASS" if any(guilds.glob("**/*.py")) else "FAIL", "")

    # 6) kernel de prueba: puerto propio, DATA_DIR propio, dev_mode apagado
    port = puerto_libre(args.port)
    datos = trabajo / "data"
    datos.mkdir()
    config = trabajo / "tylluan.toml"
    # El config de prueba lo genera el PROPIO CLI del paquete (perfil portable: sin
    # embeddings, reranker apagado), que es la receta que recibe un usuario real; a mano
    # es facil equivocarse de clave y el kernel arranca a medias. Despues se le cambia el
    # puerto por uno libre.
    cli_paquete = next((destino / c for c in ("tylluan-cli", "tylluan-cli.exe") if (destino / c).is_file()), None)
    generado = False
    if cli_paquete is not None:
        gen = subprocess.run(
            [str(cli_paquete), "install", "--profile", "portable", "--dir", str(trabajo), "--no-start", "--force"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=120,
        )
        if gen.returncode == 0 and config.is_file():
            texto = config.read_text(encoding="utf-8")
            texto = __import__("re").sub(r"^port\s*=\s*\d+", f"port = {port}", texto, count=1, flags=__import__("re").M)
            # En 0.17.2 el perfil portable generado por el CLI **no declara** `transport`,
            # y sin transporte HTTP el kernel arranca sin abrir puerto (cambio del
            # transporte por defecto, ver CHANGELOG 0.17.2). Lo declaramos aqui.
            if "transport" not in texto:
                texto = texto.replace("[nexus]", '[nexus]\ntransport = ["http"]', 1)
                anota("transporte http declarado", "PASS", 'anadido transport = ["http"] al config generado')
            config.write_text(texto, encoding="utf-8")
            generado = True
    if not generado:
        config.write_text(
            "# generado por scripts/verify_release.py -- kernel de prueba aislado\n"
            "[nexus]\n"
            'host = "127.0.0.1"\n'
            f"port = {port}\n"
            "dev_mode = false\n"
            'transport = ["http"]\n'
            "[embeddings]\n"
            'model = "none"\n'
            "vector_dimensions = 0\n"
            "[silva]\n"
            "reranker_enabled = false\n"
            "[guilds]\n"
            "always_on = []\n",
            encoding="utf-8",
        )
    anota("config de prueba", "PASS" if generado else "WARN",
          ("generado con el CLI del paquete (perfil portable)" if generado else "generado por el script"))
    nexus = None
    for cand in ("tylluan-nexus", "tylluan-nexus.exe"):
        if (destino / cand).is_file():
            nexus = destino / cand
            break
    if nexus is None and sys.platform.startswith("win"):
        nexus = next(iter(destino.glob("tylluan-nexus*")), None)
    if nexus is None:
        anota("binario del kernel", "FAIL", "no encontre tylluan-nexus en el tarball")
        cerrar_temporal(trabajo, fallos, args.conservar)
        return 1
    anota("binario del kernel", "PASS", nexus.name)

    # journal.db se resuelve SIEMPRE como ./data/journal.db relativo al CWD (no mira
    # TYLLUAN_DATA_DIR), asi que el CWD del kernel de prueba tiene que tener data/.
    datos.mkdir(exist_ok=True)
    entorno = dict(os.environ)
    entorno["TYLLUAN_DATA_DIR"] = str(datos)
    entorno["TYLLUAN_CONFIG"] = str(config)
    # que no herede nada de produccion
    entorno.pop("TYLLUAN_PORT", None)
    # El kernel loguea muchisimo al arrancar. Con `stdout=PIPE` y nadie leyendo, el buffer
    # del pipe (~64 KB en Windows) se llena y el proceso se QUEDA BLOQUEADO escribiendo
    # antes de abrir el puerto: eso era exactamente lo que hacia que /health no respondiera
    # con el kernel vivo. Se manda la salida a un fichero y se lee de ahi solo si falla.
    log_kernel = trabajo / "kernel-salida.log"
    salida_kernel = open(log_kernel, "wb")
    proceso = subprocess.Popen(
        [str(nexus), "--config", str(config)],
        cwd=str(trabajo),
        env=entorno,
        stdout=salida_kernel,
        stderr=subprocess.STDOUT,
    )
    try:
        health = espera_health(port, args.timeout)
        anota("/health responde 200", "PASS" if health["status"] == 200 else "FAIL", f"HTTP {health['status']}")
        v = str(health["body"].get("version", ""))
        anota("/health version", "PASS" if v == version else "FAIL", f"{v or '(sin version)'} (esperado {version})")
        readiness = health["body"].get("readiness")
        if readiness is not None:
            anota("readiness en el cuerpo", "PASS", str(readiness))
    except Falla as e:
        anota("/health", "FAIL", str(e))
        salida = ""
        if proceso.poll() is None:
            proceso.terminate()
            time.sleep(1)
            if proceso.poll() is None:
                proceso.kill()
        try:
            salida = log_kernel.read_text(encoding="utf-8", errors="replace")[-1200:]
        except OSError:
            salida = ""
        if salida:
            print("   --- salida del kernel (ultimas lineas) ---")
            print(salida)
    finally:
        if proceso.poll() is None:
            proceso.terminate()
            try:
                proceso.wait(timeout=15)
            except subprocess.TimeoutExpired:
                proceso.kill()
        # T4960: el handle del log tiene que cerrarse ANTES de intentar borrar el
        # directorio; en Windows un fichero abierto hace fallar shutil.rmtree y el
        # temporal se quedaba siempre (medido: "no pude borrar ... WinError 32").
        try:
            salida_kernel.close()
        except OSError:
            pass

    # 7) tylluan-cli --version del paquete
    cli = next((destino / c for c in ("tylluan-cli", "tylluan-cli.exe") if (destino / c).is_file()), None)
    if cli is None:
        anota("tylluan --version", "WARN", "el tarball no trae tylluan-cli")
    else:
        salida = subprocess.run([str(cli), "--version"], capture_output=True, text=True, timeout=60)
        texto = (salida.stdout + salida.stderr).strip().splitlines()
        dice = texto[0] if texto else ""
        anota(
            "tylluan --version",
            "PASS" if version in dice else "FAIL",
            f"rc={salida.returncode} dice {dice!r} (esperado {version})",
        )

    # resumen
    print()
    print(f"== resumen: {sum(1 for f in filas if f[1] == 'PASS')} PASS, "
          f"{sum(1 for f in filas if f[1] == 'WARN')} WARN, {fallos} FAIL")
    cerrar_temporal(trabajo, fallos, args.conservar)
    return 1 if fallos else 0


if __name__ == "__main__":
    sys.exit(main())
