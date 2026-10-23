#!/usr/bin/env python3
"""Reconstruye las FASES DE ARRANQUE del kernel a partir de logs/kernel.log.

Uso:
    python3 scripts/measure_startup_phases.py --log logs/kernel.log
    python3 scripts/measure_startup_phases.py --log logs/kernel.log --since 2026-10-22 --until 2026-10-23

Cada «arranque» se delimita por la línea `TylluanNexus kernel started` y el
siguiente arranque. Para cada uno se localizan anclas por mensaje de log y se
emite una tabla «fase -> inicio (línea) -> fin (línea) -> segundos».

Fases y anclas (mensaje literal del log):
  config          : "Loading config from" / "Config loaded and cached"
  integrity_db    : "[Integrity] Running startup database validation" -> "[Integrity] ... healthy"
  memory_layer    : "Initializing Memory Layer" -> "Mailbox schema initialized"
  onnx_probe      : primer "ONNX Runtime compat probe"
  sparse_engine   : "sparse engine ready"
  embed_preload   : "Pre-loading embedding model" -> "engine ready (ONNX)" + "fingerprint sha256"
  guild_embeddings: primer/último "Embedding guild:"
  reranker        : "Loading Jina Turbo reranker" -> "Jina Turbo reranker ready"
  registry        : "Registry loaded:"
  kernel_banner   : "Kernel operational."
  spawn_first     : primer "[Startup] Spawning always-on guild"
  repo_map        : "Repo map built: N files, N dirs, N lines (Nms)" (la duración DADA; inicio = fin - ms)
  http_bind       : "HTTP Gateway listening on"
  auto_link       : primer "[AutoLink] Starting"
  federation      : "Federation auto-sync background task started" y primer ciclo

Solo lectura: no escribe nada.
"""

from __future__ import annotations

import argparse
import datetime as dt
import re
import sys

# Anclas: nombre -> (regex de inicio, regex de fin opcional)
ANCHORS = [
    ("kernel_start", r"kernel started \(headless mode\)", None),
    ("config", r"Loading config from:", r"Config loaded and cached"),
    ("integrity_db", r"\[Integrity\] Running startup database validation", r"\[Integrity\].*healthy"),
    ("memory_layer", r"Initializing Memory Layer", r"Mailbox schema initialized"),
    ("onnx_probe", r"ONNX Runtime compat probe", None),
    ("sparse_engine", r"BGE-M3 sparse engine ready", None),
    ("embed_preload", r"Pre-loading embedding model", r"engine ready \(ONNX\)"),
    ("embed_fingerprint", r"fingerprint sha256", None),
    ("guild_embeddings", r"Embedding guild:", r"Embedding guild:"),
    ("reranker", r"Loading Jina Turbo reranker", r"Jina Turbo reranker ready"),
    ("registry", r"Registry loaded:", None),
    ("kernel_banner", r"Kernel operational\.", None),
    ("spawn_always_on", r"\[Startup\] Spawning always-on guild", r"\[Startup\] Spawning always-on guild"),
    ("repo_map", r"Repo map built:", None),
    ("http_bind", r"HTTP Gateway listening on", None),
    ("auto_link", r"\[AutoLink\] Starting", None),
    ("federation_task", r"Federation auto-sync background task started", None),
    ("federation_cycle", r"Starting scheduled auto-sync cycle", None),
]

TS_RE = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+Z)")
REPO_RE = re.compile(r"Repo map built: (\d+) files, (\d+) dirs, (\d+) lines \((\d+)ms\)")


def parse_ts(line: str) -> dt.datetime | None:
    m = TS_RE.match(line)
    if not m:
        return None
    try:
        return dt.datetime.strptime(m.group(1), "%Y-%m-%dT%H:%M:%S.%fZ").replace(tzinfo=dt.timezone.utc)
    except ValueError:
        return None


def fmt(ts: dt.datetime | None) -> str:
    return ts.strftime("%H:%M:%S.%f")[:-3] if ts else "-"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", required=True, help="ruta a kernel.log")
    ap.add_argument("--since", default="", help="fecha ISO mínima de arranque (YYYY-MM-DD)")
    ap.add_argument("--until", default="", help="fecha ISO máxima de arranque (YYYY-MM-DD)")
    ap.add_argument("--max-boots", type=int, default=10)
    args = ap.parse_args()

    lines = open(args.log, encoding="utf-8", errors="replace").read().splitlines()
    starts = [i for i, l in enumerate(lines) if "kernel started (headless mode)" in l]
    if not starts:
        print("no se encontró ningún arranque", file=sys.stderr)
        return 1
    # El siguiente arranque empieza en su línea "Windows job object set" (justo
    # antes de "kernel started"); si no está, se corta en "kernel started".
    bounds = []
    for i in starts:
        prev = next((j for j in range(i - 1, max(i - 12, -1), -1) if "Windows job object set" in lines[j]), i)
        bounds.append(prev)
    bounds.append(len(lines))

    since = dt.date.fromisoformat(args.since) if args.since else None
    until = dt.date.fromisoformat(args.until) if args.until else None

    out = []
    for b in range(min(args.max_boots, len(starts))):
        s, e = starts[b], bounds[b + 1]
        t_start = parse_ts(lines[s])
        if t_start is None:
            continue
        day = t_start.date()
        if since and day < since:
            continue
        if until and day > until:
            continue
        rows = []
        repo_info = None
        for name, pat_start, pat_end in ANCHORS:
            rx_s = re.compile(pat_start)
            st = next((i for i in range(s, e) if rx_s.search(lines[i])), None)
            if st is None:
                rows.append((name, None, None))
                continue
            en = st
            if pat_end:
                rx_e = re.compile(pat_end)
                en = next((i for i in range(st, e) if rx_e.search(lines[i])), st)
            # Las fases multi-evento (guild embeddings, spawn always-on) duran
            # hasta su ÚLTIMA aparición dentro del arranque (antes del repo map).
            if name in ("guild_embeddings", "spawn_always_on"):
                limit = next((i for i in range(st, e) if "Repo map built:" in lines[i]), e)
                hits = [i for i in range(st, limit) if rx_s.search(lines[i])]
                if hits:
                    en = hits[-1]
            if name == "auto_link":
                en = next((i for i in range(st, e) if "CERO-LLM complete" in lines[i]), st)
            if name == "federation_cycle":
                en = st  # primer ciclo: solo su inicio
            t1, t2 = parse_ts(lines[st]), parse_ts(lines[en])
            rows.append((name, (st, t1), (en, t2)))
            if name == "repo_map":
                m = REPO_RE.search(lines[st])
                if m:
                    repo_info = (int(m.group(1)), int(m.group(2)), int(m.group(3)), int(m.group(4)), st, t1, t2)
        out.append((day, t_start, rows, repo_info, e))

    for day, t_start, rows, repo_info, e in out:
        print(f"\n=== ARRANQUE {t_start.isoformat()} ===")
        for name, st, en in rows:
            if st is None:
                print(f"{name:<20} {'-':<14} {'-':<14}")
                continue
            i1, t1 = st
            i2, t2 = en
            dur = f"{(t2 - t1).total_seconds():.2f}s" if (t1 and t2 and i2 != i1) else ("inicio" if t1 else "-")
            print(f"{name:<20} {fmt(t1)} L{i1+1:<7} {fmt(t2)} L{i2+1:<7} {dur}")
        if repo_info:
            nf, nd, nl, ms, _, t_end, _ = repo_info
            t_begin = t_end - dt.timedelta(milliseconds=ms) if t_end else None
            print(f"repo_map detalle: {nf} files / {nd} dirs / {nl} lines / {ms} ms"
                  f" -> inicio estimado {fmt(t_begin)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
