# T-DSH-137 — Reindex con `reindex_node_delay_ms = 0`: nodos/min y uso de CPU

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = 4f0cb3a` (push199). Sin código: solo informe y montaje de test-run.
- Encargo (T2103): con `[memory] reindex_node_delay_ms = 0` en test-run (copia ro de `silva.db`), repetir la medición de T-DSH-135 y dar **nodos/min** y **uso de CPU**. Sin tocar producción.

## Montaje

- Copia de solo lectura de `E:\tylluan\data\silva.db` (206 MB) con `sqlite3` backup desde conexión `mode=ro` (2,4 s) → `t137\data\silva.db`. Producción solo lectura.
- Kernel de prueba en **47093**, DATA_DIR propio, config de t128 (`mxbai-embed-large`, cpu, 8 hilos, `hybrid_sparse_enabled=true`) con **`[memory] reindex_node_delay_ms = 0`** y `.fastembed_cache` con el modelo.
- Arranque, 120 s para que terminen los jobs de re-embed de arranque, `POST /api/v1/memory/reindex`, muestreo cada 15 s de `node_embeddings`, `coloquio_memory` sin embedding y **CPU del proceso** (`Get-Process.CPU` acumulado → cores usados y % sobre los 56 hilos de la máquina).
- Máquina: 56 hilos lógicos; `max_threads = 8` en la config de inferencia.

## Resultados

| métrica | valor |
|---|---|
| embeddings nuevos | **355** (6128 → 6483) |
| tiempo de la ventana | 629,4 s |
| **nodos/min** | **33,8** (≈1,77 s/nodo) |
| **CPU media del proceso** | **7,52 cores** = **13,4 %** de 56 hilos |
| `coloquio_memory` sin embedding | 964 → 964 (no le tocó el turno en la ventana) |
| stale declarado por el handler | 6580 (total 10840) |

Detalle: el ritmo se mantuvo ~10 embeddings/15 s (≈40/min) hasta ~t=450 s y luego cayó a ~2-6/15 s, con meseta al final (6483 desde ~590 s) — el bucle no llegó a agotar los 6580 stale en la ventana medida.

### Comparación con T-DSH-135 (mismo montaje, `reindex_node_delay_ms = 200`)

| | delay = 200 ms | delay = 0 |
|---|---|---|
| ventana | 665 s | 629 s |
| embeddings nuevos | 488 | 355 |
| nodos/min | ~44 | **33,8** |
| s/nodo | ~1,36 | ~1,77 |
| CPU | no medida | 7,52 cores (13,4 %) |

## Lectura

- **Quitar el `sleep(200 ms)` por nodo no acelera el reindex**: el cuello de botella es el **embedding en CPU** (~1,3-1,8 s/nodo con mxbai y 8 hilos), no la pausa. En esta corrida incluso salió más lento que con 200 ms (33,8 vs ~44/min), diferencia que cabe en la varianza entre corridas/copias y en la meseta final observada, pero desde luego **no hay mejora**.
- El proceso usa **~7,5-8 cores (13-14 % de 56)**: el reindex está limitado por `max_threads=8` de la inferencia, no por CPU disponible de la máquina.
- Conclusión práctica: el `sleep` es irrelevante para el ritmo; si se quiere acelerar el backfill hay que subir `max_threads`/usar GPU o paralelizar el embedding, no bajar la pausa. Para el objetivo de T-DSH-135 (cerrar 964 nodos), `delay=0` no aporta.

## Impact

- Dato pedido: **~34 nodos/min y ~7,5 cores (13,4 % de 56)** con `reindex_node_delay_ms=0`; frente a ~44/min con el default 200 ms. **No hay ganancia** al quitar la pausa; el límite es el embedding (CPU, 8 hilos).
- Justifica mantener el default 200 ms (no degrada el ritmo y evita ráfagas) y descarta el `delay=0` como palanca de aceleración del backfill.

## No verificado

- Una sola corrida por configuración; la diferencia 33,8 vs ~44/min puede ser varianza (copias de BD de momentos distintos, meseta final) — no se repitió para dar intervalo de confianza.
- El bucle no completó los 6580 stale en la ventana: el ritmo medido es de la ventana, no del backlog entero; la meseta final (~t=590 s) no se investigó (¿fin de lista, contención, DB?) y puede infravalorar la media.
- CPU medida con `Get-Process.CPU` (acumulado del proceso) cada 15 s; no se desglosa por hilo ni incluye otros procesos del sistema.
- No se probó con GPU, ni con `max_threads` distinto de 8, ni con `reindex_node_delay_ms` intermedio.
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); el test-run usó su propio puerto y DATA_DIR y se paró al terminar.
