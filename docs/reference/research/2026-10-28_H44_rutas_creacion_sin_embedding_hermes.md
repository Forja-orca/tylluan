# H-44 — Por qué 4 tipos quedan sin embedding al escribirse (hermes)

**Asignación:** T1993 (TL claude-code). **Base:** `ea963b1`. **Tipo:** solo lectura (código, logs, BD vía URI `mode=ro&immutable=1`); cero escrituras. Fecha: 2026-10-28 ~15:4x. Autor: hermes.

## Tabla (formato pedido: tipo -> ruta -> embebe al escribir -> evidencia)

| Tipo (faltantes 15:4x) | Ruta de creación | ¿Embebe al escribir? | Evidencia |
|---|---|---|---|
| `code_entity` (1778) | guild Python: `code_graph.py:258-299` → `silva_utils.add_node` (silva_utils.py:115) → IPC `POST /api/v1/silva/node` + stubs vía `ensure_node_exists` (silva_utils.py:264, INSERT OR IGNORE directo en SQLite) | **NO** — ninguno de los dos caminos | El handler (routes.rs:38 → api_memory.rs:46-49) hace solo `upsert_node_with_validity` (nodes.rs:163): INSERT `nodes`+`nodes_fts`, sin `save_embedding`. El docstring de `add_node` (silva_utils.py:127-131) **promete** "real embedding like any other node it creates" — **no se cumple**: el fix M17-4 redirigió por single-writer/trust (trust.rs:118, config.rs:1622) pero no añadió el embed. `provenance='guild_stub'` solo 1 (el bypass directo casi no deja rastro hoy; el grueso es de la era pre-M17-4 de INSERT crudo que el propio docstring documenta) |
| `coloquio_memory` (962) | flywheel interno: main.rs:1893 `upsert_node(...,"coloquio_memory",...)` cada 60 s | **NO** | main.rs:1865-1895: el bueno llama `upsert_node` y toca `warn!` solo si falla el upsert; ninguna llamada a embed/save_embedding. `upsert_node` → `upsert_node_with_validity` (nodes.rs:136-143), puro nodo+FTS. Los 690 SÍ vectorizados = consumo del Reindexer, no de la ruta de escritura |
| `session_digest` (792) | `agent_memory.rs:531` `upsert_node_with_provenance(...,"session_digest",...)` | **NO** | nodes.rs:148 — el wrapper delega en `upsert_node_with_validity`; sin embed. Nota de diseño (no decisión): `handler_recall.rs:688 y 919` **excluyen** `session_digest` (y `routing_anchor`) de los resultados de recall — hoy por hoy su vector no aparecería en recall; pregunta para TL: ¿deberían embedeándose o deberían excluirse de `get_stale_embeddings`? |
| `episode` (504) | mixta: (a) `tylluan_remember` (SÍ embebe), (b) blackboard drain `mod.rs:321` `upsert_node` (NO; hoy 0 nodos `bb_episode:*`), (c) migración F3 HybridMemory (copia dense byte-idéntico solo si el legacy lo tenía; 0 líneas en el log actual — ocurrió antes del rolado del 24-oct), (d) herencias de eras anteriores | **(a) SÍ; (b)(c) NO** | (a): handler_remember.rs:67 `persist_memory_embedding` (T-DSH-94) con contador + WARN en fallo. El contador **ya está en el binario en producción** (`git merge-base --is-ancestor` T-DSH-94 ⊂ `a564880` = YES) y ese WARN aparece **0 veces** en `kernel.log` → los fallos de remember en este arranque son cero; los 504 no vienen de remember fallando hoy |

## Medición pedida: cuántos 'embed' fallan o se saltan (kernel.log actual, desde 2026-10-24 16:22)

```
WARN "Reembed save failed" (anchors, FOREIGN KEY — nodo borrado en carrera): 336
WARN "Re-index save failed"  (reindexer, FOREIGN KEY — misma carrera):        77
"Batch re-index error" / "inference failed" / "embed batcher":                 0
WARN "tylluan_remember: el embedding no se pudo persistir" (T-DSH-94):        0
ticks "Agnostic Indexer: Found":                                             538  (come cola: code_entity 1851→1778 desde el snapshot H-43 de las 13:5x)
```

Los **saltos por presupuesto** (`budget_reindex.acquire()` → `continue`) son **silenciosos por diseño**: no dejan línea en log, no medibles desde fuera [no verificado]. Las 413 FK failures (336+77) no pierden embeds reales: son nodos borrados entre selección y guardado.

## Síntesis causal (por tipo)

1. **`code_entity` y `coloquio_memory`: la ruta de escritura nunca llamó a embed** — hueco estructural, no fallo: el flywheel y el handler REST solo insertan nodo+FTS. En `code_entity` se suma el caso histórico documentado (INSERT crudo pre-M17-4) y el stub de `ensure_node_exists` (bypass deliberado, hoy 1 fila).
2. **`session_digest`: igual de estructural**, con la paradoja de que recall los excluye de todos modos.
3. **`episode`: la vía viva (`tylluan_remember`) SÍ embebe y no está fallando** (contador activo, 0 hits); los 504 son residuos de vías secundarias/eras previas.

## Impact

Producción intacta: todo lo medido es lectura (código en `ea963b1`, `kernel.log`, URI readonly). No se propuso ni aplicó arreglo; las dos decisiones de diseño que abre esta evidencia para TL/Jose: (i) llamar a embed (o encolar embed) en `silva_create_node_handler` + flywheel, o (ii) filtrar tipos en `get_stale_embeddings` para no gastar ONNX en nodos que recall excluye (`session_digest`, `routing_anchor`) — ambas son una línea cada una, semánticas opuestas, elegir una es decisión de producto. El docstring de `silva_utils.add_node` miente hoy (promete embedding que el handler no hace): merece corrección documental sea cual sea la decisión.

## No verificado / límites

- [no verificado] Origen exacto por fila de los 504 `episode` (requiere correlacionar `provenance`/`federation_source`/metadata con cada vía — muestreados 5 ids: UUID de era remember y `episode__<hex>` (1 solo); el patrón `episode__*` no aparece en el código actual de crates/ ni guilds/ — era previa).
- [no verificado] Cuántos de los 1778 `code_entity` son pre-M17-4 vs post (el IPC no embedea igual, así que no se distingue por provenance — `guild_stub` solo marca los stubs).
- [no verificado] F3 migración: sin huella en el log actual (0 hits `HybridMemory` desde el 24-oct); su contribución a los 504 no acotable hoy.
- Conteos de la BD son snapshot 15:4x con kernel escribiendo y reindexer comiendo; la estructura causal es estable, los números se mueven.
