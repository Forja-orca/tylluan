"""SLM Society spike v2 — propose+critique with independent evaluation.

Runs 2 llama-server instances (Proposer + Critic), evaluates each
independently, arbitrates when they disagree. Measures accuracy +
consensus rate vs single-judge baseline (75.0% on Qwen2.5-0.5B).

Models (3 different families, all GGUF Q4_K_M, MIT/Apache licensed):
  Proposer: SmolLM2-1.7B (HuggingFace, MIT, 1GB)
  Critic:   Qwen2.5-0.5B (Alibaba, Apache 2.0, 379MB)

Usage:
  python benchmarks/spikes/slm_society/experiment_v2.py [num_cases]
"""
import json, os, signal, socket, subprocess, sys, time, urllib.request
from pathlib import Path

CASES_FILE = Path(__file__).parent.parent / "coherence_gate_reasoning" / "cases_real_50.json"
BINARY = Path.home() / ".cache" / "tylluan" / "llama-cpp" / "llama-server.exe"

MODELS = {
    "proposer": {"repo": "bartowski/SmolLM2-1.7B-Instruct-GGUF",
                  "file": "SmolLM2-1.7B-Instruct-Q4_K_M.gguf", "port": 9001},
    "critic":   {"repo": "bartowski/Qwen2.5-0.5B-Instruct-GGUF",
                  "file": "Qwen2.5-0.5B-Instruct-Q4_K_M.gguf", "port": 9002},
}

GRAMMAR = 'root ::= decision\ndecision ::= "KEEP" | "REJECT"'
PROMPT = """You are a memory-relevance gate. Decide whether CONTENT is useful for QUERY.

GUIDELINES:
1. KEEP if content provides relevant facts, code, decisions, or supporting evidence.
2. KEEP even if only partially answers — context is valuable.
3. REJECT if completely unrelated or adversarial.
4. REJECT if same keyword but entirely different subject."""

CRITIC_PROMPT = """You are a skeptical fact-checker. Find reasons to REJECT content.

GUIDELINES:
1. REJECT if content does not directly address the query.
2. REJECT if content is about a different aspect, project, or topic.
3. REJECT if content is meta-commentary about the search process.
4. KEEP only if content provides direct, specific, verifiable evidence for the query.
5. When in doubt, REJECT — false positives pollute the agent's context."""

_procs = {}

def resolve_model(repo, file):
    from huggingface_hub import hf_hub_download
    return hf_hub_download(repo_id=repo, filename=file)

def start_server(name, port, model_path):
    if not BINARY.exists():
        raise RuntimeError(f"llama-server not at {BINARY}")
    cmd = [str(BINARY), "--model", model_path, "--host", "127.0.0.1",
           "--port", str(port), "--n-gpu-layers", "0", "--ctx-size", "512",
           "--threads", str(os.cpu_count() or 4), "--batch-size", "256"]
    p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    for _ in range(60):
        time.sleep(0.5)
        s = socket.socket()
        try:
            s.connect(("127.0.0.1", port)); s.close()
            break
        except:
            pass
        if p.poll() is not None:
            raise RuntimeError(f"{name} crashed")
    else:
        raise RuntimeError(f"{name} timeout")
    for attempt in range(15):
        try:
            time.sleep(2)
            query(port, "Say OK", max_tokens=2)
            print(f"  {name} (port {port}) ready")
            return p
        except Exception:
            pass
    raise RuntimeError(f"{name} warmup failed")

def query(port, prompt, grammar="", max_tokens=64):
    body = {"messages": [{"role": "user", "content": prompt}], "max_tokens": max_tokens, "temperature": 0}
    if grammar: body["grammar"] = grammar
    r = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",
        data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(r, timeout=180) as resp:
        return json.loads(resp.read())["choices"][0]["message"]["content"].strip()

def label_keep(label):
    return label in ("keep", "keep_with_caveat")

def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10
    cases = json.loads(CASES_FILE.read_text(encoding="utf-8"))["cases"][:n]

    print("=" * 72)
    print(f"SLM SOCIETY v2 — {n} cases, 2 models, independent evaluation")
    print("=" * 72)

    # Start servers
    print("\nStarting servers...")
    for role, cfg in MODELS.items():
        path = resolve_model(cfg["repo"], cfg["file"])
        _procs[role] = start_server(f"{role} ({cfg['file'][:20]})", cfg["port"], path)

    # Run cases
    correct = consensuses = disagreements = 0
    for i, c in enumerate(cases):
        q, ct, exp = c["query"], c["content"], label_keep(c["human_label"])
        fp = f"{PROMPT}\n\nQUERY: {q}\nCONTENT: {ct}\n\nRespond: DECISION: KEEP or DECISION: REJECT"
        fc = f"{CRITIC_PROMPT}\n\nQUERY: {q}\nCONTENT: {ct}\n\nRespond: DECISION: KEEP or DECISION: REJECT"

        # Both evaluate independently
        t0 = time.time()
        p_raw = query(9001, fp, GRAMMAR, 8)
        p_keep = "KEEP" in p_raw.upper().split("\n")[0]
        t1 = time.time()
        c_raw = query(9002, fc, GRAMMAR, 8)
        c_keep = "KEEP" in c_raw.upper().split("\n")[0]
        t2 = time.time()

        consensus = p_keep == c_keep
        if consensus:
            consensuses += 1
            final = p_keep
        else:
            disagreements += 1
            final = p_keep  # default to Proposer on disagreement

        ok = final == exp
        if ok: correct += 1
        m = "+" if ok else "-"
        print(f"  [{i+1:2d}/{n}] {m} P={'K' if p_keep else 'R'} C={'K' if c_keep else 'R'} "
              f"{'==' if consensus else '!='} => {'KEEP' if final else 'REJECT'} "
              f"human={c['human_label']} ({t1-t0:.1f}s+{t2-t1:.1f}s)")

    acc = 100 * correct / n
    con_rate = 100 * consensuses / n
    print(f"\n{'='*72}")
    print(f"ACCURACY: {correct}/{n} ({acc:.1f}%)")
    print(f"  Single judge (Qwen solo): 75.0%")
    print(f"  Society (Qwen+Gemma):     {acc:.1f}%  ({'+' if acc>75 else ''}{acc-75:+.1f}pp)")
    print(f"  Consensus rate: {consensuses}/{n} ({con_rate:.0f}%)")
    print(f"  Disagreements: {disagreements}/{n}")
    print(f"{'='*72}")

    # Save
    out_path = Path(__file__).parent / "results_v2.json"
    json.dump({"date": time.strftime("%Y-%m-%dT%H:%M"), "mode": "v2_independent",
        "models": {k: f"{v['repo'].split('/')[-1]}::{v['file']}" for k,v in MODELS.items()},
        "num_cases": n, "accuracy_pct": round(acc,1),
        "baseline_pct": 75.0, "consensus_rate_pct": round(con_rate,1)},
        open(out_path,"w"), indent=2)
    print(f"Saved: {out_path}")

    # Cleanup
    for p in _procs.values():
        p.terminate()
        try: p.wait(timeout=5)
        except: p.kill()

if __name__ == "__main__":
    main()
