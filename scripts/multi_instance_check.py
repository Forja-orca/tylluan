#!/usr/bin/env python3
"""T-DSH-173 (H4): comprobacion de multiples instancias de Tylluan.

Premisa de Jose: debe haber tantas instancias de Tylluan como permitan los
recursos, nunca limitadas por las instaladas. Este script lo comprueba con
kernels de PRUEBA, sin tocar produccion:

1. Arranca N kernels (por defecto 3) en puertos propios (47091-47093 por
   defecto), cada uno con su TYLLUAN_DATA_DIR dentro de una carpeta temporal,
   nunca :47004 ni la carpeta de datos de produccion, con
   RAYON_NUM_THREADS/OMP_NUM_THREADS limitados a 2 por instancia.
2. Espera /health de cada una.
3. Escribe un recuerdo distinto en cada una (tylluan_remember) y comprueba con
   tylluan_recall que NO aparece en las otras dos.
4. Compara el sistema de ficheros antes/despues y lista los ficheros escritos
   fuera del TYLLUAN_DATA_DIR de cada instancia; falla si alguno es compartido
   (cola, pid, config, logs).
5. Apaga cada kernel por su PID exacto (Popen.terminate/kill); nunca
   taskkill por nombre.

Uso:
    python scripts/multi_instance_check.py \
        --work-dir <carpeta de trabajo> \
        --nexus <ruta a tylluan-nexus.exe> \
        [--n 3] [--base-port 47091] [--repo <raiz del repo>] [--keep]

Devuelve 0 solo si TODAS las comprobaciones pasan; 1 si alguna falla.
No arranca ni toca el kernel de produccion (:47004) ni sus datos.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

FORBIDDEN_PORTS = {47004}
DEFAULT_PORTS = [47091, 47092, 47093]
THREADS_PER_INSTANCE = 2
MARKER_PREFIX = "MARCA-T173-instancia"
HEALTH_TIMEOUT_S = 240
STOP_TIMEOUT_S = 45

results: list[tuple[str, bool, str]] = []


def check(name: str, ok: bool, detail: str) -> None:
    results.append((name, ok, detail))
    print(f"[{'PASS' if ok else 'FAIL'}] {name}: {detail}")


def log(msg: str = "") -> None:
    print(msg)


def port_is_free(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.5)
        return sock.connect_ex(("127.0.0.1", port)) != 0


def write_config(data_dir: Path, port: int) -> Path:
    cfg = data_dir / "tylluan.toml"
    cfg.write_text(
        f"""[nexus]
host = "127.0.0.1"
port = {port}
transport = ["http"]
dev_mode = true

[silva]
db_path = "{(data_dir / 'silva.db').as_posix()}"

[memory]
db_path = "{(data_dir / 'tylluan.db').as_posix()}"
""",
        encoding="utf-8",
    )
    return cfg


def start_kernel(
    nexus: str, data_dir: Path, cwd: Path, port: int, log_path: Path
) -> tuple[subprocess.Popen, object]:
    cfg = write_config(data_dir, port)
    env = dict(os.environ)
    env["TYLLUAN_DATA_DIR"] = str(data_dir)
    env["RAYON_NUM_THREADS"] = str(THREADS_PER_INSTANCE)
    env["OMP_NUM_THREADS"] = str(THREADS_PER_INSTANCE)
    # Extra: el resto de runtimes numericos no deben desbordar los 2 hilos.
    env["OPENBLAS_NUM_THREADS"] = str(THREADS_PER_INSTANCE)
    env["MKL_NUM_THREADS"] = str(THREADS_PER_INSTANCE)
    env["NUMEXPR_NUM_THREADS"] = str(THREADS_PER_INSTANCE)
    log = open(log_path, "w", encoding="utf-8", errors="replace")
    proc = subprocess.Popen(
        [nexus, "--config", str(cfg), "--port", str(port)],
        cwd=str(cwd),
        env=env,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    return proc, log


def wait_health(port: int, timeout_s: int = HEALTH_TIMEOUT_S) -> dict:
    deadline = time.time() + timeout_s
    last = None
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as resp:
                return json.loads(resp.read().decode())
        except Exception as exc:  # noqa: BLE001 - arranque en curso
            last = exc
            time.sleep(1)
    raise RuntimeError(f"/health(:{port}) no respondio en {timeout_s}s (ultimo error: {last})")


def mcp_call(port: int, tool: str, arguments: dict, call_id: int, timeout: int = 120) -> dict:
    body = {
        "jsonrpc": "2.0",
        "id": call_id,
        "method": "tools/call",
        "params": {"name": tool, "arguments": arguments},
    }
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}/mcp",
        data=json.dumps(body).encode("utf-8"),
        headers={"Content-Type": "application/json", "Accept": "application/json, text/event-stream"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        raw = resp.read().decode("utf-8", errors="replace")
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {"raw": raw}


def call_text(payload: dict) -> tuple[bool, str]:
    if "error" in payload:
        return False, f"error={payload['error']}"
    result = payload.get("result") or {}
    if result.get("isError"):
        return False, "isError=true"
    texts = [c.get("text", "") for c in result.get("content", [])]
    return True, " ".join(texts)[:500]


def stop_kernel(proc: subprocess.Popen, log_file) -> None:
    if proc.poll() is None:
        proc.terminate()  # PID exacto del hijo que arrancamos
        try:
            proc.wait(timeout=STOP_TIMEOUT_S)
        except subprocess.TimeoutExpired:
            proc.kill()  # ultimo recurso, tambien por PID exacto
            proc.wait(timeout=15)
    log_file.close()


def snapshot(root: Path, limit: int = 4000) -> dict[str, tuple[int, int]]:
    """(ruta relativa -> (mtime_ns, tamano)) de un arbol, acotado."""
    out: dict[str, tuple[int, int]] = {}
    if not root.exists():
        return out
    count = 0
    for dirpath, dirnames, filenames in os.walk(root):
        # Evita arboles gigantes que no son estado del kernel.
        dirnames[:] = [
            d
            for d in dirnames
            if d
            not in {
                "target",
                "node_modules",
                ".git",
                ".cargo",
                "__pycache__",
                ".venv",
                ".pytest_cache",
            }
        ]
        for name in filenames:
            p = Path(dirpath) / name
            try:
                st = p.stat()
            except OSError:
                continue
            rel = str(p.relative_to(root))
            out[rel] = (st.st_mtime_ns, st.st_size)
            count += 1
            if count >= limit:
                return out
    return out


def diff_snapshot(
    before: dict[str, tuple[int, int]], after: dict[str, tuple[int, int]]
) -> list[str]:
    changed = []
    for rel, meta in after.items():
        if before.get(rel) != meta:
            changed.append(rel)
    return sorted(changed)


def global_roots(repo: Path | None, work: Path) -> list[tuple[str, Path]]:
    roots: list[tuple[str, Path]] = []
    if repo is not None:
        roots.append(("repo-raiz", repo))
        for extra in ("data", "logs", ".tylluan", "crates/tylluan-kernel/data"):
            p = repo / extra
            if p.exists():
                roots.append((f"repo/{extra}", p))
    home = Path.home()
    for name in (".tylluan", ".tylluan-data", ".config/tylluan"):
        p = home / name
        if p.exists():
            roots.append((f"home/{name}", p))
    for env_name in ("APPDATA", "LOCALAPPDATA"):
        base = os.environ.get(env_name)
        if not base:
            continue
        for p in Path(base).glob("*tylluan*"):
            if p.is_dir():
                roots.append((f"{env_name}/{p.name}", p))
    # Ficheros compartidos tipicos a vigilar, aunque no existan.
    roots.append(("work-raiz", work))
    return roots


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001 - consola sin reconfigure
            pass
    ap = argparse.ArgumentParser(description="Comprobacion de multiples instancias de Tylluan")
    ap.add_argument("--work-dir", required=True, help="carpeta de trabajo (se recrea)")
    ap.add_argument("--nexus", required=True, help="ruta a tylluan-nexus(.exe)")
    ap.add_argument("--n", type=int, default=3, help="numero de instancias (por defecto 3)")
    ap.add_argument("--base-port", type=int, default=DEFAULT_PORTS[0], help="primer puerto")
    ap.add_argument("--repo", help="raiz del repo (para vigilar estado global)")
    ap.add_argument("--keep", action="store_true", help="no borrar la carpeta de trabajo")
    args = ap.parse_args()

    n = args.n
    if n < 2:
        log("ERROR: se necesitan al menos 2 instancias para comprobar aislamiento")
        return 1
    ports = [args.base_port + i for i in range(n)]
    if any(p in FORBIDDEN_PORTS for p in ports) or any(p <= 0 or p > 65535 for p in ports):
        log(f"ERROR: puertos no permitidos: {ports} (nunca :47004)")
        return 1
    if len(set(ports)) != n:
        log("ERROR: puertos repetidos")
        return 1
    busy = [p for p in ports if not port_is_free(p)]
    if busy:
        log(f"ERROR: puertos ya ocupados: {busy}; no se arranca nada")
        return 1

    nexus = str(Path(args.nexus).resolve())
    if not Path(nexus).exists():
        log(f"ERROR: no existe el binario: {nexus}")
        return 1
    work = Path(args.work_dir).resolve()
    if work.exists():
        shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    repo = Path(args.repo).resolve() if args.repo else None
    if repo is not None and not repo.exists():
        log(f"ERROR: no existe --repo: {repo}")
        return 1

    log("=== T-DSH-173: comprobacion de multiples instancias ===")
    log(f"work_dir={work}")
    log(f"nexus={nexus}")
    log(f"instancias={n} puertos={ports} hilos_por_instancia={THREADS_PER_INSTANCE}")
    log()

    # ---- 4a) snapshot ANTES de arrancar --------------------------------
    roots = global_roots(repo, work)
    before: dict[str, dict[str, tuple[int, int]]] = {
        label: snapshot(root) for label, root in roots
    }
    log(f"Estado global vigilado: {', '.join(label for label, _ in roots)}")
    log()

    instances = []
    markers = []
    try:
        # ---- 1) arranque -------------------------------------------------
        log("--- 1) arranque de kernels de prueba ---")
        for i, port in enumerate(ports):
            inst = work / f"inst{i}"
            data = inst / "data"
            cwd = inst / "cwd"
            data.mkdir(parents=True)
            cwd.mkdir(parents=True)
            marker = f"{MARKER_PREFIX}-{i}-{int(time.time() * 1000)}"
            markers.append(marker)
            proc, logf = start_kernel(nexus, data, cwd, port, inst / "kernel.log")
            instances.append(
                {"i": i, "port": port, "proc": proc, "log": logf, "data": data, "cwd": cwd, "marker": marker}
            )
            log(f"  inst{i}: pid={proc.pid} port={port} TYLLUAN_DATA_DIR={data}")
        check("arranque", True, f"{n} procesos lanzados por PID propio")

        # ---- 2) /health --------------------------------------------------
        log()
        log("--- 2) /health de cada instancia ---")
        for inst in instances:
            health = wait_health(inst["port"])
            ok = health.get("status") in {"healthy", "ok", "warming_up"}
            check(
                f"health-inst{inst['i']}",
                ok,
                f"status={health.get('status')} commit={health.get('commit')}",
            )

        # ---- 3) memoria aislada -----------------------------------------
        log()
        log("--- 3) escribir un recuerdo distinto en cada instancia ---")
        for inst in instances:
            payload = mcp_call(
                inst["port"],
                "tylluan_remember",
                {
                    "agent_id": f"dsh-t173-{inst['i']}",
                    "content": inst["marker"],
                    "node_type": "lesson",
                    "metadata": "{}",
                },
                call_id=100 + inst["i"],
            )
            ok, detail = call_text(payload)
            check(f"remember-inst{inst['i']}", ok, detail)

        log()
        log("--- 3b) cada recuerdo debe verse SOLO en su instancia ---")
        for inst in instances:
            for other in instances:
                payload = mcp_call(
                    other["port"],
                    "tylluan_recall",
                    {"query": inst["marker"], "limit": 5},
                    call_id=200 + other["i"] * 10 + inst["i"],
                )
                ok, text = call_text(payload)
                found = inst["marker"] in text
                if other["i"] == inst["i"]:
                    check(
                        f"recall-propio-inst{inst['i']}",
                        ok and found,
                        "el recuerdo se recupera en su instancia" if found else f"NO se recupera: {text}",
                    )
                else:
                    check(
                        f"recall-cruzado-{other['i']}<-{inst['i']}",
                        ok and not found,
                        "no aparece en otra instancia" if not found else f"FUGA: aparece en inst{other['i']}: {text}",
                    )
    finally:
        # ---- 5) apagado por PID exacto -----------------------------------
        log()
        log("--- 5) apagado por PID exacto ---")
        for inst in instances:
            pid = inst["proc"].pid
            stop_kernel(inst["proc"], inst["log"])
            stopped = inst["proc"].poll() is not None
            check(f"apagado-inst{inst['i']}", stopped, f"pid={pid} terminado")
        for inst in instances:
            deadline = time.time() + 10
            while time.time() < deadline and not port_is_free(inst["port"]):
                time.sleep(0.5)
            check(f"puerto-liberado-inst{inst['i']}", port_is_free(inst["port"]), f":{inst['port']} libre")

    # ---- 4b) snapshot DESPUES y ficheros fuera del DATA_DIR ------------
    log()
    log("--- 4) ficheros escritos fuera del TYLLUAN_DATA_DIR ---")
    after = {label: snapshot(root) for label, root in roots}
    inst_dir_prefixes = [str(inst["data"].parent) for inst in instances]
    global_writes: list[str] = []
    per_instance: list[str] = []
    for label, root in roots:
        for rel in diff_snapshot(before[label], after[label]):
            full = str(root / rel)
            if any(full.startswith(prefix) for prefix in inst_dir_prefixes):
                per_instance.append(f"{label}:{rel}")
            else:
                global_writes.append(f"{label}:{rel}")

    log(f"  fuera de instancias (per-instancia, esperado): {len(per_instance)}")
    for item in per_instance[:40]:
        log(f"    - {item}")
    log(f"  GLOBAL (compartido, no debe haber): {len(global_writes)}")
    for item in global_writes[:60]:
        log(f"    ! {item}")
    check(
        "sin-escrituras-globales",
        len(global_writes) == 0,
        "ninguna escritura fuera de las carpetas de instancia"
        if not global_writes
        else f"{len(global_writes)} ficheros compartidos: {global_writes[:10]}",
    )

    # Estado global compartido que exista, aunque no se haya escrito.
    log()
    log("--- 4c) estado global compartido presente (aunque no rompa nada) ---")
    candidates: list[Path] = []
    if repo is not None:
        for name in ("queue.db", "kernel.log", "tylluan.toml", "data", "logs"):
            candidates.append(repo / name)
    candidates.extend([Path.home() / ".tylluan", Path.home() / ".tylluan-data"])
    for env_name in ("APPDATA", "LOCALAPPDATA"):
        base = os.environ.get(env_name)
        if base:
            candidates.extend(Path(base).glob("*tylluan*"))
    present = sorted({str(p) for p in candidates if p.exists()})
    log(f"  {len(present)} rutas globales presentes (revisar si alguna es de produccion):")
    for p in present:
        log(f"    - {p}")
    check("estado-global-listado", True, f"{len(present)} rutas presentes (solo se listan)")

    # ---- resumen --------------------------------------------------------
    log()
    failed = [r for r in results if not r[1]]
    log(f"=== RESUMEN: {len(results) - len(failed)}/{len(results)} comprobaciones PASS ===")
    if failed:
        for name, _, detail in failed:
            log(f"  FAIL {name}: {detail}")
        return 1
    log("OK: N instancias arrancan, se aíslan y se apagan por PID sin estado compartido.")
    if args.keep:
        log(f"work_dir conservado: {work}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
