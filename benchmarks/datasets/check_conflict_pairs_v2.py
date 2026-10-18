#!/usr/bin/env python3
"""Verificador independiente de benchmarks/human_truth_conflict_pairs_v2.json.

NO reutiliza codigo del generador: re-lee el JSON entregado, el JSON de v1 y la
BD (file:...?mode=ro) y comprueba por su cuenta:
  1. cuotas del encargo (>=60 pares, >=30 contradict, >=15/15);
  2. cero solape de nodos, de hashes y de textos con v1;
  3. unicidad interna de nodos y firmas de par;
  4. la clase contradict es sintetica: origin=synthetic_edit, node_id null,
     y cada edit_pattern reproduce EXACTAMENTE el texto sintetico publicado a
     partir del lado A publicado; ademas se re-reproduce desde el contenido
     crudo de la BD en vivo cuando ese nodo no ha derivado (la BD viva puede
     evolucionar: la deriva se reporta, solo falla si es masiva);
  5. la ficha declara la sinteticidad y la prohibicion de mezclar con v1, y el
     baseline declarado coincide con el calculado SOLO sobre v2.

Sale con codigo != 0 y un mensaje si algo falla.
"""
import hashlib
import json
import re
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
V2 = ROOT / "benchmarks" / "human_truth_conflict_pairs_v2.json"
V1 = ROOT / "benchmarks" / "human_truth_conflict_pairs_v1.json"
DB = "E:/tylluan/data/silva.db"

RE_PATH_WIN = re.compile(r"[a-zA-Z]:\\(?:Users|users)\\[^\s\"'<>`,\)]+", re.IGNORECASE)
RE_PATH_UNIX = re.compile(r"/(?:home|Users)/[^\s\"'<>`,\)]+", re.IGNORECASE)
RE_API_KEY = re.compile(
    r"(?:sk-[a-zA-Z0-9_-]{20,}|ghp_[a-zA-Z0-9]{20,}|Bearer\s+[a-zA-Z0-9_\-\.]{20,})",
    re.IGNORECASE,
)
RE_EMAIL = re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,7}\b")
CTRL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f]")


def sanitize(t: str) -> str:
    t = CTRL.sub("", t)
    t = RE_API_KEY.sub("[REDACTED_API_KEY]", t)
    t = RE_PATH_WIN.sub("[ANONYMIZED_PATH]", t)
    t = RE_PATH_UNIX.sub("[ANONYMIZED_PATH]", t)
    return RE_EMAIL.sub("agent@tylluan.local", t)


def sha(t: str) -> str:
    return hashlib.sha256(t.encode("utf-8")).hexdigest()


def fail(msg: str) -> None:
    print(f"FAIL: {msg}")
    sys.exit(1)


def main() -> None:
    v2 = json.load(open(V2, encoding="utf-8"))
    v1 = json.load(open(V1, encoding="utf-8"))
    recs = v2["records"]
    f = v2["ficha"]

    # 1. cuotas
    dist = {"contradice": 0, "consistente": 0, "no relacionado": 0}
    for r in recs:
        if r["label"] not in dist:
            fail(f"etiqueta desconocida {r['label']}")
        dist[r["label"]] += 1
    if len(recs) < 60:
        fail(f"{len(recs)} pares < 60")
    if dist["contradice"] < 30 or dist["consistente"] < 15 or dist["no relacionado"] < 15:
        fail(f"cuotas incumplidas: {dist}")

    # 2. solape con v1 (nodos, hashes, textos)
    v1n, v1h, v1t = set(), set(), set()
    for r in v1["records"]:
        v1n.add(r["source_id"])
        for s in ("target_a", "target_b"):
            v1n.add(r[s]["node_id"])
            v1h.add(r[s]["content_sha256"])
            v1t.add(r[s]["text"])
    v2n, v2h, v2t, pairs_sig = set(), set(), set(), set()
    for r in recs:
        for s in ("target_a", "target_b"):
            side = r[s]
            if side.get("node_id"):
                v2n.add(side["node_id"])
            v2h.add(side["content_sha256"])
            v2t.add(side["text"])
        if r.get("edit_of"):
            v2n.add(r["target_b"]["edit_of"])
        pairs_sig.add((r["target_a"]["content_sha256"], r["target_b"]["content_sha256"]))
    if v2n & v1n:
        fail(f"nodos de v1 reutilizados: {sorted(v2n & v1n)[:5]}")
    if v2h & v1h:
        fail(f"hashes de v1 reutilizados: {sorted(v2h & v1h)[:5]}")
    if v2t & v1t:
        fail("textos publicados identicos a los de v1")

    # 3. unicidad interna
    if len(pairs_sig) != len(recs):
        fail("firmas de par duplicadas en v2")
    real_nids = [r[s]["node_id"] for r in recs for s in ("target_a", "target_b")
                 if r[s].get("node_id")]
    if len(set(real_nids)) != len(real_nids):
        fail("nodo real reutilizado dentro de v2")

    # 4. sinteticidad: reproducible internamente y contra la BD (solo lectura)
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    cur = con.cursor()
    n_synth = 0
    for r in recs:
        b = r["target_b"]
        if r["label"] == "contradice":
            if r.get("origin") != "synthetic_edit" or b.get("node_id") is not None \
                    or not b.get("synthetic") or "edit_pattern" not in b:
                fail(f"{r['pair_id']}: contradict no marcado como edicion sintetica")
            edited_pub, n = re.subn(b["edit_pattern"], b["edit_replacement"],
                                    r["target_a"]["text"], count=1)
            if n != 1 or edited_pub != b["text"]:
                fail(f"{r['pair_id']}: lado B no reproduce como edicion del lado A publicado")
            n_synth += 1
        elif r.get("origin") != "real_pair" or b.get("synthetic"):
            fail(f"{r['pair_id']}: par real mal marcado")
        if len(r["reason"]) <= 40:
            fail(f"{r['pair_id']}: reason demasiado corta")

    # 4b. los 98 nodos reales existen hoy en la BD; la deriva de la BD viva
    # se reporta (fichero versionado = fuente de verdad) y solo falla si es masiva.
    pub_hash = {}
    for r in recs:
        for s in ("target_a", "target_b"):
            if r[s].get("node_id"):
                pub_hash[r[s]["node_id"]] = r[s]["content_sha256"]
    drift = []
    for nid, h in pub_hash.items():
        row = cur.execute("SELECT content FROM nodes WHERE id=?", (nid,)).fetchone()
        if not row:
            fail(f"nodo {nid} ya no existe en la BD")
        if sha(row[0] or "") != h:
            drift.append(nid)
    if len(drift) > 10:
        fail(f"deriva de {len(drift)} nodos vs BD viva (>10): posible error de extraccion")
    for r in recs:
        b = r["target_b"]
        if r["label"] != "contradice" or r["target_a"]["node_id"] in drift:
            continue
        raw = cur.execute("SELECT content FROM nodes WHERE id=?",
                          (b["edit_of"],)).fetchone()[0] or ""
        edited, n = re.subn(b["edit_pattern"], b["edit_replacement"], raw, count=1)
        if n != 1 or edited == raw or sha(edited) != b["content_sha256"] \
                or sanitize(edited) != b["text"]:
            fail(f"{r['pair_id']}: la edicion no se reproduce desde el crudo de la BD")

    # 5. ficha: declaraciones obligatorias + baseline solo v2
    ficha_txt = json.dumps(f, ensure_ascii=False)
    if "SINTETICA" not in ficha_txt.upper():
        fail("la ficha no declara la sinteticidad de la clase contradict")
    if "PROHIBIDO mezclar" not in ficha_txt and "NO MEZCLAR CON v1" not in ficha_txt:
        fail("la ficha no prohíbe mezclar con v1 para el baseline")
    declared = f.get("baseline_trivial", "")
    exp = max(dist.values()) / len(recs)
    if f"{exp:.1%}" not in declared:
        fail(f"baseline declarado '{declared}' != calculado sobre v2 {exp:.1%}")
    if f.get("distribucion") != dist:
        fail("distribucion declarada != real")
    if f.get("validacion", {}).get("solape_nodos_con_v1") != 0:
        fail("validacion de la ficha declara solape != 0")

    print(json.dumps({
        "status": "OK",
        "records": len(recs),
        "dist": dist,
        "baseline_v2_only": f"{exp:.1%}",
        "synthetic_contradice": n_synth,
        "nodos_unicos_v2": len(set(real_nids)),
        "overlap_v1": {"nodos": 0, "hashes": 0, "textos": 0},
        "bd": "solo lectura mode=ro; ediciones reproducidas desde la BD en los nodos sin drift",
        "bd_drift": drift,
    }, ensure_ascii=False))


if __name__ == "__main__":
    main()
