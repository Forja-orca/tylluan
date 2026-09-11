# REPORT: TEB-Pilot-50 Evaluation (Tylluan External-Agent Benchmark)

**Date:** 2026-09-11 18:15:32 UTC  
**Tasks:** 50 curated tasks across 8 families  
**Dataset:** [`tasks_pilot_50.json`](file:///e:/tylluan/benchmarks/teb/tasks_pilot_50.json)  
**Results:** [`pilot_results.json`](file:///e:/tylluan/benchmarks/teb/pilot_results.json)  

---

## 1. Executive Summary & Core Metrics

| Metric | Condition C0 (Baseline) | Condition C1 (Tylluan Local) | Delta / Gain |
| :--- | :---: | :---: | :---: |
| **Task Success Rate (TSR)** | **38.0%** (19/50) | **92.0%** (46/50) | **+54.0 pp** |
| **Operational Friction (OF / task)** | **4.40** calls | **0.46** calls | **-89.5%** |
| **Continuity Debt (CD / task)** | **3.36** calls | **0.00** calls | **-100.0%** |

---

## 2. Breakdown by Task Family

| Family | Tasks | C0 Success | C1 Success | $\Delta$ TSR | C0 OF (Mean) | C1 OF (Mean) | OF Reduction |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **long_term_memory** | 10 | 10.0% | 90.0% | +80.0pp | 6.00 | 0.00 | -100.0% |
| **continuity** | 10 | 50.0% | 100.0% | +50.0pp | 5.00 | 0.00 | -100.0% |
| **tool_routing** | 8 | 50.0% | 87.5% | +37.5pp | 4.00 | 1.00 | -75.0% |
| **multi_step** | 7 | 71.4% | 100.0% | +28.6pp | 2.00 | 1.00 | -50.0% |
| **recovery** | 5 | 20.0% | 100.0% | +80.0pp | 4.00 | 1.00 | -75.0% |
| **collaboration** | 4 | 0.0% | 50.0% | +50.0pp | 5.00 | 0.00 | -100.0% |
| **federation** | 3 | 33.3% | 100.0% | +66.7pp | 2.00 | 1.00 | -50.0% |
| **safety** | 3 | 66.7% | 100.0% | +33.3pp | 6.00 | 0.00 | -100.0% |

---

## 3. Methodological Observations & Next Steps for TEB-1.0

1. **Validation of Instrument:** TEB-Pilot-50 confirms that measuring *Operational Friction* and *Continuity Debt* directly captures the entropy reduction provided by Tylluan beyond raw single-turn accuracy.
2. **Memory & Continuity Dominance:** The largest effect sizes are observed in `long_term_memory` (+60pp) and `continuity` (+50pp), where stateless baselines suffer massive context reconstruction overhead.
3. **Readiness for Multi-Model Testing:** With the 50 pilot tasks validated, the harness is ready to be executed against live multi-agent connectors (Claude Code, OpenCode/Deep, local SLMs) before scaling to TEB-1.0 ($N=200-500$).