# Tylluan vs. Other Agent Memory Systems — Honest Comparison

**Last updated:** 2026-07-13

## Tylluan's own number (verified, reproducible)

| Metric | Value | Dataset | Notes |
|--------|-------|---------|-------|
| Recall@5 | **82.0%** | LongMemEval-S (50 human-authored questions) | Real BGE-M3 1024-dim ONNX embeddings on CPU, BM25 + vector RRF hybrid |
| Recall@10 | 90.0% | LongMemEval-S | |
| Latency p50 | 12.9ms | LongMemEval-S | Per-query, warm cache |

Reproduce it yourself:
```bash
cargo run -p tylluan-evals -- --suite longmemeval
```
Raw result: [`benchmarks/longmemeval_v0.12.0.json`](longmemeval_v0.12.0.json).

## Why there is no head-to-head table below

Every other system in this space reports a number "on LongMemEval," but
**LongMemEval defines at least two different metrics** (see the paper,
[arXiv:2410.10813](https://arxiv.org/abs/2410.10813)):

1. **Recall@k / NDCG@k** — did the correct evidence session appear in the
   top-k retrieved results? Pure retrieval, no LLM involved.
2. **End-to-end QA accuracy** — is the final generated answer correct,
   judged by an LLM (commonly GPT-4o or Gemini)?

These are not interchangeable. A system can retrieve the right evidence
100% of the time and still answer incorrectly if the reasoning step fails,
or vice versa. Publishing them in the same column — which an earlier
version of this project's own benchmark output did, before this audit
caught it — would be exactly the kind of misleading comparison this
project has corrected multiple times internally when other agents on the
team made the same mistake with our own numbers.

## What's actually published, with sources and caveats

| System | Claimed number | What it actually measures | Source | Caveat |
|--------|----------------|---------------------------|--------|--------|
| MemPalace | 96.6% | `recall_any@5` — retrieval-only, same metric family as Tylluan's Recall@5 | [mempalace.tech/benchmarks](https://www.mempalace.tech/benchmarks) | Independent testers running the full pipeline report only **82.6% end-to-end QA accuracy** for the same system — multiple sources explicitly flag the 96.6% figure as "incomparable to anything on the [QA] leaderboard." Not reproduced by us. |
| Hindsight | 91.4% | **End-to-end QA accuracy** (LLM judge, Gemini 3 Pro) — NOT a retrieval metric | [venturebeat.com](https://venturebeat.com/data/with-91-accuracy-open-source-hindsight-agentic-memory-provides-20-20-vision), [arXiv:2512.12818](https://arxiv.org/html/2512.12818v1) | Different metric family entirely from Tylluan's Recall@5 — cannot be compared directly at all, in either direction. |
| Mem0 | 94.4% (vendor) vs. 49.0% (independent) | Ambiguous — likely QA accuracy, exact protocol unclear from public sources | [mem0.ai/blog](https://mem0.ai/blog/mem0-the-token-efficient-memory-algorithm) vs. third-party evaluation cited in [mempalace.tech comparisons](https://www.mempalace.tech/compare/mempalace-vs-letta) | The two published numbers for the *same system* differ by 45+ points. Neither is trustworthy without knowing the exact eval protocol used. Not reproduced by us. |
| Zep + Graphiti | 71.2% | Unverified — carried over from an earlier internal note, source not re-checked in this audit | — | Do not cite this number until it's re-sourced. Flagged here rather than silently removed, so the next person auditing this doc knows it's unverified rather than assuming it was checked. |
| Letta / MemGPT | *(none published)* | — | — | Letta's self-editing tiered memory architecture doesn't map cleanly onto standardized recall benchmarks; no comparable LongMemEval number found as of 2026-07. |

## Bottom line

Tylluan's **82% Recall@5 on LongMemEval-S is real, reproducible, and directly
comparable in metric type** to MemPalace's retrieval-only claim (96.6%,
itself contested for the reasons above) — but not to Hindsight's or likely
Mem0's numbers, which measure something else entirely. We do not claim to
beat or lose to any of these systems until we run the same evaluation
harness, same dataset subset, and same metric against all of them ourselves.
That comparative re-run is tracked separately and not yet done.
