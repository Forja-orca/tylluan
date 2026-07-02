# Vision + Agentic Skills — Investigación 2026

Investigación realizada por Qwen Desktop · Auditada por Claude Code · 2026-07-02  
**Política:** arXiv IDs verificables o repos con URL directa. Claims numéricos marcados con ⚠️ donde no hay cita primaria.

---

## Vision Models para Edge

### SmolVLM-256M / SmolVLM-2B
- **arXiv:** [2504.05299](https://arxiv.org/abs/2504.05299) ✅ (Abril 2025)
- **Repo:** [HuggingFace/SmolVLM](https://huggingface.co/HuggingFace/SmolVLM) ✅
- **Relevancia para Tylluan:** <1GB RAM, supera a Idefics-80B en benchmarks de comprensión. Alineado con el caso de uso offline-first / RPi4.
- **Key finding:** La variante 256M corre en hardware con 512MB RAM libre — compatible con el perfil `portable`.
- **Esfuerzo de integración:** Medio — requiere guild nuevo `vision_smolvlm.py` con FastMCP, modelo descargable offline.

### Moondream
- **Repo:** `vikhyatk/moondream` (verificar URL — reporte citó `m87-labs/moondream`, owner podría ser distinto) ⚠️
- **Parámetros:** 0.5B — diseñado explícitamente para "run anywhere"
- **Licencia:** Apache-2.0
- **Relevancia:** Candidato directo para spike como `vision_moondream` guild en v0.11.0. Menor footprint que SmolVLM-2B.
- **Acción:** Verificar URL exacta del repo antes de referenciar en docs públicos.

---

## Agent Memory

### Mem0
- **Repo:** [github.com/mem0ai/mem0](https://github.com/mem0ai/mem0) ✅ (>50k stars)
- **Innovaciones 2026:** Entity linking, multi-signal retrieval (semántico + BM25 + entity), temporal reasoning.
- **Benchmarks reportados:** LoCoMo 91.6, LongMemEval 94.8 ⚠️ — números específicos citados por Qwen sin enlace a paper/release. Verificar contra `mem0ai/mem0` releases antes de usar en comparativas.
- **Relevancia para Tylluan:** El patrón de entity linking es directamente aplicable a `search_hybrid()`. Multi-signal retrieval es lo que ya implementa v0.9.0 (RRF + BM25 + vector). La diferencia está en el temporal reasoning — Mem0 indexa por timestamp de forma explícita.
- **Alineación:** Su arquitectura de 3-capas (working / episodic / semantic) mapea a SilvaDB node types.

---

## Agentic RAG

### Agentic RAG Survey
- **arXiv:** [2501.09136](https://arxiv.org/abs/2501.09136) ✅ (Enero 2025, revisado Abril 2026)
- **Taxonomía de capabilities agénticas:**
  - **Reflection** — el agente evalúa su propio output antes de responder
  - **Planning** — descompone tareas en sub-pasos antes de ejecutar
  - **Tool use** — MCP / function calling ✅ (Tylluan ya lo tiene vía guilds)
  - **Multi-agent** — coordinación entre agentes ✅ (flota Deep+Padawan+Qwen)
- **Gap en Tylluan:** Reflection y Planning no están implementados explícitamente. Están implícitos en `forja_think` pero sin estructura formal.
- **Recomendación:** Reflection Pattern como guild wrapper en v0.12.0.

---

## Benchmarks de Tool Use

### BFCL — Berkeley Function Calling Leaderboard
- **Referencia:** ICML 2025 ✅
- **Qué evalúa:** serial, parallel, multi-turn function calling contra una suite de APIs reales.
- **Por qué importa:** Estándar de facto para comparar MCP tool use. Si Tylluan quiere citar rendimiento de `tylluan_do`, BFCL es el benchmark que el ecosistema reconoce.
- **Esfuerzo:** Bajo — subset de BFCL puede implementarse en `tylluan-evals` sin dependencias cloud. Los test cases son JSON estáticos.

---

## Recomendaciones por versión

### v0.11.0 (inmediato)
1. **Spike Moondream / SmolVLM-256M** como guild de visión — verificar repo correcto, instalar offline, test con imagen.
2. **BFCL subset en `tylluan-evals`** — añadir 10-20 casos de function calling para baseline.

### v0.12.0
3. **Reflection Pattern** para guilds — wrapper que evalúa el output antes de devolverlo al cliente.
4. **Entity linking en SilvaDB** — inspirado en Mem0, extraer entidades nombradas y enlazarlas como nodos `entity` en el grafo.

### v0.13.0+
5. **Temporal reasoning** — queries sobre rangos de tiempo ("episodios de esta semana").
6. **Multi-signal retrieval v2** — añadir señal de entity co-occurrence al RRF.

---

## ⚠️ Claims sin verificar — NO citar sin confirmar

| Claim | Problema |
|-------|---------|
| Moondream repo `m87-labs/moondream` | Owner probable incorrecto — verificar en browser |
| Mem0 LoCoMo 91.6 / LongMemEval 94.8 | Sin enlace a release note o paper; verificar antes de usar en comparativas |
| SmolVLM <1GB claim | Verificar variante exacta (256M vs 2B tienen footprints distintos) |

---

## Preguntas abiertas para el equipo

1. **Deep / Padawan:** ¿Moondream o SmolVLM-256M para el spike de v0.11.0? Moondream = menor footprint, SmolVLM = paper verificado.
2. **Antigravity:** Para el dashboard, ¿cómo exponer el output de vision guild? Imagen + texto en panel lateral o inline en el grafo.
3. **Jose:** ¿Priorizamos el spike de vision o el BFCL baseline para v0.11.0? Son independientes, ambos son semanas de trabajo.
