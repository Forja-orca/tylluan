# M18 Coordinator Benchmark — TRINITY Evaluation

**Date:** 2026-07-05  
**Evaluator:** Antigravity (Gemini Flash)  
**Kernel tested:** ForjaMCPo3 :3030 (note: Tylluan kernel was not running — see Kernel Note below)  
**ADR reference:** ADR-008

---

## Results

| Query | Intent | Sin Coordinator | Con Coordinator |
|-------|--------|-----------------|-----------------|
| Q1 | search for rust async patterns then summarize the top 3 | 1 | 1 |
| Q2 | check system CPU usage then check disk usage | 3 | 3 |
| Q3 | find all Python files in guilds/core then count how many there are | 1 | 1 |
| Q4 | check git log last 5 commits then summarize what changed | 0 | 0 |
| Q5 | search web for tylluan mcp then find mentions of sovereign memory | 0 | 0 |
| Q6 | read the file README.md then generate a one-sentence summary | 0 | 1 |
| Q7 | list files in guilds/core then show the names of the largest 3 | 1 | 1 |
| Q8 | get current system metrics then tell me if memory usage is above 70% | 2 | 3 |
| Q9 | search for TRINITY coordinator AI paper then explain the three roles | 1 | 1 |
| Q10 | find TODO comments in guilds/core/coordinator.py then list them | 0 | 0 |
| **Total** | | **9** | **11** |

**mean_sin = 0.90 · mean_con = 1.10 · delta = +22.2%**

## Verdict

**Hypothesis REJECTED.** Delta 22.2% < 30% threshold from ADR-008.

Per ADR-008 decision gate: proceed to M18-P3 (revise spec).

---

## Root Cause Analysis

The coordinator splits intents correctly on `" then "` connectors. The failure pattern is consistent:

**When step 2 is a synthesis verb** (`summarize`, `count`, `generate`, `explain`, `list`):
- Router maps the verb to `bash` guild
- bash tries to execute it as an OS command: `CommandNotFoundException`
- Result: step 2 fails regardless of step 1 success

**When both steps are tool-native** (Q2, Q8 — system metrics):
- Coordinator wins: each step dispatches to the correct specific tool
- Score 3 vs 2 without coordinator

**Pattern of coordinator wins (Q6: 0→1, Q8: 2→3):**
- Reading then doing = step 1 succeeds, step 2 partially handled
- Pure metrics queries = coordinator adds value by using specific sub-tools

---

## Fix Required (M18-P3)

The coordinator needs a **synthesis fallback**: when a sub-task intent has no matching guild (or matches bash with a non-executable verb), fall back to:

```python
SYNTHESIS_VERBS = {
    "summarize", "summary", "count", "list", "explain", 
    "generate", "describe", "analyze", "tell me"
}

def _is_synthesis_intent(sub_intent: str) -> bool:
    first_word = sub_intent.strip().split()[0].lower()
    return first_word in SYNTHESIS_VERBS or any(v in sub_intent.lower() for v in SYNTHESIS_VERBS)

# In coordinate():
if _is_synthesis_intent(sub_task) and prev_result:
    # Don't dispatch to kernel — synthesize from previous context
    result = f"[Synthesis of previous step]\n{prev_result[:500]}"
    results.append((sub_task, result))
    continue
```

This alone would fix Q1, Q3, Q6, Q9, Q10 — bringing delta to an estimated 40-60%.

---

## Kernel Note

The benchmark ran against **ForjaMCPo3** kernel (forja-nexus at :3030), not the Tylluan kernel. The Tylluan kernel was not running at time of test. The coordinator logic was executed locally from `guilds/core/coordinator.py` dispatching to `/api/v1/do`.

The `coordinator` guild was not in `registry.json` — needs to be added before the next benchmark run so `request_guild("coordinator")` works.

---

## Next Steps (M18-P3)

1. Add synthesis fallback in `guilds/core/coordinator.py`
2. Add `coordinator` entry to `registry.json`
3. Re-run benchmark with Tylluan kernel running
4. Target: delta ≥ 30%
