# Research Index — Tylluan v0.2.0

> Consolidación de 4 agentes. 2026-06-25.
> Fuentes: Antigravity, Qwen3.7, Hermes, Claude Code

---

## Agentes Participantes

| Agente | Dominio | Repos | Papers | Estado |
|--------|---------|-------|--------|--------|
| **Antigravity** | Multi-Agent Orchestration | 5 | 5 | ✅ |
| **Qwen3.7** | Memory Systems & KG | 8 | 7 | ✅ |
| **Hermes** | CPU Inference & Embeddings | 8 | 3 | ✅ |
| **Claude Code** | Rust Frameworks + Consolidación | 9 | 0 | ✅ |

**Total: 30 repos investigados, 15 papers fundacionales**

---

## 1. Multi-Agent Orchestration (Antigravity)

| Repo | Stars | Clave para Tylluan |
|------|-------|-------------------|
| **LangGraph** (langchain-ai/langgraph) | 35k | Stateful Directed Cyclic Graph con checkpoints |
| **AG2** (ag2-ai/ag2, ex-AutoGen) | 40k | Group Chat Manager con turn-taking dinámico |
| **CrewAI** (crewAIInc/crewAI) | 40k | Declarative Role/Goal/Backstory, delegación jerárquica |
| **MetaGPT** (geekan/MetaGPT) | 45k | SOP-driven workflows, pub-sub message broker |
| **OpenHands** (All-Hands-AI/OpenHands) | 38k | CodeAct: tools como código ejecutable |

**Papers:** ReAct (`2210.03629`), AutoGen (`2308.08155`), CAMEL (`2303.17760`), MetaGPT (`2308.00352`), CodeAct (`2402.01030`)

### Patrón top: CodeAct
Representar tool calls como scripts ejecutables en vez de JSON — reduce errores de parsing en modelos locales pequeños.

---

## 2. Memory Systems & Knowledge Graphs (Qwen3.7)

| Repo | Stars | Clave para Tylluan |
|------|-------|-------------------|
| **Letta/MemGPT** (letta-ai/letta) | 23.5k | Virtual memory paging para contexto limitado |
| **Zep** (getzep/zep) | 4.7k | Temporal knowledge graph con episodios |
| **Mem0** (mem0ai/mem0) | 59.4k | Multi-signal retrieval (semantic+BM25+entity) |
| **GraphRAG** (microsoft/graphrag) | 34k | Leiden community detection + summaries jerárquicos |
| **LightRAG** (HKUDS/LightRAG) | 37k | Dual-level retrieval (local detalles + global temas) |
| **nano-graphrag** (gusye1234/nano-graphrag) | 3.9k | ~1100 LOC reference implementation |
| **LanceDB** (lancedb/lancedb) | 10.7k | Formato Lance columnar, Rust core, zero-copy |
| **ColBERTv2** (stanford-futuredata/ColBERT) | 3.9k | Late interaction MaxSim para alta precisión |

**Papers:** MemGPT (`2310.08560`), Zep (`2501.13956`), Mem0 (`2504.19413`), GraphRAG (`2404.16130`), LightRAG (`2410.05779`), ColBERT (`2004.12832`), ColBERTv2 (`2112.01488`)

### Patrones top para Tylluan
1. **Multi-signal retrieval** (Mem0) — semantic + BM25 + entity matching en paralelo
2. **Dual-level retrieval** (LightRAG) — local para facts, global para temas
3. **Temporal KG** (Zep) — episodios con timestamps + entity extraction

---

## 3. CPU Inference & Embeddings (Hermes)

| Repo | Stars | Clave para Tylluan |
|------|-------|-------------------|
| **llama.cpp** (ggml-org/llama.cpp) | 72k | GGUF Q4_K_M: 3.8GB/7B, 15-25 tok/s CPU |
| **candle** (huggingface/candle) | 20k | Rust ML framework, ONNX import |
| **mistral.rs** (ericlboyd/mistral.rs) | 6.3k | ISQ (in-situ quantization), 2.79x faster que llama.cpp |
| **BGE-M3** (BAAI/bge-m3) | 5k | Embeddings 1024-dim (Tylluan usa 768 — ⚠️ mismatch) |
| **ONNX Runtime** (microsoft/onnxruntime) | 19k | signed INT8 falla en AVX2, usar UINT8 |
| **burn** (tracel-ai/burn) | 8k | DL framework Rust, backend trait pattern |
| **ColBERT/mxbai-edge** | 3k | mxbai-edge-v0 (17M params) como reranker ligero |

**Papers:** BGE-M3 (`2402.03216`), ColBERT (`2004.12870`), mxbai-edge (`2510.14880`)

### ⚠️ Hallazgo crítico: BGE-M3 dimension mismatch
Tylluan configura `embedding_dim = 768` en SilvaDB. BGE-M3 paper especifica **1024-dim**. Posible truncation silenciosa.

### Patrones top para Tylluan
1. **ISQ** (mistral.rs) — quantize cualquier modelo en load time
2. **BGE-M3 sparse embeddings** — hybrid search completo (dense + sparse + rerank)
3. **mxbai-edge-colbert-v0** (17M) — 6x más pequeño que Jina 280M para reranking

---

## 4. Rust Frameworks & Ecosystem (Claude Code)

| Framework | Stars | Clave para Tylluan |
|-----------|-------|-------------------|
| **Rig** (0xPlaygrounds/rig) | 7.6k | Trait-based LLM abstraction, 20+ providers, 10+ vector stores |
| **rmcp** (modelcontextprotocol/rust-sdk) | 3.5k | Official Rust MCP SDK — reemplazar custom MCP (60% de server.rs) |
| **ADK-Rust** (zavora-ai/adk-rust) | 130k+ downloads | Template/addon system, Managed Agent Runtime |
| **OpenFang** (RightNow-Labs/openfang) | 17.6k | Autonomous "Hands" con schedules, single binary |
| **Swarms-rs** (The-Swarm-Corporation/swarms-rs) | 166 | Supervisor pattern, swarm topologies |
| **Julep** (julep-ai/julep) | 6.6k | Session state y task-as-DAG (⚠️ shutting down) |

## Strategic Insight
Tylluan's **Rust kernel + Python guilds + React dashboard** architecture is **unique** — no other project has this exact split. The 2026 trend toward Rust for agent infrastructure validates the kernel choice. Closest competitors by layer:
- **OpenFang** — Rust-only agent OS, no Python guilds
- **ADK-Rust** — Rust agent framework, no dashboard-first design
- **Rig** — LLM library, not a complete system
- **LangGraph** — Python orchestration, no Rust kernel

**El Python guild bridge es el diferenciador más fuerte de Tylluan.**

---

## 5. Cross-Cutting Patterns

### Convergencia (mismos patrones aparecen en múltiples dominios)
| Patrón | Aparece en | Prioridad |
|--------|-----------|-----------|
| Hybrid search (BM25 + vector + rerank) | Mem0, BGE-M3, LightRAG | 🔴 |
| Temporal/episodic memory | Letta, Zep, Mem0 | 🔴 |
| Memory decay / salience scoring | Mem0, Zep, memory patterns | 🟡 |
| Tool-as-code (CodeAct) | OpenHands, smolmodels | 🟡 |
| Proc-macro definitions | rmcp, Rig | 🟢 |

### Lo que Tylluan ya hace bien
- Hybrid search: ✅ BGE-M3 + BM25 FTS5 + RRF k=60 + Jina Reranker
- Knowledge graph: ✅ SilvaDB (2,218 nodes, 5,372 edges)
- Session management: ✅ Session Bridge
- A2A Federation: ✅ M30 closed
- Zero-downtime: ✅ Proxy architecture

### Lo que Tylluan necesita mejorar
1. **Memory decay** — actualmente no existe, memorias persisten para siempre
2. **BGE-M3 dimension fix** — 768→1024
3. **MCP migration** — custom → rmcp para eliminar 60% de server.rs
4. **Entity linking** — extraer entidades automáticamente al guardar memorias
5. **Dual-level retrieval** — query local (detalles) vs global (temas)

---

## 6. Milestones Propuestos — Tylluan v0.2.0

### M1: Memory Decay & Salience 🔴
- Añadir exponential half-life decay a scores de retrieval en SilvaDB
- Configurable por nodo (memorias importantes no decaen)
- Referencia: Mem0 salience scoring, Zep temporal decay

### M2: Hybrid Search v2 🔴
- Fix BGE-M3 dimension: 768→1024
- Añadir sparse embeddings (BGE-M3 tiene modo sparse)
- Multi-signal retrieval: semantic + BM25 + entity boosting en paralelo
- Referencia: Mem0 multi-signal, BGE-M3 paper

### M3: Guild Auto-Discovery 🟡
- Eliminar catalog.rs + main.rs lazy list
- Escanear `guilds/` al startup, cargar todo `.py` con fastmcp
- Zero-config: drop .py → kernel lo descubre
- Inspiración: este audit reveló que tocar 3 sitios es insostenible

### M4: rmcp Migration 🟡
- Reemplazar custom MCP en server.rs/http.rs con rmcp
- Usar `#[tool]` proc macros para definiciones de guild tools
- Mantener axum para dashboard routes
- Referencia: rmcp v1.7.0, Streamable HTTP

### M5: CodeAct Tools 🟢
- Representar tool calls como scripts Python/bash ejecutables
- Reducir errores de JSON parsing en modelos locales
- Referencia: OpenHands CodeAct (`2402.01030`)

### M6: Dual-Level Retrieval 🟢
- Local mode: entidades específicas + atributos directos
- Global mode: temas macro + cadenas de relaciones
- Referencia: LightRAG dual-level (`2410.05779`)

### M7: Single-Binary Packaging 🟢
- Bundlear dashboard_v3 en el binario (como OpenFang)
- Eliminar dependencia de `npm run dev` separado
- Referencia: OpenFang single 32MB binary

---

## 7. Papers Accumulator (todos los arXiv IDs)

| ID | Título | Fuente | Prioridad |
|----|--------|--------|-----------|
| `2210.03629` | ReAct: Synergizing Reasoning and Acting | Antigravity | 🔴 |
| `2308.08155` | AutoGen: Enabling Next-Gen LLM Apps | Antigravity | 🟡 |
| `2303.17760` | CAMEL: Communicative Agents | Antigravity | 🟡 |
| `2308.00352` | MetaGPT: Meta Programming Framework | Antigravity | 🟢 |
| `2402.01030` | CodeAct: Executable Code Actions | Antigravity | 🔴 |
| `2310.08560` | MemGPT: LLMs as Operating Systems | Qwen3.7 | 🔴 |
| `2501.13956` | Zep: Temporal Knowledge Graph | Qwen3.7 | 🔴 |
| `2504.19413` | Mem0: Production-Ready Long-Term Memory | Qwen3.7 | 🔴 |
| `2404.16130` | GraphRAG: Query-Focused Summarization | Qwen3.7 | 🟡 |
| `2410.05779` | LightRAG: Simple and Fast RAG | Qwen3.7 | 🔴 |
| `2004.12832` | ColBERT: Late Interaction over BERT | Qwen3.7 | 🟢 |
| `2112.01488` | ColBERTv2: Residual Compression | Qwen3.7 | 🟢 |
| `2402.03216` | BGE-M3: Multi-lingual Embedding | Hermes | 🔴 |
| `2004.12870` | ColBERT (original) | Hermes | 🟢 |
| `2510.14880` | mxbai-edge: Efficient Edge Retrieval | Hermes | 🟢 |
