"""ADR-011 CoherenceGate reasoning spike: does a real generative small model
(Qwen3.5-2B, already cached, unsloth/Qwen3.5-2B) add judgment the current
rule-based gate (cosine threshold 0.85, coherence_gate.rs) structurally
cannot -- specifically on cases where lexical/embedding similarity is
misleading (paraphrase with low overlap, or high overlap with false/wrong
content)?

Same discipline as every spike today: real inference (BGE-M3 for the
baseline embedding-threshold decision, real Qwen3.5-2B generation for the
reasoning decision), honest 16-case set with hand-labeled ground truth
written BEFORE running either model, compare against the majority-class
baseline (lesson learned from the DistilBERT retraction earlier today),
NO-GO documented honestly if it doesn't add value. Nothing touches
coherence_gate.rs regardless of outcome.

Usage:
  python benchmarks/spikes/coherence_gate_reasoning/experiment.py
"""
import json
import sys
import time
from collections import Counter
from pathlib import Path

import numpy as np
import torch
from transformers import AutoModel, AutoModelForCausalLM, AutoTokenizer

CASES_PATH = Path(__file__).parent / (sys.argv[1] if len(sys.argv) > 1 else "cases.json")
COHERENCE_THRESHOLD = 0.85  # same constant as coherence_gate.rs

QWEN_MODEL_ID = "unsloth/Qwen3.5-2B"
BGE_MODEL_ID = "BAAI/bge-m3"  # same embedding model Tylluan uses in production


def cosine(a, b):
    a, b = np.asarray(a), np.asarray(b)
    return float(np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b)))


def bge_embed(tok, model, texts):
    """Mean-pooled BGE-M3 embedding via plain transformers (no FlagEmbedding dep)."""
    embeddings = []
    for text in texts:
        inputs = tok(text, return_tensors="pt", truncation=True, max_length=256)
        with torch.no_grad():
            out = model(**inputs)
        last_hidden = out.last_hidden_state
        mask = inputs["attention_mask"].unsqueeze(-1).float()
        pooled = (last_hidden * mask).sum(1) / mask.sum(1).clamp(min=1e-9)
        embeddings.append(pooled[0].numpy())
    return embeddings


def label_to_keep(label: str) -> bool:
    # For the binary threshold gate and majority baseline, "keep_with_caveat"
    # counts as "keep" (content survives, gate has no third option today).
    return label in ("keep", "keep_with_caveat")


def main():
    cases = json.loads(CASES_PATH.read_text(encoding="utf-8"))["cases"]
    print(f"Loaded {len(cases)} hand-labeled cases")

    labels = [label_to_keep(c["human_label"]) for c in cases]
    dist = Counter(c["human_label"] for c in cases)
    print(f"Label distribution: {dict(dist)}")
    majority_keep = sum(labels)
    majority_acc = max(majority_keep, len(labels) - majority_keep) / len(labels)
    print(f"Majority-class baseline (always predict the more common outcome): {majority_acc*100:.2f}%")

    print("\nLoading BGE-M3 for the embedding-threshold baseline (same model Tylluan uses in production)...")
    t0 = time.time()
    bge_tok = AutoTokenizer.from_pretrained(BGE_MODEL_ID)
    bge_model = AutoModel.from_pretrained(BGE_MODEL_ID)
    bge_model.eval()
    print(f"BGE-M3 loaded in {time.time()-t0:.1f}s")

    queries = [c["query"] for c in cases]
    contents = [c["content"] for c in cases]
    q_embs = bge_embed(bge_tok, bge_model, queries)
    c_embs = bge_embed(bge_tok, bge_model, contents)

    threshold_preds = []
    threshold_cosims = []
    for qe, ce in zip(q_embs, c_embs):
        cos = cosine(qe, ce)
        threshold_cosims.append(cos)
        threshold_preds.append(cos >= COHERENCE_THRESHOLD)

    threshold_correct = sum(p == l for p, l in zip(threshold_preds, labels))
    threshold_acc = 100 * threshold_correct / len(cases)
    print(f"\nRule-based threshold (cosine >= {COHERENCE_THRESHOLD}) accuracy: {threshold_correct}/{len(cases)} ({threshold_acc:.2f}%)")

    print(f"\nLoading Qwen3.5-2B ({QWEN_MODEL_ID}) for the reasoning judgment (CPU, real inference, no simulation)...")
    t0 = time.time()
    tok = AutoTokenizer.from_pretrained(QWEN_MODEL_ID)
    model = AutoModelForCausalLM.from_pretrained(QWEN_MODEL_ID, torch_dtype=torch.float32)
    model.eval()
    print(f"Qwen3.5-2B loaded in {time.time()-t0:.1f}s")

    qwen_preds = []
    qwen_reasonings = []
    for i, c in enumerate(cases):
        prompt = (
            "You are a memory-relevance gate inside an AI agent's recall pipeline. "
            "Decide whether the CONTENT below is safe and relevant enough to feed into "
            "the agent's context to answer the QUERY.\n\n"
            "CRITICAL DISTINCTION -- the most common mistake is confusing TOPICAL "
            "PROXIMITY with ACTUAL RELEVANCE. Sharing a project name, keyword, or general "
            "subject with the QUERY is NOT enough to KEEP. The CONTENT must actually "
            "answer, resolve, or directly inform the specific question asked -- not just "
            "discuss something in the same neighborhood. Before deciding, ask yourself: "
            "'If I only had this CONTENT, could I actually answer the QUERY?' If the answer "
            "is no -- even if the topic overlaps -- REJECT.\n\n"
            "Also watch for: content that superficially shares keywords with the query but "
            "is actually about a different project/scope, content that contradicts known "
            "facts, or content phrased differently but semantically equivalent to the "
            "query's intent (paraphrases should KEEP even with low keyword overlap).\n\n"
            f"QUERY: {c['query']}\n"
            f"CONTENT: {c['content']}\n\n"
            "Respond with exactly one line in this format: DECISION: KEEP or DECISION: REJECT, "
            "then one short sentence of reasoning that explicitly states whether CONTENT "
            "answers QUERY or merely shares its topic."
        )
        msgs = [{"role": "user", "content": prompt}]
        inputs = tok.apply_chat_template(msgs, add_generation_prompt=True, return_tensors="pt", return_dict=True)
        with torch.no_grad():
            out = model.generate(**inputs, max_new_tokens=80, do_sample=False, pad_token_id=tok.eos_token_id)
        response = tok.decode(out[0][inputs["input_ids"].shape[1]:], skip_special_tokens=True).strip()
        keep = "KEEP" in response.upper().split("\n")[0] if response else False
        qwen_preds.append(keep)
        qwen_reasonings.append(response)
        marker = "+" if keep == labels[i] else "-"
        print(f"  [{i+1}/{len(cases)}] {marker} {c['id']}: {response[:100]!r}")

    qwen_correct = sum(p == l for p, l in zip(qwen_preds, labels))
    qwen_acc = 100 * qwen_correct / len(cases)
    print(f"\nQwen3.5-2B reasoning accuracy: {qwen_correct}/{len(cases)} ({qwen_acc:.2f}%)")

    per_case = []
    for c, cos, tpred, qpred, qreason in zip(cases, threshold_cosims, threshold_preds, qwen_preds, qwen_reasonings):
        per_case.append({
            "id": c["id"],
            "human_label": c["human_label"],
            "cosine": round(cos, 4),
            "threshold_decision": "keep" if tpred else "reject",
            "qwen_decision": "keep" if qpred else "reject",
            "qwen_reasoning": qreason,
        })

    result = {
        "date": "2026-07-27",
        "mode": "real_bge_m3_threshold_vs_real_qwen3.5-2b_reasoning",
        "model_path": QWEN_MODEL_ID,
        "num_cases": len(cases),
        "majority_class_baseline_pct": round(majority_acc * 100, 2),
        "rule_based_threshold_accuracy_pct": round(threshold_acc, 2),
        "qwen_reasoning_accuracy_pct": round(qwen_acc, 2),
        "per_case": per_case,
        "verdict": "GO" if qwen_acc > threshold_acc and qwen_acc > majority_acc * 100 else "NO-GO",
        "verdict_basis": "qwen_reasoning_accuracy_pct must beat BOTH the majority-class baseline and the current rule-based threshold to justify adding a generative model to CoherenceGate.",
    }
    out_path = Path(__file__).parent / (sys.argv[2] if len(sys.argv) > 2 else "results.json")
    out_path.write_text(json.dumps(result, indent=2, ensure_ascii=False))
    print(f"\nResult saved: {out_path}")
    print(f"VERDICT: {result['verdict']}")


if __name__ == "__main__":
    main()
