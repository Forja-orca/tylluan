#!/usr/bin/env python3
"""T-DSH-168 (F1, tercer escalon): simulacro de recuperacion completo.

Con un kernel de PRUEBA aislado (puerto 47180/47181 y DATA_DIR propios; NUNCA
el 47004 ni los datos de produccion):

1. Escribe 10 llamadas con las 4 tools soberanas (remember/recall/think/do).
2. `tylluan-cli backup` a una carpeta.
3. Restaura en una carpeta VACIA con `restore --target-dir --port`.
4. Verifica la copia restaurada: 13 bases `PRAGMA quick_check` = ok,
   `episodes.db` con las filas esperadas y `verify_chain` valida, y un segundo
   kernel arranca sobre ella y devuelve por recall un nodo escrito antes.
5. Corta el primer kernel con kill a mitad de una escritura y comprueba que al
   reiniciar no hay filas truncadas ni cadena rota.

Uso:
    python scripts/drill_recuperacion_t168.py \
        --work-dir <carpeta de trabajo> \
        --nexus <ruta a tylluan-nexus.exe> \
        --cli <ruta a tylluan-cli.exe> \
        --repo <raiz del repo>

Devuelve 0 solo si TODAS las comprobaciones pasan; 1 si alguna falla.
No arranca ni toca el kernel de produccion (:47004) ni sus datos.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import threading
import time
import urllib.request
from pathlib import Path

PORT1 = 47180
PORT2 = 47181
MARKER = "MARCA-T168 nodo escrito antes de la copia"
DATA_DBS = [
    "agent_profiles.db",
    "audit.db",
    "contracts.db",
    "curriculum.db",
    "episodes.db",
    "jobs.db",
    "journal.db",
    "mailbox.db",
    "peers.db",
    "pending_dispatches.db",
    "silva.db",
    "task_context.db",
    "tylluan.db",
]

results: list[tuple[str, bool, str]] = []


def check(name: str, ok: bool, detail: str) -> None:
    results.append((name, ok, detail))
    print(f"[{'PASS' if ok else 'FAIL'}] {name}: {detail}")


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


def start_kernel(nexus: str, repo: Path, data_dir: Path, port: int, log_path: Path):
    cfg = write_config(data_dir, port)
    env = dict(os.environ)
    env["TYLLUAN_DATA_DIR"] = str(data_dir)
    log = open(log_path, "w", encoding="utf-8", errors="replace")
    proc = subprocess.Popen(
        [nexus, "--config", str(cfg), "--port", str(port)],
        cwd=str(repo),
        env=env,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    print(f"  kernel pid={proc.pid} port={port} DATA_DIR={data_dir}")
    return proc, log


def wait_health(port: int, timeout_s: int = 180) -> dict:
    deadline = time.time() + timeout_s
    last = None
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as resp:
                return json.loads(resp.read().decode())
        except Exception as exc:  # noqa: BLE001 - arranque en curso
            last = exc
            time.sleep(1)
    raise RuntimeError(f"/health no respondio en {timeout_s}s (ultimo error: {last})")


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
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        raw = resp.read().decode("utf-8", errors="replace")
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {"raw": raw}


def call_ok(payload: dict) -> tuple[bool, str]:
    if "error" in payload:
        return False, f"error={payload['error']}"
    result = payload.get("result") or {}
    if result.get("isError"):
        return False, "isError=true"
    texts = [c.get("text", "") for c in result.get("content", [])]
    return True, " ".join(texts)[:300]


def stop_kernel(proc: subprocess.Popen, log) -> None:
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=10)
    log.close()


def quick_check(path: Path) -> str:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute("PRAGMA quick_check;").fetchone()[0]
    finally:
        conn.close()


def verify_episodes_chain(db_path: Path) -> tuple[bool, str, int]:
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        rows = conn.execute(
            "SELECT session_id, seq, content_redacted, content_hash, prev_hash "
            "FROM episodes ORDER BY session_id ASC, seq ASC"
        ).fetchall()
    finally:
        conn.close()
    genesis = "0" * 64
    expected_prev: dict[str, str] = {}
    for session, seq, content, content_hash, prev_hash in rows:
        want_prev = expected_prev.get(session, genesis)
        recomputed = hashlib.sha256(content.encode("utf-8")).hexdigest()
        if prev_hash != want_prev:
            return False, f"sesion {session} seq {seq}: prev_hash roto", len(rows)
        if recomputed != content_hash:
            return False, f"sesion {session} seq {seq}: content_hash no coincide", len(rows)
        expected_prev[session] = content_hash
    return True, f"{len(rows)} filas, cadena valida", len(rows)


def main() -> int:
    # La consola de Windows puede estar en cp1252: forzar UTF-8 evita que un
    # mensaje con acentos o guiones largos aborte el simulacro.
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001 - consola sin reconfigure
            pass
    ap = argparse.ArgumentParser()
    ap.add_argument("--work-dir", required=True)
    ap.add_argument("--nexus", required=True)
    ap.add_argument("--cli", required=True)
    ap.add_argument("--repo", required=True)
    args = ap.parse_args()

    base = Path(args.work_dir).resolve()
    repo = Path(args.repo).resolve()
    nexus = str(Path(args.nexus).resolve())
    cli = str(Path(args.cli).resolve())
    if base.exists():
        shutil.rmtree(base, ignore_errors=True)
    data1 = base / "data1"
    backup_dir = base / "backup"
    data2 = base / "data2"
    data1.mkdir(parents=True)
    data2.mkdir(parents=True)

    print("=== T-DSH-168: simulacro de recuperacion ===")
    print(f"work_dir={base}")
    print(f"nexus={nexus}")
    print(f"cli={cli}")
    print()

    # ---- 1) kernel 1 + 10 llamadas con las 4 soberanas -------------------
    print("--- 1) kernel 1 y 10 llamadas con las 4 tools soberanas ---")
    k1, log1 = start_kernel(nexus, repo, data1, PORT1, base / "kernel1.log")
    health = wait_health(PORT1)
    print(f"  /health: status={health.get('status')} commit={health.get('commit')}")

    calls = [
        ("tylluan_remember", {"agent_id": "dsh-drill", "content": MARKER, "node_type": "lesson", "metadata": "{}"}),
        ("tylluan_remember", {"agent_id": "dsh-drill", "content": "T168 segunda memoria de prueba", "node_type": "lesson", "metadata": "{}"}),
        ("tylluan_remember", {"agent_id": "dsh-drill", "content": "T168 tercera memoria de prueba", "node_type": "lesson", "metadata": "{}"}),
        ("tylluan_recall", {"query": "MARCA-T168", "limit": 3}),
        ("tylluan_recall", {"query": "memoria de prueba T168", "limit": 3}),
        ("tylluan_think", {"query": "que comprueba el simulacro T-DSH-168"}),
        ("tylluan_think", {"query": "estado del backup de Tylluan"}),
        ("tylluan_think", {"query": "indice FTS y episodes.db"}),
        ("tylluan_do", {"intent": "whoami"}),
        ("tylluan_do", {"intent": "list available guilds"}),
    ]
    ok_calls = 0
    for i, (tool, arguments) in enumerate(calls, start=1):
        try:
            payload = mcp_call(PORT1, tool, arguments, i)
            ok, detail = call_ok(payload)
        except Exception as exc:  # noqa: BLE001 - se reporta como fallo de la llamada
            ok, detail = False, f"excepcion: {exc}"
        ok_calls += 1 if ok else 0
        print(f"  [{i:02d}] {tool}: {'ok' if ok else 'FALLO'} {detail[:120]}")
    check("10 llamadas con las 4 tools soberanas", ok_calls == 10, f"{ok_calls}/10 ok")

    # ---- 2) backup en caliente ------------------------------------------
    print("\n--- 2) tylluan-cli backup (kernel 1 vivo) ---")
    env = dict(os.environ)
    env["TYLLUAN_DATA_DIR"] = str(data1)
    backup = subprocess.run(
        [cli, "backup", str(backup_dir)],
        cwd=str(repo),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
        errors="replace",
        timeout=600,
    )
    print((backup.stdout or "").strip())
    if (backup.stderr or "").strip():
        print("[stderr]", (backup.stderr or "").strip())
    manifest_path = backup_dir / "backup-manifest.json"
    entries = []
    if manifest_path.exists():
        entries = json.loads(manifest_path.read_text(encoding="utf-8")).get("entries", [])
    names = sorted(e["name"] for e in entries)
    missing = [n for n in DATA_DBS if n not in names]
    check("backup incluye las 13 bases", not missing and len(names) == 13, f"{len(names)} bases: {names}; faltan={missing}")

    # ---- 3) kill a mitad de una escritura -------------------------------
    print("\n--- 3) kill del kernel 1 a mitad de escritura ---")
    stop_flag = threading.Event()
    sent = {"n": 0}

    def spam_writes() -> None:
        i = 100
        while not stop_flag.is_set():
            try:
                mcp_call(PORT1, "tylluan_remember", {"agent_id": "dsh-drill", "content": f"T168 escritura en vuelo {i}", "node_type": "lesson", "metadata": "{}"}, i, timeout=20)
                sent["n"] += 1
            except Exception:  # noqa: BLE001 - el kill corta la conexion a proposito
                pass
            i += 1
            time.sleep(0.02)

    writer = threading.Thread(target=spam_writes, daemon=True)
    writer.start()
    time.sleep(0.5)
    k1.kill()  # kill duro a mitad de escritura
    k1.wait(timeout=30)
    stop_flag.set()
    writer.join(timeout=10)
    log1.close()
    print(f"  kernel 1 pid={k1.pid} matado con kill; llamadas de escritura lanzadas={sent['n']}")

    # ---- 4) restore en carpeta vacia ------------------------------------
    print("\n--- 4) tylluan-cli restore --target-dir a carpeta vacia ---")
    env2 = dict(os.environ)
    env2["TYLLUAN_DATA_DIR"] = str(data2)
    restore = subprocess.run(
        [cli, "restore", str(backup_dir), "--target-dir", str(data2), "--port", str(PORT2)],
        cwd=str(repo),
        env=env2,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
        errors="replace",
        timeout=600,
    )
    print((restore.stdout or "").strip())
    if (restore.stderr or "").strip():
        print("[stderr]", (restore.stderr or "").strip())
    check("restore termina con exit 0", restore.returncode == 0, f"exit={restore.returncode}")

    # ---- 5) verificacion de la copia restaurada -------------------------
    print("\n--- 5) verificacion de la copia restaurada ---")
    checks = []
    for name in DATA_DBS:
        path = data2 / name
        if not path.exists():
            checks.append((name, False, "no existe"))
            continue
        try:
            status = quick_check(path)
            checks.append((name, status == "ok", status))
        except Exception as exc:  # noqa: BLE001
            checks.append((name, False, f"error: {exc}"))
    for name, ok, detail in checks:
        print(f"  quick_check {name}: {'ok' if ok else 'FALLO'} ({detail})")
    bad = [n for n, ok, _ in checks if not ok]
    check("13 bases con PRAGMA quick_check=ok", not bad, f"mal={bad}")

    eps = data2 / "episodes.db"
    rows = 0
    chain_ok = False
    chain_detail = "episodes.db no existe"
    if eps.exists():
        chain_ok, chain_detail, rows = verify_episodes_chain(eps)
    print(f"  episodes.db: {chain_detail}")
    # 10 llamadas x (request + response) = 20 filas esperadas.
    check("episodes.db con 20 filas (10 request + 10 response) y cadena valida", chain_ok and rows == 20, f"filas={rows}, {chain_detail}")

    # ---- 6) segundo kernel sobre la copia + recall ----------------------
    print("\n--- 6) segundo kernel sobre la copia restaurada + recall ---")
    k2, log2 = start_kernel(nexus, repo, data2, PORT2, base / "kernel2.log")
    wait_health(PORT2)
    recall_payload = None
    recall_ok = False
    recall_detail = ""
    for attempt in range(30):
        try:
            recall_payload = mcp_call(PORT2, "tylluan_recall", {"query": MARKER, "limit": 5}, 500 + attempt)
            recall_ok, recall_detail = call_ok(recall_payload)
            if recall_ok and MARKER in recall_detail:
                break
            recall_ok = False
            recall_detail = f"el texto no contiene la marca: {recall_detail[:150]}"
        except Exception as exc:  # noqa: BLE001
            recall_ok, recall_detail = False, f"excepcion: {exc}"
        time.sleep(1)
    print(f"  recall: {'ok' if recall_ok else 'FALLO'} {recall_detail[:240]}")
    check("recall devuelve el nodo escrito antes de la copia", recall_ok, recall_detail[:200])
    stop_kernel(k2, log2)

    # ---- 7) reinicio del kernel 1 tras el kill --------------------------
    print("\n--- 7) reinicio del kernel 1 tras el kill ---")
    k1b, log1b = start_kernel(nexus, repo, data1, PORT1, base / "kernel1_restart.log")
    restart_health = wait_health(PORT1)
    print(f"  /health tras reinicio: status={restart_health.get('status')} commit={restart_health.get('commit')}")
    # Parar antes de inspeccionar (WAL consistente; quick_check admite WAL).
    stop_kernel(k1b, log1b)

    bad1 = []
    for name in DATA_DBS:
        path = data1 / name
        if not path.exists():
            bad1.append(f"{name}: no existe")
            continue
        try:
            status = quick_check(path)
            if status != "ok":
                bad1.append(f"{name}: {status}")
        except Exception as exc:  # noqa: BLE001
            bad1.append(f"{name}: error {exc}")
    chain_ok1, chain_detail1, rows1 = verify_episodes_chain(data1 / "episodes.db")
    print(f"  episodes.db tras kill/reinicio: {chain_detail1}")
    check("tras kill+reinicio: 13 bases quick_check=ok", not bad1, f"mal={bad1}")
    check("tras kill+reinicio: cadena de episodes valida", chain_ok1, chain_detail1)
    check("tras kill+reinicio: sin filas truncadas en episodes (filas>0)", rows1 > 0, f"filas={rows1}")

    # ---- tabla final ----------------------------------------------------
    print("\n=== TABLA DE RESULTADOS ===")
    for name, ok, detail in results:
        print(f"{'PASS' if ok else 'FAIL'}\t{name}\t{detail}")
    failed = [n for n, ok, _ in results if not ok]
    print(f"\nRESULTADO GLOBAL: {'PASS' if not failed else 'FAIL'} ({len(results) - len(failed)}/{len(results)})")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
