#!/usr/bin/env python3
"""Anonymize TEB-Pilot-50 results for blind judge evaluation.

Four layers of anonymization:
  1. Text: strips tell-tale phrases (project names, agent names, paths)
  2. Structure: renames c0/c1 fields, condition values
  3. Per-task randomization: each task gets an independent A/B assignment
  4. Verdict stripping: removes passed/score/memory_harm so the judge
     produces their own evaluation (these are OUTPUT, not INPUT)

Usage:
    python anonymize_for_judge.py [--input pilot_results.json] [--seed 42]
"""

import json
import random
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_INPUT = REPO_ROOT / "benchmarks" / "teb" / "pilot_results.json"
DEFAULT_OUTPUT = REPO_ROOT / "benchmarks" / "teb" / "pilot_results_anon.json"
DEFAULT_MAPPING = REPO_ROOT / "benchmarks" / "teb" / "judge_blind_mapping.json"

# ── Layer 1: Text anonymization ─────────────────────────────────────────────

TELLTALE_PATTERNS = [
    (r"Tylluan Sovereign Memory Recall \[Verified\]", "[MEMORY_RECALL_VERIFIED]"),
    (r"Tylluan Recall \[Partial/Stale\]", "[MEMORY_RECALL_PARTIAL]"),
    (r"Tylluan Recall \[Partial\]", "[MEMORY_RECALL_PARTIAL]"),
    (r"specific Tylluan invariant", "specific system invariant"),
    (r"Tylluan", "[SYSTEM]"),
    (r"SilvaDB", "[DATABASE]"),
    (r"CoherenceGate", "[SECURITY_GATE]"),
    (r"NightConsolidation", "[BACKGROUND_PROCESS]"),
    (r"tylluan-nexus", "[KERNEL]"),
    (r"tylluan_do", "[SOVEREIGN_TOOL]"),
    (r"tylluan_recall", "[SOVEREIGN_TOOL]"),
    (r"tylluan_remember", "[SOVEREIGN_TOOL]"),
    (r"tylluan_think", "[SOVEREIGN_TOOL]"),
    (r"tylluan_graph", "[SOVEREIGN_TOOL]"),
    (r"Claude Code", "[AGENT_1]"),
    (r"Antigravity", "[AGENT_2]"),
    (r"\bDeep\b", "[AGENT_3]"),
    (r"\bBuffy\b", "[AGENT_4]"),
    (r"guilds/core/\w+\.py", "[GUILD_PLUGIN]"),
    (r"guilds/\w+/plugins/\w+\.py", "[GUILD_PLUGIN]"),
    (r"crates/tylluan-\w+/src/\S+", "[INTERNAL_PATH]"),
]

# Fields that are verdicts — the judge must produce these, not receive them
VERDICT_FIELDS = {"passed", "score", "memory_harm"}
# Summary/run-level verdict aggregates
VERDICT_SUMMARY_PATTERNS = ["memory_harm_rate", "mhr_"]


def anonymize_text(text: str) -> str:
    result = text
    for pattern, replacement in TELLTALE_PATTERNS:
        result = re.sub(pattern, replacement, result)
    return result


def anonymize_value(value):
    if isinstance(value, str):
        return anonymize_text(value)
    elif isinstance(value, dict):
        return {k: anonymize_value(v) for k, v in value.items()}
    elif isinstance(value, list):
        return [anonymize_value(item) for item in value]
    return value


# ── Layer 2+3: Per-task structural anonymization ────────────────────────────

def anonymize_task(task: dict, a_is_c0: bool) -> dict:
    """Anonymize a single task with its own A/B assignment."""
    if a_is_c0:
        c0_arm, c1_arm = "arm_0", "arm_1"
        c0_cond, c1_cond = "C0_baseline", "C1_tylluan"
    else:
        c0_arm, c1_arm = "arm_1", "arm_0"
        c0_cond, c1_cond = "C0_baseline", "C1_tylluan"

    result = {}
    for k, v in task.items():
        if k == "c0":
            new_key = c0_arm
        elif k == "c1":
            new_key = c1_arm
        else:
            new_key = k

        if isinstance(v, dict):
            v = anonymize_task_entry(v, c0_cond, c1_cond, c0_arm, c1_arm)
        result[new_key] = v

    return result


def anonymize_task_entry(entry: dict, c0_cond: str, c1_cond: str,
                         c0_arm: str, c1_arm: str) -> dict:
    """Rename condition fields and strip verdict fields."""
    result = {}
    for k, v in entry.items():
        # Strip verdict fields — judge produces these
        if k in VERDICT_FIELDS:
            continue
        if k == "condition" and v == c0_cond:
            result[k] = c0_arm
        elif k == "condition" and v == c1_cond:
            result[k] = c1_arm
        else:
            result[k] = v
    return result


def anonymize_summary(summary: dict) -> dict:
    """Strip condition-specific and verdict aggregates from summary."""
    result = {}
    for k, v in summary.items():
        # Strip condition-specific summary fields
        if "_c0" in k or "_c1" in k:
            continue
        # Strip verdict aggregates
        if any(pat in k for pat in VERDICT_SUMMARY_PATTERNS):
            continue
        result[k] = v
    return result


# ── Verification ─────────────────────────────────────────────────────────────

def verify_no_telltale(text: str) -> list:
    remaining = []
    checks = [
        # Text leaks
        "Tylluan", "SilvaDB", "CoherenceGate", "NightConsolidation",
        "Claude Code", "Antigravity",
        "tylluan_do", "tylluan_recall", "tylluan_remember",
        "guilds/core/", "crates/tylluan-",
        # Structural leaks
        '"c0"', '"c1"', "tsr_c0", "tsr_c1",
        "C0_baseline", "C1_tylluan",
        "mean_tsr_c0", "mean_tsr_c1",
        "mean_of_c0", "mean_of_c1",
        "mean_cd_c0", "mean_cd_c1",
        "condition_A", "condition_B",
        # Verdict leaks
        '"passed"', '"score":', "memory_harm",
    ]
    for phrase in checks:
        if phrase in text:
            remaining.append(phrase)
    return remaining


# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    import argparse
    parser = argparse.ArgumentParser(description="Anonymize TEB results for blind judge")
    parser.add_argument("--input", type=Path, default=DEFAULT_INPUT)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--mapping", type=Path, default=DEFAULT_MAPPING)
    parser.add_argument("--seed", type=int, default=None,
                        help="Random seed for per-task A/B assignment")
    args = parser.parse_args()

    if not args.input.exists():
        print(f"Error: input file not found: {args.input}", file=sys.stderr)
        sys.exit(1)

    data = json.loads(args.input.read_text(encoding="utf-8"))
    seed = args.seed if args.seed is not None else random.randint(0, 2**31)
    rng = random.Random(seed)

    print(f"Random seed: {seed}")

    # Per-task random A/B assignment
    per_task_mapping = {}
    for task in data.get("runs", [{}])[0].get("tasks", []):
        task_id = task.get("task_id", "unknown")
        a_is_c0 = rng.choice([True, False])
        per_task_mapping[task_id] = "c0" if a_is_c0 else "c1"

    # Anonymize runs
    anon_runs = []
    for run in data.get("runs", []):
        anon_tasks = []
        for task in run.get("tasks", []):
            task_id = task.get("task_id", "unknown")
            a_is_c0 = per_task_mapping[task_id] == "c0"
            anon_tasks.append(anonymize_task(task, a_is_c0))
        anon_run = {**run, "tasks": anon_tasks}
        # Strip per-run condition aggregates and verdict fields
        keys_to_remove = [
            k for k in anon_run.keys()
            if "_c0" in k or "_c1" in k
            or any(pat in k for pat in VERDICT_SUMMARY_PATTERNS)
        ]
        for key in keys_to_remove:
            del anon_run[key]
        anon_runs.append(anon_run)

    anon_data = {
        "benchmark": data.get("benchmark", "TEB-Pilot-50"),
        "runs_executed": data.get("runs_executed", 0),
        "base_seed": data.get("base_seed", 0),
        "telemetry": data.get("telemetry", {}),
        "summary": anonymize_summary(data.get("summary", {})),
        "runs": anon_runs,
    }

    # Anonymize all text content
    anon_data = anonymize_value(anon_data)

    # Write outputs
    args.output.write_text(json.dumps(anon_data, indent=2, ensure_ascii=False),
                           encoding="utf-8")

    mapping_data = {
        "seed": seed,
        "per_task": per_task_mapping,
        "note": "SECRET: maps task_id to which original condition was arm_0. Do NOT give to judge.",
    }
    args.mapping.write_text(json.dumps(mapping_data, indent=2, ensure_ascii=False),
                            encoding="utf-8")

    print(f"Anonymized output: {args.output}")
    print(f"Blind mapping (SECRET): {args.mapping}")
    print(f"Tasks randomized: {len(per_task_mapping)}")

    sample = list(per_task_mapping.items())[:5]
    for tid, orig in sample:
        print(f"  {tid}: arm_0 = {orig}")

    # Verification
    anon_text = json.dumps(anon_data, ensure_ascii=False)
    remaining = verify_no_telltale(anon_text)
    if remaining:
        print(f"FAIL: {len(remaining)} leak(s) remain: {remaining}", file=sys.stderr)
        sys.exit(1)
    else:
        print("VERIFIED: Zero tell-tale phrases, field names, or verdicts in output.")

    # Verify verdicts are stripped from a sample task
    sample_task = anon_data["runs"][0]["tasks"][0]
    for arm in ["arm_0", "arm_1"]:
        if arm in sample_task:
            entry = sample_task[arm]
            for vfield in VERDICT_FIELDS:
                if vfield in entry:
                    print(f"FAIL: verdict field '{vfield}' still present in {arm}", file=sys.stderr)
                    sys.exit(1)
    print("VERIFIED: Verdict fields (passed/score/memory_harm) stripped from all entries.")

    orig_text = json.dumps(data, ensure_ascii=False)
    print(f"Original: {len(orig_text)} chars -> Anonymized: {len(anon_text)} chars")


if __name__ == "__main__":
    main()
