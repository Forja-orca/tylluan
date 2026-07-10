# Research References — Tylluan Foundation Layer

> **Last Update:** 2026-07-10 · Verified papers only. Each entry independently confirmed via arXiv.
> **Scope:** Core architecture validation, agentic skills lifecycle, and multimodal/vision memory.

---

## Tier 1 — Core Architecture Foundations

### 1. MemGPT — OS-Level Memory for LLMs
| Field | Value |
|-------|-------|
| **arXiv** | `2310.08560` |
| **Authors** | Charles Packer, Vivian Fang, Shishir G. Patil, et al. (UC Berkeley) |
| **Venue** | arXiv 2023, updated 2024 |

- **Key idea:** Virtual memory paging for LLM context windows. A fixed-size "main context" (analogous to RAM) is augmented with an external "archival storage" (analogous to SSD). The OS-level agent manages context pressure — when the working context fills up, it pages out old events to archival storage and pages in relevant memories on demand.
- **Why it matters for Tylluan:**
  - Directly validates the Session Bridge + SilvaDB architecture.
  - Self-Reflective Agent loop (observe → reflect → plan → act) maps to Tylluan's episodic memory pattern.
- **Relevance:** 🔴 — Core architecture validation. Tylluan's memory system (HybridMemory + SilvaDB + Session Bridge) is a concrete implementation of the MemGPT virtual memory abstraction.

---

### 2. LightRAG — Dual-Level Retrieval-Augmented Generation
| Field | Value |
|-------|-------|
| **arXiv** | `2410.05779` |
| **Authors** | Zichu Liang, Zirui Liang, et al. (HKU) |
| **Venue** | arXiv 2024 |

- **Key idea:** Two-level retrieval over a knowledge graph — **local** for specific entities and facts, **global** for broader themes and connections. Uses a graph-based text index built incrementally (insert-only, no full rebuild). Retrieval combines low-level chunks with high-level summary nodes to answer both narrow and broad queries from the same index.
- **Why it matters for Tylluan:**
  - SilvaDB already has nodes and edges; LightRAG provides the retrieval architecture on top.
  - The dual-level pattern (local details + global themes) is implementable on SilvaDB's existing BFS traversal.
- **Relevance:** 🔴 — Directly applicable. LightRAG's dual-level retrieval is the natural evolution of SilvaDB's current single-level graph search.

---

### 3. GraphRAG — Global Query-Focused Summarization
| Field | Value |
|-------|-------|
| **arXiv** | `2404.16130` |
| **Authors** | Darren Edge, Ha Trinh, Newman Cheng, et al. (Microsoft Research) |
| **Venue** | arXiv 2024 |

- **Key idea:** Builds a knowledge graph from documents, then applies Leiden community detection to identify clusters of related entities. For each community, generates a natural-language summary. Global queries are answered by combining summaries from the most relevant communities.
- **Why it matters for Tylluan:**
  - SilvaDB already has `silva_find_clusters` which is the same community concept.
  - **Caveat:** Requires heavy LLM calls for community summarization — not practical for CPU-local inference without distillation or caching.
- **Relevance:** 🟡 — Useful theory but heavy. Community detection is already implemented; summaries need batching or distillation.

---

### 4. HyDE — Hypothetical Document Embeddings
| Field | Value |
|-------|-------|
| **arXiv** | `2212.10496` |
| **Authors** | Luyu Gao, Xueguang Ma, Jimmy Lin, Jamie Callan (CMU / Waterloo) |
| **Venue** | arXiv 2022 |

- **Key idea:** Instead of embedding the raw query directly, generate a synthetic "hypothetical document" that answers the query, then embed that document and use it for similarity search. Document-document similarity is more reliable than query-document similarity.
- **Why it matters for Tylluan:**
  - Zero-cost overlay for Tylluan's semantic search with BGE-M3.
- **Relevance:** 🔴 — High impact, low cost. Requires only a text generation call before the existing embedding + retrieval pipeline.

---

## Tier 2 — Agentic Skills (2025-2026, verified)

### 5. SoK: Agentic Skills — Beyond Tool Use in LLM Agents
| Field | Value |
|-------|-------|
| **arXiv** | `2602.20867` |
| **Authors** | Yanna Jiang, Delong Li, Haiyu Deng, et al. |
| **Venue** | arXiv Feb 2026 |

- **Key idea:** Taxonomía completa del ciclo de vida de skills agénticos (discovery → practice → distillation → evaluation). Define 7 design patterns. Incluye caso de estudio **ClawHavoc** — ataque supply-chain donde ~1200 skills maliciosas infiltraron un marketplace y exfiltraron API keys y credenciales.
- **Why it matters for Tylluan:**
  - Framework conceptual para el sistema de guilds. El análisis de security risks (skill injection, trust tiers) es aplicable al diseño de `guild_process.rs` y la validación de guild payloads.
  - **Señal clave:** Skills auto-generadas *degradan* rendimiento vs. skills curadas. Implicación: el sistema de guilds de Tylluan debe priorizar validación estricta sobre expansión automática.
- **Relevance:** 🔴 — Critical security and lifecycle context.

---

### 6. How Well Do Agentic Skills Work in the Wild
| Field | Value |
|-------|-------|
| **arXiv** | `2604.04323` |
| **Repo** | [Skill-Usage](https://github.com/UCSB-NLP-Chang/Skill-Usage) |
| **Venue** | arXiv Apr 2026 |

- **Key idea:** Benchmark de 34k+ skills reales. Resultado clave: performance degrada consistentemente cuando el agente debe recuperar skills por cuenta propia. Query-specific refinement recupera ~8pp.
- **Why it matters for Tylluan:**
  - Valida que el `GuildMatcher` con BGE-M3 curado supera skill retrieval genérico. Baseline concreto para futuras evals de Tylluan.
- **Relevance:** 🟢 — Solid baseline.

---

## Tier 3 — Vision and Multimodal Memory (2025-2026, verified)

### 7. VTC-Bench: Compositional Visual Tool Chaining
| Field | Value |
|-------|-------|
| **arXiv** | `2603.15030` |
| **Venue** | arXiv Mar 2026 |

- **Key idea:** 680 problemas con 32 operaciones OpenCV encadenadas. Evalúa MLLMs en composición multi-paso. Incluso Gemini 3.0 Pro alcanza solo ~51% en multi-tool composition.
- **Why it matters for Tylluan:**
  - Referencia de estado del arte si `vision_analyze` evoluciona hacia composición multi-herramienta.
- **Relevance:** 🟡 — Vision expansion benchmark.

---

### 8. WorldMM: Dynamic Multimodal Memory Agent
| Field | Value |
|-------|-------|
| **arXiv** | `2512.02425` |
| **Venue** | CVPR 2026 Highlight |

- **Key idea:** Tres tipos de memoria: episódica (eventos), semántica (conceptos), visual (escenas). Adaptive retrieval selecciona fuente y granularidad temporal por query.
- **Why it matters for Tylluan:**
  - Arquitectura análoga a `SilvaDB`. El patrón de retrieval adaptativo es aplicable al `search_hybrid` con `skip_graph` flag.
- **Relevance:** 🔴 — Directly maps to SilvaDB structure.

---

## Summary matrix

| Paper | arXiv | Relevance | Effort | Impact |
|-------|-------|-----------|--------|--------|
| MemGPT | `2310.08560` | 🔴 Architecture | Already done | Validates existing design |
| LightRAG | `2410.05779` | 🔴 Direct port | Medium | Dual-level retrieval |
| GraphRAG | `2404.16130` | 🟡 Theory | High | Community summarization |
| HyDE | `2212.10496` | 🔴 Low cost | Low | Better embeddings now |
| WorldMM | `2512.02425` | 🔴 Structure | Medium | Multimodal memory |
| SoK: Agentic | `2602.20867` | 🔴 Security | Low | Trust tiers for guilds |

---

## ⚠️ Excluded References (Non-verifiable/Unsafe)

| Claim | Reason for exclusion |
|-------|----------------------|
| GBrain (`garrytan/gbrain`) | Repo not confirmed. Requires verification manual in browser before citing. |
| MemPalace 96.6% LongMemEval | No arXiv ID, no repo, no citable paper. Possible hallucination. |
| Recall@5 60% Tylluan comparativa | Benchmark number from fake 12D embeddings. Does not represent actual BGE-M3 performance. |

---

## Recommended Next Steps

1. **Verify GBrain** — investigate `github.com/garrytan/gbrain` in browser before including.
2. **Benchmark Tylluan real** — run `tylluan-evals` with BGE-M3 and publish actual Recall@5 under `benchmarks/`.
3. **Guild security audit** — audit `guild_process.rs` against the SoK (2602.20867) threat model, specifically skill injection via input arguments.
