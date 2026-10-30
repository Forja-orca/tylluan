# T-DSH-141 — Servidor verde (G10): RAM, hilos, CPU y activaciones en kernel de prueba

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = 8060a5b` (push202). Sin código: solo informe y test-run.
- Encargo (T2126/G10): en kernel de PRUEBA con copia ro de `silva.db`, medir RAM (working set y privada), hilos, CPU en reposo y con carga de 40 consultas, y activaciones por segundo; desglosar de dónde salen los GB (modelos ONNX cargados, HNSW, caches, SQLite, pools) con números por componente; proponer 3 recortes candidatos con ahorro estimado. Nada se aplica sin A/B de no inferioridad. Sin tocar producción.

## Montaje

- Copia ro de `silva.db` (198 MB, 10.947 nodos, 6.321 embeddings densos, 6.313 sparse) en `t141\data`; kernel de prueba **47096**, config de t128 (`mxbai-embed-large`, `device=cpu`, `max_threads=8`, `hybrid_sparse_enabled=true`, `embed_batching_enabled=false`), `.fastembed_cache` con los modelos.
- Máquina: **56 hilos lógicos**; `Get-Process` (WorkingSet64, PrivateMemorySize64, Threads, HandleCount, CPU acumulado). Reposo medido 120 s después de `/health`; carga = 40 consultas `tylluan_recall` (T-DSH-95) secuenciales por MCP.

## Medición

| métrica | reposo | carga (40 consultas) |
|---|---|---|
| Working set | **4.443 MB** | **9.019 MB** |
| Memoria privada | **5.368 MB** | **10.955 MB** |
| Hilos | 76 | 78 |
| Handles | 433 | — |
| CPU (cores de 56) | 6,69 cores (**11,9 %**) | 7,24 cores (**12,9 %**) |
| Duración | 30 s de muestreo | 965,6 s |
| Latencia por consulta | — | p50 **21,05 s**, p95 **60,0 s**, máx 99,14 s |
| **Activaciones/s** | — | **0,04 consultas/s** (≈1 embedding de query cada 24 s) |

## De dónde salen los GB (números por componente)

| componente | tamaño/uso | nota |
|---|---|---|
| Modelo denso `mxbai-embed-large` (`model.onnx`) | **1.274,9 MB** en disco | cargado en ONNX Runtime (CPU) |
| Modelo sparse `bge-m3` (`model.onnx`) | **2.161,8 MB** en disco | **cargado** porque `hybrid_sparse_enabled=true` |
| Reranker `jina-reranker-v1-turbo-en` | 144,3 MB en disco | en cache; reranker no configurado explícitamente en este TOML |
| Embeddings densos en BD | 6.321 × 1024 × 4 B ≈ **25,9 MB** | raw en `node_embeddings` |
| Embeddings sparse | 6.313 filas | firma sparse, tamaño pequeño frente a modelos |
| Índice HNSW | **0 filas** en esta copia | no construido aún → no es la fuente de GB aquí |
| SQLite `silva.db` | 198,3 MB en disco | page cache por defecto |
| Hilos del proceso | 76 (reposo) / 78 (carga) | tokio + ONNX intra-op (8) + blocking pool |
| Overhead ONNX/allocator/tokenizers | **≈ 2 GB** (privada 5,4 GB − ~3,4 GB de modelos en disco) | arenas de ONNX + tokenizers + allocator |

**Lectura**: los GB son **modelos ONNX cargados** (mxbai 1,27 GB + bge-m3 2,16 GB) más el overhead de sus arenas; los embeddings en BD (26 MB) y el HNSW (vacío en esta copia) no explican el consumo. La memoria se dispara a ~11 GB privados bajo carga (arenas/allocator de ONNX crecen con las activaciones).

## 3 recortes candidatos (ninguno aplicado; requieren A/B de no inferioridad)

1. **Apagar el motor sparse** (`[silva] hybrid_sparse_enabled=false`): evita cargar `bge-m3` (2.161,8 MB en disco) y su arena → **ahorro estimado 1,5-2,5 GB de RAM privada** y parte del CPU de reposo. Riesgo: cambia el ranking híbrido (BM25+sparse); exige A/B de recuperación (40 preguntas) antes de aplicar.
2. **Bajar `[inference] max_threads` de 8 a 4** (y fijar arena de ONNX): menos hilos intra-op y arenas más pequeñas → **ahorro estimado 0,5-1 GB** y CPU más bajo, a costa de latencia de embedding (ya alta: p50 21 s). Exige A/B de latencia/recuperación.
3. **No cargar el reranker y limitar caches** (reranker `jina` 144,3 MB + caches de query/tokenizer; `cache_size`/`mmap_size` de SQLite): **ahorro estimado 200-500 MB**, con riesgo de calidad si el reranker se usa en recall; medir antes.

## Impact

- Dato para G10: en este test-run el kernel usa **4,4 GB WS / 5,4 GB privados en reposo** y **9,0/11,0 GB bajo 40 consultas**, con 76 hilos, 6,7-7,2 cores y **0,04 activaciones/s** (p50 21 s).
- El desglose señala a los **dos modelos ONNX** (mxbai + bge-m3 sparse) como origen principal de los GB, no a la memoria (26 MB) ni al HNSW (vacío). Los tres recortes propuestos atacan modelos/arenas/hilos, con ahorro estimado conjunto de ~2-4 GB.
- Sin cambios aplicados; producción intacta.

## No verificado

- El "reposo" no es reposo puro: a los 120 s de `/health` aún hay jobs de arranque/reindex (CPU 11,9 %); no se midió con el kernel completamente ocioso.
- Una sola corrida; las latencias (p50 21 s) son de este montaje con copia fría y jobs de fondo, no representativas de producción.
- No se midió el consumo aislado de cada modelo (mxbai vs bge-m3) ni el del reranker: el reparto es por tamaño en disco + diferencia privada − modelos, no por instrumentación por sesión ONNX.
- "Activaciones/s" se ha medido como consultas de recall/s (una activación de embedding de query por consulta); no se instrumentó el número de batches/activaciones internas de ONNX.
- No se probaron los recortes ni su A/B de no inferioridad; los ahorros son estimaciones por tamaño de modelo/arena, no medidas.
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); kernel de prueba en 47096 parado al terminar.
