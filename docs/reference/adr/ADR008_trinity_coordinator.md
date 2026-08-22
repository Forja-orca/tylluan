# ADR-008 — M18: TRINITY Coordinator Guild

**Status:** Accepted  
**Date:** 2026-07-05  
**Authors:** Tech Lead (Claude)  
**Depends on:** M3 guild catalog (description_override), M6 dual retrieval, M17 integrations

---

## Context

Tylluan routes intent to a single guild via semantic matching. For simple queries ("run git status", "search the web for X") this works well. For complex multi-step tasks ("research the TRINITY paper, implement a coordinator pattern, then run the tests"), single-guild routing drops the ball:

- The semantic router picks one guild and loses the rest of the intent
- The caller (agent or human) must manually chain calls
- There is no verification layer — if a sub-task fails silently, the chain breaks

**The TRINITY paper** (ICLR 2026, arXiv:2512.04695) proposes a coordinator that decomposes tasks using three specialized roles:

- **Thinker** — plans: breaks the intent into ordered sub-tasks
- **Worker** — executes: calls the appropriate tool/guild per sub-task
- **Verifier** — validates: checks results, retries on structural failure

**Hypothesis for M18:** routing complex multi-step intents through a TRINITY coordinator instead of single-guild dispatch improves measurable output quality by ≥ 30% on a 10-query benchmark.

---

## Constraints

- CONTRACT-01 inviolable — coordinator is a **guild**, not a sovereign tool. `tylluan_do` stays unchanged.
- Sovereignty first — default mode uses no external LLM. Rule-based decomposition only.
- CPU-first — no blocking loops, no runaway retries. Max 5 sub-tasks per intent, 1 retry per failure.
- No cross-guild imports — guilds communicate only via kernel HTTP (`POST /api/v1/do`).
- Must be discoverable at startup — auto-discovered via `guilds/core/coordinator.py`.

---

## Design

### 1. Routing

`tylluan_do` routes to the coordinator when the semantic match score for `coordinator` exceeds the FRACTAL_THRESHOLD (0.82). This requires adding a `description_override` entry in `catalog.rs`:

```rust
"coordinator" => "Orchestrate multi-step tasks: research then implement, do X then Y, \
                  first do A then do B, step by step workflows, plan and execute",
```

Triggers (keywords the semantic router keys on):
- "then", "after that", "finally", "step by step"
- "first … then … finally"
- Numbered patterns: "1. … 2. … 3."
- "research and implement", "plan and execute", "do X then Y"

### 2. Thinker — Task Decomposition (rule-based, no LLM)

Split the intent on connectors in priority order:

1. Explicit separator: `" then "`, `" and then "`, `" after that "`, `" finally "`
2. Numbered list: `^1\. `, `^2\. `, etc.
3. Sentence boundary (`. ` followed by a verb) — max 5 splits
4. Fallback: treat entire intent as a single task (passthrough)

Produce an ordered list of sub-task strings, maximum 5. Trim whitespace. Discard empties.

**Optional LLM mode:** If `tylluan.toml` sets `[coordinator] llm_url = "http://..."`, the Thinker sends the intent to that endpoint for richer decomposition. Response format: `{"tasks": ["sub-task 1", "sub-task 2", ...]}`. Falls back to rule-based if the endpoint is unreachable.

### 3. Worker — Sub-task Dispatch

For each sub-task, POST to the kernel's no-auth intent endpoint:

```
POST http://127.0.0.1:3030/api/v1/do
Content-Type: application/json

{"intent": "<sub-task>", "agent_id": "coordinator"}
```

Collect `result` or `output` from each response. Timeout per sub-task: **120s** (respects CPU inference guild timeouts). Execute sequentially — preserve order, pass previous result as context if the sub-task references "it" or "that".

**Context threading:** Append the previous result's first 200 chars to the next sub-task intent when it contains a reference pronoun ("it", "the result", "that output"). Pattern: `f"{sub_task} [context: {prev_result[:200]}]"`.

### 4. Verifier — Structural Validation

After each Worker call, check the result:

| Condition | Action |
|-----------|--------|
| HTTP non-200 | Retry once with `f"retry: {sub_task}"` |
| Result contains `"error"` or `"❌"` | Retry once |
| Result is empty or `null` | Retry once |
| Retry also fails | Record failure, continue to next sub-task |

Max 1 retry per sub-task. On final failure, include `⚠️ [sub-task N failed]` in the synthesis.

No LLM verification in default mode. Optional LLM verifier if `llm_url` is set (validates that each result actually satisfies the sub-task intent before proceeding).

### 5. Synthesis

Concatenate results with a header per sub-task:

```
## Step 1/N — <sub-task>
<result>

## Step 2/N — <sub-task>
<result>

---
Coordinator completed N/N steps.
```

---

## FastMCP Interface

File: `guilds/core/coordinator.py`

```python
mcp = FastMCP("coordinator")

@mcp.tool()
def coordinate(intent: str, agent_id: str = "coordinator") -> str:
    """
    Orchestrate complex multi-step tasks using Thinker/Worker/Verifier.
    Use for: multi-step tasks, research then implement, do X then Y,
    first do A then do B, step by step workflows, plan and execute,
    complex workflows, sequential tasks, chained operations.
    """
```

Single tool — `coordinate`. All three phases happen internally. The `agent_id` is forwarded to sub-task calls so SilvaDB tracks coordinator episodes under the caller's identity.

---

## Configuration (tylluan.toml)

```toml
[coordinator]
# Optional: LLM endpoint for richer Thinker/Verifier (default: rule-based)
# llm_url = "http://127.0.0.1:11434/api/generate"
# llm_model = "llama3"

# Max sub-tasks per intent (default: 5)
max_tasks = 5

# Sub-task timeout in seconds (default: 120)
task_timeout_secs = 120
```

The guild reads these via environment variables injected by the kernel at startup (`TYLLUAN_COORDINATOR_*`). If not set, defaults apply. No tylluan.toml changes required for basic operation.

---

## Catalog Registration

Deep adds to `crates/tylluan-kernel/src/router/catalog.rs`, `description_override()`:

```rust
"coordinator" => "Orchestrate multi-step tasks: research then implement, do X then Y, \
                  first do A then do B, step by step workflows, plan and execute",
```

The guild is auto-discovered from `guilds/core/coordinator.py` — no changes to `registry.json` or always_on list needed. The coordinator starts on-demand.

---

## Benchmark (P2)

**10 multi-step queries** spanning research + code + verification:

| # | Intent | Guilds involved |
|---|--------|----------------|
| 1 | "search for rust async patterns then summarize the top 3" | deep_web_research → coordinator synthesis |
| 2 | "read the file src/main.rs then count the lines" | filesystem → bash |
| 3 | "find all TODO comments in the codebase then create a list" | bash → coordinator synthesis |
| 4 | "check git status then summarize what changed" | git → coordinator synthesis |
| 5 | "search for TRINITY paper then extract the key findings" | deep_web_research → coordinator synthesis |
| 6 | "list running docker containers then show their logs" | docker → docker |
| 7 | "read the README then generate a one-sentence summary" | filesystem → coordinator synthesis |
| 8 | "search web for 'tylluan mcp' then find mentions of sovereign" | websearch → coordinator synthesis |
| 9 | "check system CPU then check disk usage" | system_metrics → system_metrics |
| 10 | "find the largest file in guilds/ then show its first 20 lines" | bash → filesystem |

**Baseline:** same 10 queries routed via single-guild `tylluan_do` (no coordinator).

**Quality metric:** human-rated completeness score 0–3 per query:
- 0 = result misses the intent entirely
- 1 = partial (one sub-task only)
- 2 = mostly complete
- 3 = all sub-tasks addressed with coherent synthesis

**Hypothesis:** coordinator mean score ≥ 1.3× baseline mean.

Benchmark runner: `cargo run -p tylluan-evals -- --suite coordinator` (Deep adds this suite in P1 alongside the implementation).

---

## Sequence Diagram

```
tylluan_do (intent: "research X then implement Y")
    │
    ├─► semantic router → score coordinator > 0.82
    │
    └─► coordinator guild
            │
            ├─► Thinker: ["research X", "implement Y"]
            │
            ├─► Worker(1): POST /api/v1/do {"intent": "research X"}
            │       └─► deep_web_research → result_1
            │
            ├─► Verifier(1): result_1 OK?
            │       └─► yes → continue
            │
            ├─► Worker(2): POST /api/v1/do {"intent": "implement Y [context: ...]"}
            │       └─► code guild → result_2
            │
            ├─► Verifier(2): result_2 OK?
            │       └─► yes → synthesize
            │
            └─► Synthesis: "## Step 1/2 ...\n## Step 2/2 ...\n---\nCompleted 2/2"
```

---

## P1 Deliverables (Deep)

1. **`guilds/core/coordinator.py`** — FastMCP guild implementing Thinker/Worker/Verifier
2. **`crates/tylluan-kernel/src/router/catalog.rs`** — add coordinator to `description_override()`
3. **`tests/python/test_coordinator.py`** — unit tests for Thinker decomposition logic (no kernel needed)
4. **`tylluan-evals --suite coordinator`** — stub runner that runs 10-query benchmark (P2 fills in results)

## P2 Deliverables (Claude + Qwen)

1. Run 10-query benchmark with/without coordinator
2. Score each result 0–3
3. Compute delta. If ≥ 30% improvement: `docs/research/trinity_benchmark.md` + merge recommendation
4. If < 30%: document findings in `docs/research/trinity_benchmark.md` + revise spec in P3

---

## Consequences

**Positive:**
- Complex multi-step tasks are handled automatically without manual chaining
- The coordinator pattern is reusable for any guild sequence
- Rule-based decomposition preserves sovereignty (no LLM call required)
- CONTRACT-01 untouched — `tylluan_do` interface unchanged for clients

**Negative:**
- Adds latency for simple intents that accidentally match coordinator routing
- Mitigation: `score > 0.82` threshold is strict; passthrough is fast (≤ 5ms overhead)
- Sequential sub-task execution can be slow for 3-step tasks with heavy guilds
- Mitigation: 120s timeout per step; parallel execution is a future enhancement

---

## M18 Closure Gate

Benchmark result in `docs/research/trinity_benchmark.md` with delta score calculated.
Decision (merge or revise) is data-driven, not deadline-driven.
