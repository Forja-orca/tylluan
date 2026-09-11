# TEB-Pilot-50: Tylluan External-Agent Benchmark (Pilot Specification)

## 1. Executive Summary & Objective
**TEB-Pilot-50** is the first operational milestone of the **TEB-1.0 (Tylluan External-Agent Benchmark)** protocol proposed in Coloquio (Turns 286, 294). Its purpose is not to claim premature superiority, but to **calibrate the measuring instrument** and establish an empirical, causal baseline evaluating:

> **What does Tylluan add to an external agent compared to the exact same agent without Tylluan, and at what operational cost?**

---

## 2. Experimental Conditions

The benchmark strictly compares paired runs of the **same base agent**:

$$\begin{aligned}
C_0 &= \text{Agent}(\text{Model}, \text{System Prompt}, \text{Base Tools}, \text{Environment, Stateless}) \\
C_1 &= \text{Agent}(\text{Model}, \text{System Prompt}, \text{Base Tools} + \text{Tylluan Local MCP/API}, \text{Environment})
\end{aligned}$$

### Invariant Rules:
1. **Identical Agent Baseline:** Same underlying model, temperature, token budget, and filesystem initial snapshot.
2. **No Information Privileging:** $C_1$ has access to Tylluan's sovereign capabilities (`tylluan_recall`, `tylluan_remember`, `tylluan_do`, `tylluan_think`, `tylluan_graph`, `coloquio`), while $C_0$ relies on standard tool search/in-context brute force.
3. **Reproducibility:** Seed-fixed, deterministic verifiers where possible.

---

## 3. Core Metrics

### Primary Endpoint
- **Paired Task Success Rate ($TSR$):** Percentage of tasks where the agent meets the ground-truth acceptance criteria:
  $$\Delta TSR = TSR(C_1) - TSR(C_0)$$

### Secondary Endpoints
1. **Operational Friction ($OF$):** Measures operational entropy per task:
   $$OF = N_{\text{retries}} + N_{\text{redundant\_calls}} + N_{\text{repeated\_context\_scans}} + N_{\text{failed\_capabilities}} + N_{\text{human\_interventions}}$$
   $$\text{Friction Reduction} = OF(C_0) - OF(C_1)$$

2. **Continuity Debt ($CD$):** Number of tool calls and tokens expended merely reconstructing past session state:
   $$CD = N_{\text{state\_reconstruction\_calls}}$$

3. **System Economics & Cost:**
   - $p50$, $p95$ End-to-End Latency
   - Total Tool Calls & Invocations
   - Resource Consumption (CPU / Memory overhead)

4. **Safety & Harm Rate:** Rate of incorrect/hallucinated state injection or boundary violations.

---

## 4. Distribution of the 50 Pilot Tasks (8 Families)

The pilot benchmark consists of **50 curated tasks** grounded in Tylluan's real operational history and dogfooding logs:

| Family | ID Prefix | Tasks | Focus & Capability Tested |
| :--- | :--- | :---: | :--- |
| **F1: Long-term Memory** | `mem_` | **10** | SilvaDB recall, cross-session architectural invariants, past decisions |
| **F2: Continuity & Resumption** | `cont_` | **10** | Resuming interrupted refactors, unread Coloquio sync, compact state |
| **F3: Tool & Capability Routing** | `tool_` | **8** | Precise guild/tool dispatch without trial-and-error exploration |
| **F4: Multi-step Execution** | `step_` | **7** | End-to-end workflows: recall $\to$ verify $\to$ execute $\to$ audit log |
| **F5: Recovery & Fault Tolerance** | `rec_` | **5** | Handling timeouts, cold starts, missing files, partial failures |
| **F6: Multi-agent Coordination** | `collab_` | **4** | Coloquio thread digestion, asymmetric work handoff, no work duplication |
| **F7: Federation & Mesh** | `fed_` | **3** | Peer capability discovery, remote dispatch semantics, gossip sync |
| **F8: Safety & Policy Boundaries** | `safe_` | **3** | Intent filter, ACL boundary enforcement, dangerous command blocking |
| **Total** | | **50** | Balanced, reproducible, representative |

---

## 5. Verification & Scoring Modes

Each task in `tasks_pilot_50.json` specifies an explicit verifier mode:
1. **`deterministic_test`:** Automated regex, JSON schema, or code execution validation.
2. **`exact_token_match`:** Specific factual tokens/constants required in the answer.
3. **`state_audit`:** Verifies that the correct record/log exists in the database or filesystem.
4. **`rubric_criteria`:** Multi-factor checklist of required assertions.

---

## 6. Execution Roadmap
1. **`TEB-Pilot-50` (Phase 1):** Validate the harness, evaluate $C_0$ vs $C_1$, inspect friction metrics, identify instrument flaws.
2. **`TEB-1.0` (Phase 2, $N \approx 200-500$):** Scaled multi-model benchmark with held-out hidden evaluation set once the pilot instrument is proven stable.
