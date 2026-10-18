#!/usr/bin/env python3
"""Extracción de candidatos para pares de conflicto v2 (T1241+ Coloquio) — SOLO LECTURA.

Abre `data/silva.db` de la instancia compartida con `file:...?mode=ro`
(sqlite URI read-only: imposible escribir). A diferencia de v1, v2 NO se
construye del fan-out de `flag_contradiction_nodes`: la clase `contradice`
existe en v2 solo de forma SINTÉTICA (una afirmación real editada — ver
`finalize_conflict_pairs_v2.py`), porque en v1 salieron 0 contradicciones
reales en 100 pares. Esto se declara en la ficha del dataset y PROHÍBE mezclar
v1 y v2 para calcular el baseline.

Elegibilidad (decisiones fijadas ANTES de leer contenidos, por tipo de nodo):
  - tipo en POOL_QUOTA (narrativo/proposicional, sin código simbólico);
  - 150 <= len(content) <= 300 chars: el texto completo viaja en el JSON
    (sin depender de la BD viva ni de extractos truncados);
  - id NO usado en v1 (ni source_id ni lado) y content_sha256 NO usado en v1
    — doble exclusión de nodos y de contenido de human_truth_conflict_pairs_v1.

Selección determinista: orden canónico por id, barajado con Random(SEED) por
tipo, cuota fija POOL_QUOTA por tipo.

Salida: /tmp/pairs_v2_pool.json (texto CRUDO — el saneado con las regex de
benchmarks/datasets/anonymize_silva.py ocurre en el script finalize).
"""
import hashlib
import json
import random
import sqlite3
import sys
from pathlib import Path

DB = "E:/tylluan/data/silva.db"  # instancia compartida — SOLO LECTURA
REPO_ROOT = Path(__file__).resolve().parents[2]
V1 = REPO_ROOT / "benchmarks" / "human_truth_conflict_pairs_v1.json"
OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/pairs_v2_pool.json"
SEED = 20261019

# Cuotas de pool por tipo — fijadas por recuento de elegibles (150..300,
# sin solape con v1) ANTES de leer ningún contenido:
#   episode 339, lesson 127, session_digest 78, routing_anchor 67,
#   coloquio_memory 44 (rango estrecho), agent_reputation 17.
POOL_QUOTA = {
    "episode": 70,
    "lesson": 30,
    "session_digest": 30,
    "routing_anchor": 20,
    "coloquio_memory": 44,
    "agent_reputation": 17,
}
MIN_LEN, MAX_LEN = 150, 300


def v1_sets(path):
    v1 = json.load(open(path, encoding="utf-8"))
    nodes, hashes = set(), set()
    for r in v1["records"]:
        nodes.add(r["source_id"])
        for side in ("target_a", "target_b"):
            nodes.add(r[side]["node_id"])
            hashes.add(r[side]["content_sha256"])
    return nodes, hashes, len(v1["records"])


def main() -> None:
    v1_nodes, v1_hashes, v1_records = v1_sets(V1)
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    cur = con.cursor()

    stats = {"eligible": {}, "excluded_v1_id": 0, "excluded_v1_content": 0}
    pool = []
    for t, quota in POOL_QUOTA.items():
        ids = []
        for nid, content in cur.execute(
            "SELECT id, content FROM nodes WHERE type=?", (t,)
        ):
            c = content or ""
            h = hashlib.sha256(c.encode("utf-8")).hexdigest()
            if nid in v1_nodes:
                stats["excluded_v1_id"] += 1
                continue
            if h in v1_hashes:
                stats["excluded_v1_content"] += 1
                continue
            if not (MIN_LEN <= len(c) <= MAX_LEN):
                continue
            ids.append(nid)
        stats["eligible"][t] = len(ids)
        ids.sort()
        # Sanidad: el pool es limitado y los nodos únicos; si caen por debajo
        # de las cuotas de anotación (34+32+32=98) algo falló — abortar aquí,
        # no entregar un dataset incompleto.
        if len(ids) < quota:
            raise SystemExit(
                f"pool insuficiente para {t}: {len(ids)} < {quota} "
                f"(cuota de anotación exige el pool completo)"
            )
        random.Random(SEED).shuffle(ids)
        for nid in ids[:quota]:
            _type, content, upd = cur.execute(
                "SELECT type, content, updated_at FROM nodes WHERE id=?", (nid,)
            ).fetchone()
            txt = content or ""
            pool.append(
                {
                    "node_id": nid,
                    "node_type": _type,
                    "content_sha256": hashlib.sha256(txt.encode("utf-8")).hexdigest(),
                    "content_len": len(txt),
                    "updated_at": upd,
                    "text_raw": txt,
                }
            )

    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(pool, f, ensure_ascii=False, indent=1)

    total_nodes = cur.execute("SELECT COUNT(*) FROM nodes").fetchone()[0]
    print(
        json.dumps(
            {
                "out": OUT,
                "pool": len(pool),
                "by_type": {t: sum(1 for p in pool if p["node_type"] == t) for t in POOL_QUOTA},
                "stats": stats,
                "v1_records_excluded_against": v1_records,
                "db_total_nodes": total_nodes,
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()