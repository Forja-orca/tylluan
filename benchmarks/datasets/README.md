# SilvaDB Reference Benchmark Dataset (Anonymized)

**Created:** silva_reference_anon.db  
**Node Count:** 6776  
**Size:** 94.79 MB  
**Status:** Reproducible / Anonymized / MIT Licensed  

---

## 1. Purpose

This snapshot provides a realistic, structurally faithful, and reproducible reference database for:
1. **Stage 1 Hybrid Search Cascade Benchmarks (`search.rs:444-486`):** Testing the agreement gate (`FTS5 + BGE-M3 + Sparse`) on real graph densities (~6.7k nodes, ~12k edges).
2. **CPU Latency & Multi-Agent Concurrency Evaluation:** Reproducing loaded conditions without depending on local untracked databases.
3. **Graph Retrieval & Degree Centrality Experiments:** Verifying PageRank / PPR degree penalties (`pr / (1 + deg * 0.1)`).

---

## 2. Anonymization Protocol & Privacy Guarantees

The snapshot was generated using `benchmarks/datasets/anonymize_silva.py` applying strict sanitization passes:

- **Credentials & API Keys:** All matches for `sk-...`, `ghp_...`, and Bearer tokens are scrubbed with `[REDACTED_API_KEY]`.
- **Filesystem Paths:** Absolute host paths (`C:\Users\...`, `/home/...`) are scrubbed with `[ANONYMIZED_PATH]`.
- **User / Email Identifiers:** Replaced with `agent@tylluan.local`.
- **Session Tables Cleared:** `mcp_sessions`, `recall_misses`, and `recall_feedback` emptied.
- **FTS5 Index Rebuilt:** Full-text search index re-indexed from sanitized text.

---

## 3. Usage in Benchmark Harnesses

To run retrieval benchmarks or latency sweeps against this reference snapshot:

```bash
# Point the kernel to the snapshot via environment or CLI flag
TYLLUAN_SILVA_DB=benchmarks/datasets/silva_reference_anon.db cargo test -p tylluan-evals
```
