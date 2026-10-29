# T-DSH-135 — Backlog de `coloquio_memory` sin embedding: relleno con el reindexer existente (medición en test-run)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = efc2f40` (push196). Sin código: solo informe y montaje de test-run.
- Encargo (T2094): con el backlog de **964 `coloquio_memory` sin embedding** (H-49), medir en test-run con **copia de solo lectura de `silva.db`** cuántos se rellenarían con el reindexer existente y cuánto tarda por lote. **Sin tocar producción.**

## Montaje

- Copia de `E:\tylluan\data\silva.db` (205 MB) hecha con **`sqlite3` backup desde una conexión `mode=ro`** (2,3 s), a `E:\tylluan-targets\dsh-t7-data\t135\data\silva.db`. Producción solo lectura; el kernel de prueba nunca abrió la BD real.
- Kernel de prueba en **47092**, DATA_DIR propio, config de t128 (`embedding_model = "mxbai-embed-large"`, `device=cpu`, `max_threads=8`, `hybrid_sparse_enabled=true`) y `.fastembed_cache` con el modelo mxbai ya descargado. `[runtime]` no necesario.
- Arranque, espera a que terminen los jobs de re-embed de arranque, y `POST /api/v1/memory/reindex` (el handler calcula la lista stale y lanza un `tokio::spawn` que embebe nodo a nodo con `save_embedding`, con `sleep(200 ms)` por nodo — `api_memory.rs:106-170`).
- Medición: `node_embeddings` y `coloquio_memory` sin embedding consultados en la copia cada 30-60 s.

## Estado de partida (copia)

- `coloquio_memory`: **1653** nodos, **964 sin embedding** (coincide con H-49).
- `POST /memory/reindex` responde `stale=6550-6619`, `total=10800`: la lista stale incluye **todos los tipos** (no solo coloquio) más el backfill sparse (`hybrid_sparse_enabled=true`); los 964 de coloquio **sí están** en ella.
- El orden de la lista no es rowid puro: en las primeras ventanas no se toca ningún coloquio.

## Resultados medidos

| ventana | embeddings nuevos | coloquio rellenados | ritmo |
|---|---|---|---|
| 0-304 s (5 min) | +314 | 0 | ~62/min, ~0,97 s/nodo, lote 10 ≈ 9,7 s |
| 0-665 s (11 min) | +488 | **14** (964 → 950; primer coloquio rellenado a los ~650 s) | ~44/min global, ~1,36 s/nodo, lote 10 ≈ 13,6 s |

- **Sí se rellenan**: el reindexer existente llena `coloquio_memory` (confirmado en vivo: los primeros 14 aparecieron en el minuto siguiente a que le tocara el turno).
- El ritmo medido para el subconjunto de coloquio en su primer minuto de turno fue **~14/min**; a ese ritmo, los 964 tardarían **~69 min**. Al ritmo global medido (~44/min) serían ~22 min de procesamiento efectivo, pero el orden real intercala coloquio con el resto de stale (6550), así que el tiempo de pared es mayor.
- Coste por lote de 10 medido: **9,7-13,6 s** (el `sleep(200 ms)` por nodo más el embedding en CPU dominan).

## Impact

- El reindexer existente **es suficiente** para cerrar el backlog de 964 `coloquio_memory` sin embedding: no hace falta un job nuevo, solo dejarlo correr (o lanzar `POST /memory/reindex`) — confirmado en test-run sobre una copia de la BD real.
- Coste estimado: ~1 s/nodo, ~10-14 s por lote de 10; 964 nodos ≈ 15-70 min según el ritmo efectivo (intercalado con el resto de stale: 6550 nodos en total ≈ 2 h a 44/min).
- El `sleep(200 ms)` por nodo del handler es el principal lastre del ritmo (además del embedding en CPU); si se quiere acelerar, ese sleep es el primer candidato (no se ha tocado: solo informe).

## No verificado

- Una sola ejecución sobre una copia; no se completó el backlog entero (se paró al confirmar que el primer coloquio se rellena), así que el tiempo total de pared para los 964 es una **estimación** a partir del ritmo medido, no una medida completa.
- El ritmo depende de CPU/carga (aquí CPU, 8 hilos, `hybrid_sparse_enabled=true`); no se probó con GPU ni con otro `max_threads`.
- No se probó el reindexer periódico de `main.rs` (600 s) — se usó el endpoint `/memory/reindex`, que ejecuta la misma función de embedding pero con su propio bucle.
- La copia se hizo con la BD de producción viva (backup ro consistente); no se verificó que el WAL de producción estuviera completamente volcado en el momento de la copia (el backup de SQLite lo garantiza por diseño, pero no se contrastó byte a byte).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); el test-run usó su propio puerto y DATA_DIR y se paró al terminar.
