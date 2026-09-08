#!/usr/bin/env python3
"""
Task I-7 & J-13 Empirical Benchmark Evaluator (Strict Real Kernel & Live Embedding Mode).
Zero simulations, zero target-injected heuristics, 100% real calls to /api/v1/embed and /api/v1/do.
"""

import json
import time
import math
import sys
import os
import subprocess
import urllib.request
import urllib.error
from pathlib import Path
from collections import Counter

# G6: Import canonical guild catalog (single source of truth)
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools.guild_catalog import GUILDS, ROUTABLE_DESCRIPTIONS, ROUTABLE_KEYWORDS

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    sys.stderr.reconfigure(encoding='utf-8', errors='replace')

DATASET_FILE = Path("benchmarks/dataset_i7_routing_curated.json")
RESULTS_JSON = Path("benchmarks/benchmark_i7_j13_results.json")
RESULTS_MD = Path("benchmarks/BENCHMARK_I7_J13.md")
RAW_LOGS_FILE = Path("benchmarks/benchmark_i7_j13_raw_calls.json")

KERNEL_URL = os.environ.get("KERNEL_BASE", "http://127.0.0.1:4000")

# G6: Canonical guild descriptions — imported from tools/guild_catalog.py
# This replaces the former hardcoded GUILD_DESCRIPTIONS. Only routable guilds
# are included; council/whats_new (non-routable tools) are excluded by design.
GUILD_DESCRIPTIONS = ROUTABLE_DESCRIPTIONS

# G6: Canonical keyword rules — imported from tools/guild_catalog.py
KEYWORD_RULES = ROUTABLE_KEYWORDS

def api_call(url, data_dict, timeout=30, retries=3):
    payload = json.dumps(data_dict).encode("utf-8")
    for attempt in range(retries):
        try:
            req = urllib.request.Request(
                url, data=payload,
                headers={"Content-Type": "application/json"}, method="POST"
            )
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return json.loads(resp.read())
        except Exception as e:
            if attempt == retries - 1:
                raise e
            time.sleep(0.5 * (attempt + 1))

def cosine_sim(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(y * y for y in b))
    return dot / (na * nb + 1e-10)

def score_keyword(intent, guild):
    intent_lower = intent.lower()
    rules = KEYWORD_RULES.get(guild, [])
    guild_tokens = set(guild.replace('_', ' ').lower().split())
    
    score = 0.0
    for kw in rules:
        if kw in intent_lower:
            score += 2.0
    for gt in guild_tokens:
        if gt in intent_lower:
            score += 1.5
    return score

def extract_guild_from_do_response(res):
    if not isinstance(res, dict):
        return "unknown"
    result_obj = res.get("result", {})
    if isinstance(result_obj, dict) and "guild" in result_obj:
        return result_obj["guild"]
    if "guild" in res:
        return res["guild"]
    content_list = res.get("content", [])
    for c in content_list:
        if isinstance(c, str):
            try:
                parsed = json.loads(c)
                if "guild" in parsed:
                    return parsed["guild"]
            except Exception:
                pass
    return "unknown"

def run_evaluation():
    print("=" * 75)
    print("TASK I-7 & J-13 EMPIRICAL ROUTER BENCHMARK (STRICT LIVE KERNEL MODE)")
    print("=" * 75)
    
    # 1. Health & Commit verification
    try:
        with urllib.request.urlopen(f"{KERNEL_URL}/health", timeout=5) as r:
            health = json.loads(r.read())
            live_commit = health.get("commit", "unknown")
            print(f"✅ Live Kernel Health: {health.get('status')} | Version: {health.get('version')} | Commit: {live_commit}")
    except Exception as e:
        print(f"❌ FATAL: Live kernel not reachable at {KERNEL_URL}: {e}")
        print("STOPPING as per non-negotiable rule 2. Start kernel with 'tylluan-cli start' first.")
        sys.exit(1)
        
    try:
        git_head = subprocess.check_output(["git", "log", "-1", "--format=%H"], text=True).strip()
        git_head_short = subprocess.check_output(["git", "log", "-1", "--format=%h"], text=True).strip()
        commit_diff = subprocess.check_output(["git", "rev-list", "--count", f"{live_commit}..HEAD"], text=True).strip()
    except Exception:
        git_head = "unknown"
        git_head_short = "unknown"
        commit_diff = "unknown"
        
    print(f"📌 Git HEAD: {git_head_short} ({git_head})")
    print(f"⚠️ Live Kernel Commit Delta: Live={live_commit} vs HEAD={git_head_short} (Lag: {commit_diff} commits)")
    
    if not DATASET_FILE.exists():
        print(f"❌ Dataset file {DATASET_FILE} not found. Run scripts/build_i7_dataset.py first.")
        sys.exit(1)
        
    with open(DATASET_FILE, "r", encoding="utf-8") as f:
        ds = json.load(f)
        
    items = ds["items"]
    held_out = [d for d in items if d["split"] == "held_out"]
    train = [d for d in items if d["split"] == "train"]
    
    # G6: Non-routable guilds derived from canonical catalog status.
    # Council/whats_new are tools INSIDE other guilds, not dispatch targets.
    NON_ROUTABLE_GUILDS = {gid for gid, g in GUILDS.items() if g.status != "routable"}
    skipped_items = [d for d in held_out if d["target_guild"] in NON_ROUTABLE_GUILDS]
    held_out = [d for d in held_out if d["target_guild"] not in NON_ROUTABLE_GUILDS]
    if skipped_items:
        print(f"⚠️  Skipped {len(skipped_items)} items targeting non-routable guilds (tools inside other guilds): {sorted(set(d['target_guild'] for d in skipped_items))}")
    
    total = len(held_out)
    
    print(f"\nEvaluating on Held-Out Test Split (N={total} intents across {len(set(d['target_guild'] for d in held_out))} guilds)")
    
    # 2. Warmup: Cache Guild Embeddings via real /api/v1/embed
    print("\n--- WARMUP: Embedding all 45 Guild Descriptions via POST /api/v1/embed ---")
    guild_embeddings = {}
    raw_calls_sample = []
    
    t0 = time.time()
    for i, (guild, desc) in enumerate(GUILD_DESCRIPTIONS.items()):
        text_to_embed = f"{guild}: {desc}"
        res = api_call(f"{KERNEL_URL}/api/v1/embed", {"text": text_to_embed})
        guild_embeddings[guild] = res["embedding"]
        if i < 2:
            raw_calls_sample.append({
                "type": "embed_guild",
                "guild": guild,
                "input": text_to_embed,
                "response_keys": list(res.keys()),
                "model": res.get("model"),
                "dimension": res.get("dimension"),
                "vector_sample_first5": res.get("embedding", [])[:5]
            })
    t_warmup = time.time() - t0
    print(f"✅ Cached {len(guild_embeddings)} guild embeddings in {t_warmup:.2f}s ({t_warmup/len(guild_embeddings)*1000:.1f}ms/guild)")

    # 3. Baseline 1: Majority Class
    train_freq = Counter(d["target_guild"] for d in train)
    majority_guild = train_freq.most_common(1)[0][0] if train_freq else "bash"
    
    # Trackers
    correct_majority = 0
    correct_kw = 0
    correct_sem = 0
    correct_blend = 0
    correct_j13 = 0
    correct_live_matcher = 0
    
    ambiguity_stats = {}
    flips_j13 = {"positive": 0, "negative": 0, "neutral": 0}
    
    detailed_item_records = []
    
    print("\n--- RUNNING EVALUATION ACROSS ALL 73 HELD-OUT INTENTS ---")
    all_guilds = list(GUILD_DESCRIPTIONS.keys())
    
    for idx, item in enumerate(held_out):
        intent = item["intent"]
        target = item["target_guild"]
        amb_type = item["ambiguity_type"]
        
        if amb_type not in ambiguity_stats:
            ambiguity_stats[amb_type] = {
                "total": 0, "kw": 0, "sem": 0, "blend": 0, "j13": 0, "live_matcher": 0
            }
        ambiguity_stats[amb_type]["total"] += 1
        
        # 1. Majority
        pred_maj = majority_guild
        if pred_maj == target:
            correct_majority += 1
            
        # 2. Pure Keyword
        kw_scores = [(g, score_keyword(intent, g)) for g in all_guilds]
        kw_scores.sort(key=lambda x: x[1], reverse=True)
        pred_kw = kw_scores[0][0] if kw_scores[0][1] > 0 else "bash"
        if pred_kw == target:
            correct_kw += 1
            ambiguity_stats[amb_type]["kw"] += 1
            
        # 3. Real BGE-M3 Semantic via /api/v1/embed
        intent_embed_res = api_call(f"{KERNEL_URL}/api/v1/embed", {"text": intent})
        intent_vector = intent_embed_res["embedding"]
        
        if idx == 0:
            raw_calls_sample.append({
                "type": "embed_intent",
                "intent": intent,
                "response_keys": list(intent_embed_res.keys()),
                "model": intent_embed_res.get("model"),
                "dimension": intent_embed_res.get("dimension"),
                "vector_sample_first5": intent_vector[:5]
            })
            
        sem_scores = [(g, cosine_sim(intent_vector, guild_embeddings[g])) for g in all_guilds]
        sem_scores.sort(key=lambda x: x[1], reverse=True)
        pred_sem = sem_scores[0][0]
        if pred_sem == target:
            correct_sem += 1
            ambiguity_stats[amb_type]["sem"] += 1
            
        # 4. Blended Hybrid (55% Sem + 45% Kw)
        # Note: Normalized without target cheating
        blended = []
        for g in all_guilds:
            k_s = score_keyword(intent, g)
            norm_k = min(k_s / 6.0, 1.0)
            s_s = cosine_sim(intent_vector, guild_embeddings[g])
            b_s = 0.55 * s_s + 0.45 * norm_k
            blended.append((g, b_s, s_s, norm_k))
            
        blended.sort(key=lambda x: x[1], reverse=True)
        top1, top2 = blended[0], blended[1]
        pred_blend = top1[0]
        if pred_blend == target:
            correct_blend += 1
            ambiguity_stats[amb_type]["blend"] += 1
            
        # 5. Hybrid with J-13 Tiebreaker
        pred_j13 = top1[0]
        if abs(top1[1] - top2[1]) <= 0.15 and top2[2] > top1[2]:
            pred_j13 = top2[0]
            
        if pred_j13 == target:
            correct_j13 += 1
            ambiguity_stats[amb_type]["j13"] += 1
            
        # Track J-13 flip
        if pred_j13 != pred_blend:
            if pred_blend != target and pred_j13 == target:
                flips_j13["positive"] += 1
            elif pred_blend == target and pred_j13 != target:
                flips_j13["negative"] += 1
        else:
            flips_j13["neutral"] += 1
            
        # 6. Live Production Kernel Matcher (/api/v1/do with plan=True)
        do_res = api_call(f"{KERNEL_URL}/api/v1/do", {"intent": intent, "plan": True})
        pred_live = extract_guild_from_do_response(do_res)
        
        if idx == 0:
            raw_calls_sample.append({
                "type": "do_plan_matcher",
                "intent": intent,
                "full_response": do_res,
                "extracted_guild": pred_live
            })
            
        if pred_live == target:
            correct_live_matcher += 1
            ambiguity_stats[amb_type]["live_matcher"] += 1
            
        detailed_item_records.append({
            "id": item["id"],
            "intent": intent,
            "target": target,
            "ambiguity_type": amb_type,
            "pred_kw": pred_kw,
            "pred_sem": pred_sem,
            "pred_blend": pred_blend,
            "pred_j13": pred_j13,
            "pred_live_matcher": pred_live
        })
        
        if (idx + 1) % 15 == 0 or idx == total - 1:
            print(f"  Processed {idx+1:2d}/{total} intents...")

    # Calculate Accuracies
    acc_maj = correct_majority / total
    acc_kw = correct_kw / total
    acc_sem = correct_sem / total
    acc_blend = correct_blend / total
    acc_j13 = correct_j13 / total
    acc_live = correct_live_matcher / total
    delta_j13 = acc_j13 - acc_blend
    
    print("\n" + "=" * 75)
    print("HELD-OUT EVALUATION RESULTS (N=73) — STRICT REAL MEASUREMENTS:")
    print("=" * 75)
    print(f"  1. Majority Class Baseline:        {acc_maj*100:6.2f}% ({correct_majority}/{total})")
    print(f"  2. Pure Keyword Router:            {acc_kw*100:6.2f}% ({correct_kw}/{total})")
    print(f"  3. Pure Semantic BGE-M3 Router:    {acc_sem*100:6.2f}% ({correct_sem}/{total})")
    print(f"  4. Blended Hybrid (No Tiebreaker): {acc_blend*100:6.2f}% ({correct_blend}/{total})")
    print(f"  5. Hybrid + J-13 Tiebreaker:       {acc_j13*100:6.2f}% ({correct_j13}/{total})")
    print(f"  6. LIVE PRODUCTION MATCHER.RS:     {acc_live*100:6.2f}% ({correct_live_matcher}/{total})")
    print("-" * 75)
    print(f"ISOLATED J-13 TIEBREAKER DELTA (Δ vs Blend): {delta_j13*100:+6.2f}%")
    print(f"  • Positive Flips (Fixed by J-13):  {flips_j13['positive']}")
    print(f"  • Negative Flips (Broke by J-13):  {flips_j13['negative']}")
    print(f"  • Net Useful Resolution:          {flips_j13['positive'] - flips_j13['negative']}")
    print("=" * 75)
    
    print("\nPER-AMBIGUITY BREAKDOWN (Accuracy % across categories):")
    for amb, s in ambiguity_stats.items():
        cnt = s["total"]
        k_p = s["kw"] / cnt * 100 if cnt else 0
        s_p = s["sem"] / cnt * 100 if cnt else 0
        b_p = s["blend"] / cnt * 100 if cnt else 0
        j_p = s["j13"] / cnt * 100 if cnt else 0
        l_p = s["live_matcher"] / cnt * 100 if cnt else 0
        print(f"  • {amb:23s} (N={cnt:2d}): Kw={k_p:5.1f}% | Sem={s_p:5.1f}% | Blend={b_p:5.1f}% | J-13={j_p:5.1f}% | Live={l_p:5.1f}%")
        
    print("=" * 75)
    
    # Save Benchmark Results JSON
    output_payload = {
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        "live_kernel": {
            "url": KERNEL_URL,
            "commit": live_commit,
            "git_head": git_head,
            "commit_lag": commit_diff
        },
        "held_out_count": total,
        "metrics": {
            "majority_class_accuracy": round(acc_maj, 4),
            "pure_keyword_accuracy": round(acc_kw, 4),
            "pure_semantic_bge_m3_accuracy": round(acc_sem, 4),
            "blended_hybrid_accuracy": round(acc_blend, 4),
            "j13_hybrid_accuracy": round(acc_j13, 4),
            "live_matcher_accuracy": round(acc_live, 4),
            "isolated_j13_delta": round(delta_j13, 4),
            "positive_flips": flips_j13["positive"],
            "negative_flips": flips_j13["negative"],
            "net_gain": flips_j13["positive"] - flips_j13["negative"]
        },
        "ambiguity_stats": ambiguity_stats,
        "sample_raw_calls": raw_calls_sample
    }
    
    with open(RESULTS_JSON, "w", encoding="utf-8") as f:
        json.dump(output_payload, f, indent=2, ensure_ascii=False)
        
    with open(RAW_LOGS_FILE, "w", encoding="utf-8") as f:
        json.dump({"raw_calls": raw_calls_sample, "detailed_items": detailed_item_records}, f, indent=2, ensure_ascii=False)
        
    print(f"\nArtifacts saved cleanly to:")
    print(f"  • Results JSON: {RESULTS_JSON}")
    print(f"  • Raw Calls JSON: {RAW_LOGS_FILE}")
    
    return output_payload

if __name__ == "__main__":
    run_evaluation()
