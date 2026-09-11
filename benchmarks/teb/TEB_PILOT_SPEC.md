# TEB-Pilot-50: Tylluan External-Agent Benchmark (Pilot Specification)

## 1. Executive Summary & Objective
**TEB-Pilot-50** is the first operational milestone of the **TEB-1.0 (Tylluan External-Agent Benchmark)** protocol proposed in Coloquio (Turns 286, 294, 305). Its purpose is to **calibrate the measuring instrument** and establish an empirical, causal baseline evaluating:

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

## 3. Core Metrics & Operational Definitions

### Primary Endpoint
- **Paired Task Success Rate ($TSR$):** Percentage of tasks where the agent meets the ground-truth acceptance criteria:
  $$\Delta TSR = TSR(C_1) - TSR(C_0)$$

### Secondary Endpoints
1. **Operational Friction ($OF$):** Measures operational entropy per task:
   $$OF = N_{\text{retries}} + N_{\text{redundant\_calls}} + N_{\text{repeated\_context\_scans}} + N_{\text{failed\_capabilities}} + N_{\text{human\_interventions}}$$
   $$\text{Friction Reduction} = OF(C_0) - OF(C_1)$$

2. **Continuity Debt ($CD$):** Number of tool calls and tokens expended merely reconstructing past session state:
   $$CD = N_{\text{state\_reconstruction\_calls}}$$

3. **Memory Harm Rate ($MHR$):**
   - **Operational Definition:** A retrieved memory injection is classified as **Harmful** if and only if it satisfies one of three causal conditions:
     1. *Stale/Contradictory Overwrite:* The recalled node injects an obsolete architecture invariant or deprecated API that causes the agent to fail a task it would have otherwise solved ($C_0$ succeeds or remains neutral, but $C_1$ fails solely due to outdated memory).
     2. *Hallucination / Entropy Amplification:* The recalled memory sends the agent down a non-existent execution path, increasing Operational Friction ($OF$) by $>2$ failed tool calls.
     3. *Safety / Policy Breach:* The recalled context injects unauthorized tokens or commands that trigger security blocklists.
   - **Formula:**
     $$MHR = \frac{\sum_{i=1}^{N} \mathbb{I}(\text{Task}_i \text{ failed due to harmful/stale memory})}{\text{Total Tasks with Memory Recall Injected}} \times 100\%$$

4. **Real End-to-End Latency & Telemetry:**
   - Sourced directly from `data/audit.db` (`guild_audit_log.latency_ms`) and `data/silva.db` (`recall_feedback`).
   - Reports $p50$, $p90$, $p95$, and $p99$ end-to-end intent-to-result execution latencies.

---

## 4. Distribution of the 50 Pilot Tasks (8 Families)

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

## 5. Orchestrator & CLI Tooling

The benchmark includes `benchmarks/teb/teb_orchestrator.py` supporting automated multi-run execution:
```bash
python benchmarks/teb/teb_orchestrator.py --runs 3 --seed 42 --export-feedback --output-md PILOT_HARNESS_REPORT.md
```
