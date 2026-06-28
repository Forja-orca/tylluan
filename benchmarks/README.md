# Tylluan Benchmarks

Reproducible latency and throughput measurements against a running kernel.

## Quick start

```bash
# Start the kernel first
tylluan.bat         # Windows
./tylluan.sh        # Linux/Mac

# Run benchmarks (echo mode, no LLM needed)
python benchmarks/run.py

# Skip slow embedding benchmarks
python benchmarks/run.py --skip-memory

# Custom kernel URL
python benchmarks/run.py --kernel http://127.0.0.1:3033
```

Results are saved to `benchmarks/results/<timestamp>.json`.

## What is measured

| Benchmark | Method | N | What it tests |
|---|---|---|---|
| `http_baseline_ms` | GET /health | 100 | Raw HTTP overhead |
| `coloquio_post_ms` | POST /coloquio/.../post | 50 | Message persistence + SSE broadcast |
| `memory_write_ms` | POST /memory/store | 10 | BGE-M3 embedding + SQLite write (CPU-bound) |
| `memory_recall_ms` | POST /memory/search | 10 | Vector search + BM25 + Jina reranker |
| `kernel_rss_mb` | — | — | Process RSS at idle |

## Methodology

- **No warmup skipped**: first request counted, reflects cold cache behavior.
- **Sequential requests**: no parallelism, measures single-client latency not throughput.
- **CPU-only**: BGE-M3 and Jina run on CPU. Expect 2-8s for memory_write on typical hardware.
- **Percentiles reported**: p50, p95, p99 across N samples.

## Reference numbers (baseline, i7-12700K, Windows 11, no GPU)

```
Benchmark                        p50     p95     p99
----------------------------------------------------
http_baseline_ms               0.4ms   0.7ms   1.1ms
coloquio_post_ms               2.1ms   3.8ms   6.2ms
memory_write_ms             2840.0ms  3200ms  3800ms   (BGE-M3 CPU)
memory_recall_ms              180.0ms  240ms   310ms
kernel_rss_mb                 142.0 MB
```

## Reproducing these numbers

1. Clone the repo and build: `cargo build -p tylluan-kernel`
2. Start: `tylluan.bat`
3. Run: `python benchmarks/run.py --out benchmarks/results/your_machine.json`
4. Share your results in [GitHub Discussions](https://github.com/Forja-orca/tylluan/discussions)

## Sharing your results

Open a Discussion with the `benchmark` label and paste your JSON output.
We are collecting results across hardware to build a performance baseline.
