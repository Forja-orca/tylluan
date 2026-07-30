# CoherenceGate Layer 4 — Hybrid Filter Design

> v1.0 — Deep, 2026-07-28. Replaces full-reasoning Layer 4 (NO-GO for <2B models)
> with a hybrid: deterministic zones for the bulk + cheap LLM classification
> only for genuine ambiguity.

---

## 1. Problem

Layer 4 (full reasoning prompt, Qwen3.5-2B) was benchmarked at 78.85% but:
- Needs ≥2B model → too heavy for recall hot path (52 candidates × 15s = 13min)
- With <2B models (SmolLM2-135M, Qwen2.5-0.5B): accuracy ≤55%, below baseline
- The grammar fix eliminates format errors but doesn't fix the REJECT-ALL bias

**The gap**: Layers 1-3 are deterministic and fast. They correctly flag ~55% of problematic
candidates. But they produce a binary penalty (×0.1) with no nuance — every flagged
candidate gets the same treatment regardless of how ambiguous it actually is.

## 2. What "Genuine Ambiguity" Means

A candidate is genuinely ambiguous when the deterministic layers disagree or produce
a borderline signal — not a clear verdict:

### Zone A: Soft semantic boundary (cosine ∈ [0.70, 0.90))
- **Below 0.70**: clearly unrelated → auto-REJECT, no LLM needed
- **Above 0.90**: clearly related → auto-KEEP, no LLM needed
- **[0.70, 0.90)**: might be tangentially relevant → LLM classification needed

The current threshold is 0.85 — everything below gets ×0.1. This is too coarse.
The [0.70, 0.85) zone has real cases where content IS relevant but cosine is low
(e.g., different vocabulary for the same concept, multilingual mismatch).

### Zone B: Conflicting signals (provenance vs weight)
- Layer 2 flagged (federation_peer) BUT weight > 0.5
- The provenance says "untrusted source", the weight says "useful content"
- Deterministic rules can't resolve this conflict → LLM tiebreaker

### Zone C: Close-call score (final score within 0.10 of median survivor)
- After all penalties, the candidate's score is borderline
- Keeping or rejecting both seem reasonable
- The LLM provides a second opinion on the content itself, not the score

### Zone D: Lexical match / semantic mismatch (keyword overlap, cosine low)
- Content shares trigger words with query but cosine is < 0.60
- Classic false-positive: "git status" matches "git" keyword but content is about GitLens, not git commands
- Currently: if cosine < 0.85 → ×0.1 (same as everything else)
- With LLM: classify as IRRELEVANT (stronger penalty) vs AMBIGUOUS (keep with weaker penalty)

## 3. Triggers — When to Call the LLM

The LLM is called ONLY when at least one of these conditions is true:

```
TRIGGER = (
    (0.70 <= cosine < 0.90)                          // Zone A
    OR (provenance == "federation_peer" AND weight > 0.5)  // Zone B
    OR (|score - median_survivor_score| < 0.10)      // Zone C
    OR (keyword_match_count >= 2 AND cosine < 0.60)   // Zone D
)
```

If NONE of these triggers fire → use the deterministic verdict from Layers 1-3:

```
no_trigger: 
    if penalized → REJECT (Layers 1-3 already flagged it)
    if clean → KEEP (Layers 1-3 found no issues)
```

**Corrected 2026-07-29** (originally estimated ~20-30% here): real measured trigger rate against
live `tylluan_recall` traffic is **59.6% (31/52 candidates)** — Zone A alone accounts for the bulk
of it, and Zone C is a full subset of Zone A. The 20-30% figure below was a directional guess made
before the real trigger-rate script existed; kept crossed out rather than silently deleted so the
correction has a paper trail. See `benchmarks/spikes/coherence_gate_reasoning/trigger_rate_live.json`.

~~Expected: ~20-30% of recall candidates trigger the LLM (not all 52).~~

## 4. LLM Classification (Cheap, Not Reasoning)

Instead of the full v3/v4 reasoning prompt (200+ tokens input, 48 tokens output,
15-20s latency), use a lightweight 3-way classification:

```
Grammar: root ::= "IRRELEVANT" | "AMBIGUOUS" | "RELEVANT"

Input (short): "Classify this recall candidate:
  Query: {first 80 chars of query}
  Content: {first 200 chars of content}
  Cosine: {cosine_score}
  Flagged by: {which layers flagged it}
  Respond with one word: IRRELEVANT, AMBIGUOUS, or RELEVANT."
```

Cost comparison:
- Full reasoning: ~400 tokens input + ~48 tokens output = ~450 tokens/call
- Lightweight classification: ~100 tokens input + ~2 tokens output = ~102 tokens/call
- **~4.4x cheaper per call**

Latency:
- Full reasoning: 15-20s per call (CPU)
- Lightweight classification: ~3-5s per call (shorter prompt, grammar-forced single token)
- **~4x faster per call**

## 5. Decision Matrix

| LLM verdict | Current status | Action |
|-------------|---------------|--------|
| IRRELEVANT | penalized (L2/L3) | REJECT (confirm existing penalty, or apply stronger penalty) |
| IRRELEVANT | clean (no flags) | REJECT (LLM overrides — this was a false-negative from L1-3) |
| AMBIGUOUS | penalized | KEEP with 0.5x penalty (softer than the current 0.1x) |
| AMBIGUOUS | clean | KEEP with 0.7x penalty (LLM found something L1-3 missed, but not conclusive) |
| RELEVANT | penalized | KEEP, remove penalty (LLM overrides — this was a false-positive from L1-3) |
| RELEVANT | clean | KEEP (confirm existing clean status) |

## 6. Performance Estimates

> **Corrected 2026-07-29 (Claude, verified against the actual file cited below).**
> The original estimate of "~28% (15/52)" was wrong -- Zone A alone (cosine in
> [0.70, 0.90)) already covers **31/52 (~60%)** of the real dataset in
> `results_real_50_v3.json`. Zones B/C/D need per-candidate provenance/weight/
> keyword-match fields that aren't in that benchmark file, so the true union
> trigger rate can't be computed from it and is likely higher than 60%, not
> lower. Treat every number below as directional, not verified, until re-measured
> against a live run with real candidate metadata.

- **Trigger rate**: **59.6% (31/52) confirmed** (`trigger_rate_live.json`) -- Zone A dominates entirely (31/52), Zone C is a subset of Zone A (17/52, all also in A), Zones B and D contribute 0 additional cases *on this specific 52-case dataset*.
- **LLM calls per recall**: ~31 calls (not 15, not 52)
- **Total latency**: 31 × 4s = ~124s for LLM portion, parallelizable -- more than double the original ~28% estimate

> **Caveat on the Zone B/D "0 contribution" result -- read before trusting it.**
> The "live" run against `tylluan_recall` returned **0 matching nodes for all
> 52 synthetic benchmark queries** (`live_provenance: []` in every single
> case of `trigger_rate_live.json`) -- these benchmark queries don't have real
> counterparts in this SilvaDB instance. That means Zone B (which needs live
> provenance/weight data) was never actually exercised -- "0/52" is a null
> result from an empty data pull, not evidence that provenance conflicts
> don't happen in production. Zone A/C/D numbers above are real (computed
> from the static cosine values and query/content text overlap, which don't
> need live data), but **Zone B's true contribution in production is still
> unknown**. Don't read "60% confirmed, B/D don't matter" as license to drop
> the Zone B check from the implementation -- it's untested, not disproven.

> **Resolved 2026-07-29 (Deep, verified).** The earlier open question about
> whether Qwen2.5-0.5B-Instruct collapses on this classification task (like
> the SLM-society NO-GO) is closed: re-ran with runs_per_case=3 on 10 real
> Zone A cases, 0/20 variance (deterministic per-case, as expected with
> temperature=0 + grammar) and 3 distinct labels used across different cases
> (50.0% accuracy, above the 33% chance baseline) -- the model discriminates
> by content, it does not collapse to one constant answer. Minor watch-item,
> not disqualifying: 7/10 cases classified RELEVANT, a real skew worth
> re-checking on a larger sample given the earlier documented "superficial
> KEEP bias" pattern in this same CoherenceGate work.

For the RECALL HOT PATH:
- Option A (blocking): 60s added to recall → unacceptable
- Option B (fire-and-forget): same as current observe_layer4 — spawn background task, don't block
- Option C (pre-filter): LLM runs on flagged candidates BEFORE they enter the survivor list, but in parallel

**Recommendation: Option B (fire-and-forget)** for now. Same architecture as observe_layer4.
Results logged to friction_log. Scores adjusted **after** the initial recall response is sent
— next recall will benefit, not current one.

## 7. What the LLM Model Needs

- Grammar-constrained decoding (llama.cpp GBNF) ✅ already wired in llama_backend.py
- Small enough to not dominate recall latency: ≤1B params, ideally ≤500M
- Better than random on 3-way classification (>33% baseline)
- MIT or Apache licensed

Candidate: **Qwen2.5-0.5B-Instruct** (379MB, Apache 2.0). At 3-5s per lightweight classification
call, it's fast enough for fire-and-forget background processing. The 3-way classification
is easier than the full reasoning task (the grammar handles the output format).

## 8. Fallback

If llama_backend is unavailable (model not downloaded, server down):
- All triggers → use deterministic verdict (no LLM)
- The filter degrades gracefully to Layers 1-3 behavior
- Log a `guild_error` friction event for observability
