# REPORT: TEB-Pilot-50 Orchestrated Multi-Run Benchmark

**Date:** 2026-09-12 02:04:03 UTC  
**Runs Evaluated:** 3 (Seeds: 42..44)  
**Dataset:** [`tasks_pilot_50.json`](file:///e:/tylluan/benchmarks/teb/tasks_pilot_50.json) (50 tasks across 8 families)  
**Results Data:** [`pilot_results.json`](file:///e:/tylluan/benchmarks/teb/pilot_results.json)  

---

## 1. Executive Summary & Orchestrated Endpoints

| Metric | Condition C0 (Baseline) | Condition C1 (Tylluan Local) | Delta / Gain | Operational Target | Status |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Task Success Rate (TSR)** | **20.0%** | **100.0%** | **+80.0 pp** | $\ge +15.0	ext{pp}$ | **PASS** |
| **Operational Friction (OF)** | **4.40** calls/task | **0.30** calls/task | **-93.2%** | $\ge 50\%%$ reduction | **PASS** |
| **Continuity Debt (CD)** | **3.36** calls/task | **0.00** calls/task | **-100.0%** | $\ge 80\%$ reduction | **PASS** |
| **Memory Harm Rate (MHR)** | — | **0.0%** | — | $\le 2.0\%$ | **PASS** |
| **p95 Latency (Telemetry)** | — | **43.0 ms** | — | $< 1000	ext{ms}$ | **PASS** |

---

## 2. Telemetry & Feedback Signal Loop Integration

- **Real Audit Telemetry:** Connected directly to `data/audit.db` (`guild_audit_log.latency_ms`, `human_intervention`).
- **Signal Loop Feed:** Exported **50** verified task interactions directly to `recall_feedback` in `data/silva.db`, actively unblocking the data requirement for ADR-011 LightReranker cutover.

---

## 3. Conclusion & Next Steps for TEB-1.0

All 3 assigned gaps have been formally closed and verified:
1. **Automated Orchestrator:** Complete batch runner CLI with multi-seed statistical aggregation.
2. **Real Telemetry Wiring:** End-to-end latency and HITL signals sourced from `data/audit.db`.
3. **Operational Memory Harm Rate (MHR):** Formally defined and instrumented in the evaluation harness.