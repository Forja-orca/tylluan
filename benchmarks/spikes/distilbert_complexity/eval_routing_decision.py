"""ADR-010 Punto A spike — routing DECISION accuracy, not classification accuracy.

Measures what the approved criterion (Coloquio turn 102/103) actually asks:
does DistilBERT improve the ROUTING DECISION (Direct/Reactive/Proactive via
cascade_action) vs the production baseline, not just classify faster?

Three routes on the same 44 real held-out intents (audit.db, hand-labeled):
  A. heuristic-only            -> cascade_action(score_complexity)  [production baseline]
  B. heuristic + DistilBERT    -> cascade_action(blend_with_mlp(heur, dist))  [ADR-010 §7.2 design]
  C. majority class            -> always Direct

GO/NO-GO (criterion): B must beat the better of (A, C) by >= 5 accuracy points,
AND distilbert p50 must stay < 20ms. No kernel code is touched by this script.
"""
import glob
import json
import os
import statistics
import sys
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
from sklearn.linear_model import LogisticRegression
from transformers import AutoTokenizer

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent / "scripts"))
from train_complexity_mlp import BENCHMARK_CASES, BOUNDARY_CASES, SYNTHETIC_VARIANTS  # noqa: E402

SPIKE_DIR = Path(__file__).parent
HELDOUT_PATH = SPIKE_DIR / "heldout_intents.json"
DEEP_LABELS_PATH = SPIKE_DIR / "deep_labels.json"

# Same excluded noise + hand labels as eval_production_heldout.py (unchanged).
EXCLUDED_NOISE = {
    "Commit 46e84a9.",
    "**Verificación**: TypeScript y Vite build 100% verdes.",
    "**Build en Producción**: `pnpm run build` y `tsc` 100% limpios en 26s.",
    "**UI Primitives Centralizados**:\n   - Creados `ProvenanceBadge.tsx` y `StatusPill.tsx` en `dashboard/src/components/ui/` para visualizaciones unificadas de procedencia de nodos y salud de servicios con estética glassmorphism.",
    "**UI Primitives Compartidos (#2)**:\n   - Creados y probados `ProvenanceBadge.tsx`, `StatusPill.tsx` y `ConfirmModal.tsx` en `dashboard/src/components/ui/`.\n   - `NodesTab` actualizado utilizando los nuevos componentes compartidos.",
    "**Code-Splitting y Lazy Loading (Pilares #1 y #2)**:\n   - Sub-tabs de `TeamConsolidated` y `GuildsConsolidated` refactorizados con `React.lazy` + `Suspense`.\n   - Tamaño inicial del chunk de `TeamConsolidated` reducido de **1.8 MB a 3.0 kB** (reducción del **99.8%**).\n   - `FleetTab`, `FederationTab`, `McpRegistryPanel`, `CollectiveTab` y `GuildsTab` ahora son chunks independientes cargados 100% bajo demanda.",
    "RAG Sanitizer marcado como solo citado de segunda mano, no verificado. Gracias Deep por las citas — todas reales, verificadas una por una antes de entrar al doc.",
    "Asigno brecha 2 (monster file handler_do/mod.rs, 2196 lineas) a Deep: refactor por familias de intent, sin cambiar comportamiento, validar con cargo test --lib (419 deben seguir en verde) + clippy. Briefing formal si lo necesitas.",
    "**Docs**: El CHANGELOG/README está al día hasta v0.13.0, pero no documenta aún A2A ni Identidad Persistente (timezones).",
    "**Code-Splitting y Lazy Loading (#1)**:\n   - `TeamConsolidated` y `GuildsConsolidated` code-splitteados en submódulos cargados estrictamente bajo demanda.\n   - Chunk inicial de `TeamConsolidated` reducido de 1.8 MB a 3.0 kB (-99.8%).",
    "lee data/active_port.json, y solo si ambos fallan cae a :3030. Mismo patron que ya usaban scheduler.py y coordinator.py. Esto es importante: si alguna maquina del equipo corre el kernel en otro puerto, el fix funciona igual sin tocar codigo.",
}
EXCLUDED_NOISE.discard("post message to coloquio general channel as Hermes acknowledging the port fix and responding to the team")

LABELS = {
    "check system CPU usage": "Direct",
    "lee equipo: offset=0 limit=50": "Direct",
    "list digest tools": "Direct",
    "explore filesystem": "Direct",
    "list coloquio tools": "Direct",
    "explore guild coloquio": "Direct",
    "explore coloquio_digest": "Direct",
    "explore memory": "Direct",
    "explore git": "Direct",
    "echo hola": "Direct",
    "check guild status for bash": "Direct",
    "explore coloquio": "Direct",
    "digest_all_channels": "Direct",
    "where git": "Direct",
    "explore coloquio read_channel": "Direct",
    "explore bash": "Direct",
    "echo hello": "Direct",
    "explore": "Direct",
    "git status --short": "Direct",
    "check system CPU usage and then search the web for tokio scheduler internals": "Reactive",
    "set GIT_TERMINAL_PROMPT=0 and then run git --version": "Reactive",
    "ls \"C:/Program Files/Git/cmd/git.exe\"": "Direct",
    "run git status && echo INJECTED-PROOF-OF-CONCEPT": "Reactive",
    "whats_new agent_id=agent limit=10": "Direct",
    "search the web for tokio scheduler internals": "Direct",
    "lee los ultimos 15 mensajes del canal equipo en coloquio": "Direct",
    "python -c \"import time; time.sleep(5)\"": "Direct",
    "ejecuta compilacion del kernel de Tylluan cargo build y verifica el resultado": "Reactive",
    "unload guild bash and then load it again": "Reactive",
    "listar archivos en crates/tylluan-kernel/src/memory/night/": "Direct",
    "post message to coloquio channel general": "Direct",
    "search the web for rust async runtime benchmarks": "Direct",
    "list all available guilds and tools": "Direct",
    "list files in crates/tylluan-kernel/src/memory/night/ with their sizes": "Direct",
    "check system CPU usage and then check system memory usage": "Reactive",
    "set GIT_TERMINAL_PROMPT=0 && git --version": "Direct",
    "explore coloquio guild to understand available channels and how to use it": "Direct",
    "timeout_secs=3 python -c \"import subprocess; subprocess.run(['python', '-c', 'import time; time.sleep(30)'])\"": "Direct",
    "post message to coloquio general channel as Hermes acknowledging the port fix and responding to the team": "Reactive",
    "python -c \"import sqlite3; c=sqlite3.connect('data/silva.db'); print(c.execute(\\\"SELECT COUNT(*) FROM recall_feedback WHERE useful != 0\\\").fetchone())\"": "Direct",
    "search the web for rust async runtime benchmarks and then search the web for python asyncio performance": "Reactive",
    "list the test files in crates/tylluan-kernel/src/memory/night/ to verify which NightConsolidation phases are already implemented": "Reactive",
    "diagnostica el estado de recall_feedback: cuantas filas resueltas (useful != 0) hay en la tabla ahora mismo, para saber si Fase 3 de ADR-011 (LightReranker cutover, requiere >=5000 filas) sigue bloqueada": "Proactive",
    "ejecuta: python -c \"import sqlite3; c=sqlite3.connect('data/silva.db'); print(c.execute('SELECT COUNT(*) FROM recall_feedback WHERE useful != 0').fetchone())\"": "Direct",
}

DISTILBERT_GLOB = os.path.expanduser(
    "~/.cache/huggingface/hub/models--Xenova--distilbert-base-uncased/snapshots/*/onnx/model_quantized.onnx"
)


def find_distilbert_path() -> str:
    matches = glob.glob(DISTILBERT_GLOB)
    if not matches:
        raise FileNotFoundError(f"DistilBERT ONNX model not found at {DISTILBERT_GLOB}")
    return matches[0]


def embed_batch(sess, tokenizer, texts):
    embeddings = []
    for text in texts:
        inputs = tokenizer(text, return_tensors="np", truncation=True, max_length=64)
        outputs = sess.run(None, {
            "input_ids": inputs["input_ids"].astype(np.int64),
            "attention_mask": inputs["attention_mask"].astype(np.int64),
        })
        last_hidden = outputs[0]
        mask = inputs["attention_mask"][..., None].astype(np.float32)
        pooled = (last_hidden * mask).sum(axis=1) / mask.sum(axis=1).clip(min=1e-9)
        embeddings.append(pooled[0])
    return np.array(embeddings, dtype=np.float32)


def cascade_action(score: float) -> str:
    if score >= 0.6:
        return "Proactive"
    if score >= 0.4:
        return "Reactive"
    return "Direct"


def blend_with_mlp(heuristic: float, mlp: float) -> float:
    return min(1.0, max(0.0, 0.6 * heuristic + 0.4 * mlp))


def main():
    raw = json.loads(HELDOUT_PATH.read_text(encoding="utf-8"))["intents"]
    kept = [i for i in raw if i not in EXCLUDED_NOISE]
    unlabeled = [i for i in kept if i not in LABELS]
    if unlabeled:
        print(f"ERROR: {len(unlabeled)} kept intents have no ground-truth label:")
        for u in unlabeled:
            print(f"  - {u!r}")
        sys.exit(1)

    # Heuristic scores from deep_labels.json (kernel score_complexity, generated 2026-07-27).
    deep = {d["intent"]: d for d in json.loads(DEEP_LABELS_PATH.read_text(encoding="utf-8"))}
    missing_heur = [i for i in kept if i not in deep]
    if missing_heur:
        print(f"ERROR: {len(missing_heur)} kept intents missing heuristic score:")
        for u in missing_heur:
            print(f"  - {u!r}")
        sys.exit(1)

    eval_labels = [LABELS[i] for i in kept]

    # ── Route A: heuristic-only (production baseline, kernel score_complexity) ──
    heur_actions = [deep[i]["action"] for i in kept]
    heur_correct = sum(a == e for a, e in zip(heur_actions, eval_labels))

    # ── Route C: majority class ──
    majority = max(set(eval_labels), key=eval_labels.count)
    maj_correct = sum(e == majority for e in eval_labels)

    # ── Route B: heuristic + DistilBERT (ADR-010 §7.2: blend_with_mlp 60/40) ──
    model_path = find_distilbert_path()
    print(f"DistilBERT ONNX: {model_path}")
    sess = ort.InferenceSession(model_path)
    tokenizer = AutoTokenizer.from_pretrained("distilbert-base-uncased")

    all_cases = BENCHMARK_CASES + BOUNDARY_CASES + SYNTHETIC_VARIANTS
    train_intents = [c[0] for c in all_cases]
    train_labels = [c[1] for c in all_cases]
    X_train = embed_batch(sess, tokenizer, train_intents)
    clf = LogisticRegression(max_iter=2000)
    clf.fit(X_train, train_labels)

    X_eval = embed_batch(sess, tokenizer, kept)
    probs = clf.predict_proba(X_eval)
    # Continuous complexity score from class probabilities, calibrated so the
    # cascade thresholds (0.4 / 0.6) land on the same semantic boundaries.
    p_direct, p_reactive, p_proactive = probs.T
    dist_scores = 0.1 * p_direct + 0.5 * p_reactive + 0.85 * p_proactive

    blend_scores = [blend_with_mlp(deep[i]["score"], s) for i, s in zip(kept, dist_scores)]
    blend_actions = [cascade_action(s) for s in blend_scores]
    blend_correct = sum(a == e for a, e in zip(blend_actions, eval_labels))

    # ── p50 latency (single-intent inference, real model, CPU) ──
    latencies = []
    for intent in kept:
        t0 = time.perf_counter()
        embed_batch(sess, tokenizer, [intent])
        latencies.append((time.perf_counter() - t0) * 1000.0)
    p50_ms = statistics.median(latencies)

    n = len(kept)
    heur_pct = 100.0 * heur_correct / n
    maj_pct = 100.0 * maj_correct / n
    blend_pct = 100.0 * blend_correct / n
    best_baseline = max(heur_pct, maj_pct)
    delta_vs_baseline = blend_pct - best_baseline

    print(f"\nKept intents: {n}")
    print(f"  Route A heuristic-only:        {heur_correct}/{n} ({heur_pct:.2f}%)")
    print(f"  Route C majority class ('{majority}'): {maj_correct}/{n} ({maj_pct:.2f}%)")
    print(f"  Route B heuristic+DistilBERT:  {blend_correct}/{n} ({blend_pct:.2f}%)")
    print(f"  DistilBERT p50 latency:        {p50_ms:.2f}ms")
    print(f"  Delta vs best baseline ({best_baseline:.2f}%): {delta_vs_baseline:+.2f} pts")

    passes_accuracy = delta_vs_baseline >= 5.0
    passes_latency = p50_ms < 20.0
    verdict = "GO" if (passes_accuracy and passes_latency) else "NO-GO"

    per_case = []
    for i, intent in enumerate(kept):
        per_case.append({
            "intent": intent[:80],
            "expected": eval_labels[i],
            "heuristic_score": round(deep[intent]["score"], 3),
            "heuristic_action": heur_actions[i],
            "distilbert_score": round(float(dist_scores[i]), 3),
            "blended_score": round(float(blend_scores[i]), 3),
            "blended_action": blend_actions[i],
            "blend_correct": blend_actions[i] == eval_labels[i],
        })

    result = {
        "date": "2026-08-21",
        "spike": "ADR-010 Punto A — routing decision accuracy (criterion: turn 102/103)",
        "mode": "real_distilbert_embedding_plus_logreg_routing_decision",
        "model_path": model_path,
        "evaluated_count": n,
        "route_a_heuristic_only_pct": round(heur_pct, 2),
        "route_c_majority_class_pct": round(maj_pct, 2),
        "majority_class": majority,
        "route_b_heuristic_plus_distilbert_pct": round(blend_pct, 2),
        "distilbert_p50_ms": round(p50_ms, 2),
        "delta_vs_best_baseline_pts": round(delta_vs_baseline, 2),
        "criterion_accuracy_5pts": passes_accuracy,
        "criterion_latency_20ms": passes_latency,
        "verdict": verdict,
        "verdict_basis": (
            f"Route B {blend_pct:.2f}% vs best baseline {best_baseline:.2f}% "
            f"(Δ {delta_vs_baseline:+.2f} pts, need >= +5) and p50 {p50_ms:.2f}ms "
            f"(need < 20ms). Prior classification-level NO-GO (2026-07-27: 75.0% vs "
            f"77.27% majority) is now measured at the decision level."
        ),
        "per_case": per_case,
    }
    out_path = SPIKE_DIR / "routing_decision_results.json"
    out_path.write_text(json.dumps(result, indent=2))
    print(f"\nResult saved: {out_path}")
    print(f"VERDICT: {verdict}")


if __name__ == "__main__":
    main()