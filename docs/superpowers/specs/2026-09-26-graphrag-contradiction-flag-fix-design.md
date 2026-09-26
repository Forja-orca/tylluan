# GraphRAG Contradiction False-Positive & Legacy Nesting Cleanup — Design

**Fecha:** 2026-09-26
**Autor:** Claude Code (tech lead), aprobado por José
**Contexto:** Investigación en vivo tras reporte de José de sistema completo lento (Coloquio no respondía). Diagnóstico confirmó `tylluan-nexus.exe` (producción, PID 44364) con ~2000% CPU sostenido y 323 hilos — misma firma del incidente de 2026-08-30 (GraphRAG nesting, supuestamente cerrado en `7648a7a`).

## Causa raíz (confirmada leyendo código, no supuesta)

`flag_contradiction_nodes()` (`crates/tylluan-kernel/src/memory/silva/nodes.rs:750-814`) marca `conflicted=1` en cualquier nodo *source* que tenga más de un *target* distinto para el mismo tipo de arista, salvo `related_to` (única exclusión existente). No excluye `member_of` — la arista que `GraphRagManager::save_summary()` usa para vincular un nodo resumen de clúster con **todos sus miembros** (fan-out intencional, `crates/tylluan-kernel/src/memory/graph_rag.rs:174`). Como esas aristas se crean sin `valid_from`, caen en la rama "todo NULL → sin señal temporal → marcar conflictado" (línea 787).

Consecuencia: **todo nodo `graphrag_summary:*` se marca `conflicted=1` en cada pasada de `flag_contradiction_nodes()`**. `ConsensusEngine::resolve_conflicts()` (`crates/tylluan-kernel/src/consensus.rs:22-52`) los recoge, no encuentra nodo similar, y los marca `conflicted=false` de vuelta (línea 46) — pero la siguiente pasada de `flag_contradiction_nodes()` los vuelve a marcar `conflicted=1` (solo toca filas con `conflicted=0`, línea 787/801). Resultado: **bucle perpetuo flag→resuelve→flag**, para cada uno de los miles de nodos GraphRAG existentes, en cada ciclo, para siempre.

Este bug lleva activo desde al menos el **5 de julio de 2026** (primera línea de nesting encontrada en `logs/kernel.log`), muy anterior al "fix" de agosto — ese fix (`7648a7a`) cerró correctamente la vía de creación de nesting *nuevo* (guard en `save_summary()`, con test de regresión), pero nunca tocó los miles de nodos ya corruptos ni identificó esta causa de fondo del reprocesamiento perpetuo.

## Auditoría de otras aristas de fan-out (para no repetir el mismo bug con otro tipo)

Revisadas todas las aristas de producción con riesgo de fan-out real:

| Arista | Dirección | ¿Un source, muchos targets? | ¿Necesita exclusión? |
|---|---|---|---|
| `member_of` (graph_rag.rs:174) | resumen → miembros | Sí — exactamente el patrón del bug | **Sí** |
| `consolidated_into` (dream_cycle.rs:261) | original → resumen | No — cada original tiene 1 target | No |
| `contributed_to` (memory/consensus.rs:267) | contribuyente → síntesis | No — cada contribuyente tiene 1 target | No |

Solo `member_of` necesita exclusión.

## Parte 1 — Fix del bug

**Archivo:** `crates/tylluan-kernel/src/memory/silva/nodes.rs`

Cambiar la cláusula `WHERE` de la consulta de aristas (línea ~757):
```rust
"SELECT source, type, target, valid_from FROM edges
 WHERE type != 'related_to' AND type != 'member_of'
 ORDER BY source, type, valid_from ASC NULLS LAST"
```
Con un comentario documentando por qué (`member_of` es fan-out intencional de GraphRAG, no una contradicción factual — un resumen de clúster legítimamente tiene N miembros).

**Test de regresión** (mismo archivo, módulo de tests): crear un nodo con 5+ aristas `member_of` a targets distintos (todas sin `valid_from`), correr `flag_contradiction_nodes()`, verificar que el nodo sigue con `conflicted=0`. Nombrar el test explicando el bug real que previene, siguiendo el patrón ya usado en `graph_rag.rs` para el guard de `save_summary()`.

## Parte 2 — Limpieza de nodos legacy corruptos

**Identificación:** cualquier fila cuyo `id` empiece por `graphrag_summary:` y contenga la subcadena `graphrag_summary:` una segunda vez:
```sql
id LIKE 'graphrag_summary:%graphrag_summary:%'
```
Este patrón captura cualquier nivel de anidamiento real (2+) sin falsos positivos sobre resúmenes válidos de un solo nivel (`graphrag_summary:cluster:<hub_id>`, que contiene la subcadena una sola vez).

**Borrado en cascada** (orden por integridad referencial):
1. `DELETE FROM edges WHERE source LIKE '...' OR target LIKE '...'`
2. `DELETE FROM cluster_summaries WHERE cluster_id LIKE '%graphrag_summary:%'`
3. `DELETE FROM nodes WHERE id LIKE 'graphrag_summary:%graphrag_summary:%'`

**Dónde vive:** como paso de arranque en `crates/tylluan-kernel/src/memory/silva/schema.rs` (mismo patrón que otras migraciones de esquema del proyecto), ejecutado tras `init_schema()`. Naturalmente idempotente: la condición de búsqueda deja de encontrar filas tras la primera limpieza, así que no necesita flag de versión — corre gratis (`SELECT COUNT(*)` primero; si es 0, no ejecuta los `DELETE`) en cada arranque futuro.

**Logging:** loggear el conteo de nodos/edges/cluster_summaries borrados al arrancar, para que quede constancia en `kernel.log` de que la limpieza ocurrió y cuántas filas afectó.

## Testing

- Unit test para el guard de exclusión de `member_of` en `flag_contradiction_nodes()` (Parte 1).
- Unit test para la limpieza: poblar una DB en memoria con nodos anidados de prueba + un resumen válido de un solo nivel, correr el paso de limpieza, verificar que el anidado desaparece y el válido permanece intacto (incluyendo sus edges `member_of` y su fila en `cluster_summaries`).
- Verificación manual post-fix (José, tras reconstruir y reiniciar): `grep -c "graphrag_summary:cluster:graphrag_summary" logs/kernel.log` tras el reinicio no debe crecer; `SELECT COUNT(*) FROM nodes WHERE conflicted=1` debe estabilizarse en un número bajo, no oscilar en cada ciclo.

## Adenda (2026-09-26, tras verificación cruzada de la flota)

Durante la revisión cruzada del equipo (Deep + Antigravity, ambos verificados independientemente por el tech lead antes de aceptar sus hallazgos) surgieron 3 hallazgos adicionales confirmados en código real, que amplían el alcance:

### Parte 0.5 — `remembers` es la misma clase de bug que `member_of`

`crates/tylluan-kernel/src/main.rs:1356` y `handler_remember.rs:289` crean aristas `agent_node_id -> memory_id` de tipo `remembers` cada vez que un agente recuerda algo. Cada nodo de identidad de agente (`agent_memory:claude-code`, `agent_memory:deep`, etc.) acumula muchas de estas aristas a targets distintos → mismo falso positivo que `member_of`. **Añadir `remembers` a la misma exclusión de la Parte 1**:
```rust
WHERE type != 'related_to' AND type != 'member_of' AND type != 'remembers'
```
`contains`, `documents`, `mentions`, `calls` (reportados por Antigravity) se verificaron y **no existen** como tipos de arista reales en el código — no se incluyen.

### Parte 3 — Tope de paralelismo de NightConsolidation

`crates/tylluan-kernel/src/memory/night/mod.rs:86-90`: el semáforo que gobierna cuántas fases de NightConsolidation corren en paralelo se dimensiona con `std::thread::available_parallelism()` — sin tope configurable, ocupa todos los núcleos de la máquina en cada corrida (cada 30 min, `main.rs:1888`). Esto explica la magnitud real de los picos de ~2000% CPU (episódicos, no constantes) — mucho más que el bucle perpetuo por sí solo.

Añadir `[night] max_parallel_phases` (config, default sensato — no todos los núcleos) cableado a ese semáforo, y `[night] consolidation_interval_secs` si se decide alargar la cadencia de 30 min. Test: verificar que el semáforo nunca excede el valor configurado.

### Parte 4 — descartada tras verificación adicional

El "Caso C" de ambigüedad (`memory/consensus.rs:118-124`) **no es un bug**: `test_consolidate_ambiguous_scores_mark_both_without_resolving` (línea 438-454) fija como contrato deliberado que los nodos genuinamente ambiguos permanezcan `conflicted=1` — es una cola de revisión humana a propósito. Tocarlo rompería ese contrato. Las Partes 0/0.5 ya resuelven el síntoma real: una vez `member_of`/`remembers` dejan de marcarse conflictados, esos nodos nunca entran en `get_conflicted_embeddings()` / `get_semantic_conflicted_groups()`, así que la cola vuelve a contener solo contradicciones genuinas y raras — el tamaño N que alimenta el coste O(N²) se desploma sin tocar este código.

El bug idéntico en la rama *merge* de `src/consensus.rs::resolve_conflicts()` (línea 43-44, nunca limpia `conflicted`) se verificó real pero actualmente inalcanzable en producción — `find_similar()` busca un embedding con id `"query:{content}"` que nunca existe, así que esa rama nunca se ejecuta. Se deja anotado como deuda pero **fuera de alcance de este plan** salvo que se decida arreglar `find_similar` también.

## Fuera de alcance (explícitamente)

- La investigación de alternativas a ONNX / modelos de embeddings más ligeros (pedida por José, en cola, no parte de este fix).
- El spike de ONNX raw session para el reranker (P1) — pendiente de la decisión de investigación anterior.
- El bug de `find_similar()` en `src/consensus.rs` (embedding con id inexistente) — real pero inalcanzable actualmente, no bloquea nada.
- Reescribir el algoritmo O(N²) de `get_semantic_conflicted_groups()` — las Partes 0/0.5 deberían reducir N lo bastante para que el coste actual sea aceptable; revisar solo si tras el fix sigue siendo un problema medido.
- Limpieza menor de procesos `node.exe` duplicados (`@googlemaps/code-assist-mcp`) y rotación de `kernel.log` (995MB sin rotar desde junio) — housekeeping de infraestructura, no código del kernel.
