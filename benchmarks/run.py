#!/usr/bin/env python3
"""
Tylluan benchmark suite — reproducible latency and throughput measurements.

Measures four things any deployment can reproduce:
  1. HTTP baseline  — GET /health, pure network overhead
  2. Coloquio post  — POST message end-to-end (persist + broadcast)
  3. Memory write   — POST /api/v1/memory/store (embed + store, CPU-bound)
  4. Kernel RSS     — idle memory footprint in MB

Results saved to benchmarks/results/<timestamp>.json

Usage:
    python benchmarks/run.py
    python benchmarks/run.py --kernel http://127.0.0.1:3033 --out results/my_run.json
    python benchmarks/run.py --skip-memory   # skip slow embedding benchmark
"""

import argparse
import json
import os
import platform
import statistics
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


# -- helpers -----------------------------------------------------------------

def _req(url: str, method: str = "GET", body=None, token=None, timeout: int = 15) -> tuple:
    """Returns (status_code, response_dict, elapsed_ms)."""
    data = json.dumps(body).encode() if body is not None else None
    headers = {}
    if data:
        headers["Content-Type"] = "application/json"
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


def percentiles(samples: list) -> dict:
    s = sorted(samples)
    n = len(s)
    return {
        "n": n,
        "min": round(s[0], 2),
        "p50": round(s[n // 2], 2),
        "p95": round(s[int(n * 0.95)], 2),
        "p99": round(s[min(int(n * 0.99), n - 1)], 2),
        "max": round(s[-1], 2),
        "mean": round(statistics.mean(s), 2),
    }


def kernel_rss_mb(kernel: str, token=None) -> float:
    """Read kernel RSS from /api/v1/system/metrics if available."""
    _, data, _ = _req(f"{kernel}/api/v1/system/metrics", token=token, timeout=5)
    rss = data.get("memory_rss_bytes") or data.get("rss_bytes")
    if rss:
        return round(rss / 1024 / 1024, 1)
    # fallback: ask the OS about any process named tylluan-nexus
    try:
        import subprocess
        out = subprocess.check_output(
            ["powershell", "-Command",
             "(Get-Process tylluan-nexus -ErrorAction SilentlyContinue | Measure-Object WorkingSet -Sum).Sum"],
            timeout=5
        ).decode().strip()
        if out and out.isdigit():
            return round(int(out) / 1024 / 1024, 1)
    except Exception:
        pass
    return -1


# -- benchmark functions -----------------------------------------------------

def bench_http_baseline(kernel: str, token, n: int) -> dict:
    print(f"  [1/4] HTTP baseline  ({n} requests) ...", end=" ", flush=True)
    samples = []
    errors = 0
    for _ in range(n):
        status, _, ms = _req(f"{kernel}/health", token=token, timeout=5)
        if status == 200:
            samples.append(ms)
        else:
            errors += 1
    print(f"p50={statistics.median(samples):.1f}ms")
    return {**percentiles(samples), "errors": errors}


def bench_coloquio_post(kernel: str, token, n: int) -> dict:
    print(f"  [2/4] Coloquio post  ({n} messages) ...", end=" ", flush=True)
    channel = "bench-coloquio"
    # ensure channel exists
    _req(f"{kernel}/api/v1/coloquio/channels", "POST",
         {"id": channel, "name": "Benchmark channel"}, token=token)
    samples = []
    errors = 0
    for i in range(n):
        status, _, ms = _req(
            f"{kernel}/api/v1/coloquio/channels/{channel}/post", "POST",
            {"content": f"bench msg {i}", "author_id": "benchmark", "role": "agent"},
            token=token,
        )
        if status in (200, 201):
            samples.append(ms)
        else:
            errors += 1
    print(f"p50={statistics.median(samples):.1f}ms")
    return {**percentiles(samples), "errors": errors}


def bench_memory_write(kernel: str, token, n: int) -> dict:
    print(f"  [3/4] Memory write   ({n} embeddings, CPU-bound) ...", end=" ", flush=True)
    samples = []
    errors = 0
    for i in range(n):
        status, _, ms = _req(
            f"{kernel}/api/v1/memory/store", "POST",
            {"content": f"benchmark fact number {i}: the sky is blue and models are large",
             "tags": ["bench"], "source": "benchmark"},
            token=token, timeout=60,
        )
        if status in (200, 201):
            samples.append(ms)
        else:
            errors += 1
    if not samples:
        print("skipped (endpoint unavailable)")
        return {"skipped": True, "errors": errors}
    print(f"p50={statistics.median(samples):.1f}ms  (includes BGE-M3 embedding on CPU)")
    return {**percentiles(samples), "errors": errors, "note": "includes BGE-M3 CPU embedding"}


def bench_memory_recall(kernel: str, token, n: int) -> dict:
    print(f"  [3b/4] Memory recall ({n} queries) ...", end=" ", flush=True)
    samples = []
    errors = 0
    for i in range(n):
        status, _, ms = _req(
            f"{kernel}/api/v1/memory/search", "POST",
            {"query": "benchmark fact", "limit": 5},
            token=token, timeout=30,
        )
        if status == 200:
            samples.append(ms)
        else:
            errors += 1
    if not samples:
        print("skipped")
        return {"skipped": True, "errors": errors}
    print(f"p50={statistics.median(samples):.1f}ms")
    return {**percentiles(samples), "errors": errors}


# -- main --------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Tylluan benchmark suite")
    parser.add_argument("--kernel", default="http://127.0.0.1:3000")
    parser.add_argument("--token", default=None)
    parser.add_argument("--http-n", type=int, default=100, help="HTTP baseline requests")
    parser.add_argument("--post-n", type=int, default=50, help="Coloquio post samples")
    parser.add_argument("--mem-n", type=int, default=10, help="Memory write samples (slow)")
    parser.add_argument("--skip-memory", action="store_true", help="Skip embedding benchmarks")
    parser.add_argument("--out", default=None, help="Output JSON path (auto-named if omitted)")
    args = parser.parse_args()

    # Verify kernel
    status, health, _ = _req(f"{args.kernel}/health", token=args.token)
    if status != 200:
        print(f"ERROR: kernel not reachable at {args.kernel}")
        sys.exit(1)

    version = health.get("version", "unknown")
    print(f"\nTylluan benchmark — kernel {version} at {args.kernel}")
    print(f"Platform: {platform.system()} {platform.machine()} Python {platform.python_version()}\n")

    results = {
        "meta": {
            "kernel": args.kernel,
            "version": version,
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "platform": platform.system(),
            "python": platform.python_version(),
            "cpu_count": os.cpu_count(),
        },
        "benchmarks": {}
    }

    results["benchmarks"]["http_baseline_ms"] = bench_http_baseline(
        args.kernel, args.token, args.http_n)

    results["benchmarks"]["coloquio_post_ms"] = bench_coloquio_post(
        args.kernel, args.token, args.post_n)

    if not args.skip_memory:
        results["benchmarks"]["memory_write_ms"] = bench_memory_write(
            args.kernel, args.token, args.mem_n)
        results["benchmarks"]["memory_recall_ms"] = bench_memory_recall(
            args.kernel, args.token, args.mem_n)
    else:
        print("  [3/4] Memory benchmarks skipped (--skip-memory)")

    print(f"  [4/4] Kernel RSS ...", end=" ", flush=True)
    rss = kernel_rss_mb(args.kernel, args.token)
    results["benchmarks"]["kernel_rss_mb"] = rss
    print(f"{rss} MB" if rss >= 0 else "unavailable")

    # Save results
    out_dir = Path(__file__).parent / "results"
    out_dir.mkdir(exist_ok=True)
    if args.out:
        out_path = Path(args.out)
    else:
        ts = datetime.now().strftime("%Y%m%d_%H%M%S")
        out_path = out_dir / f"{ts}.json"

    out_path.write_text(json.dumps(results, indent=2))

    # Print summary table
    print(f"\n{'─' * 52}")
    print(f"  {'Benchmark':<28} {'p50':>6} {'p95':>6} {'p99':>6}")
    print(f"{'─' * 52}")
    for name, data in results["benchmarks"].items():
        if isinstance(data, dict) and "p50" in data:
            unit = " MB" if "rss" in name else " ms"
            print(f"  {name:<28} {data['p50']:>5.1f}{unit} {data['p95']:>5.1f}{unit} {data['p99']:>5.1f}{unit}")
        elif name == "kernel_rss_mb":
            print(f"  {'kernel_rss_mb':<28} {data:>5.1f} MB")
    print(f"{'─' * 52}")
    print(f"\n  Results saved to {out_path}")
    print(f"\n  To reproduce: python benchmarks/run.py --kernel {args.kernel}")


if __name__ == "__main__":
    main()
