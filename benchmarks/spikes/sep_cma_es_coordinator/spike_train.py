"""sep-CMA-ES Coordinator Spike — ADR-010 Sec 6.5
Trains an MLP (10->8->1, ~97 params) via SepCMA to replace the fixed _plan()
in coordinator.py. Evaluates against the fixed pipeline baseline.

Fitness is measured via real HTTP calls to the kernel's POST /api/v1/do.
Port is resolved from data/active_port.json at startup (survives dynamic ports).

Usage:
  # Dry-run with simulated fitness (no kernel needed):
  python benchmarks/spikes/sep_cma_es_coordinator/spike_train.py --dry-run

  # Real run with kernel HTTP calls:
  python benchmarks/spikes/sep_cma_es_coordinator/spike_train.py

  # Eval-only with last saved weights:
  python benchmarks/spikes/sep_cma_es_coordinator/spike_train.py --eval-only
"""
import sys
import json
import time
import math
import re
import argparse
import http.client
import threading
import urllib.request
import urllib.error
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

import numpy as np
from cmaes import SepCMA

# --- Config ---
SPIKE_DIR = Path(__file__).resolve().parent
HELDOUT_FILE = SPIKE_DIR / "heldout_set.json"
OUT_DIR = SPIKE_DIR / "results"
OUT_DIR.mkdir(parents=True, exist_ok=True)

POP_SIZE = 15          # lambda
MAX_GENS = 100
EARLY_STOP_GENS = 20
SIGMA0 = 0.3
ALPHA_FAIL_PENALTY = 0.5

# Reuse coordinator.py patterns for _split_intent()
_CONNECTORS_RE = re.compile(
    r"\s+(?:then|and then|after that|finally|y luego|luego|despu.s|finalmente)\s+",
    re.IGNORECASE,
)
_NUMBERED_RE = re.compile(r"\s*\d+\.\s+")

_CTX_REFS_PATTERN = re.compile(
    r"\b(?:it|the\s+result|that|eso|el\s+resultado|them|those|ese\s+resultado)\b",
    re.IGNORECASE,
)

_SYNTHESIS_SIGNALS = [
    "synthesize", "synthesise", "synthesis", "summarize", "summarise",
    "summary", "sum up", "count", "explain", "describe", "analyze",
    "tell me", "generate", "produce", "create", "combine", "merge",
    "unify", "consolidate", "collect results", "gather results",
    "wrap up", "conclude", "finalize", "put it together", "put together",
    "list them", "list the", "list all", "show the", "show them",
    "show names", "print", "display",
    "generar resumen", "resumir", "sintetizar", "combinar", "unificar",
    "consolidar", "concluir", "finalizar", "dame un resumen",
    "resume todo", "contar", "lista", "listar", "explicar",
    "describir", "analizar", "mostrar", "imprimir",
]

def split_intent(intent):
    parts = _CONNECTORS_RE.split(intent)
    if len(parts) > 1:
        return [p.strip() for p in parts if p.strip()]
    parts = _NUMBERED_RE.split(intent)
    parts = [p.strip() for p in parts if p.strip()]
    if len(parts) > 1:
        return parts
    return [intent.strip()]

def is_synthesis(task):
    return any(s in task.lower() for s in _SYNTHESIS_SIGNALS)

def needs_prior_ctx(task):
    return bool(_CTX_REFS_PATTERN.search(task)) or is_synthesis(task)

# --- MLP definition ---
# Features per sub-task (10): len_norm, ctx_ref, synthesis, position, has_next,
#                             + 5 PCA-compressed embedding dims (use hash-based proxy)
# Architecture: 10 -> 8 (ReLU) -> 1 (sigmoid)  =>  88 + 9 = 97 params

def compute_features(sub_tasks, kernel_url=None):
    """Compute feature vectors for each sub-task. Returns list of np.array[10]."""
    n = len(sub_tasks)
    feats = []
    for i, task in enumerate(sub_tasks):
        f = np.zeros(10, dtype=np.float32)
        f[0] = min(len(task) / 500.0, 1.0)          # len_norm
        f[1] = 1.0 if needs_prior_ctx(task) else 0.0 # ctx_ref
        f[2] = 1.0 if is_synthesis(task) else 0.0    # synthesis
        f[3] = i / max(n, 1)                         # position
        f[4] = 1.0 if (i < n - 1) else 0.0           # has_next
        # Hash-based text proxy for PCA embedding (deterministic, no kernel call)
        h = hash(task) & 0xFFFFFFFF
        for j in range(5):
            f[5 + j] = ((h >> (j * 6)) & 0x3F) / 64.0  # 6 bits per dim -> 0..1
        feats.append(f)
    return feats

# --- MLP loader ---
def mlp_predict(features, weights, n_input=10, n_hidden=8):
    """Forward pass of 10->8->1 MLP. Returns list of scores [0..1] per sub-task."""
    # Reshape weights: first n_input*n_hidden + n_hidden for layer1,
    # then n_hidden*1 + 1 for layer2
    w1_end = n_input * n_hidden
    b1_end = w1_end + n_hidden
    w2_end = b1_end + n_hidden

    w1 = weights[:w1_end].reshape(n_input, n_hidden)
    b1 = weights[w1_end:b1_end]
    w2 = weights[b1_end:w2_end].reshape(n_hidden, 1)
    b2 = weights[w2_end:w2_end + 1]

    scores = []
    for f in features:
        x = f.reshape(1, -1)
        h = np.maximum(0, x @ w1 + b1)  # ReLU
        y = 1.0 / (1.0 + np.exp(-(h @ w2 + b2)))  # sigmoid
        scores.append(float(y[0, 0]))
    return scores

def plan_with_mlp(sub_tasks, weights):
    """Like coordinator._plan() but threshold-based using MLP scores.
    score > 0.5 => sequential (needs prior ctx), else parallel."""
    features = compute_features(sub_tasks)
    scores = mlp_predict(features, weights)

    plan = []
    par_batch = []
    for i, (task, score) in enumerate(zip(sub_tasks, scores)):
        if score > 0.5:
            # Sequential
            if par_batch:
                plan.append({"type": "parallel", "tasks": list(par_batch)})
                par_batch.clear()
            plan.append({"type": "sequential", "tasks": [(i, task)]})
        else:
            par_batch.append((i, task))
    if par_batch:
        plan.append({"type": "parallel", "tasks": list(par_batch)})
    return plan

# --- Fixed pipeline plan (baseline) ---
def plan_fixed(sub_tasks):
    """Original coordinator._plan() logic."""
    plan = []
    par_batch = []
    for i, task in enumerate(sub_tasks):
        if needs_prior_ctx(task):
            if par_batch:
                plan.append({"type": "parallel", "tasks": list(par_batch)})
                par_batch.clear()
            plan.append({"type": "sequential", "tasks": [(i, task)]})
        else:
            par_batch.append((i, task))
    if par_batch:
        plan.append({"type": "parallel", "tasks": list(par_batch)})
    return plan

# --- Kernel URL resolution (replicates coordinator.py pattern) ---
_THREAD_LOCAL = threading.local()

def _resolve_kernel_url() -> str:
    """Read active port from data/active_port.json. Falls back to :4000."""
    port_file = Path(__file__).resolve().parent.parent.parent.parent / "data" / "active_port.json"
    try:
        data = json.loads(port_file.read_text())
        port = data.get("port", 4000)
        return f"http://127.0.0.1:{port}"
    except Exception:
        return "http://127.0.0.1:4000"

KERNEL_URL = _resolve_kernel_url()

def _dispatch(sub_intent: str, agent_id: str = "coordinator-worker") -> tuple[str, float]:
    """Send a sub-task to the kernel via POST /api/v1/do. Returns (result_str, elapsed_ms)."""
    payload = json.dumps({"intent": sub_intent, "agent_id": agent_id}).encode()
    headers = {"Content-Type": "application/json"}
    t0 = time.perf_counter()
    try:
        req = urllib.request.Request(KERNEL_URL + "/api/v1/do", data=payload, headers=headers)
        with urllib.request.urlopen(req, timeout=150) as resp:
            body = resp.read().decode("utf-8", errors="replace")
        elapsed = (time.perf_counter() - t0) * 1000
        data = json.loads(body)

        # Extract text from kernel's MCP-style response:
        #   {"content": [...], "is_error": bool, "response": "..."}
        # Content items can be Annotated{raw:Text{text:"..."}} or plain strings.
        text_parts = []
        for item in data.get("content", []):
            if isinstance(item, dict):
                raw = item.get("raw") or item.get("text") or item
                if isinstance(raw, dict):
                    raw = raw.get("text") or raw.get("raw") or ""
                text_parts.append(str(raw))
            elif isinstance(item, str):
                text_parts.append(item)
        result = " | ".join(text_parts) if text_parts else data.get("response", "")

        if data.get("is_error"):
            result = "error: " + str(result)
        return str(result), elapsed
    except Exception as e:
        elapsed = (time.perf_counter() - t0) * 1000
        return f"error: {e}", elapsed

# --- Fitness evaluation (REAL HTTP calls to kernel) ---
def _is_failure(result: str) -> bool:
    """Detect errors. Mirrors coordinator.py:_is_failure()."""
    if not result or not result.strip():
        return True
    lowered = result.lower()
    return "error" in lowered or "failed" in lowered or "timeout" in lowered

def compute_fitness(sub_tasks, plan, kernel_url=None):
    """Execute sub-tasks against the REAL kernel according to the plan.
    Measures actual wall-clock time and actual success/failure per step.
    Parallel steps use ThreadPoolExecutor. Sequential steps run in order.
    Returns fitness = (n_success/n) * penalty / (wall_time_ms/1000 + 1)."""
    n = len(sub_tasks)
    if n == 0:
        return 0.0

    results = {}
    prev_result = ""
    t_start = time.perf_counter()

    for step in plan:
        if step["type"] == "parallel" and len(step["tasks"]) > 1:
            with ThreadPoolExecutor(max_workers=min(len(step["tasks"]), 4)) as pool:
                futures = {
                    pool.submit(_dispatch, task, "coordinator-worker"): (idx, task)
                    for idx, task in step["tasks"]
                }
                for future in as_completed(futures):
                    idx, task = futures[future]
                    try:
                        result, _elapsed = future.result()
                    except Exception as e:
                        result = f"error: {e}"
                    results[idx] = result
                    prev_result = result
        else:
            for idx, task in step["tasks"]:
                result, _elapsed = _dispatch(task)
                results[idx] = result
                prev_result = result

    wall_time_ms = (time.perf_counter() - t_start) * 1000
    n_success = sum(1 for i in range(n) if i in results and not _is_failure(results[i]))

    success_rate = n_success / n
    penalty = 1.0 - ALPHA_FAIL_PENALTY * max(0, n - n_success)
    fitness = (success_rate * penalty) / (wall_time_ms / 1000.0 + 1.0)
    return fitness

# --- Fitness evaluation (SIMULATED — for --dry-run) ---
def compute_fitness_simulated(sub_tasks, plan, kernel_url=None):
    """Simulated fitness: no kernel calls. Fast, deterministic, for validation only."""
    n = len(sub_tasks)
    total_time = 0.0
    n_success = n

    for step in plan:
        if step["type"] == "parallel":
            total_time += max(0.5, len(step["tasks"]) * 0.3)
        else:
            total_time += 1.0

    if n == 0:
        return 0.0
    success_rate = n_success / n
    penalty = 1.0 - ALPHA_FAIL_PENALTY * max(0, n - n_success)
    fitness = (success_rate * penalty) / (total_time + 1.0)
    return fitness

# --- SepCMA training ---
def train_sepcma(train_scenarios, dry_run=False):
    n_features = 10
    n_hidden = 8
    n_params = n_features * n_hidden + n_hidden + n_hidden * 1 + 1

    optimizer = SepCMA(mean=np.zeros(n_params), sigma=SIGMA0, bounds=None,
                       population_size=POP_SIZE)

    fitness_fn = compute_fitness_simulated if dry_run else compute_fitness

    best_fitness = -float("inf")
    best_weights = None
    no_improve_gens = 0
    history = []

    mode = "SIMULATED (--dry-run)" if dry_run else f"REAL HTTP -> {KERNEL_URL}"
    print(f"\nSepCMA: {n_params} params, pop={POP_SIZE}, max_gens={MAX_GENS}")
    print(f"Fitness: {mode}")
    print(f"Train scenarios: {len(train_scenarios)}")
    print("-" * 60)

    for gen in range(MAX_GENS):
        solutions = []
        for _ in range(optimizer.population_size):
            x = optimizer.ask()
            solutions.append(x)

        fitnesses = []
        for weights in solutions:
            total_f = 0.0
            for sc in train_scenarios:
                sub_tasks = sc["sub_tasks"]
                plan = plan_with_mlp(sub_tasks, weights)
                f = fitness_fn(sub_tasks, plan)
                total_f += f
            avg_f = total_f / max(len(train_scenarios), 1)
            fitnesses.append(avg_f)

        optimizer.tell(list(zip(solutions, fitnesses)))

        gen_best = max(fitnesses)
        gen_mean = float(np.mean(fitnesses))
        history.append({"gen": gen, "best": float(gen_best), "mean": float(gen_mean)})

        if gen_best > best_fitness:
            best_fitness = gen_best
            best_weights = solutions[fitnesses.index(gen_best)].copy()
            no_improve_gens = 0
        else:
            no_improve_gens += 1

        if gen % 10 == 0 or gen < 5:
            print(f"  gen {gen:3d} | best={gen_best:.4f} mean={gen_mean:.4f}")

        if no_improve_gens >= EARLY_STOP_GENS:
            print(f"\nEarly stop at gen {gen}: no improvement for {EARLY_STOP_GENS} generations")
            break

    print("-" * 60)
    print(f"Best fitness: {best_fitness:.4f}")
    return best_weights, best_fitness, history

# --- Evaluation ---
def evaluate(weights, scenarios, dry_run=False, label="eval"):
    fitness_fn = compute_fitness_simulated if dry_run else compute_fitness
    fixed_scores = []
    mlp_scores = []
    wins = 0
    losses = 0
    ties = 0

    for sc in scenarios:
        sub_tasks = sc["sub_tasks"]
        plan_f = plan_fixed(sub_tasks)
        plan_m = plan_with_mlp(sub_tasks, weights)

        f_fixed = fitness_fn(sub_tasks, plan_f)
        f_mlp = fitness_fn(sub_tasks, plan_m)

        fixed_scores.append(f_fixed)
        mlp_scores.append(f_mlp)

        if f_mlp > f_fixed:
            wins += 1
        elif f_mlp < f_fixed:
            losses += 1
        else:
            ties += 1

    n = len(scenarios)
    result = {
        "label": label,
        "count": n,
        "wins": wins,
        "losses": losses,
        "ties": ties,
        "win_rate": wins / n if n > 0 else 0.0,
        "mlp_mean_fitness": float(np.mean(mlp_scores)),
        "fixed_mean_fitness": float(np.mean(fixed_scores)),
        "per_scenario": [
            {
                "intent": sc["intent"][:100],
                "n_tasks": len(sc["sub_tasks"]),
                "fixed_fitness": float(f_fixed),
                "mlp_fitness": float(f_mlp),
            }
            for sc, f_fixed, f_mlp in zip(scenarios, fixed_scores, mlp_scores)
        ],
    }
    return result

# --- Main ---
def main():
    parser = argparse.ArgumentParser(description="sep-CMA-ES Coordinator Spike")
    parser.add_argument("--kernel", default="http://127.0.0.1:4000", help="Kernel URL (Tylluan default: 4000)")
    parser.add_argument("--max-gens", type=int, default=MAX_GENS)
    parser.add_argument("--eval-only", action="store_true", help="Skip training, evaluate last model")
    parser.add_argument("--dry-run", action="store_true", help="Simulate without kernel calls")
    args = parser.parse_args()

    with open(HELDOUT_FILE, encoding="utf-8") as f:
        data = json.load(f)

    train_scenarios = data["train"]
    heldout_scenarios = data["heldout"]

    print(f"Kernel: {KERNEL_URL}")
    print(f"Train: {len(train_scenarios)} | Held-out: {len(heldout_scenarios)}")
    print(f"Dry run (simulated fitness): {args.dry_run}")

    weights_file = OUT_DIR / "best_weights.npy"
    history_file = OUT_DIR / "training_history.json"
    eval_file = OUT_DIR / "evaluation.json"

    if args.eval_only and weights_file.exists():
        print("\n=== Eval-only mode ===")
        best_weights = np.load(weights_file)
        with open(history_file) as f:
            history = json.load(f)
        best_fitness = history[-1]["best"] if history else 0.0
    else:
        print("\n=== Training sep-CMA-ES ===")
        best_weights, best_fitness, history = train_sepcma(train_scenarios, dry_run=args.dry_run)
        np.save(weights_file, best_weights)
        with open(history_file, "w") as f:
            json.dump(history, f, indent=2)
        print(f"\nWeights saved: {weights_file}")
        print(f"History saved: {history_file}")

    if best_weights is None:
        print("No weights produced. Exiting.")
        sys.exit(1)

    print("\n=== Evaluation ===")

    # Evaluate on train set (sanity check)
    train_eval = evaluate(best_weights, train_scenarios, dry_run=args.dry_run, label="train")
    print(f"Train: {train_eval['wins']}W / {train_eval['losses']}L / {train_eval['ties']}T "
          f"(win_rate={train_eval['win_rate']:.1%})")
    print(f"  MLP fitness={train_eval['mlp_mean_fitness']:.4f}  "
          f"Fixed fitness={train_eval['fixed_mean_fitness']:.4f}")

    # Evaluate on held-out set (the real test)
    heldout_eval = evaluate(best_weights, heldout_scenarios, dry_run=args.dry_run, label="heldout")
    print(f"Held-out: {heldout_eval['wins']}W / {heldout_eval['losses']}L / {heldout_eval['ties']}T "
          f"(win_rate={heldout_eval['win_rate']:.1%})")
    print(f"  MLP fitness={heldout_eval['mlp_mean_fitness']:.4f}  "
          f"Fixed fitness={heldout_eval['fixed_mean_fitness']:.4f}")

    passed = heldout_eval["win_rate"] >= 0.6
    status = "PASS" if passed else "FAIL"
    print(f"\n{'='*60}")
    print(f"RESULT: {status} (threshold: 60% win rate)")
    print(f"{'='*60}")

    full_result = {
        "date": time.strftime("%Y-%m-%d"),
        "best_fitness": float(best_fitness),
        "passed": passed,
        "train_eval": train_eval,
        "heldout_eval": heldout_eval,
        "history": history,
        "params": {
            "pop_size": POP_SIZE,
            "max_gens": args.max_gens,
            "sigma0": SIGMA0,
            "n_params": len(best_weights),
            "early_stop_gens": EARLY_STOP_GENS,
        },
    }
    with open(eval_file, "w") as f:
        json.dump(full_result, f, indent=2)
    print(f"Evaluation saved: {eval_file}")

    return 0 if passed else 1

if __name__ == "__main__":
    sys.exit(main())
