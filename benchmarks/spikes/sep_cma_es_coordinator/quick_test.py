"""Quick test of HTTP dispatch against real kernel for one scenario."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path("benchmarks/spikes/sep_cma_es_coordinator")))
from spike_train import _dispatch, _is_failure, KERNEL_URL

print(f"Kernel: {KERNEL_URL}")

tests = [
    "check system CPU usage",
    "check system memory usage",
    "list files in current directory",
    "show me the last 5 git commits",
]

for t in tests:
    result, elapsed = _dispatch(t)
    failed = _is_failure(result)
    print(f"\n  Intent: {t}")
    print(f"  Elapsed: {elapsed:.0f}ms")
    print(f"  Failed: {failed}")
    print(f"  Result: {result[:200]}")
