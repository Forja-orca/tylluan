"""Minimum-risk experiment: does a DistilBERT-embedding + logistic-regression
head beat the existing 6-feature MLP (61.11% on the 18-case benchmark,
delta +0.0% vs heuristic-only, commit f2148b4)?

Same discipline as the sep-CMA-ES spike (ADR-010 §6.5.9-10): real data, real
inference, honest null result if it doesn't beat the baseline -- nothing
touches production regardless of outcome.

Reuses the exact 372-sample dataset (18 benchmark + 11 boundary + synthetic
perturbations) from scripts/train_complexity_mlp.py instead of inventing a
new one. Trains a logistic-regression head on real DistilBERT ONNX
embeddings (not the raw MLP's hand-engineered 6 features), evaluates on the
same 18 held-out benchmark cases the MLP was scored against.

Usage:
  python benchmarks/spikes/distilbert_complexity_head/experiment.py
"""
import glob
import os
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
from sklearn.linear_model import LogisticRegression
from transformers import AutoTokenizer

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent / "scripts"))
from train_complexity_mlp import (  # noqa: E402
    BENCHMARK_CASES, BOUNDARY_CASES, SYNTHETIC_VARIANTS,
)

# Genuinely held-out intents -- NOT present in BENCHMARK_CASES, BOUNDARY_CASES,
# or SYNTHETIC_VARIANTS. Both train_complexity_mlp.py's own evaluation and the
# first run of this script trained on a superset that INCLUDED the 18-case
# benchmark, then "evaluated" on that same subset -- 100% was memorization,
# not generalization. This set exists to catch that honestly.
HELD_OUT_CASES = [
    ("clone the repo and check its status", "Direct"),
    ("what's in this directory", "Direct"),
    ("show disk usage", "Direct"),
    ("build the project with cargo", "Direct"),
    ("fetch remote branches, merge main, then run the test suite", "Reactive"),
    ("grep for TODO across the codebase and count matches per file", "Reactive"),
    ("compare CPU load across all containers and restart the slowest one", "Reactive"),
    ("design a caching layer, benchmark three approaches, and recommend one with tradeoffs", "Proactive"),
    ("audit the auth flow for vulnerabilities and draft a remediation plan", "Proactive"),
    ("summarize this week's commits, group by theme, and flag anything risky", "Proactive"),
]

DISTILBERT_GLOB = os.path.expanduser(
    "~/.cache/huggingface/hub/models--Xenova--distilbert-base-uncased/snapshots/*/onnx/model_quantized.onnx"
)


def find_distilbert_path() -> str:
    matches = glob.glob(DISTILBERT_GLOB)
    if not matches:
        raise FileNotFoundError(f"DistilBERT ONNX model not found at {DISTILBERT_GLOB}")
    return matches[0]


def embed_batch(sess, tokenizer, texts: list[str]) -> np.ndarray:
    """Mean-pooled DistilBERT embedding per text (real ONNX inference, no simulation)."""
    embeddings = []
    for text in texts:
        inputs = tokenizer(text, return_tensors="np", truncation=True, max_length=64)
        outputs = sess.run(None, {
            "input_ids": inputs["input_ids"].astype(np.int64),
            "attention_mask": inputs["attention_mask"].astype(np.int64),
        })
        last_hidden = outputs[0]  # [1, seq_len, 768]
        mask = inputs["attention_mask"][..., None].astype(np.float32)
        pooled = (last_hidden * mask).sum(axis=1) / mask.sum(axis=1).clip(min=1e-9)
        embeddings.append(pooled[0])
    return np.array(embeddings, dtype=np.float32)


def label_action(score: float) -> str:
    if score < 0.4:
        return "Direct"
    if score < 0.6:
        return "Reactive"
    return "Proactive"


def main():
    model_path = find_distilbert_path()
    print(f"DistilBERT ONNX: {model_path}")
    sess = ort.InferenceSession(model_path)
    tokenizer = AutoTokenizer.from_pretrained("distilbert-base-uncased")

    all_cases = BENCHMARK_CASES + BOUNDARY_CASES + SYNTHETIC_VARIANTS
    intents = [c[0] for c in all_cases]
    labels = [c[1] for c in all_cases]

    print(f"Embedding {len(intents)} training samples via DistilBERT (real inference)...")
    X = embed_batch(sess, tokenizer, intents)

    print("Training logistic regression head (3-class: Direct/Reactive/Proactive)...")
    clf = LogisticRegression(max_iter=2000)
    clf.fit(X, labels)

    print("\nEvaluating on the 18 real benchmark cases (same set as commit f2148b4)...")
    benchmark_intents = [c[0] for c in BENCHMARK_CASES]
    benchmark_labels = [c[1] for c in BENCHMARK_CASES]
    X_bench = embed_batch(sess, tokenizer, benchmark_intents)
    preds = clf.predict(X_bench)

    correct = 0
    for intent, expected, pred in zip(benchmark_intents, benchmark_labels, preds):
        ok = pred == expected
        correct += ok
        marker = "+" if ok else "-"
        short = intent[:55] + "..." if len(intent) > 55 else intent
        print(f"  {marker} expected={expected:>9} pred={pred:>9}  {short}")

    accuracy = 100 * correct / len(benchmark_intents)
    print(f"\nDistilBERT+LogReg accuracy (LEAKED -- benchmark cases were in training set): {correct}/{len(benchmark_intents)} ({accuracy:.2f}%)")
    print("NOTE: train_complexity_mlp.py's own 61.11% baseline has the identical leak")
    print("(trains on BENCHMARK_CASES+BOUNDARY_CASES+SYNTHETIC_VARIANTS, evaluates on")
    print("BENCHMARK_CASES, a subset of what it just trained on) -- so this number is")
    print("comparable to the baseline under the same bias, but neither proves generalization.")

    print(f"\nEvaluating on {len(HELD_OUT_CASES)} GENUINELY held-out intents (never seen in training)...")
    held_out_intents = [c[0] for c in HELD_OUT_CASES]
    held_out_labels = [c[1] for c in HELD_OUT_CASES]
    X_held_out = embed_batch(sess, tokenizer, held_out_intents)
    held_out_preds = clf.predict(X_held_out)
    held_out_correct = 0
    for intent, expected, pred in zip(held_out_intents, held_out_labels, held_out_preds):
        ok = pred == expected
        held_out_correct += ok
        marker = "+" if ok else "-"
        short = intent[:55] + "..." if len(intent) > 55 else intent
        print(f"  {marker} expected={expected:>9} pred={pred:>9}  {short}")
    held_out_accuracy = 100 * held_out_correct / len(HELD_OUT_CASES)
    print(f"\nDistilBERT+LogReg TRUE held-out accuracy: {held_out_correct}/{len(HELD_OUT_CASES)} ({held_out_accuracy:.2f}%)")

    result = {
        "date": "2026-07-27",
        "mode": "real_distilbert_embedding_plus_logreg",
        "model_path": model_path,
        "num_training_samples": len(intents),
        "leaked_eval_note": "benchmark_cases_accuracy is inflated -- those 18 cases were in the training set, same bias as the 61.11% MLP baseline",
        "benchmark_cases_accuracy_pct_LEAKED": round(accuracy, 2),
        "true_held_out_cases": len(HELD_OUT_CASES),
        "true_held_out_accuracy_pct": round(held_out_accuracy, 2),
        "baseline_heuristic_only_pct": 61.11,
        "baseline_heuristic_plus_6feature_mlp_pct": 61.11,
        "verdict": "GO" if held_out_accuracy > 61.11 else "NO-GO",
        "verdict_basis": "true_held_out_accuracy_pct, not the leaked benchmark number",
    }

    import json
    out_path = Path(__file__).parent / "results.json"
    out_path.write_text(json.dumps(result, indent=2))
    print(f"\nResult saved: {out_path}")
    print(f"VERDICT: {result['verdict']}")


if __name__ == "__main__":
    main()
