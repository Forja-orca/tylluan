"""Evaluate DistilBERT+LogReg against the 55-intent production held-out set
(commit 4606f48, extracted from guild_audit_log / audit.db).

Ground-truth labels below were hand-assigned (Direct/Reactive/Proactive)
because guild_audit_log has no complexity label column -- this script does
NOT invent accuracy numbers without labels.

11 of the 55 raw entries were excluded: they are markdown report/status
fragments that leaked into the `intent` column against unrelated guilds
(git, bash, comfy_ui, code_reviewer) -- a real data-contamination bug in the
audit pipeline, reported separately in Coloquio, not a complexity-labeling
question. See EXCLUDED_NOISE below for the exact strings and why.

Decision rule (per team plan): >70% on this real set -> replace mlp_scorer
in routing.rs. <70% -> null result, documented, nothing touches production.
"""
import glob
import json
import os
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
from sklearn.linear_model import LogisticRegression
from transformers import AutoTokenizer

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent / "scripts"))
from train_complexity_mlp import BENCHMARK_CASES, BOUNDARY_CASES, SYNTHETIC_VARIANTS  # noqa: E402

HELDOUT_PATH = Path(__file__).parent / "heldout_intents.json"

# Excluded: markdown report/status fragments logged as `intent` against
# unrelated guilds (git/bash/comfy_ui/code_reviewer) -- audit pipeline bug,
# not real routing intents, so they have no meaningful complexity label.
EXCLUDED_NOISE = {
    "Commit 46e84a9.",
    "**Verificación**: TypeScript y Vite build 100% verdes.",
    "**Build en Producción**: `pnpm run build` y `tsc` 100% limpios en 26s.",
    "**UI Primitives Centralizados**:\n   - Creados `ProvenanceBadge.tsx` y `StatusPill.tsx` en `dashboard/src/components/ui/` para visualizaciones unificadas de procedencia de nodos y salud de servicios con estética glassmorphism.",
    "post message to coloquio general channel as Hermes acknowledging the port fix and responding to the team",  # note: this one IS a real intent, kept -- see below
    "**UI Primitives Compartidos (#2)**:\n   - Creados y probados `ProvenanceBadge.tsx`, `StatusPill.tsx` y `ConfirmModal.tsx` en `dashboard/src/components/ui/`.\n   - `NodesTab` actualizado utilizando los nuevos componentes compartidos.",
    "**Code-Splitting y Lazy Loading (Pilares #1 y #2)**:\n   - Sub-tabs de `TeamConsolidated` y `GuildsConsolidated` refactorizados con `React.lazy` + `Suspense`.\n   - Tamaño inicial del chunk de `TeamConsolidated` reducido de **1.8 MB a 3.0 kB** (reducción del **99.8%**).\n   - `FleetTab`, `FederationTab`, `McpRegistryPanel`, `CollectiveTab` y `GuildsTab` ahora son chunks independientes cargados 100% bajo demanda.",
    "RAG Sanitizer marcado como solo citado de segunda mano, no verificado. Gracias Deep por las citas — todas reales, verificadas una por una antes de entrar al doc.",
    "Asigno brecha 2 (monster file handler_do/mod.rs, 2196 lineas) a Deep: refactor por familias de intent, sin cambiar comportamiento, validar con cargo test --lib (419 deben seguir en verde) + clippy. Briefing formal si lo necesitas.",
    "**Docs**: El CHANGELOG/README está al día hasta v0.13.0, pero no documenta aún A2A ni Identidad Persistente (timezones).",
    "**Code-Splitting y Lazy Loading (#1)**:\n   - `TeamConsolidated` y `GuildsConsolidated` code-splitteados en submódulos cargados estrictamente bajo demanda.\n   - Chunk inicial de `TeamConsolidated` reducido de 1.8 MB a 3.0 kB (-99.8%).",
    "lee data/active_port.json, y solo si ambos fallan cae a :3030. Mismo patron que ya usaban scheduler.py y coordinator.py. Esto es importante: si alguna maquina del equipo corre el kernel en otro puerto, el fix funciona igual sin tocar codigo.",
}
# fix: the Hermes-acknowledgement line IS a real intent (post + acknowledge),
# don't drop it -- remove from noise set at runtime instead of editing above.
EXCLUDED_NOISE.discard("post message to coloquio general channel as Hermes acknowledging the port fix and responding to the team")

# Hand-labeled ground truth for the remaining genuine intents (Direct = single
# atomic action, Reactive = 2+ sequential dependent actions, Proactive =
# open-ended reasoning/design/diagnosis requiring judgment beyond execution).
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


def main():
    raw = json.loads(HELDOUT_PATH.read_text(encoding="utf-8"))["intents"]
    excluded = [i for i in raw if i in EXCLUDED_NOISE]
    kept = [i for i in raw if i not in EXCLUDED_NOISE]
    unlabeled = [i for i in kept if i not in LABELS]

    print(f"Raw entries: {len(raw)}")
    print(f"Excluded as audit-pipeline noise (not real intents): {len(excluded)}")
    print(f"Kept as genuine intents: {len(kept)}")
    if unlabeled:
        print(f"ERROR: {len(unlabeled)} kept intents have no ground-truth label:")
        for u in unlabeled:
            print(f"  - {u!r}")
        sys.exit(1)

    model_path = find_distilbert_path()
    print(f"DistilBERT ONNX: {model_path}")
    sess = ort.InferenceSession(model_path)
    tokenizer = AutoTokenizer.from_pretrained("distilbert-base-uncased")

    all_cases = BENCHMARK_CASES + BOUNDARY_CASES + SYNTHETIC_VARIANTS
    train_intents = [c[0] for c in all_cases]
    train_labels = [c[1] for c in all_cases]
    print(f"Training on {len(train_intents)} samples (BENCHMARK+BOUNDARY+SYNTHETIC, unchanged from 15cb585)...")
    X_train = embed_batch(sess, tokenizer, train_intents)
    clf = LogisticRegression(max_iter=2000)
    clf.fit(X_train, train_labels)

    print(f"\nEvaluating on {len(kept)} production held-out intents (audit.db, commit 4606f48, never in training)...")
    eval_labels = [LABELS[i] for i in kept]
    X_eval = embed_batch(sess, tokenizer, kept)
    preds = clf.predict(X_eval)

    correct = 0
    for intent, expected, pred in zip(kept, eval_labels, preds):
        ok = pred == expected
        correct += ok
        marker = "+" if ok else "-"
        short = intent[:55].replace("\n", " ") + ("..." if len(intent) > 55 else "")
        print(f"  {marker} expected={expected:>9} pred={pred:>9}  {short}")

    accuracy = 100 * correct / len(kept)
    print(f"\nDistilBERT+LogReg accuracy on real production held-out set: {correct}/{len(kept)} ({accuracy:.2f}%)")

    result = {
        "date": "2026-07-27",
        "mode": "real_distilbert_embedding_plus_logreg_production_heldout",
        "model_path": model_path,
        "source_commit": "4606f48",
        "raw_heldout_count": len(raw),
        "excluded_noise_count": len(excluded),
        "excluded_noise_reason": "markdown report/status fragments leaked into guild_audit_log.intent against unrelated guilds (git/bash/comfy_ui/code_reviewer) -- audit pipeline bug, reported separately",
        "evaluated_count": len(kept),
        "correct": int(correct),
        "accuracy_pct": round(accuracy, 2),
        "decision_threshold_pct": 70.0,
        "baseline_heuristic_plus_6feature_mlp_pct": 61.11,
        "verdict": "GO" if accuracy > 70.0 else "NO-GO",
        "verdict_basis": "accuracy_pct on real production held-out intents from audit.db, hand-labeled, never in training set",
    }
    out_path = Path(__file__).parent / "production_heldout_results.json"
    out_path.write_text(json.dumps(result, indent=2))
    print(f"\nResult saved: {out_path}")
    print(f"VERDICT: {result['verdict']}")


if __name__ == "__main__":
    main()
