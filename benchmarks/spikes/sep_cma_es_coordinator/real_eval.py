"""Eval the SepCMA-trained MLP vs fixed pipeline against the REAL kernel (HTTP)."""
import json
import numpy as np
import sys
from pathlib import Path

sys.path.insert(0, str(Path("benchmarks/spikes/sep_cma_es_coordinator")))
from spike_train import plan_with_mlp, plan_fixed, compute_fitness

heldout = json.load(
    open("benchmarks/spikes/sep_cma_es_coordinator/heldout_set.json", encoding="utf-8")
)
weights = np.load(
    "benchmarks/spikes/sep_cma_es_coordinator/results/best_weights.npy"
)

# Filter: skip web search to avoid API costs, skip complex coloquio posts
simple = [
    s
    for s in heldout["heldout"]
    if "search the web" not in s["intent"].lower()
    and len(s["sub_tasks"]) <= 3
]
print(f"Evaluating {len(simple)} simple scenarios with REAL kernel HTTP calls...")
print()

wins = 0
losses = 0
ties = 0
fixed_fs = []
mlp_fs = []
per_scenario = []

for i, sc in enumerate(simple):
    sub = sc["sub_tasks"]
    label = sc["intent"][:70]
    print(f"  [{i+1}/{len(simple)}] {label}...", end=" ", flush=True)

    f_f = compute_fitness(sub, plan_fixed(sub))
    f_m = compute_fitness(sub, plan_with_mlp(sub, weights))

    fixed_fs.append(f_f)
    mlp_fs.append(f_m)

    if f_m > f_f:
        print(f"MLP WINS ({f_m:.4f} vs {f_f:.4f})")
        wins += 1
    elif f_m < f_f:
        print(f"FIXED WINS ({f_f:.4f} vs {f_m:.4f})")
        losses += 1
    else:
        print(f"TIE")
        ties += 1

    per_scenario.append(
        {
            "intent": sc["intent"][:100],
            "n_tasks": len(sub),
            "fixed_fitness": float(f_f),
            "mlp_fitness": float(f_m),
        }
    )

n = len(simple)
print()
print(f"=== REAL HTTP EVAL ({n} scenarios) ===")
print(f"{wins}W / {losses}L / {ties}T (win_rate={wins/n:.1%})")
print(f"MLP mean fitness: {np.mean(mlp_fs):.4f}")
print(f"Fixed mean fitness: {np.mean(fixed_fs):.4f}")

passed = wins / n >= 0.6 if n > 0 else False
print(f"PASS: {passed} (threshold: 60%)")

# Save result
result = {
    "date": "2026-07-26",
    "mode": "real_http_eval",
    "passed": passed,
    "wins": wins,
    "losses": losses,
    "ties": ties,
    "win_rate": wins / n if n > 0 else 0.0,
    "mlp_mean_fitness": float(np.mean(mlp_fs)),
    "fixed_mean_fitness": float(np.mean(fixed_fs)),
    "per_scenario": per_scenario,
}
out = Path("benchmarks/spikes/sep_cma_es_coordinator/results/real_eval.json")
with open(out, "w") as f:
    json.dump(result, f, indent=2)
print(f"Saved: {out}")
