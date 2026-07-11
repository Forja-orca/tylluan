#!/usr/bin/env python3
"""
M18-P3b — Coordinator parallelism re-benchmark.

Compares two dispatch strategies for the same multi-step intents:
  "sin" (baseline)  — each sub-task sent as its own POST /api/v1/do call,
                       run sequentially by this script (no coordinator
                       involved at all).
  "con" (coordinator) — the full multi-step intent sent as ONE
                       POST /api/v1/do call, letting the M20 Complexity
                       Cascade route it to the coordinator guild, which
                       dispatches independent sub-tasks concurrently via
                       ThreadPoolExecutor (guilds/core/coordinator.py).

Each query's sub-tasks are independent (no "then summarize" / pronoun
back-reference), so the coordinator is expected to run them in parallel
rather than falling back to its sequential context-dependent path.

Reproduces the previously-lost benchmark referenced in STATUS.md (M18-P3b)
after the spawn_blocking audit-log fix (commit 5698051) — that fix removed
a tokio::spawn'd synchronous rusqlite write that was blocking a runtime
worker thread during concurrent coordinator dispatches, which was suspected
to be masking the real parallelism win.

Usage:
    python benchmarks/coordinator_bench.py
    python benchmarks/coordinator_bench.py --kernel http://127.0.0.1:4000 --repeats 5
    python benchmarks/coordinator_bench.py --out results/coordinator_latencies.json
"""

import argparse
import json
import statistics
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

# Each entry: (label, [independent sub-task strings]).
# Sub-tasks use guilds that are always-on (system_metrics, websearch) so
# guild cold-start latency doesn't contaminate the measurement.
QUERIES = [
    ("cpu_and_disk", ["check system CPU usage", "check system disk usage"]),
    ("cpu_and_memory", ["check system CPU usage", "check system memory usage"]),
    ("two_web_searches", ["search the web for rust async runtime benchmarks",
                          "search the web for python asyncio performance"]),
    ("three_metrics", ["check system CPU usage", "check system memory usage",
                       "check system disk usage"]),
    ("mixed_metrics_search", ["check system CPU usage",
                              "search the web for tokio scheduler internals"]),
]


def _req(url: str, method: str = "GET", body=None, token=None, timeout: int = 30):
    data = json.dumps(body).encode() if body is not None else None
    headers = {"Content-Type": "application/json"} if data else {}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            elapsed = (time.perf_counter() - t0) * 1000
            return r.status, json.loads(r.read()), elapsed
    except urllib.error.HTTPError as e:
        elapsed = (time.perf_counter() - t0) * 1000
        return e.code, {}, elapsed
    except Exception as exc:
        elapsed = (time.perf_counter() - t0) * 1000
        return 0, {"_error": str(exc)}, elapsed


def run_baseline(kernel: str, token, sub_tasks: list) -> tuple:
    """Sequential /api/v1/do calls, one per sub-task. Returns (total_ms, error_count)."""
    total = 0.0
    errors = 0
    for task in sub_tasks:
        status, _, ms = _req(f"{kernel}/api/v1/do", method="POST",
                              body={"intent": task}, token=token)
        total += ms
        if status != 200:
            errors += 1
    return total, errors


def run_coordinator(kernel: str, token, sub_tasks: list) -> tuple:
    """One /api/v1/do call with the full multi-step intent joined by 'and then'."""
    intent = " and then ".join(sub_tasks)
    status, _, ms = _req(f"{kernel}/api/v1/do", method="POST",
                          body={"intent": intent}, token=token, timeout=60)
    return ms, (0 if status == 200 else 1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--kernel", default="http://127.0.0.1:4000")
    ap.add_argument("--token", default=None)
    ap.add_argument("--repeats", type=int, default=3,
                    help="repeats per query, per strategy (default 3)")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    print(f"Coordinator benchmark against {args.kernel} ({args.repeats} repeats/query)")

    results = []
    for label, sub_tasks in QUERIES:
        sin_samples, con_samples = [], []
        errors_sin = errors_con = 0

        for _ in range(args.repeats):
            ms, err = run_baseline(args.kernel, args.token, sub_tasks)
            sin_samples.append(ms)
            errors_sin += err

        for _ in range(args.repeats):
            ms, err = run_coordinator(args.kernel, args.token, sub_tasks)
            con_samples.append(ms)
            errors_con += err

        mean_sin = statistics.mean(sin_samples)
        mean_con = statistics.mean(con_samples)
        delta_pct = ((mean_sin - mean_con) / mean_sin) * 100 if mean_sin else 0.0

        print(f"  {label:24s} sin={mean_sin:8.1f}ms  con={mean_con:8.1f}ms  "
              f"delta={delta_pct:+6.1f}%  errors(sin/con)={errors_sin}/{errors_con}")

        results.append({
            "query": label,
            "sub_tasks": sub_tasks,
            "mean_sin_ms": mean_sin,
            "mean_con_ms": mean_con,
            "delta_pct": delta_pct,
            "errors_sin": errors_sin,
            "errors_con": errors_con,
        })

    valid = [r for r in results if r["errors_sin"] == 0 and r["errors_con"] == 0]
    mean_sin_all = statistics.mean(r["mean_sin_ms"] for r in valid) if valid else 0
    mean_con_all = statistics.mean(r["mean_con_ms"] for r in valid) if valid else 0
    improvement_pct_of_means = (
        ((mean_sin_all - mean_con_all) / mean_sin_all) * 100 if mean_sin_all else 0
    )
    mean_individual_delta_pct = (
        statistics.mean(r["delta_pct"] for r in valid) if valid else 0
    )

    summary = {
        "metadata": {
            "timestamp": time.time(),
            "generated_at_utc": datetime.now(timezone.utc).isoformat(),
            "kernel": args.kernel,
            "repeats_per_query": args.repeats,
            "mean_sin_ms": mean_sin_all,
            "mean_con_ms": mean_con_all,
            "improvement_pct_of_means": improvement_pct_of_means,
            "mean_individual_delta_pct": mean_individual_delta_pct,
            "valid_queries": len(valid),
            "total_queries": len(results),
        },
        "queries": results,
    }

    print()
    print(f"Delta of means:            {improvement_pct_of_means:+.1f}%")
    print(f"Mean of per-query deltas:  {mean_individual_delta_pct:+.1f}%")
    print(f"Valid queries: {len(valid)}/{len(results)}")

    out_path = Path(args.out) if args.out else Path(__file__).parent / "results" / "coordinator_latencies.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(summary, indent=2))
    print(f"\nSaved: {out_path}")


if __name__ == "__main__":
    main()
