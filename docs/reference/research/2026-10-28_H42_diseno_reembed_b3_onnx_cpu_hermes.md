# H-42 — DISEÑO del re-embed B3 (1878 nodos legacy) en ONNX CPU — para decisión de Jose

**Asignación:** T1979 (TL claude-code). **Base:** `3e76953`. **Tipo:** DISEÑO puro — sin ejecutar el re-embed, sin tocar producción (solo lectura de código/logs/config medida hoy). Fecha: 2026-10-28. Autor: hermes.

## Hallazgo que reencuadra la tarea (medido hoy, antes de diseñar nada)

1. **El re-embed ya existe en el kernel y está corriendo en producción.** El `Agnostic Reindexer` (main.rs:1905-2000) selecciona nodos con `model_name != engine_id()` (nodes.rs:644-666, incluye rows con `model_hash` del motor anterior y nodos sin embedding) y los re-embeddea en background. Su huella en `kernel.log` (fichero actual: arranca 2026-10-24 16:22): **538 ticks**, samples de hoy `Found 6464 → 6364 → 6566 stale` (11:45/11:55/12:05).
2. **NO converge.** El tope es `take(100)` nodos por tick de 600 s (main.rs:1952) ≈ 100/10min. El flujo de entradas (writes nuevos de los agentes + filas mxbai antiguas con `model_hash` viejo) iguala ese tope → meseta ~6.400 stale. El total real de trabajo NO son 1878: son ~**6.400** (1878 legacy de nombre de modelo + ~4.184 filas mxbai escritas por binarios previos con hash antiguo + nodos sin embedding). **Con el ritmo actual B3 no termina nunca**; a ritmo cero-entradas serían 64 ticks ≈ 10,7 h.
3. **Ruido en el conteo:** 77× `Re-index save failed: FOREIGN KEY constraint failed` (acumulado del log desde el 24-oct) — nodos borrados entre selección y guardado (carrera inofensiva que sesga las cifras).

## Piezas que ya existen (nada que escribir para el lote básico)

| Pieza | Hecho verificado |
|---|---|
| Lotes | batch 32 textos/tick, 500 ms de descanso entre chunks (main.rs:1951, 1990) |
| Límite de hilos | `[inference] max_threads` → pool global ONNX `intra=N, inter=1` + ORT/OMP/RAYON (embeddings.rs:803, 855-875). **Hoy no está fijado en tylluan.toml de producción** (solo `threads=0` del llama-server :160 y `threads=4` del sentinel :340) → ORT dimensiona solo sobre 56 CPUs lógicas |
| Reanudable | por diseño: la selección es `model_name != engine_id()`; lo ya re-embedeado deja de ser stale. Los `save_embedding` son idempotentes (`ON CONFLICT(node_id) DO UPDATE`, nodes.rs:469-476) |
| Rollback | **no existe dentro de la BD**: PK de `node_embeddings` es `node_id` a una fila (schema.rs:449) → el re-embed SOBRESCRIBE el vector viejo. Rollback = copia previa de la BD |
| Pacing de CPU | `BackgroundBudget` (semáforo + skip de tick si está agotado) + `GuardedTask` de 120 s por ronda (main.rs:1932) |
| Coalescing con recall | `embed_batching_enabled` — producción lo tiene **false** (toml:292, tras la medición de Jose del 09-19) |
| Modelo | fastembed, cache `E:/tylluan/.fastembed_cache/models--mixedbread-ai--mxbai-embed-large-v1` (1,3 GB en disco). HW medido: 56 CPUs lógicas, 221,9 GB RAM |

## Medición de tiempo estimado (framing; marcado no verificado)

- Ancla real publicada: batch ONNX de 32 textos en CPU = **2-8 s** (comentario medido 2026-09-07 en el propio reindexer, main.rs:1963-1968).
- Trabajo puro 1878 legacy: 1878/32 ≈ 59 batches ≈ **2,5-8 min de ONNX**. Los ~6400 stale totales: ≈ **9-27 min de ONNX**. El coste no está en la velocidad (56 cores) sino en el tope 100/tick y en la contención con recall.
- Calibración antes de decidir (solo lectura): contar `Found N stale` por tick con el patrón gawk de H-41 y dividir el delta de N entre ticks.

## Opciones para Jose

**A — Dejar correr al reindexer (cero código) + techo de hilos.** Reiniciar producción post-`e123c37` (ya obligatorio para B1) y fijar `[inference] max_threads = 8` (de 56) para cotas de impacto. Monitoring con los comandos de H-41. Verdad incómoda: con B1 en el gate, las filas legacy ya no penalizan cross-model — B3 pasa a ser higiene de espacio vectorial, no urgencia. **Converger no está garantizado** (punto 2). Coste: un valor de config + esperar.

**B — PR mínimo para que converja (recomendada si Jose quiere B3 cerrada).** Hacer configurable lo hoy hardcodeado: `[memory] reindex_nodes_per_tick` (hoy 100) y `reindex_tick_secs` (hoy 600). Ej. 500/tick cada 120 s ≈ 6400 stale en ~13 ticks ≈ **30 min reales**, respetando budget/guard/hilos. Tamaño ~15-20 líneas (config.rs + main.rs), `clippy -D warnings`, `## Impact`. Riesgo: más ONNX de fondo concurrente con recall — mitigado por budget+guard+max_threads; activar coalescing a la vez sería un experimento aparte (historial toml:288-292).

**C — Script offline con el kernel parado** (fallback solo si no se puede reiniciar): fastembed sobre copia de silva.db y swap del fichero. **No recomendada**: el texto que re-embedea el kernel es `build_contextual_text(metadata, content)` (Rust) y hay sidecar sparse + reconstrucción HNSW en arranque; replicar el pipeline en Python introduce desalineación semántica y writes que el kernel no vio. Coste: kernel parado 15-30 min.

**Anti-opciones (probadas y documentadas en el repo):** (i) renombrar `model_name` a mano → es el error histórico de `distill-v1`: deja las filas permanentemente stale y re-embedea en bucle cada 10 min (comentario embedding.rs:84-94, el bucle 98/196 del 2026-09-28); (ii) tabla nueva de rollback vectorial → innecesaria si hay copia de BD.

## Rollback obligatorio antes de ejecutar cualquier B3

PK de fila única ⇒ sin copia no hay marcha atrás. Procedimiento: `VACUUM INTO 'E:/tylluan/data/silva.pre-b3.db'` desde una conexión del kernel o CLI (backup online consistente, escribe solo el fichero nuevo; la BD vivo no se toca). Restauración = parar kernel, sustituir fichero. Nota: el patrón `silva.db.bak-*` existe desde el 25-oct (copias de fichero de 200 MB) — copiar a mano con kernel en marcha arriesga WAL inconsistente; preferir `VACUUM INTO`. [no verificado: no ejecuté VACUUM — es el paso 1 cuando B3 se apruebe.]

## Impact

Ninguna escritura ni proceso ejecutado para este diseño: lecturas de código en `3e76953`, `kernel.log`, `tylluan.toml` y consulta readonly de la BD (heredada de H-41). Si Jose elige A: un valor de config en el reinicio. Si B: PR ~20 líneas sin cambios de arquitectura. Si C: ventana de kernel parado. La cifra «1878» de la spec queda corregida: el universo stale real medido hoy es ~6.400.

## No verificado / límites

- [no verificado] Desglose exacto de los 6400 stale (legacy-name vs hash-viejo vs sin-embedding): requeriría el join de `get_stale_embeddings` ejecutado contra la BD; se puede anclar con la consulta readonly de H-41 ampliada — lo dejo propuesto, no ejecutado, por "sin ejecutar nada".
- [no verificado] 2-8 s/batch es la medición del 09-07 en otro binario; recalibrar con el patrón gawk antes de cronometrar B.
- [no verificado] Que `VACUUM INTO` funcione con esta BD SQLCipher/estándar concreta (features de compilación): probarlo en seco antes de aprobar.
- Conteos de logs son del día de hoy sobre `kernel.log` y rollados citados; la meseta puede moverse con la actividad de los agentes.
