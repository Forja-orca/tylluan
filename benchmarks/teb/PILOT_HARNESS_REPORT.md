# REPORT: TEB-Pilot-50 Real Agent Multi-Run Benchmark

**Date:** 2026-09-26 17:52:22 UTC  
**Runs Evaluated:** 3 (Seeds: 42..44)  
**Provider:** `auto`  
**Dataset:** [`tasks_pilot_50.json`](file:///e:/tylluan/benchmarks/teb/tasks_pilot_50.json) (50 tasks across 8 families)  
**Results Data:** [`pilot_results.json`](file:///e:/tylluan/benchmarks/teb/pilot_results.json)  

---

## 1. Executive Summary & Orchestrated Endpoints

| Metric | Condition C0 (Baseline) | Condition C1 (Tylluan Local) | Delta / Gain | Operational Target | Status |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Task Success Rate (TSR)** | **2.0%** | **6.0%** | **+4.0 pp** | $\ge +15.0\text{pp}$ | **CHECK** |
| **Operational Friction (OF)** | **4.10** calls/task | **2.18** calls/task | **-46.8%** | $\ge 50\%$ reduction | **CHECK** |
| **Continuity Debt (CD)** | **2.48** calls/task | **0.00** calls/task | **-100.0%** | $\ge 80\%$ reduction | **PASS** |
| **Memory Harm Rate (MHR)** | — | **80.0%** | — | $\le 2.0\%$ | **CHECK** |
| **p95 Latency (Telemetry)** | — | **114.0 ms** | — | $< 1000\text{ms}$ | **PASS** |

---

## 2. Telemetry & Feedback Signal Loop Integration

- **Real Audit Telemetry:** Connected directly to `data/audit.db` (`guild_audit_log.latency_ms`, `human_intervention`).
- **Signal Loop Feed:** Exported **50** verified task interactions directly to `recall_feedback` in `data/silva.db`.

---

## 3. Methodological Validation (Zero Prompt Leakage)

1. **Stateless Baseline ($C_0$):** In-context execution with `DPC_SYSTEM_ANCHOR` without persistent memory injection.
2. **Sovereign Recall ($C_1$):** Memory graph retrieval strictly via SilvaDB FTS5 BM25.
3. **Zero Ground-Truth Leakage:** Task ground-truth and keywords are strictly isolated from the agent reasoning prompts and evaluated solely post-generation by deterministic verifiers.