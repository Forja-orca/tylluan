# Tylluan Benchmarks

Reproducible latency and throughput measurements against a running kernel.

## Quick start

> **This suite writes to the kernel it targets** (nodes with `agent_id=benchmark`, and messages in the Coloquio channel `bench-coloquio`). Point it at a throwaway kernel with its own directory, config, `TYLLUAN_DATA_DIR` and `--port`, never at a kernel whose memory you care about. On Windows + WSL2, `127.0.0.1` inside WSL is forwarded to Windows, so an omitted `--kernel` can reach your real kernel: always pass it explicitly.

```bash
# Start a separate kernel first (see docs/getting-started/QUICKSTART.md)
tylluan-cli start

# Run benchmarks (echo mode, no LLM needed)
python benchmarks/run.py

# Skip slow embedding benchmarks
python benchmarks/run.py --skip-memory

# Custom kernel URL
python benchmarks/run.py --kernel http://127.0.0.1:47019
```

Results are saved to `benchmarks/results/<timestamp>.json`.

## What is measured

| Benchmark | Method | N | What it tests |
|---|---|---|---|
| `http_baseline_ms` | GET /health | 100 | Raw HTTP overhead |
| `coloquio_post_ms` | POST /coloquio/.../post | 50 | Message persistence + SSE broadcast |
| `memory_write_ms` | POST /api/v1/memory/write | 10 | Node upsert into SQLite (includes embedding cost if the write path embeds) |
| `memory_recall_ms` | POST /api/v1/memory/search | 10 | Search over whatever the database holds, after the writes above |
| `kernel_rss_mb` | OS process table | — | RSS of the single local `tylluan-nexus` process (`unavailable` if there are none or several; it does not read the process behind `--kernel`) |

## Methodology

- **No warmup skipped**: first request counted, reflects cold cache behavior.
- **Sequential requests**: no parallelism, measures single-client latency not throughput.
- **Depends on your configuration**: the embedding model, profile and hardware change every number. No per-model latency figure is published here because none has been measured on a fixed reference setup.
- **Percentiles reported**: p50, p95, p99 across N samples.

## About the other documents in this folder

The `.md` reports here (`BENCHMARK_*`, `RESEARCH_*`, `DEEP_RESEARCH_*`, `AUDIT_*`, `COMPARISON`) are dated records. Each one's numbers belong to the kernel commit and date it cites, and none has been re-run against v0.17.1 unless it says so. Read them as what was measured then, not as the current behaviour of the router or the retrieval.

## Reference numbers

No pre-published numbers. Hardware varies too much to be meaningful.

Run the suite yourself and submit your results to
[GitHub Discussions](https://github.com/Forja-orca/tylluan/discussions) —
that is how the community baseline gets built.

```bash
python benchmarks/run.py --out benchmarks/results/my_machine.json
```

The JSON output includes platform, CPU count, and kernel version so results
are attributable to specific hardware.

## Sharing your results

Open a Discussion with the `benchmark` label and paste your JSON output.
We are collecting results across hardware to build a performance baseline.
