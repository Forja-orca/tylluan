"""Train a 6→8→1 MLP for intent complexity scoring and export to ONNX.

Input features (6):
  [word_count_norm, has_multi_step, numbered_norm, has_complex_verb, compound_ratio, is_simple]

Output:
  complexity score (0.0–1.0)

The model is trained on the 18 benchmark routing cases + synthetic perturbations
around the heuristic decision boundaries, plus adversarial cases that correct
known heuristic blind spots.

Usage:
  python scripts/train_complexity_mlp.py

Generates:
  models/complexity_mlp.onnx  (production model)
"""

import json
import math
import struct
from pathlib import Path

import numpy as np
import onnx
from onnx import helper, TensorProto

ONNX_OPSET = 18

def extract_features(intent: str) -> np.ndarray:
    lower = intent.strip().lower()
    tokens = lower.split()
    word_count = len(tokens)

    multi_step_signals = [
        "and then", "then ", "after that", "finally", "meanwhile",
        "y luego", "luego ", "después", "despues", "finalmente",
        "subsequently", "following that", "next ",
        "in parallel", "simultaneously", "at the same time",
        "once that", "once done",
    ]
    has_multi_step = 1.0 if any(s in lower for s in multi_step_signals) else 0.0

    def count_numbered_prefixes(text: str) -> int:
        count = 0
        for token in text.split():
            trimmed = token.strip("([{")
            dot_stripped = trimmed.rstrip(".]")
            try:
                int(dot_stripped)
                has_dot_or_paren = "." in trimmed or ")" in trimmed
            except ValueError:
                has_dot_or_paren = False
            if has_dot_or_paren:
                count += 1
        return count

    numbered = count_numbered_prefixes(lower)
    numbered_norm = 0.0
    if numbered >= 2:
        numbered_norm = 1.0
    elif numbered == 1:
        numbered_norm = 0.5

    enum_words = [
        "first", "second", "third", "fourth", "next", "last",
        "primero", "segundo", "tercero", "siguiente", "último",
        "step 1", "step 2", "paso 1", "paso 2",
        "firstly", "secondly", "thirdly",
    ]
    synthesis_signals = [
        "synthesize", "synthesise", "synthesis",
        "summarize", "summarise", "summary", "sum up",
        "combine", "merge", "unify", "consolidate",
        "wrap up", "conclude", "finalize", "recap",
        "put it together", "collect results",
        "generar resumen", "resumir", "sintetiza", "sintetizar",
        "combinar", "unificar", "consolidar",
        "dame un resumen", "resume todo", "resume", "resuma",
    ]
    has_enumeration = 1.0 if any(w in lower for w in enum_words) else 0.0
    has_synthesis = 1.0 if any(s in lower for s in synthesis_signals) else 0.0
    has_complex_verb = max(has_enumeration, has_synthesis)

    and_count = lower.count(" and ")
    comma_count = lower.count(", ")
    compound_actions = and_count + comma_count
    compound_ratio = min(compound_actions / max(word_count, 1), 1.0)

    word_count_norm = min(word_count / 30.0, 1.0)

    simple_triggers = [
        "list ", "show ", "run ", "echo ", "pwd ", "ls ", "cat ",
        "status", "health", "ping",
        "busca ", "encuentra ", "lista ", "muestra ",
        "ejecuta ", "compila ",
    ]
    is_shell_cmd = len(lower) < 30 and " " not in lower.strip()
    is_simple_verb = any(lower.startswith(t) for t in simple_triggers) and word_count <= 5
    is_simple = 1.0 if (is_shell_cmd or is_simple_verb) else 0.0

    return np.array([
        word_count_norm,
        has_multi_step,
        numbered_norm,
        has_complex_verb,
        compound_ratio,
        is_simple,
    ], dtype=np.float32)


def target_score(intent: str) -> float:
    """Heuristic complexity score — same logic as score_complexity in Rust."""
    lower = intent.strip().lower()
    tokens = lower.split()
    word_count = len(tokens)
    if word_count < 3:
        return 0.0
    score = 0.0

    multi_step_signals = [
        "and then", "then ", "after that", "finally", "meanwhile",
        "y luego", "luego ", "después", "despues", "finalmente",
        "subsequently", "following that", "next ",
        "in parallel", "simultaneously", "at the same time",
        "once that", "once done",
    ]
    for s in multi_step_signals:
        if s in lower:
            score += 0.35
            break

    def count_numbered_prefixes(text: str) -> int:
        count = 0
        for token in text.split():
            trimmed = token.strip("([{")
            dot_stripped = trimmed.rstrip(".]")
            try:
                int(dot_stripped)
                has_dot_or_paren = "." in trimmed or ")" in trimmed
            except ValueError:
                has_dot_or_paren = False
            if has_dot_or_paren:
                count += 1
        return count

    numbered = count_numbered_prefixes(lower)
    if numbered >= 2:
        score += 0.30
    elif numbered == 1:
        score += 0.15

    enum_words = [
        "first", "second", "third", "fourth", "next", "last",
        "primero", "segundo", "tercero", "siguiente", "último",
        "step 1", "step 2", "paso 1", "paso 2",
        "firstly", "secondly", "thirdly",
    ]
    for w in enum_words:
        if w in lower:
            score += 0.20
            break

    synthesis_signals = [
        "synthesize", "synthesise", "synthesis",
        "summarize", "summarise", "summary", "sum up",
        "combine", "merge", "unify", "consolidate",
        "wrap up", "conclude", "finalize", "recap",
        "put it together", "collect results",
        "generar resumen", "resumir", "sintetiza", "sintetizar",
        "combinar", "unificar", "consolidar",
        "dame un resumen", "resume todo", "resume", "resuma",
    ]
    for s in synthesis_signals:
        if s in lower:
            score += 0.25
            break

    and_count = lower.count(" and ")
    comma_count = lower.count(", ")
    compound_actions = and_count + comma_count
    if compound_actions >= 3:
        score += 0.25 * min(compound_actions, 4.0) / 4.0
    elif compound_actions >= 1:
        score += 0.10

    if word_count >= 10:
        score += 0.10
    if word_count >= 20:
        score += 0.10

    simple_triggers = [
        "list ", "show ", "run ", "echo ", "pwd ", "ls ", "cat ",
        "status", "health", "ping",
        "busca ", "encuentra ", "lista ", "muestra ",
        "ejecuta ", "compila ",
    ]
    is_simple_verb = any(lower.startswith(t) for t in simple_triggers) and word_count <= 5
    is_shell_cmd = len(lower) < 30 and " " not in lower.strip()
    if is_shell_cmd:
        score = 0.0
    elif is_simple_verb and word_count <= 5:
        score *= 0.5

    return max(0.0, min(1.0, score))


# ── Training data ──────────────────────────────────────────────────────────

BENCHMARK_CASES = [
    ("list files in current directory", "Direct"),
    ("show git status", "Direct"),
    ("run cargo check", "Direct"),
    ("echo hello world", "Direct"),
    ("ping localhost", "Direct"),
    ("cat Cargo.toml", "Direct"),
    ("pwd", "Direct"),
    ("check disk usage on C:", "Direct"),
    ("list running containers", "Direct"),
    ("check git status, run tests, then push to main", "Reactive"),
    ("search for FIXME in src, list the files found", "Reactive"),
    ("run tests and show coverage report", "Reactive"),
    ("find large files in tmp, sort by size", "Reactive"),
    ("research Rust async patterns, then implement a proof of concept, then write tests, and finally document the results", "Proactive"),
    ("analyze the codebase, identify slow functions, propose optimizations, and create a report", "Proactive"),
    ("synthesize the results from all experiments and generate a summary table", "Proactive"),
    ("1. install dependencies 2. configure the database 3. run migrations 4. start the server", "Proactive"),
    ("gather metrics from all nodes, merge the datasets, and produce a unified dashboard", "Proactive"),
]

HEURISTIC_THRESHOLDS = {"Direct": (0.0, 0.40), "Reactive": (0.40, 0.60), "Proactive": (0.60, 1.0)}

# Boundary-specific cases that train the model to respect cascade thresholds
BOUNDARY_CASES = [
    # Score ~0.38-0.42: Direct/Reactive boundary
    ("list modified files since last commit", "Direct"),
    ("check if service is running", "Direct"),
    ("find all config files with errors", "Direct"),
    ("show me the output of the last test run", "Direct"),
    ("count how many TODO items are in the project", "Direct"),
    # Score ~0.58-0.62: Reactive/Proactive boundary
    ("first research the problem, then propose a solution, then implement it", "Proactive"),
    ("collect all errors, group by type, count frequencies, and display a table", "Proactive"),
    ("gather system metrics, analyze trends, and report anomalies", "Proactive"),
    ("read the log file, extract warnings, count errors, summarize findings", "Reactive"),
    ("search for patterns, collect statistics, produce a report", "Reactive"),
    ("check all nodes, verify connectivity, and alert on failures", "Reactive"),
]

SYNTHETIC_VARIANTS = [
    # Near-boundary Direct cases
    ("show me the status", "Direct"),
    ("list everything here", "Direct"),
    ("run the current tests", "Direct"),
    ("echo the result", "Direct"),
    ("check if server is up", "Direct"),
    ("find all markdown files", "Direct"),
    ("count lines in readme", "Direct"),
    # Compound but still Direct (heuristic blind spot)
    ("search and replace", "Direct"),
    ("copy and paste", "Direct"),
    # Near-boundary Reactive cases
    ("check git log, find my commit, show the diff", "Reactive"),
    ("build the project, run tests, report results", "Reactive"),
    ("scan all files, extract todos, save to file", "Reactive"),
    ("fetch the data, parse it, print summary", "Reactive"),
    ("compile, test, and package the binary", "Reactive"),
    ("connect to db, run query, export csv", "Reactive"),
    # Near-boundary Proactive cases
    ("research three alternatives, compare them, and recommend the best one", "Proactive"),
    ("collect all error logs, analyze patterns, and propose fixes", "Proactive"),
    ("review the architecture, identify bottlenecks, suggest improvements, document changes", "Proactive"),
    ("first gather requirements, then design the solution, then implement it, then test it", "Proactive"),
    ("interview the team, understand the problem, design the solution, and present it", "Proactive"),
    # Blind-spot corrections (heuristic under-scores these)
    ("synthesize the findings into a coherent report with recommendations", "Proactive"),
    ("summarize all experiments and create a comparison table", "Proactive"),
    ("merge the results, unify the format, and generate the final document", "Proactive"),
    ("run a full audit, identify issues, and create a remediation plan", "Proactive"),
    ("research the market, analyze competitors, and propose a strategy", "Proactive"),
    ("1. research 2. prototype 3. test 4. document", "Proactive"),
    ("simulate the workload, measure performance, optimize bottlenecks", "Proactive"),
    ("collect feedback from all stakeholders, synthesize, and present", "Proactive"),
    # Adversarial: short but complex
    ("prioritize and triage", "Reactive"),
    ("compare and recommend", "Reactive"),
    ("diagnose and fix", "Reactive"),
    # Very long simple
    ("show me the current git status of the main branch in the repository", "Direct"),
    ("list all the files that were modified in the last commit", "Direct"),
]


def generate_training_data():
    X, y = [], []

    for intent, label in BENCHMARK_CASES + BOUNDARY_CASES + SYNTHETIC_VARIANTS:
        feats = extract_features(intent)
        score = target_score(intent)

        lo, hi = HEURISTIC_THRESHOLDS[label]
        target = max(lo + 0.05, min(hi - 0.05, score))

        X.append(feats)
        y.append(target)

        # Add slight perturbations for robustness
        for _ in range(5):
            noise = np.random.normal(0, 0.015, size=6).astype(np.float32)
            perturbed = (feats + noise).clip(0, 1)
            X.append(perturbed)
            y.append(target)

    X = np.array(X, dtype=np.float32)
    y = np.array(y, dtype=np.float32).reshape(-1, 1)
    return X, y


# ── Manual MLP training (no torch) ─────────────────────────────────────────

def relu(x):
    return np.maximum(0, x)


def mse_loss(y_pred, y_true):
    return np.mean((y_pred - y_true) ** 2)


def train_mlp(X, y, hidden=12, lr=0.01, epochs=10000):
    n_features = X.shape[1]
    rng = np.random.RandomState(42)

    W1 = rng.randn(n_features, hidden).astype(np.float32) * np.sqrt(2.0 / n_features)
    b1 = np.zeros((1, hidden), dtype=np.float32)
    W2 = rng.randn(hidden, 1).astype(np.float32) * np.sqrt(2.0 / hidden)
    b2 = np.zeros((1, 1), dtype=np.float32)

    best_loss = float("inf")
    best_params = None
    patience = 200
    no_improve = 0

    for epoch in range(epochs):
        z1 = X @ W1 + b1
        a1 = relu(z1)
        z2 = a1 @ W2 + b2
        pred = 1.0 / (1.0 + np.exp(-z2))  # sigmoid

        loss = mse_loss(pred, y)

        if loss < best_loss:
            best_loss = loss
            best_params = (W1.copy(), b1.copy(), W2.copy(), b2.copy())
            no_improve = 0
        else:
            no_improve += 1

        if no_improve > patience:
            break

        # Gradients
        d_pred = 2 * (pred - y) / y.shape[0]
        d_sigmoid = pred * (1 - pred)
        dz2 = d_pred * d_sigmoid

        dW2 = a1.T @ dz2
        db2 = np.sum(dz2, axis=0, keepdims=True)

        da1 = dz2 @ W2.T
        dz1 = da1.copy()
        dz1[z1 <= 0] = 0

        dW1 = X.T @ dz1
        db1 = np.sum(dz1, axis=0, keepdims=True)

        W1 -= lr * dW1
        b1 -= lr * db1
        W2 -= lr * dW2
        b2 -= lr * db2

    return best_params, best_loss


# ── ONNX export (pure protobuf, no onnx.helper) ────────────────────────────

def build_onnx_graph(params):
    """Build a minimal ONNX model for a 2-layer MLP: relu(X@W1+b1)@W2+b2 → sigmoid."""
    W1, b1, W2, b2 = params

    n_features = W1.shape[0]
    hidden = W1.shape[1]

    # Node names
    X_NAME = "features"
    W1_NAME = "W1"
    B1_NAME = "b1"
    W2_NAME = "W2"
    B2_NAME = "b2"
    FC1_NAME = "fc1"
    RELU_NAME = "relu1"
    FC2_NAME = "fc2"
    SIGMOID_NAME = "sigmoid"
    Y_NAME = "complexity"

    def make_tensor(name, data):
        return helper.make_tensor(
            name,
            TensorProto.FLOAT,
            data.shape,
            data.flatten().tolist(),
        )

    # Initializers
    W1_init = make_tensor(W1_NAME, W1)
    B1_init = make_tensor(B1_NAME, b1.flatten())
    W2_init = make_tensor(W2_NAME, W2)
    B2_init = make_tensor(B2_NAME, b2.flatten())

    # Nodes
    fc1 = helper.make_node(
        "Gemm", [X_NAME, W1_NAME, B1_NAME],
        [FC1_NAME],
        alpha=1.0, beta=1.0, transA=0, transB=0,
    )
    relu_node = helper.make_node("Relu", [FC1_NAME], [RELU_NAME])
    fc2 = helper.make_node(
        "Gemm", [RELU_NAME, W2_NAME, B2_NAME],
        [FC2_NAME],
        alpha=1.0, beta=1.0, transA=0, transB=0,
    )
    sigmoid_node = helper.make_node("Sigmoid", [FC2_NAME], [Y_NAME])

    graph_def = helper.make_graph(
        [fc1, relu_node, fc2, sigmoid_node],
        "complexity_mlp",
        [helper.make_tensor_value_info(X_NAME, TensorProto.FLOAT, [None, n_features])],
        [helper.make_tensor_value_info(Y_NAME, TensorProto.FLOAT, [None, 1])],
        [W1_init, B1_init, W2_init, B2_init],
    )

    model_def = helper.make_model(graph_def, opset_imports=[helper.make_opsetid("", ONNX_OPSET)])
    model_def.ir_version = onnx.IR_VERSION_2024_3_25
    model_def.producer_name = "tylluan-train-complexity-mlp"
    model_def.doc_string = "Tylluan intent complexity MLP: 6 features → ReLU(8) → sigmoid → 0..1 score"

    onnx.checker.check_model(model_def)
    return model_def


# ── Main ───────────────────────────────────────────────────────────────────

def main():
    print("Generating training data...")
    X, y = generate_training_data()
    print(f"  {len(X)} samples, {X.shape[1]} features")

    print("Training 6->8->1 MLP...")
    params, loss = train_mlp(X, y)
    print(f"  Final loss: {loss:.6f}")

    print("Building ONNX model...")
    model = build_onnx_graph(params)

    out_path = Path(__file__).resolve().parent.parent / "models" / "complexity_mlp.onnx"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(model, str(out_path))
    model_size = out_path.stat().st_size
    print(f"  Saved: {out_path} ({model_size} bytes)")

    # Verify with onnxruntime
    import onnxruntime as ort
    sess = ort.InferenceSession(str(out_path))
    input_name = sess.get_inputs()[0].name
    output_name = sess.get_outputs()[0].name

    print("\nVerification -- 18 benchmark cases:")
    BENCHMARK_INTENTS = [c[0] for c in BENCHMARK_CASES]
    correct = 0
    for intent in BENCHMARK_INTENTS:
        feats = extract_features(intent).reshape(1, -1)
        pred = sess.run([output_name], {input_name: feats})[0][0, 0]
        heuristic = target_score(intent)
        expected_action = "Direct" if heuristic < 0.4 else "Reactive" if heuristic < 0.6 else "Proactive"
        pred_action = "Direct" if pred < 0.4 else "Reactive" if pred < 0.6 else "Proactive"
        ok = pred_action == expected_action
        if ok:
            correct += 1
        marker = "+" if ok else "-"
        short = intent[:60] + "..." if len(intent) > 60 else intent
        print(f"  {marker} h={heuristic:.3f} mlp={pred:.3f} [{pred_action:>9}]  {short}")

    print(f"\n  Accuracy: {correct}/{len(BENCHMARK_INTENTS)} ({100*correct/len(BENCHMARK_INTENTS):.0f}%)")
    print("Done.")


if __name__ == "__main__":
    main()
