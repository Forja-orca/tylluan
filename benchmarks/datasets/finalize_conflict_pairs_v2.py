#!/usr/bin/env python3
"""Genera benchmarks/human_truth_conflict_pairs_v2.json (muestra v2, Coloquio).

Requiere previamente build_conflict_pairs_v2.py (pool congelado en tmp) y los
dos ficheros de anotacion manual versionados junto a este script:
  - conflict_pairs_v2_contra.json  — 34 ediciones sinteticas sobre nodo real
  - conflict_pairs_v2_pairs.json   — 16 consistentes + 16 no relacionados

Contrato de v2 (encargo del TL):
  - >= 60 pares NUEVOS, sin repetir nodos ni contenido de v1;
  - >= 30 'contradice', >= 15 de cada otra clase;
  - la clase 'contradice' es SINTETICA (una afirmacion de un nodo real
    editada), porque las contradicciones reales casi no existen (v1 midio 0);
  - la ficha DEBE declarar que esa clase es sintetica y que NO se mezcla con
    v1 para calcular el baseline.

Validaciones que abortan la generacion si fallan: cuotas, unicidad de nodos
dentro de v2, solape de nodos/hashes/textos con v1, y que cada patron de
edicion tenga EXACTAMENTE un match en su nodo.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
POOL = sys.argv[1] if len(sys.argv) > 1 else str(
    Path(__import__("tempfile").gettempdir()) / "pairs_v2_pool.json"
)
DST = sys.argv[2] if len(sys.argv) > 2 else str(
    REPO_ROOT / "benchmarks" / "human_truth_conflict_pairs_v2.json"
)
V1 = REPO_ROOT / "benchmarks" / "human_truth_conflict_pairs_v1.json"

# Mismas reglas que benchmarks/datasets/anonymize_silva.py
RE_PATH_WIN = re.compile(r"[a-zA-Z]:\\(?:Users|users)\\[^\s\"'<>`,\)]+", re.IGNORECASE)
RE_PATH_UNIX = re.compile(r"/(?:home|Users)/[^\s\"'<>`,\)]+", re.IGNORECASE)
RE_API_KEY = re.compile(
    r"(?:sk-[a-zA-Z0-9_-]{20,}|ghp_[a-zA-Z0-9]{20,}|Bearer\s+[a-zA-Z0-9_\-\.]{20,})",
    re.IGNORECASE,
)
RE_EMAIL = re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,7}\b")
CTRL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f]")


def sanitize(text: str) -> str:
    text = CTRL.sub("", text)
    text = RE_API_KEY.sub("[REDACTED_API_KEY]", text)
    text = RE_PATH_WIN.sub("[ANONYMIZED_PATH]", text)
    text = RE_PATH_UNIX.sub("[ANONYMIZED_PATH]", text)
    text = RE_EMAIL.sub("agent@tylluan.local", text)
    return text


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def main() -> None:
    pool = json.load(open(POOL, encoding="utf-8"))
    contra = json.load(open(Path(__file__).parent / "conflict_pairs_v2_contra.json", encoding="utf-8"))
    pairs = json.load(open(Path(__file__).parent / "conflict_pairs_v2_pairs.json", encoding="utf-8"))
    v1 = json.load(open(V1, encoding="utf-8"))

    by_id = {n["node_id"]: n for n in pool}
    assert len(by_id) == len(pool), "nid duplicado en el pool"

    v1_nodes, v1_hashes, v1_texts = set(), set(), set()
    for r in v1["records"]:
        v1_nodes.add(r["source_id"])
        for side in ("target_a", "target_b"):
            v1_nodes.add(r[side]["node_id"])
            v1_hashes.add(r[side]["content_sha256"])
            v1_texts.add(r[side]["text"])

    def real_side(nid: str) -> dict:
        n = by_id[nid]  # KeyError si el nid no esta en el pool
        raw = n["text_raw"]
        assert len(raw) <= 300, f"{nid}: texto >300 chars, se publicaria truncado"
        return {
            "node_id": nid,
            "node_type": n["node_type"],
            "content_sha256": sha(raw),
            "content_len": len(raw),
            "text": sanitize(raw),
        }

    records = []
    used_nids = []

    # 1) contradice — edicion sintetica de UNA afirmacion de un nodo real
    for item in contra["items"]:
        n = by_id[item["nid"]]
        raw = n["text_raw"]
        edited, count = re.subn(item["pat"], item["rep"], raw, count=1)
        assert count == 1, f"{item['nid']}: patron con {count} matches (se exige 1): {item['pat']}"
        assert edited != raw
        a = real_side(item["nid"])
        records.append({
            "pair_id": f"cp2-{len(records) + 1:03d}",
            "origin": "synthetic_edit",
            "label": "contradice",
            "reason": item["reason"],
            "edge_type": None,
            "source_id": None,
            "target_a": a,
            "target_b": {
                "node_id": None,
                "synthetic": True,
                "edit_of": item["nid"],
                "edit_pattern": item["pat"],
                "edit_replacement": item["rep"],
                "node_type": n["node_type"],
                "content_sha256": sha(edited),
                "content_len": len(edited),
                "text": sanitize(edited),
            },
        })
        used_nids.append(item["nid"])

    # 2) consistente / no relacionado — DOS nodos reales
    for item in pairs["items"]:
        assert item["label"] in ("consistente", "no relacionado")
        a, b = real_side(item["a"]), real_side(item["b"])
        records.append({
            "pair_id": f"cp2-{len(records) + 1:03d}",
            "origin": "real_pair",
            "label": item["label"],
            "reason": item["reason"],
            "edge_type": None,
            "source_id": None,
            "target_a": a,
            "target_b": b,
        })
        used_nids.extend([item["a"], item["b"]])

    # ── Validaciones del encargo ─────────────────────────────────────────
    dist = {"contradice": 0, "consistente": 0, "no relacionado": 0}
    for r in records:
        dist[r["label"]] += 1
    assert len(records) >= 60, f"solo {len(records)} pares (min 60)"
    assert dist["contradice"] >= 30, dist
    assert dist["consistente"] >= 15, dist
    assert dist["no relacionado"] >= 15, dist
    assert len(set(used_nids)) == len(used_nids), "nid repetido dentro de v2"
    overlap_n = set(used_nids) & v1_nodes
    assert not overlap_n, f"nodos de v1 reutilizados: {sorted(overlap_n)[:5]}"
    v2_real_hashes = {r[s]["content_sha256"] for r in records for s in ("target_a", "target_b")
                      if r[s].get("node_id")}
    assert not (v2_real_hashes & v1_hashes), "hash de contenido de v1 reutilizado"
    v2_texts = {r[s]["text"] for r in records for s in ("target_a", "target_b")}
    assert not (v2_texts & v1_texts), "texto publicado identico al de v1"
    for r in records:
        assert len(r["reason"]) > 40, f"{r['pair_id']}: reason demasiado corta"
    pair_sigs = {(r["target_a"]["content_sha256"], r["target_b"]["content_sha256"]) for r in records}
    assert len(pair_sigs) == len(records), "firma de par duplicada en v2"

    baseline = max(dist.values()) / len(records)
    type_counts = {}
    for nid in used_nids:
        t = by_id[nid]["node_type"]
        type_counts[t] = type_counts.get(t, 0) + 1
    out = {
        "dataset": "human_truth_conflict_pairs",
        "version": 2,
        "created": "2026-10-18",
        "created_by": "Buffy (agente, worktree tylluan-wt-buffy)",
        "encargo": "Muestra v2 de verdad de campo para el duelo P2 (verificador NLI sobre "
                   "flag_contradiction_nodes): >=60 pares nuevos sin repetir nodos ni contenido "
                   "de v1, >=30 contradict y >=15 de cada otra clase, contradice construida "
                   "editando la afirmacion de un nodo real.",
        "ficha": {
            "origen": "data/silva.db de la instancia compartida (E:\\tylluan), abierta "
                      "EXCLUSIVAMENTE con file:...?mode=ro (SQLite read-only; "
                      "build_conflict_pairs_v2.py no puede escribir). Pool extraido 2026-10-18 "
                      "con la BD viva: 211 nodos elegibles (episode 70, lesson 30, "
                      "session_digest 30, routing_anchor 20, coloquio_memory 44, "
                      "agent_reputation 17) tras el filtro 150-300 chars (texto completo "
                      "publicado, sin extractos truncados) y la exclusion de TODO nodo/hash de "
                      "human_truth_conflict_pairs_v1. Semilla Random(20261019); cuotas de pool "
                      "fijadas por recuento de elegibles ANTES de leer contenidos.",
            "clase_contradice_sintetica": (
                "LA CLASE contradict DE v2 ES SINTETICA AL 100% (34/34): cada lado B se "
                "construyo editando UNA afirmacion del contenido real de un nodo del pool "
                "(sustitucion regex con exactamente 1 match, verificado al generar) por su "
                "contrario — metrica, exito/fallo, existencia de canal, pertenencia a "
                "allow-list, decision documentada, etc. Esas afirmaciones editadas NO existen "
                "en la BD ni en ningun registro del sistema: son material de prueba creado por "
                "Buffy porque las contradicciones reales casi no existen (v1 midio 0 en 100 "
                "pares). Toda metrica publicada debe etiquetar esta clase como sintetica."
            ),
            "baseline_sin_v1": (
                "PROHIBIDO mezclar este dataset con human_truth_conflict_pairs_v1 para calcular "
                "el baseline: v2 introduce 34 contradice sinteticas que cambian la distribucion "
                "por construccion. El baseline de v2 se calcula SOLO sobre estos 66 pares "
                f'(siempre {max(dist, key=dist.get)} = {baseline:.1%}); v1 conserva su propio '
                "baseline declarado en su ficha (siempre consistente = 91%, sobre sus 100 pares "
                "reales). Mezclarlos produciria una cifra que no describe ninguno de los dos."
            ),
            "muestreo": "98 nodos distintos usados una sola vez cada uno (34 en clase "
                        "contradice + 64 en los 32 pares de nodos reales). Los pares "
                        "consistente/noRelacionado se emparejaron a mano desde el pool leyendo "
                        "el contenido: criterio NLI de 3 vias fijado antes de etiquetar "
                        "(contradice = no pueden ser ciertas a la vez sobre el mismo aspecto; "
                        "consistente = mismo sujeto y compatibles; no relacionado = temas sin "
                        "bearings proposicional).",
            "quien_etiqueto": "Buffy (agente de la flota), razon individual por pareja en "
                              "'reason'. Un solo anotador, sin acuerdo inter-anotador (no hay "
                              "kappa). NO es etiquetado humano: declararlo asi en cualquier "
                              "metrica publicada (misma salvedad que v1 y el dataset de intents).",
            "sanitizacion": "Textos saneados con las regex de benchmarks/datasets/"
                            "anonymize_silva.py (API keys, rutas absolutas, emails) + strip de "
                            "caracteres de control; todos los textos miden <=300 chars y se "
                            "publican COMPLETOS (a diferencia de v1, sin depender de la BD viva "
                            "para recuperar el texto); content_sha256 = SHA-256 del contenido "
                            "CRUDO pre-saneado.",
            "composicion": {"por_origen": {"synthetic_edit": dist["contradice"],
                                           "real_pair": dist["consistente"] + dist["no relacionado"]},
                            "por_nodo": dict(sorted(type_counts.items()))},
            "distribucion": dist,
            "baseline_trivial": f"siempre {max(dist, key=dist.get)} = {baseline:.1%} "
                                "(calculado SOLO sobre v2; ver baseline_sin_v1)",
            "validacion": {
                "pares": len(records),
                "nodos_unicos_en_v2": len(set(used_nids)),
                "solape_nodos_con_v1": len(set(used_nids) & v1_nodes),
                "solape_hashes_con_v1": len(v2_real_hashes & v1_hashes),
                "solape_textos_con_v1": len(v2_texts & v1_texts),
                "firmas_par_duplicadas": len(records) - len(pair_sigs),
                "ediciones_con_un_solo_match": dist["contradice"],
            },
            "sesgos_y_limitaciones": [
                "ANOTADOR NO HUMANO (agente), un solo anotador — acuerdo inter-anotador no medible.",
                "CLASE contradict SINTETICA (34/34): las ediciones invierten el valor o el "
                "signo de una afirmacion, lo que deja marcas lexicas faciles (negacion, "
                "numero opuesto, ✅/❌): un clasificador puede resolverla por atajo en vez de "
                "por inferencia. Es material de prueba, NO prevalencia real de contradicciones "
                "en SilvaDB (v1 midio 0/100 reales).",
                "NO MEZCLAR CON v1 para baseline, distribucion ni ninguna cifra agregada: v1 es "
                "muestra real de fan-out (91% consistente) y v2 es muestra construida (51.5% "
                "contradice sintetico) — poblaciones distintas por diseño.",
                "Pares de nodos reales del mismo evento (mensajes espejo del coloquio, digests "
                "duplicados) -> correlacion entre muestras; si hay split train/test, agrupar por "
                "evento/nodo, nunca por fila.",
                "BD viva: re-ejecutar build_conflict_pairs_v2.py hoy puede dar otro pool; este "
                "fichero versionado (con hashes) es la fuente de verdad del dataset.",
                "Cuotas de tipo de nodo fijadas por recuento antes de leer contenidos, pero la "
                "eleccion de QUE nodos van a cada clase y las ediciones fueron decision del "
                "anotador tras leer el pool: sesgo de seleccion declarado.",
            ],
        },
        "records": records,
    }
    with open(DST, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)
    print(json.dumps({"out": DST, "records": len(records), "dist": dist,
                      "baseline": f"{baseline:.1%}", "nodos": len(set(used_nids)),
                      "overlap_v1": 0}, ensure_ascii=False))


if __name__ == "__main__":
    main()
