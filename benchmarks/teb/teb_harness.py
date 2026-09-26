#!/usr/bin/env python3
"""TEB-Pilot-50 Evaluation Harness (Single-Run Runner).

Runs paired organic evaluation across 50 curated tasks using real LLM agent evaluation:
  Condition C0: Baseline / Stateless Agent (In-context, no persistent memory)
  Condition C1: Tylluan Local Agent (SilvaDB Memory + Live Audit + Coloquio)

Measures:
  - Task Success Rate (TSR)
  - Operational Friction (OF): retries + redundant calls + failed attempts
  - Continuity Debt (CD): state reconstruction calls
  - Memory Harm Rate (MHR)
  - End-to-End Latency & Invocations
"""

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT))

from benchmarks.teb.teb_orchestrator import (
    main,
    run_orchestrator,
    evaluate_task_c0,
    evaluate_task_c1,
    query_silva_memory,
    log_real_audit,
    score_task_exact_tokens,
    LlmClient,
)

if __name__ == "__main__":
    main()
