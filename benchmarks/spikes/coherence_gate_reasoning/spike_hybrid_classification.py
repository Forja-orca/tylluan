"""Spike: 3-way classification (IRRELEVANT/AMBIGUOUS/RELEVANT) on Zone A cases.

Tests whether Qwen2.5-0.5B can discriminate between 3 relevance levels
on the genuinely ambiguous recall candidates (cosine ∈ [0.70, 0.90)).
Runs 2-3 times per case to measure variance — the exact problem that
killed the SLM Society spike (0% variance, model collapsed to constant).

Key question: does the model output vary per case, or is it always the same word?

Usage: python benchmarks/spikes/coherence_gate_reasoning/spike_hybrid_classification.py [runs_per_case] [num_cases]
"""
import json, sys, time, urllib.request
from pathlib import Path

KERNEL = "http://127.0.0.1:4000"
CASES_FILE = Path(__file__).parent / "zone_a_cases.json"
GRAMMAR = 'root ::= decision\ndecision ::= "IRRELEVANT" | "AMBIGUOUS" | "RELEVANT"'

CLASSIFY_PROMPT = """Classify this recall candidate by relevance to the query.
Output exactly one word: IRRELEVANT, AMBIGUOUS, or RELEVANT.

IRRELEVANT = content is about a completely different topic, not useful at all.
AMBIGUOUS = content shares some context but is not clearly relevant.
RELEVANT = content directly addresses the query, provides useful evidence.

QUERY: {query}
CONTENT (first 200 chars): {content}

Respond with one word:"""


def call_llama(prompt, grammar=""):
    body = {"intent": "query_model", "prompt": prompt, "max_tokens": 3, "temperature": 0}
    if grammar:
        body["grammar"] = grammar
    data = json.dumps(body).encode()
    r = urllib.request.Request(
        f"{KERNEL}/api/v1/guilds/llama_backend/tools/query_model",
        data=data, headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(r, timeout=60) as resp:
        res = json.loads(resp.read())
        return res["content"][0]["text"].strip().upper()


def label_to_expected(human_label):
    if human_label == "reject":
        return "IRRELEVANT"
    elif human_label == "keep":
        return "RELEVANT"
    elif human_label == "keep_with_caveat":
        return "AMBIGUOUS"
    return "AMBIGUOUS"


def main():
    runs = int(sys.argv[1]) if len(sys.argv) > 1 else 2
    max_cases = int(sys.argv[2]) if len(sys.argv) > 2 else 15
    cases = json.loads(CASES_FILE.read_text(encoding="utf-8"))["cases"][:max_cases]

    print("=" * 72)
    print(f"HYBRID CLASSIFICATION SPIKE — {len(cases)} Zone A cases, {runs} runs each")
    print(f"Model: Qwen2.5-0.5B via llama_backend, grammar: IRRELEVANT|AMBIGUOUS|RELEVANT")
    print("=" * 72)

    all_results = []
    total = 0
    correct = 0
    constant_runs = 0  # cases where all runs gave the same answer
    total_variance = 0

    for i, c in enumerate(cases):
        prompt = CLASSIFY_PROMPT.format(query=c["query"], content=c["content"][:200])
        expected = label_to_expected(c["human_label"])
        responses = []
        latencies = []
        for run in range(runs):
            t0 = time.time()
            try:
                resp = call_llama(prompt, GRAMMAR)
            except Exception as e:
                resp = f"ERROR:{e}"
            latencies.append(time.time() - t0)
            responses.append(resp)

        unique = set(responses)
        variance = len(unique) - 1  # 0 = all same, 1 = 2 unique, 2 = 3 unique
        total_variance += variance
        if variance == 0:
            constant_runs += 1

        # Use majority vote (first if tie). Compare prefix — grammar may truncate.
        from collections import Counter
        majority = Counter(responses).most_common(1)[0][0]
        # Accept partial match: "IRRELEV" = "IRRELEVANT", "AMBIGU" = "AMBIGUOUS"
        is_correct = (majority.startswith(expected[:5]) or expected.startswith(majority[:5]))
        if is_correct:
            correct += 1
        total += 1

        marker = "+" if is_correct else "-"
        var_marker = "≡" if variance == 0 else ("~" if variance == 1 else "≠")
        print(f"  [{i+1:2d}/{len(cases)}] {marker} {var_marker} {c['id']}: "
              f"cos={c['cosine']:.2f} responses={responses} "
              f"majority={majority} expected={expected} "
              f"({latencies[0]:.1f}s)")

        all_results.append({"id": c["id"], "cosine": c["cosine"],
            "human_label": c["human_label"], "expected": expected,
            "responses": responses, "majority": majority, "correct": is_correct,
            "variance": variance, "constant": variance == 0,
            "latency": round(sum(latencies)/len(latencies), 1)})

    acc = 100 * correct / total
    const_pct = 100 * constant_runs / total

    print(f"\n{'='*72}")
    print(f"RESULTS — {correct}/{total} ({acc:.1f}%)")
    print(f"  Constant responses (0% variance): {constant_runs}/{total} ({const_pct:.0f}%)")
    print(f"  Total variance score: {total_variance} (max possible: {total*(runs-1)})")

    unique_across = len(set(r["majority"] for r in all_results))
    collapsed = unique_across <= 1
    verdict = "GO" if not collapsed and acc > 33 else "NO-GO"
    reason = ""
    if collapsed:
        reason = f"model always outputs '{all_results[0]['majority']}' regardless of case — no discrimination"
    elif acc <= 33:
        reason = f"accuracy {acc:.1f}% at or below chance (33%) for 3-way classification"
    else:
        reason = f"model discriminates ({unique_across} unique responses across cases) with {acc:.1f}% accuracy, deterministic within-case"

    print(f"  VERDICT: {verdict} — {reason}")
    print(f"{'='*72}")

    out_path = Path(__file__).parent / "results_hybrid_classification_v1.json"
    json.dump({"date": time.strftime("%Y-%m-%dT%H:%M"), "model": "Qwen2.5-0.5B-Instruct (llama_backend)",
        "grammar": "IRRELEVANT|AMBIGUOUS|RELEVANT", "num_cases": total, "runs_per_case": runs,
        "accuracy_pct": round(acc, 1),         "unique_responses_across_cases": unique_across, "deterministic": const_pct > 90,
        "variance_score": total_variance, "verdict": verdict, "reason": reason,
        "per_case": all_results},
        open(out_path, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
    print(f"Saved: {out_path}")


if __name__ == "__main__":
    main()
