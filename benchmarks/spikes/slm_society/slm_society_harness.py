#!/usr/bin/env python3
"""Phase 0 SLM Society 3-Arm Benchmark Harness for NightConsolidation.

Evaluates memory relevance and discrimination on cases_real_50.json across 3 arms:
  - Arm A (Baseline): 1-pass CoT (T=0.2)
  - Arm B (Self-MoA): 3-pass independent CoT (T=0.6) + synthesis
  - Arm C (A-SSA): Asymmetric Sequential Scratchpad Arbitration
                   (Proposer CoT -> Skeptical Auditor CoT -> Consolidation Arbiter)

Outputs:
  - benchmarks/spikes/slm_society/night_consolidation_results.json
  - benchmarks/spikes/slm_society/NIGHT_CONSOLIDATION_REPORT.md
  - Prints SLM_SOCIETY_RESULT_JSON={...} for Rust NightConsolidation phase.
"""
import json
import os
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

# Paths
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
CASES_FILE = REPO_ROOT / "benchmarks" / "spikes" / "coherence_gate_reasoning" / "cases_real_50.json"
RESULTS_FILE = Path(__file__).resolve().parent / "night_consolidation_results.json"
REPORT_FILE = Path(__file__).resolve().parent / "NIGHT_CONSOLIDATION_REPORT.md"

LLAMA_SERVER_BIN = Path.home() / ".cache" / "tylluan" / "llama-cpp" / "llama-server.exe"
MODEL_PATH = Path.home() / ".cache" / "huggingface" / "hub" / "models--bartowski--SmolLM2-1.7B-Instruct-GGUF" / "snapshots" / "1f03464768bfcc0319fc50da8ff5fb20b6417ba2" / "SmolLM2-1.7B-Instruct-Q4_K_M.gguf"

SERVER_PORT = 9105

# Prompts
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

PROMPT_ARM_C_AUDITOR = """You are the Skeptical Auditor agent in a memory deliberation council.
Your role is to strictly audit the Proposer's claim and explain any reasons why the CONTENT might be off-topic, conversational noise, superficial keyword match, or irrelevant to the QUERY.

QUERY: {query}
CONTENT: {content}
PROPOSER ARGUMENT: {proposer_arg}

State your critical assessment and factual objections in 1-2 concise sentences."""

PROMPT_ARM_C_ARBITER = """You are the Consolidation Arbiter resolving a memory relevance deliberation.
The Proposer and Skeptical Auditor analyzed whether the memory CONTENT is genuinely relevant to the QUERY.

QUERY: {query}
CONTENT: {content}
PROPOSER ARGUMENT: {proposer_arg}
AUDITOR ASSESSMENT: {auditor_arg}

Weigh both arguments carefully, resolve any disagreement in 1 sentence, and state the final verdict:
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
        except Exception:
            if attempt == 2:
                raise
            time.sleep(2)


def parse_verdict(text):
    text_upper = text.upper()
    if "VERDICT: KEEP" in text_upper or "DECISION: KEEP" in text_upper:
        return "keep"
    if "VERDICT: REJECT" in text_upper or "DECISION: REJECT" in text_upper:
        return "reject"
    if "REJECT" in text_upper and "KEEP" not in text_upper:
        return "reject"
    if "KEEP" in text_upper and "REJECT" not in text_upper:
        return "keep"
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
    print("SLM SOCIETY PHASE 0: 3-ARM NIGHTCONSOLIDATION HARNESS")
    print("=" * 76)

    cases_data = json.loads(CASES_FILE.read_text(encoding="utf-8"))["cases"]
    print(f"Loaded {len(cases_data)} benchmark cases from {CASES_FILE.name}")

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

        arm_a_verdicts = []
        arm_b_verdicts = []
        arm_c_verdicts = []

        for idx, item in enumerate(cases_data, 1):
            cid = item["id"]
            query = item["query"]
            content = item["content"]
            human_label = item["human_label"]
            ground_truth = "keep" if human_label in ("keep", "keep_with_caveat") else "reject"

            print(f"\n--- [{idx:02d}/{len(cases_data)}] Case {cid} (GT: {ground_truth.upper()}) ---")
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

            # ── Arm C: Asymmetric Sequential Scratchpad Arbitration (A-SSA) ──
            prompt_c_prop = PROMPT_ARM_C_PROPOSER.format(query=query, content=content[:200])
            prop_resp = call_server(SERVER_PORT, prompt_c_prop, temp=0.5, max_tokens=35)
            prop_verdict = parse_verdict(prop_resp)

            prompt_c_aud = PROMPT_ARM_C_AUDITOR.format(
                query=query, content=content[:200], proposer_arg=prop_resp[:100]
            )
            aud_resp = call_server(SERVER_PORT, prompt_c_aud, temp=0.5, max_tokens=35)

            jacc = jaccard_similarity(prop_resp, aud_resp)
            jaccard_scores.append(jacc)

            prompt_c_arb = PROMPT_ARM_C_ARBITER.format(
                query=query, content=content[:200], proposer_arg=prop_resp[:100], auditor_arg=aud_resp[:100]
            )
            resp_c = call_server(SERVER_PORT, prompt_c_arb, temp=0.4, max_tokens=40)
            verdict_c = parse_verdict(resp_c)
            match_c = is_match_ground_truth(verdict_c, human_label)
            if match_c:
                arm_c_correct += 1
            arm_c_verdicts.append(verdict_c)
            print(f"  [Arm C] Proposer: {prop_verdict} | Jaccard: {jacc:.2f}")
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
                    "auditor": aud_resp,
                    "arbiter": resp_c,
                    "jaccard_prop_crit": round(jacc, 4),
                    "disagreed": False,
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

        var_a = len(set(arm_a_verdicts)) > 1
        var_b = len(set(arm_b_verdicts)) > 1
        var_c = len(set(arm_c_verdicts)) > 1

        is_jaccard_pass = avg_jaccard < 85.0
        is_variance_pass = var_c
        is_c_beats_b = acc_c > acc_b

        if not is_jaccard_pass or not is_variance_pass or not is_c_beats_b:
            gate_verdict = "HARD NO-GO"
            verdict_explanation = (
                f"Gate FAILED: Jaccard pass={is_jaccard_pass}, Variance pass={is_variance_pass}, "
                f"Arm C > Arm B pass={is_c_beats_b}."
            )
        else:
            gate_verdict = "GO"
            verdict_explanation = (
                f"Gate PASSED: Arm C ({acc_c:.1f}%) strictly outperformed Self-MoA Arm B ({acc_b:.1f}%) "
                f"with healthy entropy (Jaccard: {avg_jaccard:.1f}%)."
            )

        print("\n" + "=" * 76)
        print("SUMMARY RESULTS")
        print("=" * 76)
        print(f"Total Cases: {n_cases}")
        print(f"Arm A (Baseline): {arm_a_correct}/{n_cases} ({acc_a:.1f}%)")
        print(f"Arm B (Self-MoA):  {arm_b_correct}/{n_cases} ({acc_b:.1f}%)")
        print(f"Arm C (A-SSA):     {arm_c_correct}/{n_cases} ({acc_c:.1f}%)")
        print(f"Avg Jaccard:       {avg_jaccard:.2f}%")
        print(f"Gate Verdict:      {gate_verdict}")

        payload = {
            "timestamp": int(time.time()),
            "model": MODEL_PATH.name,
            "num_cases": n_cases,
            "metrics": {
                "arm_a_accuracy": round(acc_a, 2),
                "arm_b_accuracy": round(acc_b, 2),
                "arm_c_accuracy": round(acc_c, 2),
                "avg_jaccard_similarity": round(avg_jaccard, 2),
                "arm_a_variance": var_a,
                "arm_b_variance": var_b,
                "arm_c_variance": var_c,
            },
            "gate": {
                "verdict": gate_verdict,
                "explanation": verdict_explanation,
            },
            "cases": results,
        }
        RESULTS_FILE.write_text(json.dumps(payload, indent=2), encoding="utf-8")

        # Machine-readable output for Rust NightConsolidation phase
        summary_line = {
            "arm_a_accuracy": round(acc_a, 2),
            "arm_b_accuracy": round(acc_b, 2),
            "arm_c_accuracy": round(acc_c, 2),
            "avg_jaccard_similarity": round(avg_jaccard, 2),
            "gate_verdict": gate_verdict,
            "num_cases": n_cases,
        }
        print(f"SLM_SOCIETY_RESULT_JSON={json.dumps(summary_line)}")

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
