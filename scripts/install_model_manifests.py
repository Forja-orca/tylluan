#!/usr/bin/env python3
"""Instala los manifiestos de rol D-156 en `<models_dir>/<id>/model.json`.

El contrato vive en `crates/tylluan-kernel/src/models.rs`: `GET /api/v1/models`
declara el rol de cada carpeta leyendo su `model.json`, sin adivinar por el
nombre. Este script copia los manifiestos versionados en
`scripts/model-manifests/` a la carpeta de modelos real.

Uso:
    python scripts/install_model_manifests.py --models-dir models           # simulacion
    python scripts/install_model_manifests.py --models-dir E:\\tylluan\\models --apply
    python scripts/install_model_manifests.py --models-dir ... --apply --force

Sin `--apply` no escribe nada (solo dice que haria). Sin `--force` no pisa un
`model.json` que ya exista: un rol declarado a mano por el operador manda.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

MANIFESTS_DIR = Path(__file__).resolve().parent / "model-manifests"


def load_manifests() -> list[tuple[str, dict]]:
    manifests = []
    for path in sorted(MANIFESTS_DIR.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        manifests.append((path.stem, data))
    return manifests


def main() -> int:
    parser = argparse.ArgumentParser(description="Instala los manifiestos D-156 (role por carpeta de modelo).")
    parser.add_argument("--models-dir", default="models", help="carpeta de modelos (por defecto: models, relativa al cwd)")
    parser.add_argument("--apply", action="store_true", help="escribir de verdad (sin esto es una simulacion)")
    parser.add_argument("--force", action="store_true", help="pisar un model.json existente")
    args = parser.parse_args()

    models_dir = Path(args.models_dir)
    if not models_dir.is_dir():
        print(f"ERROR: no existe la carpeta de modelos: {models_dir}", file=sys.stderr)
        return 2

    manifests = load_manifests()
    if not manifests:
        print(f"ERROR: no hay manifiestos en {MANIFESTS_DIR}", file=sys.stderr)
        return 2

    written = kept = missing_dir = 0
    for model_id, manifest in manifests:
        target_dir = models_dir / model_id
        role = manifest.get("role")
        if not target_dir.is_dir():
            print(f"OMITIDO   {model_id:<32} no existe la carpeta (role={role})")
            missing_dir += 1
            continue
        target = target_dir / "model.json"
        if target.exists() and not args.force:
            print(f"YA EXISTE {model_id:<32} {target} (declarado en disco; usa --force para pisarlo)")
            kept += 1
            continue
        if args.apply:
            payload = json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
            # bytes explicitos: write_bytes no anade BOM y no depende de la
            # codificacion de la consola (incidente de `ab_rss\tylluan.toml`).
            target.write_bytes(payload.encode("utf-8"))
            print(f"ESCRITO   {model_id:<32} {target} role={role}")
            written += 1
        else:
            print(f"SIMULACION {model_id:<31} escribiria {target} role={role}")

    total = len(manifests)
    print(
        f"\ntotal={total} escritos={written} ya_existian={kept} sin_carpeta={missing_dir} "
        f"{'APLICADO' if args.apply else 'SIMULACION (usa --apply)'}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
