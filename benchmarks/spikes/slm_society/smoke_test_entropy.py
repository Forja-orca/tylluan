#!/usr/bin/env python3
"""Phase 0-Pre Entropy Smoke-Test for Internal SLM Society.

Evaluates 15 real cases from cases_real_50.json across 3 arms:
  Arm A: Baseline 1-pass CoT (T=0.2)
  Arm B: Self-MoA 3-pass independent CoT (T=0.6) + synthesis
  Arm C: Asymmetric Dialectic (Proposer CoT -> Critic CoT -> Synthesizer CoT, T=0.5)

Computes:
  - Accuracy vs ground-truth human_label
  - Proposer-Critic Jaccard lexical similarity
  - Genuine disagreement rate
  - Inter-query decision variance

Evaluates Gate:
  NO-GO if Jaccard > 85% OR Variance == 0 OR Arm B >= Arm C.
  GO if Arm C > Arm B with verifiable variance.
"""
import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

# Paths
SPIKES_DIR = Path(__file__).resolve().parent.parent
CASES_FILE = SPIKES_DIR / "coherence_gate_reasoning" / "cases_real_50.json"
RESULTS_FILE = Path(__file__).resolve().parent / "smoke_test_entropy_results.json"
REPORT_FILE = Path(__file__).resolve().parent / "SMOKE_TEST_REPORT.md"

LLAMA_SERVER_BIN = Path.home() / ".cache" / "tylluan" / "llama-cpp" / "llama-server.exe"
MODEL_PATH = Path.home() / ".cache" / "huggingface" / "hub" / "models--bartowski--SmolLM2-1.7B-Instruct-GGUF" / "snapshots" / "1f03464768bfcc0319fc50da8ff5fb20b6417ba2" / "SmolLM2-1.7B-Instruct-Q4_K_M.gguf"

SERVER_PORT = 9105

PROMPT_ARM_A = """You are an internal memory-relevance evaluator for an AI agent.
Analyze whether the retrieved memory CONTENT is genuinely relevant to the user QUERY.

QUERY: {query}
CONTENT: {content}

Think step by step in 1-2 brief sentences explaining your reasoning, then conclude with exactly "VERDICT: KEEP" (if relevant or supporting context) or "VERDICT: REJECT" (if irrelevant, generic, or off-topic)."""

PROMPT_ARM_B_SAMPLE = """You are a memory-relevance evaluator.
Analyze whether the retrieved memory CONTENT is useful context for the user QUERY.

QUERY: {query}
CONTENT: {content}

Explain your reasoning in 1-2 sentences, then state:
VERDICT: KEEP or VERDICT: REJECT"""

PROMPT_ARM_B_SYNTHESIS = """You are a consensus aggregator.
Three independent evaluations analyzed whether the CONTENT is relevant to the QUERY:

EVALUATION 1: {eval_1}
EVALUATION 2: {eval_2}
EVALUATION 3: {eval_3}

QUERY: {query}
CONTENT: {content}

Synthesize these evaluations in 1-2 sentences and give the final decision:
VERDICT: KEEP or VERDICT: REJECT"""

PROMPT_ARM_C_PROPOSER = """You are the Proposer agent in a memory deliberation council.
Your role is to find reasons why the memory CONTENT provides valuable supporting context, architectural facts, or relevant background for the user QUERY.

QUERY: {query}
CONTENT: {content}

State your supporting argument in 1-2 sentences, then conclude with:
VERDICT: KEEP or VERDICT: REJECT"""

PROMPT_ARM_C_CRITIC = """You are the Skeptical Critic agent in a memory deliberation council.
Your role is to strictly audit the Proposer's claim and find reasons why the CONTENT might be off-topic, noisy, or irrelevant to the QUERY.

QUERY: {query}
CONTENT: {content}
PROPOSER ARGUMENT: {proposer_arg}

State your critical assessment in 1-2 sentences, then conclude with:
VERDICT: KEEP or VERDICT: REJECT"""

PROMPT_ARM_C_SYNTHESIZER = """You are the Synthesizer agent resolving a memory relevance deliberation.
The Proposer and Critic analyzed whether the CONTENT is relevant to the QUERY.

QUERY: {query}
CONTENT: {content}
PROPOSER: {proposer_arg}
CRITIC: {critic_arg}

Weigh both arguments, explain your resolution in 1-2 sentences, and state the final verdict:
VERDICT: KEEP or VERDICT: REJECT"""


def start_server(port, model_path):
    if not LLAMA_SERVER_BIN.exists():
        raise RuntimeError(f"llama-server binary not found at {LLAMA_SERVER_BIN}")
    if not model_path.exists():
        raise RuntimeError(f"Model GGUF not found at {model_path}")

    cmd = [
        str(LLAMA_SERVER_BIN),
        "--model", str(model_path),
        "--host", "127.0.0.1",
        "--port", str(port),
        "--n-gpu-layers", "0",
        "--ctx-size", "512",
        "--threads", "4",
        "--batch-size", "128",
        "-sps", "0.0",
    ]
    print(f"Starting llama-server on port {port} with {model_path.name} (CPU, 4 threads, lean ctx)...")
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)

    # Wait for socket
    for _ in range(60):
        time.sleep(0.5)
        s = socket.socket()
        try:
            s.connect(("127.0.0.1", port))
            s.close()
            break
        except Exception:
            pass
        if proc.poll() is not None:
            err = proc.stderr.read() if proc.stderr else "Unknown error"
            raise RuntimeError(f"llama-server crashed: {err}")
    else:
        raise RuntimeError(f"Timeout waiting for llama-server on port {port}")

    # Warmup
    for attempt in range(15):
        try:
            time.sleep(1)
            call_server(port, "Warmup check", temp=0.1, max_tokens=4)
            print(f"llama-server ready on port {port}.")
            return proc
        except Exception:
            pass
    raise RuntimeError("Warmup check failed on llama-server.")


def call_server(port, prompt, temp=0.5, max_tokens=40):
    url = f"http://127.0.0.1:{port}/v1/chat/completions"
    body = {
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": temp,
        "top_p": 0.9,
        "stream": False,
    }
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                result = json.loads(resp.read().decode("utf-8"))
                return result["choices"][0]["message"]["content"].strip()
        except Exception as e:
            if attempt == 2:
                raise
            time.sleep(2)


def parse_verdict(text):
    text_upper = text.upper()
    if "VERDICT: KEEP" in text_upper or "DECISION: KEEP" in text_upper:
        return "keep"
    if "VERDICT: REJECT" in text_upper or "DECISION: REJECT" in text_upper:
        return "reject"
    # Fallback heuristic
    if "REJECT" in text_upper and "KEEP" not in text_upper:
        return "reject"
    if "KEEP" in text_upper and "REJECT" not in text_upper:
        return "keep"
    # Word boundary
    words = text_upper.split()
    if "REJECT" in words:
        return "reject"
    return "keep"


def jaccard_similarity(text_a, text_b):
    tokens_a = set(text_a.lower().split())
    tokens_b = set(text_b.lower().split())
    if not tokens_a or not tokens_b:
        return 0.0
    intersection = tokens_a.intersection(tokens_b)
    union = tokens_a.union(tokens_b)
    return len(intersection) / len(union)


def is_match_ground_truth(verdict, human_label):
    norm_label = "keep" if human_label in ("keep", "keep_with_caveat") else "reject"
    return verdict == norm_label


def main():
    print("=" * 76)
    print("SLM SOCIETY PHASE 0-PRE: ENTROPY & DISCRIMINATION SMOKE TEST")
    print("=" * 76)

    cases_data = json.loads(CASES_FILE.read_text(encoding="utf-8"))["cases"][:15]
    print(f"Loaded {len(cases_data)} benchmark cases from {CASES_FILE.name}")

    # Load existing checkpoint if available
    existing_results = {}
    if RESULTS_FILE.exists():
        try:
            old_data = json.loads(RESULTS_FILE.read_text(encoding="utf-8"))
            for c in old_data.get("cases", []):
                existing_results[c["case_id"]] = c
            print(f"Loaded checkpoint with {len(existing_results)} existing cases.")
        except Exception:
            pass

    proc = start_server(SERVER_PORT, MODEL_PATH)

    try:
        results = []
        arm_a_correct = 0
        arm_b_correct = 0
        arm_c_correct = 0
        jaccard_scores = []
        c_disagreements = 0

        arm_a_verdicts = []
        arm_b_verdicts = []
        arm_c_verdicts = []

        for idx, item in enumerate(cases_data, 1):
            cid = item["id"]
            query = item["query"]
            content = item["content"]
            human_label = item["human_label"]
            ground_truth = "keep" if human_label in ("keep", "keep_with_caveat") else "reject"

            print(f"\n--- [{idx:02d}/15] Case {cid} (GT: {ground_truth.upper()}) ---")
            print(f"  Query: {query[:60]}...")

            if cid in existing_results:
                saved = existing_results[cid]
                verdict_a = saved["arm_a"]["verdict"]
                match_a = saved["arm_a"]["correct"]
                verdict_b = saved["arm_b"]["verdict"]
                match_b = saved["arm_b"]["correct"]
                verdict_c = saved["arm_c"]["verdict"]
                match_c = saved["arm_c"]["correct"]
                jacc = saved["arm_c"]["jaccard_prop_crit"]
                disagree = saved["arm_c"]["disagreed"]

                if match_a:
                    arm_a_correct += 1
                arm_a_verdicts.append(verdict_a)
                if match_b:
                    arm_b_correct += 1
                arm_b_verdicts.append(verdict_b)
                if match_c:
                    arm_c_correct += 1
                arm_c_verdicts.append(verdict_c)
                jaccard_scores.append(jacc)
                if disagree:
                    c_disagreements += 1

                print(f"  [CHECKPOINT REUSED] Arm A: {verdict_a} | Arm B: {verdict_b} | Arm C: {verdict_c}")
                results.append(saved)
                continue

            # ── Arm A: Baseline 1-pass CoT ───────────────────────────
            prompt_a = PROMPT_ARM_A.format(query=query, content=content[:200])
            resp_a = call_server(SERVER_PORT, prompt_a, temp=0.2, max_tokens=40)
            verdict_a = parse_verdict(resp_a)
            match_a = is_match_ground_truth(verdict_a, human_label)
            if match_a:
                arm_a_correct += 1
            arm_a_verdicts.append(verdict_a)
            print(f"  [Arm A] Verdict: {verdict_a.upper()} ({'[OK]' if match_a else '[X]'}) | {resp_a[:80]}...")

            # ── Arm B: Self-MoA (3 independent passes + synthesis) ───
            prompt_b_sample = PROMPT_ARM_B_SAMPLE.format(query=query, content=content[:200])
            s1 = call_server(SERVER_PORT, prompt_b_sample, temp=0.6, max_tokens=35)
            s2 = call_server(SERVER_PORT, prompt_b_sample, temp=0.6, max_tokens=35)
            s3 = call_server(SERVER_PORT, prompt_b_sample, temp=0.6, max_tokens=35)

            prompt_b_synth = PROMPT_ARM_B_SYNTHESIS.format(
                query=query, content=content[:200], eval_1=s1[:100], eval_2=s2[:100], eval_3=s3[:100]
            )
            resp_b = call_server(SERVER_PORT, prompt_b_synth, temp=0.4, max_tokens=40)
            verdict_b = parse_verdict(resp_b)
            match_b = is_match_ground_truth(verdict_b, human_label)
            if match_b:
                arm_b_correct += 1
            arm_b_verdicts.append(verdict_b)
            print(f"  [Arm B] Verdict: {verdict_b.upper()} ({'[OK]' if match_b else '[X]'}) | {resp_b[:80]}...")

            # ── Arm C: Asymmetric Dialectic (Proposer -> Critic -> Synthesizer)
            prompt_c_prop = PROMPT_ARM_C_PROPOSER.format(query=query, content=content[:200])
            prop_resp = call_server(SERVER_PORT, prompt_c_prop, temp=0.5, max_tokens=35)
            prop_verdict = parse_verdict(prop_resp)

            prompt_c_crit = PROMPT_ARM_C_CRITIC.format(
                query=query, content=content[:200], proposer_arg=prop_resp[:100]
            )
            crit_resp = call_server(SERVER_PORT, prompt_c_crit, temp=0.5, max_tokens=35)
            crit_verdict = parse_verdict(crit_resp)

            jacc = jaccard_similarity(prop_resp, crit_resp)
            jaccard_scores.append(jacc)
            disagree = prop_verdict != crit_verdict
            if disagree:
                c_disagreements += 1

            prompt_c_synth = PROMPT_ARM_C_SYNTHESIZER.format(
                query=query, content=content[:200], proposer_arg=prop_resp[:100], critic_arg=crit_resp[:100]
            )
            resp_c = call_server(SERVER_PORT, prompt_c_synth, temp=0.4, max_tokens=40)
            verdict_c = parse_verdict(resp_c)
            match_c = is_match_ground_truth(verdict_c, human_label)
            if match_c:
                arm_c_correct += 1
            arm_c_verdicts.append(verdict_c)
            print(f"  [Arm C] Proposer: {prop_verdict} | Critic: {crit_verdict} (Disagreed: {disagree}, Jaccard: {jacc:.2f})")
            print(f"          Final: {verdict_c.upper()} ({'[OK]' if match_c else '[X]'}) | {resp_c[:80]}...")

            case_entry = {
                "case_id": cid,
                "query": query,
                "content": content,
                "human_label": human_label,
                "ground_truth": ground_truth,
                "arm_a": {"verdict": verdict_a, "correct": match_a, "raw": resp_a},
                "arm_b": {"verdict": verdict_b, "correct": match_b, "samples": [s1, s2, s3], "raw": resp_b},
                "arm_c": {
                    "verdict": verdict_c,
                    "correct": match_c,
                    "proposer": prop_resp,
                    "critic": crit_resp,
                    "synthesizer": resp_c,
                    "jaccard_prop_crit": round(jacc, 4),
                    "disagreed": disagree,
                },
            }
            results.append(case_entry)
            existing_results[cid] = case_entry

            # Incremental save
            RESULTS_FILE.write_text(json.dumps({"cases": results}, indent=2), encoding="utf-8")

        n_cases = len(cases_data)
        acc_a = (arm_a_correct / n_cases) * 100.0
        acc_b = (arm_b_correct / n_cases) * 100.0
        acc_c = (arm_c_correct / n_cases) * 100.0
        avg_jaccard = (sum(jaccard_scores) / len(jaccard_scores)) * 100.0 if jaccard_scores else 0.0
        disagreement_rate = (c_disagreements / n_cases) * 100.0

        # Variance check
        var_a = len(set(arm_a_verdicts)) > 1
        var_b = len(set(arm_b_verdicts)) > 1
        var_c = len(set(arm_c_verdicts)) > 1

        print("\n" + "=" * 76)
        print("SUMMARY RESULTS & GATE EVALUATION")
        print("=" * 76)
        print(f"Total Cases Evaluated: {n_cases}")
        print(f"Arm A (Baseline 1-Pass CoT):       {arm_a_correct}/{n_cases} ({acc_a:.1f}%) | Variance: {'YES' if var_a else 'NO'}")
        print(f"Arm B (Self-MoA 3-Pass CoT):       {arm_b_correct}/{n_cases} ({acc_b:.1f}%) | Variance: {'YES' if var_b else 'NO'}")
        print(f"Arm C (Asymmetric Dialectic CoT):  {arm_c_correct}/{n_cases} ({acc_c:.1f}%) | Variance: {'YES' if var_c else 'NO'}")
        print(f"Proposer-Critic Avg Jaccard Sim:   {avg_jaccard:.2f}% (Threshold: <85.0%)")
        print(f"Critic Disagreement Rate:          {c_disagreements}/{n_cases} ({disagreement_rate:.1f}%)")

        # Gate logic
        is_jaccard_pass = avg_jaccard < 85.0
        is_variance_pass = var_c
        is_c_beats_b = acc_c > acc_b

        print("\nGATE CRITERIA:")
        print(f"  1. Jaccard < 85% (Anti-Capitulation):    {'PASS' if is_jaccard_pass else 'FAIL'} ({avg_jaccard:.1f}%)")
        print(f"  2. Non-Zero Output Variance:            {'PASS' if is_variance_pass else 'FAIL'}")
        print(f"  3. Arm C > Arm B (Beat Self-MoA):       {'PASS' if is_c_beats_b else 'FAIL'} (Arm C: {acc_c:.1f}% vs Arm B: {acc_b:.1f}%)")

        if not is_jaccard_pass or not is_variance_pass or not is_c_beats_b:
            gate_verdict = "HARD NO-GO"
            verdict_explanation = (
                f"Gate FAILED: Jaccard pass={is_jaccard_pass}, Variance pass={is_variance_pass}, "
                f"Arm C > Arm B pass={is_c_beats_b}. Society does NOT outperform compute-matched Self-MoA or lacks real entropy."
            )
        else:
            gate_verdict = "GO (Unlocks Phase 0 Full Harness)"
            verdict_explanation = (
                f"Gate PASSED: Arm C ({acc_c:.1f}%) strictly outperformed Self-MoA Arm B ({acc_b:.1f}%) "
                f"with healthy entropy (Jaccard: {avg_jaccard:.1f}%, Disagreements: {disagreement_rate:.1f}%)."
            )

        print(f"\nFINAL VERDICT: {gate_verdict}")
        print(f"Detail: {verdict_explanation}")

        # Save results JSON
        payload = {
            "timestamp": int(time.time()),
            "model": MODEL_PATH.name,
            "num_cases": n_cases,
            "metrics": {
                "arm_a_accuracy": round(acc_a, 2),
                "arm_b_accuracy": round(acc_b, 2),
                "arm_c_accuracy": round(acc_c, 2),
                "avg_jaccard_similarity": round(avg_jaccard, 2),
                "critic_disagreement_rate": round(disagreement_rate, 2),
                "arm_a_variance": var_a,
                "arm_b_variance": var_b,
                "arm_c_variance": var_c,
            },
            "gate": {
                "verdict": gate_verdict,
                "explanation": verdict_explanation,
                "jaccard_pass": is_jaccard_pass,
                "variance_pass": is_variance_pass,
                "c_beats_b_pass": is_c_beats_b,
            },
            "cases": results,
        }
        RESULTS_FILE.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        print(f"\nRaw results written to {RESULTS_FILE}")

        # Save Report Markdown
        report_md = f"""# REPORT: Phase 0-Pre Entropy Smoke-Test Results

**Date:** 2026-09-07  
**Model:** `{MODEL_PATH.name}` (SmolLM2-1.7B-Instruct-Q4_K_M)  
**Dataset:** 15 real cases from `cases_real_50.json` (ground truth verified)  
**Gate Verdict:** **{gate_verdict}**

---

## 1. Summary Metrics

| Arm | Architecture | Acc ({n_cases} cases) | Variance | Description |
| :--- | :--- | :---: | :---: | :--- |
| **Arm A** | Baseline (1-pass CoT, T=0.2) | **{acc_a:.1f}%** ({arm_a_correct}/{n_cases}) | {'YES' if var_a else 'NO'} | Single-shot prompt with scratchpad |
| **Arm B** | Self-MoA (3-pass CoT, T=0.6) | **{acc_b:.1f}%** ({arm_b_correct}/{n_cases}) | {'YES' if var_b else 'NO'} | Compute-matched stochastic sampling + synthesis |
| **Arm C** | Asymmetric Dialectic (CoT, T=0.5) | **{acc_c:.1f}%** ({arm_c_correct}/{n_cases}) | {'YES' if var_c else 'NO'} | Proposer -> Critic -> Synthesizer |

*   **Proposer-Critic Jaccard Similarity:** **{avg_jaccard:.1f}%** (Threshold: <85.0%)
*   **Critic Genuine Disagreement Rate:** **{disagreement_rate:.1f}%** ({c_disagreements}/{n_cases})

---

## 2. Gate Evaluation

1. **Anti-Capitulation Check (Jaccard < 85%):** {'[PASS]' if is_jaccard_pass else '[FAIL]'} ({avg_jaccard:.1f}%)
2. **Non-Zero Output Variance:** {'[PASS]' if is_variance_pass else '[FAIL]'}
3. **Hypothesis Test (Arm C > Arm B):** {'[PASS]' if is_c_beats_b else '[FAIL]'} (Arm C: {acc_c:.1f}% vs Arm B: {acc_b:.1f}%)

**Conclusion:** {verdict_explanation}

---

## 3. Case by Case Trace

| # | ID | Ground Truth | Arm A | Arm B | Arm C (Prop / Crit / Final) | Jaccard |
|---|---|:---:|:---:|:---:|:---:|:---:|
"""
        for idx, r in enumerate(results, 1):
            c_prop = parse_verdict(r["arm_c"]["proposer"])
            c_crit = parse_verdict(r["arm_c"]["critic"])
            c_final = r["arm_c"]["verdict"]
            report_md += f"| {idx:02d} | `{r['case_id']}` | **{r['ground_truth'].upper()}** | {r['arm_a']['verdict']} ({'[OK]' if r['arm_a']['correct'] else '[X]'}) | {r['arm_b']['verdict']} ({'[OK]' if r['arm_b']['correct'] else '[X]'}) | {c_prop} / {c_crit} / **{c_final}** ({'[OK]' if r['arm_c']['correct'] else '[X]'}) | {r['arm_c']['jaccard_prop_crit'] * 100:.1f}% |\n"

        REPORT_FILE.write_text(report_md, encoding="utf-8")
        print(f"Report written to {REPORT_FILE}")

    finally:
        print("\nShutting down llama-server...")
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        print("llama-server stopped.")


if __name__ == "__main__":
    main()
