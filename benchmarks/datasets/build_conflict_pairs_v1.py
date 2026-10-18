#!/usr/bin/env python3
"""Extracción de pares de conflicto (T1240, Coloquio) — SOLO LECTURA.

Abre `data/silva.db` de la instancia compartida con `file:...?mode=ro`
(sqlite URI read-only: imposible escribir). Reproduce el agrupamiento de
`flag_contradiction_nodes` (nodes.rs) sobre aristas ACTIVAS
(valid_until IS NULL, con las tres exclusiones del código):

  candidato = grupo (source, type) con >1 targets distintos cuyo source
  está hoy marcado `conflicted = 1`.

Selección determinista (reproducible con la misma BD):
  - estratificación por tipo de arista con cuota fija (QUOTA);
  - orden canónico por (source, type), barajado con Random(20261018);
  - dentro de cada grupo: los dos primeros targets por orden de id;
  - DEDUPE de pares: una firma (sha256(contenido A), sha256(contenido B))
    solo se acepta una vez (chucks idénticos indexados bajo documentos
    distintos); si el par ya existe se pasa al siguiente candidato de la
    misma cuota.

Salida: /tmp/pairs_raw.json (textos saneados con las regex de
benchmarks/datasets/anonymize_silva.py, truncados a 300 chars, con
sha256 del contenido CRUDO para verificación contra la BD).
"""
import collections
import hashlib
import json
import random
import re
import sqlite3
import sys

DB = "E:/tylluan/data/silva.db"  # instancia compartida — SOLO LECTURA
OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/pairs_raw.json"
SEED = 20261018
# Cuota estratificada: cobertura COMPLETA de los tipos con texto semántico
# (same_topic 7/9 grupos = los 9 con 2 pares idénticos tras dedupe de
# firma, remembers 4/4, contributed_to 1/1 = 12 pares ricos) y el resto
# proporcional entre los tipos simbólicos (defines 122 / imports 60 de
# 182 -> 59/29 de 88). Decisión de diseño previa a ver contenidos: los
# tipos se eligen por recuento de tipos de arista, nunca por el contenido.
QUOTA = {"defines": 59, "imports": 29, "same_topic": 7, "remembers": 4, "contributed_to": 1}

# Mismas reglas que benchmarks/datasets/anonymize_silva.py
RE_PATH_WIN = re.compile(r"[a-zA-Z]:\\(?:Users|users)\\[^\s\"'<>`,\)]+", re.IGNORECASE)
RE_PATH_UNIX = re.compile(r"/(?:home|Users)/[^\s\"'<>`,\)]+", re.IGNORECASE)
RE_API_KEY = re.compile(
    r"(?:sk-[a-zA-Z0-9_-]{20,}|ghp_[a-zA-Z0-9]{20,}|Bearer\s+[a-zA-Z0-9_\-\.]{20,})",
    re.IGNORECASE,
)
RE_EMAIL = re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,7}\b")


def sanitize(text: str) -> str:
    if not text:
        return ""
    text = RE_API_KEY.sub("[REDACTED_API_KEY]", text)
    text = RE_PATH_WIN.sub("[ANONYMIZED_PATH]", text)
    text = RE_PATH_UNIX.sub("[ANONYMIZED_PATH]", text)
    text = RE_EMAIL.sub("agent@tylluan.local", text)
    return text


def main() -> None:
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    cur = con.cursor()
    n_conf, n_nodes = cur.execute(
        "SELECT (SELECT COUNT(*) FROM nodes WHERE conflicted=1), (SELECT COUNT(*) FROM nodes)"
    ).fetchone()

    rows = cur.execute(
        """SELECT source, type, target, valid_from FROM edges
        WHERE type != 'related_to'
          AND NOT (type = 'member_of' AND source LIKE 'graphrag_summary:%')
          AND NOT (type = 'remembers' AND source LIKE 'agent_memory:%')
          AND valid_until IS NULL"""
    ).fetchall()
    groups = collections.defaultdict(list)
    for s, t, tg, _vf in rows:
        groups[(s, t)].append(tg)

    cand = []
    for key, u in groups.items():
        su = sorted(set(u))
        if len(su) > 1:
            r = cur.execute("SELECT conflicted FROM nodes WHERE id=?", (key[0],)).fetchone()
            if r and r[0] == 1:
                cand.append((key, su))

    by_type = collections.defaultdict(list)
    for key, su in cand:
        by_type[key[1]].append((key, su))

    rng = random.Random(SEED)
    selected = []
    seen_signatures = set()

    def content_hash(nid):
        r = cur.execute("SELECT content FROM nodes WHERE id=?", (nid,)).fetchone()
        txt = (r[0] if r and r[0] else "") or ""
        return hashlib.sha256(txt.encode("utf-8")).hexdigest()

    for t, q in QUOTA.items():
        pool = sorted(by_type[t])
        rng.shuffle(pool)
        taken = 0
        for key, su in pool:
            if taken >= q:
                break
            sig = (content_hash(su[0]), content_hash(su[1]))
            if sig in seen_signatures:
                continue
            seen_signatures.add(sig)
            selected.append((key, su[0], su[1]))
            taken += 1
        if taken < q:
            raise SystemExit(f"cuota imposible tras dedupe: {t} tiene {taken} < {q}")

    def node(nid):
        return cur.execute("SELECT type, content FROM nodes WHERE id=?", (nid,)).fetchone()

    out = []
    for i, ((src, et), ta, tb) in enumerate(selected, start=1):
        sr = node(src)
        ca = node(ta)
        cb = node(tb)

        def side(nid, c):
            txt = (c[1] if c else "") or ""
            return {
                "node_id": nid,
                "node_type": c[0] if c else None,
                "content_sha256": hashlib.sha256(txt.encode("utf-8")).hexdigest(),
                "content_len": len(txt),
                "text": sanitize(txt[:300]),
            }

        out.append(
            {
                "pair_id": f"cp-{i:03d}",
                "edge_type": et,
                "source_id": src,
                "source_type": sr[0] if sr else None,
                "target_a": side(ta, ca),
                "target_b": side(tb, cb),
                "label": None,
                "reason": None,
            }
        )

    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)
    print(
        json.dumps(
            {
                "conflicted_nodes": n_conf,
                "total_nodes": n_nodes,
                "candidate_groups": len(cand),
                "by_type": {k: len(v) for k, v in sorted(by_type.items())},
                "selected": len(out),
                "out": OUT,
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()
