"""Analyze MLP vs Fixed plan decisions on held-out set."""
import json, numpy as np, sys
from pathlib import Path
sys.path.insert(0, str(Path("benchmarks/spikes/sep_cma_es_coordinator")))
from spike_train import plan_with_mlp, plan_fixed

heldout = json.load(open("benchmarks/spikes/sep_cma_es_coordinator/heldout_set.json", encoding="utf-8"))
weights = np.load("benchmarks/spikes/sep_cma_es_coordinator/results/best_weights.npy")

different = 0
for sc in heldout["heldout"]:
    sub = sc["sub_tasks"]
    f_plan = plan_fixed(sub)
    m_plan = plan_with_mlp(sub, weights)

    f_par = sum(1 for s in f_plan if s["type"] == "parallel")
    f_seq = sum(1 for s in f_plan if s["type"] == "sequential")
    m_par = sum(1 for s in m_plan if s["type"] == "parallel")
    m_seq = sum(1 for s in m_plan if s["type"] == "sequential")

    if f_par != m_par or len(f_plan) != len(m_plan):
        different += 1
        print(f"  DIFFERENT: {sc['intent'][:80]}")
        print(f"    Fixed: {f_par}P/{f_seq}S in {len(f_plan)} blocks")
        print(f"    MLP:   {m_par}P/{m_seq}S in {len(m_plan)} blocks")
        print()

print(f"Total held-out: {len(heldout['heldout'])} | Plans differ: {different}")
print()

# What are the MLP weights doing? Check a few feature-score mappings.
print("MLP decision analysis (first 5 held-out):")
for i, sc in enumerate(heldout["heldout"][:5]):
    sub = sc["sub_tasks"]
    from spike_train import compute_features, mlp_predict
    feats = compute_features(sub)
    scores = mlp_predict(feats, weights)
    fixed_assign = []
    mlp_assign = []
    from spike_train import needs_prior_ctx
    for j, task in enumerate(sub):
        f_label = "SEQ" if needs_prior_ctx(task) else "PAR"
        m_label = "SEQ" if scores[j] > 0.5 else "PAR"
        fixed_assign.append(f"{task[:30]}:{f_label}")
        mlp_assign.append(f"{task[:30]}:{m_label}({scores[j]:.2f})")
    print(f"\n  [{i}] {sc['intent'][:60]}")
    print(f"    Fixed: {fixed_assign}")
    print(f"    MLP:   {mlp_assign}")
