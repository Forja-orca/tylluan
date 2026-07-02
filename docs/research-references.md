# Tylluan — Referencias de Investigación Verificadas

Documento curado por Antigravity · Actualizado 2026-07-02  
**Política:** solo papers con arXiv ID confirmado o repos con URL verificable. Sin claims sin fuente.

---

## Tier 1 — Fundacionales (ya relevantes en el stack actual)

| Paper | arXiv ID | Año | Relevancia para Tylluan |
|-------|----------|-----|------------------------|
| **MemGPT: Towards LLMs as Operating Systems** | [2310.08560](https://arxiv.org/abs/2310.08560) | 2023 | Arquitectura de memoria jerárquica (main/archival) — base conceptual de `silva/` y episodic store |
| **From Local to Global: GraphRAG** | [2404.16130](https://arxiv.org/abs/2404.16130) | 2024 | Graph-augmented RAG con comunidades — referente para `local_query_graph` + PPR + degree penalty |
| **LightRAG: Simple and Fast RAG** | [2410.05779](https://arxiv.org/abs/2410.05779) | 2024 | Integración de grafo de conocimiento ligero con LLM — patrón de doble indexación (vector + grafo) |
| **HyDE: Precise Zero-Shot Dense Retrieval** | [2212.10496](https://arxiv.org/abs/2212.10496) | 2022 | Hypothetical Document Embeddings — técnica de query expansion aplicable a `search_hybrid` |

> **Nota:** El GraphRAG original de Microsoft es el arXiv 2404.16130. El ID 2310.05240 mencionado en reportes anteriores **no corresponde a GraphRAG** — usar 2404.16130.

---

## Tier 2 — Agentic Skills (2025-2026, verificados)

### SoK: Agentic Skills — Beyond Tool Use in LLM Agents
- **arXiv:** [2602.20867](https://arxiv.org/abs/2602.20867) · Febrero 2026
- **Autores:** Yanna Jiang, Delong Li, Haiyu Deng et al.
- **Qué aporta:** Taxonomía completa del ciclo de vida de skills agénticos (discovery → practice → distillation → evaluation). Define 7 design patterns. Incluye caso de estudio **ClawHavoc** — ataque supply-chain donde ~1200 skills maliciosas infiltraron un marketplace y exfiltraron API keys y credenciales.
- **Relevancia para Tylluan:** Framework conceptual para el sistema de guilds. El análisis de security risks (skill injection, trust tiers) es directamente aplicable al diseño de `guild_process.rs` y la validación de guild payloads.
- **Señal clave:** Skills auto-generadas *degradan* rendimiento vs. skills curadas. Implicación: el sistema de guilds de Tylluan debe priorizar validación estricta sobre expansión automática.

### How Well Do Agentic Skills Work in the Wild
- **arXiv:** [2604.04323](https://arxiv.org/abs/2604.04323) · Abril 2026
- **Repo:** https://github.com/UCSB-NLP-Chang/Skill-Usage ✅ verificado
- **Qué aporta:** Benchmark de 34k+ skills reales. Resultado clave: performance degrada consistentemente cuando el agente debe recuperar skills por cuenta propia. Query-specific refinement recupera ~8pp. Mejora Claude Opus en Terminal-Bench 2.0: 57.7% → 65.5%.
- **Relevancia para Tylluan:** Valida que el `GuildMatcher` con BGE-M3 curado supera skill retrieval genérico. Baseline concreto para futuras evals de Tylluan.

---

## Tier 3 — Vision y Memoria Multimodal (2025-2026, verificados)

### VTC-Bench: Compositional Visual Tool Chaining
- **arXiv:** [2603.15030](https://arxiv.org/abs/2603.15030) · Marzo 2026
- **Qué aporta:** 680 problemas con 32 operaciones OpenCV encadenadas. Evalúa MLLMs en composición multi-paso. Incluso Gemini 3.0 Pro alcanza solo ~51% en multi-tool composition.
- **Relevancia para Tylluan:** Referencia de estado del arte si `vision_analyze` evoluciona hacia composición multi-herramienta. Define el techo a superar.

### WorldMM: Dynamic Multimodal Memory Agent
- **arXiv:** [2512.02425](https://arxiv.org/abs/2512.02425) · Diciembre 2025 · **CVPR 2026 Highlight**
- **Afiliaciones:** KAIST, NTU Singapore, DeepAuto.ai
- **Qué aporta:** Tres tipos de memoria: episódica (eventos), semántica (conceptos), visual (escenas). Adaptive retrieval selecciona fuente y granularidad temporal por query. +8.4% sobre SOTA en 5 benchmarks long-video QA.
- **Relevancia para Tylluan:** Arquitectura análoga a `SilvaDB`. El patrón de retrieval adaptativo es aplicable al `search_hybrid` con `skip_graph` flag.

---

## Tier 4 — Benchmarks Agentic (referencias rápidas)

| Benchmark | Descripción | Fuente |
|-----------|-------------|--------|
| Terminal-Bench 2.0 | Tareas CLI reales, ejecución autónoma | Citado en 2604.04323 |
| EmbodiedBench | Agentes embodied con visión: navegación → organización | ICML 2026 |
| BALROG | Razonamiento agéntico en entornos de juego complejos | OpenReview (verificado) |

---

## ⚠️ Referencias EXCLUIDAS (no verificables)

| Claim | Motivo de exclusión |
|-------|---------------------|
| GBrain (`garrytan/gbrain`) | Repo no confirmado. Requiere verificación manual en browser antes de citar. |
| MemPalace 96.6% LongMemEval | Sin arXiv ID, sin repo, sin paper citable. Posible alucinación. |
| Recall@5 60% Tylluan comparativa | Número de benchmark con embeddings fake 12D. No representa rendimiento real con BGE-M3. |
| GraphRAG arXiv 2310.05240 | ID incorrecto. El ID real de GraphRAG es 2404.16130. |

---

## Próximos pasos recomendados

1. **Verificar GBrain** — abrir `github.com/garrytan/gbrain` en browser antes de incluir.
2. **Benchmark Tylluan real** — ejecutar `tylluan-evals` con BGE-M3 y publicar Recall@5 propio en `benchmarks/`.
3. **Portear patrón WorldMM** — la distinción episódica/semántica/visual es roadmap claro para `SilvaDB` en v0.12.0+.
4. **Guild security audit** — revisar `guild_process.rs` contra el threat model de SoK (2602.20867), especialmente skill injection via guild payloads.
