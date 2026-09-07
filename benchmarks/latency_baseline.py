#!/usr/bin/env python3
"""CPU-only latency baseline for tylluan_recall / tylluan_do against the live kernel.
3 runs x (12 recall + 12 do) = 72 HTTP calls. Saves raw per-call latencies."""
import json, time, urllib.request, statistics, datetime, pathlib

BASE = "http://127.0.0.1:4000/api/v1/do"
OUT = pathlib.Path("benchmarks/results/latency_baseline_%s.json" % datetime.date.today().strftime("%Y%m%d"))

RECALL = [
    "sovereignty kernel memory architecture design principles",
    "que aprendimos sobre el router de desempate semantic",
    "SQLite schema migrations node_embeddings",
    "CoherenceGate hybrid classify opt-in flag",
    "portability roundtrip memory survives copy restart",
    "HTTP hang under reindex load root cause",
    "cycle_in_flight federation auto sync panic guard",
    "I-7 curated routing dataset scrapling canonical",
    "FSRS memory decay per node stability",
    "lifecycle transitions active quiet consolidated archived",
    "embed_batch_async blocking pool onnx inference",
    "agent summary superseded pruning drift ratio",
]

DO = [
    ("check if the configuration file tylluan.toml exists", "filesystem"),
    ("list the .py files inside crates/tylluan-kernel/src", "filesystem"),
    ("search indexed documentation for ADR-012 lifecycle", "search"),
    ("find all graph triples connecting entity SilvaDB with PageRank", "knowledge"),
    ("calculate the degree centrality of nodes in the knowledge graph", "knowledge"),
    ("read the latest 5 messages in the general coloquio channel", "coloquio"),
    ("post a message to channel general saying benchmark running", "coloquio"),
    ("what files are located inside the guilds/core directory", "filesystem"),
    ("inspect the schema of the node_embeddings table in data/silva.db", "database"),
    ("show the git log for the last 10 commits", "git"),
    ("check current cpu temperature ram usage and disk space", "system_metrics"),
    ("run an explain query plan on select from nodes", "database"),
]


def post(payload):
    t = time.perf_counter()
    try:
        req = urllib.request.Request(BASE, data=json.dumps(payload).encode(),
                                     headers={"Content-Type": "application/json"}, method="POST")
        urllib.request.urlopen(req, timeout=120).read()
        status = "ok"
    except Exception as e:
        status = "ERR:%s" % e
    return round((time.perf_counter() - t) * 1000, 2), status


def pct(v, p):
    s = sorted(v)
    return s[min(len(s) - 1, int(len(s) * p))]


results = {"recall": [], "do": []}
for run in range(3):
    for q in RECALL:
        ms, st = post({"tool": "tylluan_recall", "intent": q, "query": q, "agent_id": "ci-latency-bench"})
        results["recall"].append({"run": run, "query": q, "ms": ms, "status": st})
    for intent, guild in DO:
        ms, st = post({"tool": "tylluan_do", "intent": intent, "guild": guild, "agent_id": "ci-latency-bench"})
        results["do"].append({"run": run, "intent": intent, "ms": ms, "status": st})

summary = {}
for k in ("recall", "do"):
    v = [r["ms"] for r in results[k]]
    summary[k] = {"p50": pct(v, .5), "p95": pct(v, .95), "p99": pct(v, .99),
                  "min": min(v), "max": max(v), "mean": round(statistics.mean(v), 2), "n": len(v)}

out = {"generated": datetime.date.today().isoformat(),
       "kernel_commit": "667243f", "device": "cpu", "summary": summary, "raw": results}
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(json.dumps(out, indent=2), encoding="utf-8")
print(json.dumps(summary, indent=2))
print("saved:", OUT)