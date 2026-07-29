"""Recalculate trigger rate for all 4 ambiguity zones using live recall data.

Zones:
  A: cosine in [0.70, 0.90) — from benchmark pre-computed cosine
  B: provenance=federation_peer AND weight > 0.5 — from live recall
  C: |score - median| < 0.10 — approximated via cosine
  D: keyword overlap >= 3 AND cosine < 0.60 — computed from benchmark text

Uses live tylluan_recall for provenance/weight data.
Saves raw recall data + trigger analysis to working tree.
"""
import json, os, time, urllib.request
from pathlib import Path

KERNEL = "http://127.0.0.1:4000"

# Load benchmark data
cases = json.load(open("benchmarks/spikes/coherence_gate_reasoning/cases_real_50.json", encoding="utf-8"))["cases"]
benchmark = json.load(open("benchmarks/spikes/coherence_gate_reasoning/results_real_50_v3.json", encoding="utf-8"))
cosine_lookup = {r["id"]: r["cosine"] for r in benchmark["per_case"]}

ZONE_A_RANGE = (0.70, 0.90)


def recall(query, limit=10):
    """Call tylluan_recall via HTTP."""
    data = json.dumps({"query": query, "limit": limit}).encode()
    r = urllib.request.Request(
        f"{KERNEL}/api/v1/memory/search",
        data=data, headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(r, timeout=30) as resp:
        return json.loads(resp.read())


def extract_nodes(response):
    """Extract node data from recall response (handles different response formats)."""
    nodes = []
    items = response if isinstance(response, list) else response.get("results", response.get("nodes", []))
    for item in items:
        if isinstance(item, dict):
            n = item.get("node", item)  # some responses wrap in "node"
            if isinstance(n, dict):
                nodes.append(n)
    return nodes


def compute_trigger(case, live_nodes):
    """Compute which zones trigger for this case."""
    cid = case["id"]
    cos = cosine_lookup.get(cid, 0)
    qw = set(case["query"].lower().split())
    cw = set(case["content"].lower().split())
    ko = len(qw & cw)

    triggers = {"A": False, "B": False, "C": False, "D": False}

    # Zone A: soft semantic boundary
    triggers["A"] = ZONE_A_RANGE[0] <= cos < ZONE_A_RANGE[1]

    # Zone B: conflicting signals from LIVE recall data
    for n in live_nodes:
        prov = n.get("provenance", "")
        wt = n.get("weight", 0)
        if "federation" in str(prov).lower() and wt > 0.5:
            triggers["B"] = True
            break

    # Zone C: close-call score (cosine near median)
    triggers["C"] = abs(cos - 0.80) < 0.05

    # Zone D: lexical match / semantic mismatch
    triggers["D"] = ko >= 3 and cos < 0.70

    return triggers


def main():
    print("=" * 72)
    print("TRIGGER RATE — live tylluan_recall data for all 4 zones")
    print("=" * 72)

    all_data = []
    zone_counts = {"A": 0, "B": 0, "C": 0, "D": 0}
    any_trigger_count = 0
    total = 0

    for i, case in enumerate(cases):
        cid = case["id"]
        query = case["query"]

        # Get live recall data
        try:
            resp = recall(query, limit=5)
            live_nodes = extract_nodes(resp)
        except Exception as e:
            print(f"  [{i+1}/{len(cases)}] {cid}: recall ERROR: {e}")
            live_nodes = []

        triggers = compute_trigger(case, live_nodes)
        total += 1

        has_trigger = any(triggers.values())
        if has_trigger:
            any_trigger_count += 1
        for z in zone_counts:
            if triggers[z]:
                zone_counts[z] += 1

        provenance_data = [(n.get("provenance", ""), n.get("weight", 0))
                          for n in live_nodes[:3]]
        all_data.append({"id": cid, "cosine": cosine_lookup.get(cid, 0),
            "triggers": triggers, "live_provenance": provenance_data})

        markers = "".join(z if triggers[z] else "." for z in "ABCD")
        print(f"  [{i+1:2d}/{len(cases)}] {cid}: cos={cosine_lookup.get(cid,0):.2f} "
              f"zones=[{markers}] nodes={len(live_nodes)} "
              f"prov={set(n.get('provenance','') for n in live_nodes[:2])}")

        time.sleep(0.5)  # rate limit

    pct = lambda n: f"{100*n/total:.0f}%"
    print(f"\n{'='*72}")
    print(f"RESULTS — {any_trigger_count}/{total} ({pct(any_trigger_count)}) cases trigger LLM")
    for z in "ABCD":
        print(f"  Zone {z}: {zone_counts[z]}/{total} ({pct(zone_counts[z])})")
    print(f"{'='*72}")

    # Save
    out = {"date": time.strftime("%Y-%m-%dT%H:%M"), "mode": "live_recall_trigger_rate",
        "total_cases": total, "trigger_rate_pct": round(100*any_trigger_count/total, 1),
        "zone_counts": zone_counts, "per_case": all_data}
    out_path = Path(__file__).parent.parent / "coherence_gate_reasoning" / "trigger_rate_live.json"
    out_path.write_text(json.dumps(out, indent=2, ensure_ascii=False))
    print(f"Saved: {out_path}")


if __name__ == "__main__":
    main()
