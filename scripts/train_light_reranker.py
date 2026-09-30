"""Train the ADR-011 LightReranker (4 -> 16 -> 1 FFN) and export to ONNX.

Input features (4, same order as router::light_reranker::RerankFeatures):
  [score_rrf, score_graph, recency_score, agent_affinity]

Output:
  sigmoid score in (0, 1) — higher means "more likely to be useful to this agent".

Data source: SilvaDB's recall_feedback table (ADR-011 Signal Loop), joined
against nodes for recency, and against edges for score_graph. Only rows with
useful != 0 (resolved by FeedbackSignalPhase in NightConsolidation) count.

This script deliberately REFUSES to train below ADR-011 Section 3.3's stated
minimum (5,000 resolved rows) — a model trained on less data than that
threshold is exactly the "false positives that contaminate results" risk
the ADR calls out. Run it, it will tell you how far you are from that bar
rather than silently producing an overfit model.

Usage:
  python scripts/train_light_reranker.py --db data/silva.db

Generates (only if the data threshold is met):
  models/light_reranker.onnx
"""

import argparse
import sqlite3
import sys
from pathlib import Path

import numpy as np
import onnx
from onnx import helper, TensorProto

ONNX_OPSET = 18
MIN_RESOLVED_ROWS = 5000  # ADR-011 §3.3


def load_training_data(db_path: Path) -> tuple[np.ndarray, np.ndarray]:
    """Reads resolved recall_feedback rows and builds (X, y).

    Since schema v27 (ADR-011 §Fase 3), recall_feedback persists the real
    per-row retrieval features: score_rrf, score_graph (LightRAG RRF
    contribution recorded in search_hybrid) and recency_score (FSRS
    retrievability from nodes.created_at). All three are nullable: NULL means
    "row written before the migration" or "not computable at recall time"
    (graph skipped, cache-hit path). For those rows ONLY, we fall back to the
    documented proxies below — post-migration rows with real values always
    train on the real signal, and the ratio is printed so the mix is visible
    instead of silent.

    agent_affinity is computed as this agent's historical useful-rate for the
    node's type, from the same table.
    """
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row

    # Column detection keeps the script runnable against pre-v27 databases.
    cols = {r[1] for r in conn.execute("PRAGMA table_info(recall_feedback)").fetchall()}
    feature_cols = ", score_rrf, score_graph, recency_score" if {"score_rrf", "score_graph", "recency_score"} <= cols else ""
    if not feature_cols:
        print(
            "WARNING: recall_feedback lacks score_rrf/score_graph/recency_score "
            "(schema < v27). Training on proxy features only — run the kernel "
            "once on this DB to migrate."
        )

    rows = conn.execute(
        "SELECT memory_id, agent_id, rank_position, useful"
        + (", score_rrf, score_graph, recency_score" if feature_cols else "")
        + " FROM recall_feedback WHERE useful != 0"
    ).fetchall()

    if len(rows) < MIN_RESOLVED_ROWS:
        conn.close()
        raise SystemExit(
            f"Only {len(rows)} resolved recall_feedback rows — ADR-011 §3.3 requires "
            f">= {MIN_RESOLVED_ROWS} before training. Not training on insufficient data. "
            f"Let the Signal Loop run in production longer."
        )

    # agent_affinity: per-agent historical useful-rate (leave-one-out would be
    # more rigorous; simple running rate is the honest first version).
    affinity: dict[str, list[int]] = {}
    for r in rows:
        affinity.setdefault(r["agent_id"], []).append(1 if r["useful"] == 1 else 0)
    affinity_rate = {aid: (sum(v) / len(v)) for aid, v in affinity.items()}

    X, y = [], []
    real_feature_rows = 0
    for r in rows:
        # Real values when present; the documented proxies ONLY for pre-v27
        # (NULL) rows. The old constant-proxies-for-every-row behavior is gone:
        # score_graph/recency now carry actual variance for new rows.
        has_real = feature_cols and r["score_rrf"] is not None
        if has_real:
            real_feature_rows += 1
        score_rrf = r["score_rrf"] if has_real else 1.0 / (60.0 + r["rank_position"] + 1.0)  # same RRF constant as search.rs
        score_graph = (r["score_graph"] if r["score_graph"] is not None else 0.0) if feature_cols else 0.0
        recency_score = (r["recency_score"] if r["recency_score"] is not None else 0.5) if feature_cols else 0.5
        agent_affinity = affinity_rate.get(r["agent_id"], 0.5)
        X.append([score_rrf, score_graph, recency_score, agent_affinity])
        y.append(1.0 if r["useful"] == 1 else 0.0)

    if rows:
        pct = 100.0 * real_feature_rows / len(rows)
        print(
            f"Features: {real_feature_rows}/{len(rows)} rows use real persisted "
            f"score_rrf/score_graph/recency_score ({pct:.1f}%); the rest fall "
            f"back to the documented rank/recency proxies (pre-v27 rows)."
        )
    conn.close()
    return np.array(X, dtype=np.float32), np.array(y, dtype=np.float32)


# ── Manual FFN training (no torch, CPU-only, same pattern as train_complexity_mlp.py) ──

def sigmoid(x):
    return 1.0 / (1.0 + np.exp(-x))


def train_ffn(X, y, hidden=16, lr=0.01, epochs=5000):
    n_features = X.shape[1]
    rng = np.random.default_rng(42)
    W1 = rng.normal(0, 0.5, (n_features, hidden)).astype(np.float32)
    b1 = np.zeros(hidden, dtype=np.float32)
    W2 = rng.normal(0, 0.5, (hidden, 1)).astype(np.float32)
    b2 = np.zeros(1, dtype=np.float32)

    for epoch in range(epochs):
        h = np.maximum(0, X @ W1 + b1)  # ReLU
        out = sigmoid(h @ W2 + b2).flatten()

        loss_grad = (out - y) / len(y)  # BCE + sigmoid combined gradient
        dW2 = h.T @ loss_grad.reshape(-1, 1)
        db2 = loss_grad.sum()
        dh = (loss_grad.reshape(-1, 1) @ W2.T) * (h > 0)
        dW1 = X.T @ dh
        db1 = dh.sum(axis=0)

        W1 -= lr * dW1
        b1 -= lr * db1
        W2 -= lr * dW2
        b2 -= lr * db2

        if epoch % 1000 == 0:
            bce = -np.mean(y * np.log(out + 1e-7) + (1 - y) * np.log(1 - out + 1e-7))
            print(f"epoch {epoch}: bce={bce:.4f}")

    return W1, b1, W2, b2


def export_onnx(W1, b1, W2, b2, out_path: Path):
    input_tensor = helper.make_tensor_value_info("features", TensorProto.FLOAT, [1, 4])
    output_tensor = helper.make_tensor_value_info("score", TensorProto.FLOAT, [1, 1])

    w1_init = helper.make_tensor("W1", TensorProto.FLOAT, W1.shape, W1.flatten().tolist())
    b1_init = helper.make_tensor("b1", TensorProto.FLOAT, b1.shape, b1.flatten().tolist())
    w2_init = helper.make_tensor("W2", TensorProto.FLOAT, W2.shape, W2.flatten().tolist())
    b2_init = helper.make_tensor("b2", TensorProto.FLOAT, b2.shape, b2.flatten().tolist())

    nodes = [
        helper.make_node("Gemm", ["features", "W1", "b1"], ["h"], alpha=1.0, beta=1.0),
        helper.make_node("Relu", ["h"], ["h_relu"]),
        helper.make_node("Gemm", ["h_relu", "W2", "b2"], ["logit"], alpha=1.0, beta=1.0),
        helper.make_node("Sigmoid", ["logit"], ["score"]),
    ]
    graph = helper.make_graph(nodes, "light_reranker", [input_tensor], [output_tensor],
                               initializer=[w1_init, b1_init, w2_init, b2_init])
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", ONNX_OPSET)])
    onnx.checker.check_model(model)
    onnx.save(model, str(out_path))
    print(f"Saved {out_path} ({out_path.stat().st_size} bytes)")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", type=Path, default=Path("data/silva.db"))
    parser.add_argument("--out", type=Path, default=Path("models/light_reranker.onnx"))
    args = parser.parse_args()

    if not args.db.exists():
        sys.exit(f"No SilvaDB found at {args.db} — nothing to train on yet.")

    X, y = load_training_data(args.db)
    print(f"Training on {len(X)} resolved recall_feedback rows.")
    W1, b1, W2, b2 = train_ffn(X, y)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    export_onnx(W1, b1, W2, b2, args.out)


if __name__ == "__main__":
    main()
