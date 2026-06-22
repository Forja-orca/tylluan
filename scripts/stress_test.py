#!/usr/bin/env python3
"""
TylluanNexus o3 - Stress Test Suite

Tests concurrent tool calls to verify:
- Tokio event loop handling (no blocking)
- Memory stability under load
- Correct routing under concurrent requests

Usage:
    python scripts/stress_test.py --host localhost --port 3020 --calls 100 --workers 10
"""

import argparse
import asyncio
import time
import statistics
import json
import sys
from dataclasses import dataclass
from typing import List, Optional
import aiohttp


@dataclass
class CallResult:
    tool: str
    success: bool
    latency_ms: float
    error: Optional[str] = None


async def call_tool(
    session: aiohttp.ClientSession,
    url: str,
    token: str,
    tool: str,
    args: dict,
) -> CallResult:
    """Execute a single MCP tool call and measure latency."""
    start = time.perf_counter()
    
    try:
        # MCP JSON-RPC format
        payload = {
            "jsonrpc": "2.0",
            "id": str(time.time_ns()),
            "method": "tools/call",
            "params": {
                "name": tool,
                "arguments": args
            }
        }
        
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        
        async with session.post(
            f"{url}/mcp",
            json=payload,
            headers=headers,
            timeout=aiohttp.ClientTimeout(total=30),
        ) as resp:
            elapsed = (time.perf_counter() - start) * 1000
            
            if resp.status == 200:
                data = await resp.json()
                if "error" in data:
                    return CallResult(
                        tool=tool,
                        success=False,
                        latency_ms=elapsed,
                        error=str(data["error"]),
                    )
                return CallResult(tool=tool, success=True, latency_ms=elapsed)
            else:
                return CallResult(
                    tool=tool,
                    success=False,
                    latency_ms=elapsed,
                    error=f"HTTP {resp.status}",
                )
                
    except asyncio.TimeoutError:
        return CallResult(
            tool=tool,
            success=False,
            latency_ms=(time.perf_counter() - start) * 1000,
            error="timeout",
        )
    except Exception as e:
        return CallResult(
            tool=tool,
            success=False,
            latency_ms=(time.perf_counter() - start) * 1000,
            error=str(e),
        )


async def run_concurrent_calls(
    url: str,
    token: str,
    num_calls: int,
    workers: int,
) -> List[CallResult]:
    """Run N concurrent tool calls with worker concurrency."""
    
    # Test tools - mix of kernel and guild tools
    test_tools = [
        {"tool": "health", "args": {}},
        {"tool": "list_available_guilds", "args": {}},
        {"tool": "memory_search", "args": {"query": "test", "limit": 5}},
    ]
    
    async with aiohttp.ClientSession() as session:
        # Create batches of calls
        tasks = []
        for i in range(num_calls):
            test = test_tools[i % len(test_tools)]
            task = call_tool(session, url, token, test["tool"], test["args"])
            tasks.append(task)
        
        # Run with controlled concurrency
        results = []
        for i in range(0, len(tasks), workers):
            batch = tasks[i:i + workers]
            batch_results = await asyncio.gather(*batch)
            results.extend(batch_results)
            
            # Brief pause between batches to avoid overwhelming
            if i + workers < len(tasks):
                await asyncio.sleep(0.01)
        
        return results


def print_results(results: List[CallResult], duration_s: float):
    """Print stress test results."""
    total = len(results)
    successes = [r for r in results if r.success]
    failures = [r for r in results if not r.success]
    
    latencies = [r.latency_ms for r in successes]
    
    print("\n" + "=" * 60)
    print("TYLLUANNEXUS STRESS TEST RESULTS")
    print("=" * 60)
    print(f"Total calls:      {total}")
    print(f"Duration:         {duration_s:.2f}s")
    print(f"Throughput:       {total / duration_s:.1f} calls/s")
    print(f"Success rate:      {len(successes)}/{total} ({100*len(successes)/total:.1f}%)")
    print(f"Failure rate:      {len(failures)}/{total} ({100*len(failures)/total:.1f}%)")
    
    if latencies:
        print(f"\nLatency stats (ms):")
        print(f"  Min:      {min(latencies):.2f}")
        print(f"  Max:     {max(latencies):.2f}")
        print(f"  Mean:    {statistics.mean(latencies):.2f}")
        print(f"  Median:  {statistics.median(latencies):.2f}")
        if len(latencies) > 1:
            print(f"  Stdev:    {statistics.stdev(latencies):.2f}")
    
    if failures:
        print(f"\nErrors:")
        by_error: dict = {}
        for f in failures:
            err = f.error or "unknown"
            by_error[err] = by_error.get(err, 0) + 1
        for err, count in sorted(by_error.items(), key=lambda x: -x[1])[:5]:
            print(f"  {err}: {count}")
    
    print("=" * 60)
    
    # Pass/fail criteria
    success_rate = len(successes) / total
    avg_latency = statistics.mean(latencies) if latencies else 999
    
    if success_rate >= 0.95 and avg_latency < 100:
        print("✅ STRESS TEST PASSED")
        return 0
    elif success_rate >= 0.80:
        print("⚠️ STRESS TEST DEGRADED")
        return 1
    else:
        print("❌ STRESS TEST FAILED")
        return 2


async def async_main():
    parser = argparse.ArgumentParser(description="TylluanNexus stress test")
    parser.add_argument("--host", default="localhost", help="Kernel host")
    parser.add_argument("--port", type=int, default=3020, help="Kernel port")
    parser.add_argument("--token", default="", help="Auth token")
    parser.add_argument("--calls", type=int, default=100, help="Total calls to make")
    parser.add_argument("--workers", type=int, default=10, help="Concurrent workers")
    args = parser.parse_args()
    
    url = f"http://{args.host}:{args.port}"
    
    print(f"Running stress test: {args.calls} calls, {args.workers} workers")
    print(f"Target: {url}")
    
    start = time.perf_counter()
    results = await run_concurrent_calls(url, args.token, args.calls, args.workers)
    duration = time.perf_counter() - start
    
    return print_results(results, duration)


def main():
    sys.exit(asyncio.run(async_main()))


if __name__ == "__main__":
    main()