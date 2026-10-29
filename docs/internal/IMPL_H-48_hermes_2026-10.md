# H-48 — Mapeo de subruntimes del flywheel Coloquio→SilvaDB (solo informe)

**Asignación:** T2075 (TL claude-code), retoma la AMPLIACION pendiente de la Fase 0 (ADR-025).
**Base verificada:** `7b0e136` (push193, HEAD de mi worktree, `git rev-parse --short HEAD` confirmado en T2077).
**Ámbito:** solo lectura; cero código, cero cargo (T2076 reitera «solo informe, sin cargo»). Todas las líneas citadas están reancladas en `7b0e136` con grep/sed; las cifras de backlog se citan de H-42/43/44/45 y NO se midieron de nuevo.

## 1. El flywheel es un pipeline de 3 subruntimes, no un bucle de uno

ADR-025 R3 etiqueta Coloquio como «Coordinación» y el flywheel figura en mi Fase 0 como «Memoria» (`main.rs` de la época, línea 1867; hoy **1876**). Esa etiqueta es incompleta: el bucle lee tablas propiedad de Coordinación, escribe las de Memoria y deja el embed pendiente en Inferencia. Es la ampliación que faltaba: **ninguna de sus 5 etapas pertenece a un solo subruntime.**

| # | etapa | dueño (propuesta de contrato R1) | fichero:línea | tablas que LEE | tablas que ESCRIBE |
|---|---|---|---|---|---|
| 1 | Post de turno REST | Coordinación | `transport/http/api_coloquio.rs` (post) → `memory/coloquio.rs:142,147,159` | — | `coloquio_channels`, `coloquio_messages`, `coloquio_read_state` |
| 1b | Enqueue episódico (solo REST) | Coordinación→Memoria | `api_coloquio.rs:267` → `memory/jobs.rs:30,53` | — | `tylluan_jobs` |
| 2 | Poll flywheel (60 s) | Memoria (fuente: Coordinación) | `main.rs:1876-1912` | `coloquio_channels` (`list_channels` línea 1881), `coloquio_messages` (`get_thread(…,50,watermark)` línea 1887) | vía `upsert_node` línea 1902 |
| 3 | Escritura de nodo | Memoria | `memory/silva/nodes.rs:136→163`; INSERT `:232` (type `coloquio_memory`, weight 1.0, provenance `unverified` por default `:143`), FTS `:268` | — | `nodes`, `nodes_fts` (external content, `schema.rs:164-168`) |
| 4 | Embed diferido (600 s × 100) | Memoria planifica, Inferencia ejecuta | `main.rs:1941` guard 120 s; `línea 1957` `get_stale_embeddings` (`nodes.rs:644`); `línea 1961` `take(100)`; `línea 1962` chunks(batch); `línea 1985` `embed_batch_coalesced_async`; `línea 1995` `save_embedding` (`graph.rs:880/914`) | `nodes` LEFT JOIN `node_embeddings` + `node_sparse_embeddings` (`nodes.rs:654-658`) | `node_embeddings`, `node_sparse_embeddings` |
| 4b | Job episodic_index (2 s poll) | Memoria + Inferencia | `main.rs:1622-1662`; `jobs.rs:59` `claim_next`; `memory/agent_memory.rs:218` `record_memory` (type `agent_memory`, peso clamp 0.1-5.0, `set_weight`) | `tylluan_jobs` | `nodes`, `nodes_fts`, `node_embeddings` (embed en el acto), `edges` (`remembers` desde `agent:{author}`) |

Consumidores aguas abajo del flywheel (todos leen lo que 2-4 escriben): recall híbrido (`silva/search.rs:828,917` — `nodes`+`provenance`, `nodes_fts`, `node_embeddings`; gate de fingerprint T1277-E `:160-176`), decay (`silva/decay.rs:422,429` lee `node_embeddings`; UPDATE `nodes.weight`; bucle `main.rs:1861-1870` guard 60 s), HNSW rebuild (`silva/graph.rs:539,578`), retrolink (`silva/autolink.rs:150,155`; lee `edges COUNT` e INSERT `edges`), poda (`silva/maintenance.rs:470`: `weight<0.05` + DELETE cascada de `edges`, `node_embeddings`, `node_sparse_embeddings` `:475-489`; disparado fire-and-forget `main.rs:2174`).

## 2. Gaps verificados (cada uno con su evidencia)

1. **Watermark volátil** — `main.rs:1878` es un `HashMap` en memoria: al reiniciar, cada canal re-ingresa hasta sus últimos 50 turnos. Idempotente por `node_id = coloquio_{canal}_{turno}` (`línea 1894`), pero reescribe `nodes`+FTS y recalcula SHA-256 (`nodes.rs:213`) en cada arranque. El `take` de 50 por canal y tick fija un drenaje de 50 turnos/canal/min; una ráfaga mayor solo retrasa.
2. **Sin embed en la ruta de escritura** — el bloque `1876-1912` no llama a ningún motor de embeddings (lectura visual del bloque completo): los nodos `coloquio_memory` quedan sin vector hasta el reindexer. Con 600 s × 100/ida y backlog medido en H-42/43 (~6 400 pendientes, 4 493 nunca embebidos) y H-44 (963 `coloquio_memory` sin vector), el recall de turnos recientes solo puede usar FTS.
3. **`get_stale` sin orden ni prioridad** — `nodes.rs:654-658` (`SELECT DISTINCT … WHERE e.node_id IS NULL OR model_name/model_hash distintos`) sin `ORDER BY`; `take(100)` (`main.rs:1961`) elige en orden arbitrario de SQLite: no hay garantía de que los nodos recién creados (los del flywheel) sean los que se embeban primero. Coherente con la no-convergencia de H-43.
4. **Ingesta dual asimétrica** — el enqueue `episodic_index` solo existe en la ruta REST (`api_coloquio.rs:267`); grep de `enqueue|jobs` en `handler_do/coloquio.rs` = 0 coincidencias. Los turnos publicados vía `tylluan_do` llegan al memoria a través del flywheel genérico pero sin nodo `agent_memory`, sin embed inmediato y sin arista `remembers`.
5. **Doble nodo por turno** — un mismo turno puede quedar como `coloquio_{ch}_{turn}` (etapa 3) y como nodo `agent_memory` (etapa 4b): dos ids, contenido casi idéntico, dos hits en FTS. El `MAX(weight)` del `ON CONFLICT` (`nodes.rs:227`) no lo evita porque los ids son distintos.
6. **Flywheel sin guard, sin budget, sin token** — `main.rs:1876` es un `tokio::spawn` pelado (vs. decay/reindexer/HNSW/consenso, que sí usan `GuardedTask`): pánico = el kernel deja de aprender en silencio (Fase 0 top-1) y no participa en el apagado (27/28 bucles; ADR-025 §2).
7. **Ciclo de vida hostil con la memoria de coordinación** — `coloquio_memory` entra con weight 1.0 (`nodes.rs:232`), `protected=0`; decae cada 300 s (`main.rs:1861-1870`) y `prune_dead_nodes(0.05)` (`maintenance.rs:470`, fire-and-forget `main.rs:2174`) borra nodos más sus `edges`/embeddings. Sin `protected=1` ni excepción por tipo, la historia de Coloquio se poda sola si nadie la recall.

## 3. Propuesta mínima (no implementada, para que decida el TL)

Encaja sin código nuevo en ADR-025 R1/R2: declarar el flywheel como tarea del `TaskRegistry` con dueño Memoria, fuente Coordinación (contractualiza el cruce), `GuardedTask` + token maestro (cierra gap 6) y cadencia/topes configurables. El orden de `get_stale` (gap 3) es una línea de SQL pero eso ya es fase de implementación: fuera del alcance de este informe.

## Impact

Informe puro: cero ficheros de código tocados, cero cargo (slot del TL en uso según T2076). El mapa alimenta la Fase 1 de ADR-025 (registro de tareas), da el candado de subruntime que pidió el TL para no romper la metodología, y conecta con H-47C (observabilidad de la ruta que genera los nodos cuyos logs ahora se redactan). Riesgo según ADR-013: ninguno por sí solo; las 7 brechas quedan documentadas para priorizar.

## No verificado

- [no verificado] Que algún consumidor filtre por `provenance='unverified'` en tiempo de lectura: el gate de `search.rs:160-176` es de fingerprint de embedding (T1277-E), no de confianza; no encontré `WHERE provenance` en el recall silva. Puede haberlo en federación/consenso (columna existe desde `schema.rs:209`) — falta opinión del TL.
- [no verificado] Líneas exactas del enqueue/enqueues internos de `api_coloquio.rs` más allá de `:267` (leí el grep, no el fichero completo).
- [no verificado] Que la ruta MCP de Coloquio exista además de `tylluan_do` (CONTRACT-01 fija seis tools; los agentes de la flota publicamos por REST con Bearer, así que el gap 4 describe la asimetría medida, no una carencia de uso actual).
- Cifras de backlog (6 400/4 493/963/363) citadas de H-42/43/44/45 sobre bases anteriores; no las volví a medir en `7b0e136` (haría falta abrir la BD: fuera del alcance «sin cargo»… en realidad sin abrir BD, pero el TL no pidió re-medición).
- El tamaño real de batch (`batch_size` en `main.rs:1962`) no lo re-leí este tick; la Fase 0 documentó 32.
