#!/usr/bin/env python3
"""Reproduce el recorrido de `RepoMap::build` (crates/tylluan-kernel/src/repo_map.rs)
fuera del kernel, con las MISMAS reglas de exclusión, y desglosa el coste.

Uso:
    python3 scripts/measure_repo_map_scan.py --root E:/tylluan
    python3 scripts/measure_repo_map_scan.py --root . --skip-extra data,logs

Reglas replicadas de `repo_map.rs`:
- `SKIP_DIRS` (línea 43): se omite el SUBÁRBOL entero cuyo nombre de directorio
  coincida exactamente (walkdir `filter_entry`).
- Todo fichero restante se cuenta como `total_files` sin filtrar por extensión.
- Para `total_lines` se hace `std::fs::read_to_string(path)`; si el fichero no es
  UTF-8 válido, Rust devuelve Err y cuenta 0 líneas (pero YA ha leído los bytes).
- Los `.rs` se leen una SEGUNDA vez para extraer identificadores.
- La extensión solo se usa para clasificar el lenguaje (mapa de líneas 112-130).

Salida: totales, tabla por directorio de primer nivel (ficheros, bytes, líneas,
% de líneas, no-UTF8), tabla por extensión y simulación de exclusiones extra.
Solo lectura: no escribe nada.
"""

from __future__ import annotations

import argparse
import os
import sys
import time

# Réplica literal de repo_map.rs (línea 43).
SKIP_DIRS = {
    ".git", "node_modules", "target", "__pycache__", ".venv",
    ".mypy_cache", ".pytest_cache", ".ruff_cache", ".hypothesis",
}

# Réplica del mapa extensión -> lenguaje (repo_map.rs, líneas 112-130).
LANG = {
    "rs": "Rust", "py": "Python", "ts": "TypeScript", "tsx": "TypeScript",
    "js": "JavaScript", "jsx": "JavaScript",
    "css": "CSS", "scss": "CSS", "sass": "CSS", "less": "CSS",
    "html": "HTML", "htm": "HTML", "json": "JSON", "toml": "TOML",
    "yaml": "YAML", "yml": "YAML", "md": "Markdown", "sql": "SQL",
    "sh": "Shell", "bash": "Shell", "bat": "Script", "cmd": "Script",
    "ps1": "Script", "proto": "Protobuf", "vue": "Vue", "svelte": "Svelte",
}


def walk(root: str, extra_skip: set[str]):
    """Devuelve (ficheros, directorios) con la semántica de walkdir.filter_entry."""
    files, dirs = [], 0
    stack = [root]
    while stack:
        cur = stack.pop()
        try:
            entries = list(os.scandir(cur))
        except OSError:
            continue
        for e in entries:
            try:
                is_dir = e.is_dir(follow_symlinks=False)
            except OSError:
                continue
            if is_dir:
                if e.name in SKIP_DIRS or e.name in extra_skip:
                    continue
                dirs += 1
                stack.append(e.path)
            else:
                files.append(e.path)
    return files, dirs


def count_lines(path: str) -> int:
    """Réplica de count_lines(): read_to_string + str::lines().count()."""
    try:
        with open(path, "rb") as fh:
            data = fh.read()
    except OSError:
        return 0
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return 0  # Rust read_to_string falla -> unwrap_or(0)
    # str::lines(): separa por \n y no cuenta una línea final vacía.
    return len(text.splitlines())


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".", help="raíz del workspace (como find_workspace_root)")
    ap.add_argument("--skip-extra", default="", help="dirs extra a simular, separados por coma")
    ap.add_argument("--no-lines", action="store_true", help="solo contar ficheros/bytes (rápido)")
    args = ap.parse_args()

    root = os.path.abspath(args.root)
    extra = {s.strip() for s in args.skip_extra.split(",") if s.strip()}
    t0 = time.time()
    files, dirs = walk(root, extra)
    walk_s = time.time() - t0

    per_dir: dict[str, dict] = {}
    per_ext: dict[str, dict] = {}
    total_lines = 0
    total_bytes = 0
    unreadable = 0
    t1 = time.time()
    for i, path in enumerate(files):
        rel = os.path.relpath(path, root)
        top = rel.split(os.sep)[0] if os.sep in rel else "<raíz>"
        ext = os.path.splitext(path)[1].lower().lstrip(".")
        try:
            size = os.path.getsize(path)
        except OSError:
            size = 0
        d = per_dir.setdefault(top, {"files": 0, "lines": 0, "bytes": 0, "non_utf8": 0})
        d["files"] += 1
        d["bytes"] += size
        total_bytes += size
        if not args.no_lines:
            lines = count_lines(path)
            if lines == 0 and size > 0:
                d["non_utf8"] += 1
                unreadable += 1
            d["lines"] += lines
            total_lines += lines
            lang = LANG.get(ext, "Other")
            e = per_ext.setdefault(lang, {"files": 0, "lines": 0})
            e["files"] += 1
            e["lines"] += lines
        if (i + 1) % 5000 == 0:
            print(f"  ... {i + 1}/{len(files)} ficheros", file=sys.stderr)
    read_s = time.time() - t1
    elapsed = time.time() - t0

    print(f"\nRAÍZ: {root}")
    print(f"reglas: SKIP_DIRS={sorted(SKIP_DIRS)}"
          + (f" + extra={sorted(extra)}" if extra else ""))
    print(f"totales: {len(files)} ficheros, {dirs} directorios, {total_bytes:,} bytes"
          + ("" if args.no_lines else f", {total_lines:,} líneas ({unreadable:,} ficheros no-UTF8)"))
    print(f"tiempo: recorrido {walk_s:.1f}s + lectura/recuento {read_s:.1f}s = {elapsed:.1f}s")

    print("\n== por directorio de primer nivel ==")
    print(f"{'directorio':<28} {'ficheros':>9} {'bytes':>14} {'lineas':>12} {'%lineas':>8} {'no-UTF8':>8}")
    for name, d in sorted(per_dir.items(), key=lambda kv: -kv[1]["bytes"]):
        pct = (100.0 * d["lines"] / total_lines) if total_lines else 0.0
        print(f"{name:<28} {d['files']:>9} {d['bytes']:>14,} {d['lines']:>12,} {pct:>7.2f}% {d['non_utf8']:>8}")

    if not args.no_lines:
        print("\n== por lenguaje ==")
        for lang, e in sorted(per_ext.items(), key=lambda kv: -kv[1]["lines"]):
            pct = (100.0 * e["lines"] / total_lines) if total_lines else 0.0
            print(f"{lang:<14} {e['files']:>9} ficheros {e['lines']:>12,} lineas {pct:>7.2f}%")

    # Top 15 ficheros por tamaño (para explicar el coste de lectura).
    print("\n== top 15 ficheros por tamaño ==")
    sized = []
    for path in files:
        try:
            sized.append((os.path.getsize(path), os.path.relpath(path, root)))
        except OSError:
            pass
    for size, rel in sorted(sized, reverse=True)[:15]:
        print(f"{size:>14,} bytes  {rel}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
