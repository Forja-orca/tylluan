"""Real HTTP eval: SepCMA MLP vs Fixed pipeline on coordinator benchmark queries."""
import json, sys, time, numpy as np
from pathlib import Path

sys.path.insert(0, str(Path("benchmarks/spikes/sep_cma_es_coordinator")))
from spike_train import (
    plan_with_mlp, plan_fixed, compute_fitness, split_intent, KERNEL_URL
)

weights = np.load("benchmarks/spikes/sep_cma_es_coordinator/results/best_weights.npy")

# Coordinador benchmark queries (from coordinator_latencies.json)
# Excluding web_search queries to avoid API costs
scenarios = [
    {"intent": "check system CPU usage and then check system disk usage", "label": "cpu_and_disk"},
    {"intent": "check system CPU usage and then check system memory usage", "label": "cpu_and_memory"},
    {"intent": "check system CPU usage and then check system memory usage and then check system disk usage", "label": "three_metrics"},
]

print(f"Kernel: {KERNEL_URL}")
print(f"Evaluating {len(scenarios)} real scenarios with HTTP calls...")
print(f"Repeats per scenario: 2 (first run cold, second warm)")
print()

results = []
for sc in scenarios:
    intent = sc["intent"]
    label = sc["label"]
    sub = split_intent(intent)
    print(f"[{label}] {intent}")

    f_plan = plan_fixed(sub)
    m_plan = plan_with_mlp(sub, weights)

    # Fixed pipeline (2 repeats)
    fixed_times = []
    for r in range(2):
        t0 = time.perf_counter()
        f_fit = compute_fitness(sub, f_plan)
        fixed_times.append((time.perf_counter() - t0) * 1000)
        print(f"  Fixed run {r+1}: fitness={f_fit:.6f} ({fixed_times[-1]:.0f}ms)")

    # MLP pipeline (2 repeats)
    mlp_times = []
    for r in range(2):
        t0 = time.perf_counter()
        m_fit = compute_fitness(sub, m_plan)
        mlp_times.append((time.perf_counter() - t0) * 1000)
        print(f"  MLP   run {r+1}: fitness={m_fit:.6f} ({mlp_times[-1]:.0f}ms)")

    fixed_fit = compute_fitness(sub, f_plan)  # Already computed but re-evaluate
    mlp_fit = compute_fitness(sub, m_plan)

    if mlp_fit > fixed_fit:
        winner = "MLP"
    elif fixed_fit > mlp_fit:
        winner = "FIXED"
    else:
        winner = "TIE"

    print(f"  -> {winner}: MLP={mlp_fit:.6f} vs Fixed={fixed_fit:.6f}")
    print()

    results.append({
        "label": label,
        "intent": intent,
        "n_tasks": len(sub),
        "fixed_fitness": float(fixed_fit),
        "mlp_fitness": float(mlp_fit),
        "winner": winner,
        "fixed_warm": float(np.mean(fixed_times[1:])),
        "mlp_warm": float(np.mean(mlp_times[1:])),
    })

# Summary
wins = sum(1 for r in results if r["winner"] == "MLP")
losses = sum(1 for r in results if r["winner"] == "FIXED")
ties = sum(1 for r in results if r["winner"] == "TIE")
n = len(results)

print(f"=== REAL HTTP EVAL ({n} scenarios) ===")
print(f"{wins}W / {losses}L / {ties}T (win_rate={wins/n:.1%})")
print(f"MLP mean fitness: {np.mean([r['mlp_fitness'] for r in results]):.6f}")
print(f"Fixed mean fitness: {np.mean([r['fixed_fitness'] for r in results]):.6f}")
print(f"MLP warm latency: {np.mean([r['mlp_warm'] for r in results]):.0f}ms")
print(f"Fixed warm latency: {np.mean([r['fixed_warm'] for r in results]):.0f}ms")

passed = wins / n >= 0.6 if n > 0 else False
print(f"PASS: {passed} (threshold: 60%)")

out = Path("benchmarks/spikes/sep_cma_es_coordinator/results/real_eval_v2.json")
with open(out, "w") as f:
    json.dump({
        "date": "2026-07-26",
        "mode": "real_http_eval_v2",
        "kernel": KERNEL_URL,
        "passed": passed,
        "wins": wins, "losses": losses, "ties": ties,
        "win_rate": wins / n if n > 0 else 0.0,
        "mlp_mean_fitness": float(np.mean([r['mlp_fitness'] for r in results])),
        "fixed_mean_fitness": float(np.mean([r['fixed_fitness'] for r in results])),
        "scenarios": results,
    }, f, indent=2)
print(f"Saved: {out}")
