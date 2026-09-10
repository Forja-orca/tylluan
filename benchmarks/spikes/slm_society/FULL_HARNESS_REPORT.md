# REPORT: Phase 0 SLM Society Full Harness Benchmark (N=52)

**Date:** 2026-09-10 20:12:20 UTC  
**Model:** `SmolLM2-1.7B-Instruct-Q4_K_M.gguf` (SmolLM2-1.7B-Instruct-Q4_K_M)  
**Dataset:** 52 real cases from `cases_real_50.json` (`benchmarks/spikes/coherence_gate_reasoning/cases_real_50.json`)  
**Gate Verdict:** **NO-GO**  

---

## 1. Executive Summary & Gate Evaluation

| Gate Criterion | Formal Threshold | Measured Value | Status |
| :--- | :---: | :---: | :---: |
| **Global Accuracy (Arm C)** | $\ge 70.0\%$ | **48.1%** (25/52) | **FAIL** |
| **Advantage over Self-MoA (C vs B)** | $\ge +4.0\text{pp}$ | **-1.9pp** (48.1% vs 50.0%) | **FAIL** |
| **Anti-Capitulation (Proposer-Auditor Jaccard)** | $< 85.0\%$ | **18.3%** | **PASS** |
| **Output Variance** | Non-trivial distribution | Var(C)=True | **PASS** |

**Final Verdict:** **NO-GO**  
*Gate FAILED: Arm C accuracy=48.1% (threshold >=70.0%: FAIL), Arm C vs Arm B delta=-1.9pp (threshold >=+4.0pp: FAIL), Jaccard=18.3% (threshold <85.0%: PASS), Variance=PASS.*

---

## 2. 3-Arm Accuracy & Architecture Comparison

| Arm | Architecture | Accuracy (N=52) | Variance | Description |
| :--- | :--- | :---: | :---: | :--- |
| **Arm A** | Baseline (1-pass CoT, $T=0.2$) | **48.1%** (25/52) | YES | Single-shot prompt with scratchpad |
| **Arm B** | Self-MoA (3-pass CoT, $T=0.6$ + synth) | **50.0%** (26/52) | YES | Compute-matched stochastic sampling + synthesis aggregator |
| **Arm C** | A-SSA Dialectic ($T=0.5$ + Arbiter) | **48.1%** (25/52) | YES | Proposer $\to$ Skeptical Auditor (prose) $\to$ Consolidation Arbiter |

*   **Delta Arm C vs Arm A (Baseline):** **+0.0pp**
*   **Delta Arm C vs Arm B (Self-MoA):** **-1.9pp**
*   **Mean Proposer-Auditor Lexical Jaccard:** **18.35%**

---

## 3. Discrepancy & Dialectic Breakdown

- **Cases where Arm C (A-SSA) succeeded while Arm B (Self-MoA) failed:** **3** cases
- **Cases where Arm B (Self-MoA) succeeded while Arm C (A-SSA) failed:** **4** cases
- **Cases where both deliberative arms failed:** **23** cases
- **Cases where both deliberative arms agreed and succeeded:** **22** cases

### 3.1 Arm C Wins over Arm B (A-SSA Overcame Self-MoA Mode Collapse)
- **`real_17`** (GT: **KEEP**): Query: *"GLiNER Guard PII detection modelo ya en disco constructor la..."*
  - Arm B verdict: `REJECT` [FAIL] (Samples: `['keep', 'keep', 'keep']`)
  - Arm C verdict: `KEEP` [OK] (Proposer: `keep`, Auditor: *"Your QUERY is off-topic as it is unrelated to the CONTENT provided. Th..."*)
- **`real_22`** (GT: **REJECT**): Query: *"sociedad interna de modelos pequenos generativos 1-4B hibrid..."*
  - Arm B verdict: `KEEP` [FAIL] (Samples: `['keep', 'keep', 'keep']`)
  - Arm C verdict: `REJECT` [OK] (Proposer: `keep`, Auditor: *"Your proposal is off-topic as it is not directly related to the given ..."*)
- **`real_51`** (GT: **REJECT**): Query: *"check_test_count script fix reconciliar conteo tests README..."*
  - Arm B verdict: `KEEP` [FAIL] (Samples: `['keep', 'keep', 'keep']`)
  - Arm C verdict: `REJECT` [OK] (Proposer: `keep`, Auditor: *"Your content is off-topic, as the query is about test count and reconc..."*)

### 3.2 Arm B Wins over Arm C
- **`real_21`** (GT: **KEEP**): Query: *"sociedad interna de modelos pequenos generativos 1-4B hibrid..."*
  - Arm B verdict: `KEEP` [OK]
  - Arm C verdict: `REJECT` [FAIL]
- **`real_39`** (GT: **KEEP**): Query: *"check_test_count script fix reconciliar conteo tests README..."*
  - Arm B verdict: `KEEP` [OK]
  - Arm C verdict: `REJECT` [FAIL]
- **`real_46`** (GT: **REJECT**): Query: *"sociedad interna de modelos pequenos generativos 1-4B hibrid..."*
  - Arm B verdict: `REJECT` [OK]
  - Arm C verdict: `KEEP` [FAIL]
- **`real_48`** (GT: **REJECT**): Query: *"sep-CMA-ES TRINITY spike NO-GO 33.3 por ciento win rate..."*
  - Arm B verdict: `REJECT` [OK]
  - Arm C verdict: `KEEP` [FAIL]

---

## 4. Full Case-by-Case Trace (N=50)

| # | ID | Ground Truth | Arm A (Baseline) | Arm B (Self-MoA) | Arm C (A-SSA Proposer / Auditor / Arbiter) | Jaccard |
|---|---|:---:|:---:|:---:|:---:|:---:|
| 01 | `real_1` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 26.8% |
| 02 | `real_2` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 14.3% |
| 03 | `real_3` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 28.9% |
| 04 | `real_4` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 13.2% |
| 05 | `real_5` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 17.5% |
| 06 | `real_6` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 11.4% |
| 07 | `real_7` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 10.3% |
| 08 | `real_8` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 25.7% |
| 09 | `real_9` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 5.4% |
| 10 | `real_10` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 17.9% |
| 11 | `real_11` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 17.5% |
| 12 | `real_12` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 44.8% |
| 13 | `real_13` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 23.1% |
| 14 | `real_14` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 35.1% |
| 15 | `real_15` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 14.3% |
| 16 | `real_16` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 41.7% |
| 17 | `real_17` | **KEEP** | keep [OK] | reject [X] | keep / [Audit] / **keep** [OK] | 25.0% |
| 18 | `real_18` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 21.9% |
| 19 | `real_19` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 7.1% |
| 20 | `real_20` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 32.6% |
| 21 | `real_21` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **reject** [X] | 9.5% |
| 22 | `real_22` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **reject** [OK] | 10.0% |
| 23 | `real_23` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 28.6% |
| 24 | `real_24` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 44.4% |
| 25 | `real_25` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 20.5% |
| 26 | `real_26` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 28.1% |
| 27 | `real_27` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 7.5% |
| 28 | `real_28` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 17.2% |
| 29 | `real_29` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 11.9% |
| 30 | `real_30` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 14.7% |
| 31 | `real_31` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 11.8% |
| 32 | `real_32` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 10.5% |
| 33 | `real_33` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 20.0% |
| 34 | `real_34` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 7.3% |
| 35 | `real_35` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 19.4% |
| 36 | `real_36` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 14.0% |
| 37 | `real_37` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 37.5% |
| 38 | `real_38` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **keep** [OK] | 6.4% |
| 39 | `real_39` | **KEEP** | keep [OK] | keep [OK] | keep / [Audit] / **reject** [X] | 13.2% |
| 40 | `real_40` | **REJECT** | reject [OK] | reject [OK] | keep / [Audit] / **reject** [OK] | 12.5% |
| 41 | `real_41` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 16.2% |
| 42 | `real_42` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 2.9% |
| 43 | `real_43` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 21.4% |
| 44 | `real_44` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 8.1% |
| 45 | `real_45` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 14.6% |
| 46 | `real_46` | **REJECT** | keep [X] | reject [OK] | keep / [Audit] / **keep** [X] | 15.0% |
| 47 | `real_47` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 29.4% |
| 48 | `real_48` | **REJECT** | keep [X] | reject [OK] | keep / [Audit] / **keep** [X] | 11.1% |
| 49 | `real_49` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 2.6% |
| 50 | `real_50` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 15.4% |
| 51 | `real_51` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **reject** [OK] | 30.3% |
| 52 | `real_52` | **REJECT** | keep [X] | keep [X] | keep / [Audit] / **keep** [X] | 7.3% |
