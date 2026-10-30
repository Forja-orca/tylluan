# T-DSH-139 — Simulación de cobertura de embeddings: A/B con 40 preguntas (G5)

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = 8060a5b` (push202). Sin código: solo informe y test-run.
- Encargo (T2120/G5): simular la cobertura de embeddings con A/B de 40 preguntas **antes de aplicar nada**; sin tocar producción.

## Diseño de la simulación

- Copia de solo lectura de `silva.db` de producción (205 MB, `sqlite3` backup desde `mode=ro`) en dos DATA_DIR propios: **A** (sin tocar) y **B** (misma copia).
- **A (47094)**: cobertura actual → 689/1653 `coloquio_memory` con embedding (42 %).
- **B (47095)**: misma copia + backfill de embeddings de `coloquio_memory` con el reindexer existente (`POST /api/v1/memory/reindex`, `reindex_skip_types` = todos los tipos menos `coloquio_memory`, `reindex_node_delay_ms=0`) → 1621/1653 con embedding (**98 %**); quedaron 32 sin embedding (nodos que el reindexer no pudo resolver en la ventana).
- **A/B**: mismas 40 preguntas de T-DSH-95 (`ITEMS`), mismo `tylluan_recall` MCP, `limit=10`, mismo orden; se compara Recall@5 y rango de la respuesta esperada. Sin escrituras: solo lecturas.
- Producción nunca abierta en escritura; ambos kernels en puertos propios y parados al terminar.

## Resultados (40 preguntas)

| métrica | A (cobertura actual) | B (cobertura simulada 98 %) |
|---|---|---|
| Recall@5 | **0,275** (11/40) | **0,200** (8/40) |
| rango medio (cuando aparece) | **3,15** | **4,79** |
| aciertos solo A | — | — |
| aciertos solo B | — | — |
| ambos | 7 | 7 |
| ninguno | 28 | 28 |
| A acierta y B no (`worse_b`) | — | **4** |
| B acierta y A no (`better_b`) | — | **1** |
| B empeora rango | — | 3 |
| B mejora rango | — | 5 |

Preguntas con diferencia: #11 (A=1, B=None), #16 (A=None, B=6), #23 (A=1, B=8), #27 (A=1, B=8), #28 (A=2, B=6), #32 (A=None, B=10), #34 (A=5, B=3), #35 (A=6, B=4), #36 (A=10, B=9), #38 (A=2, B=1).

## Lectura

- **La simulación NO es no-inferior**: con cobertura de coloquio al 98 % el Recall@5 baja de 0,275 a 0,200 y el rango medio empeora (3,15 → 4,79); 4 preguntas que A acertaba dejan de acertar en B, y solo 1 mejora.
- La causa probable: las 40 preguntas de T-DSH-95 preguntan por **hechos de tareas/commits** (episodios, digests), no por memorias de coloquio; añadir ~900 embeddings de `coloquio_memory` mete candidatos que compiten en el top-5 y desplazan a la respuesta correcta. Es un efecto de **ruido/competición**, no de falta de cobertura.
- **Conclusión para G5**: el backfill de `coloquio_memory` **no se justifica** con este A/B (empeora la recuperación en el set de 40). No aplicar en producción sin un A/B con preguntas dirigidas a memorias de coloquio (donde la cobertura sí debería ayudar) y/o sin una mitigación de ranking.

## Impact

- Dato que faltaba para G5: simular cobertura completa de `coloquio_memory` (42 % → 98 %) **degrada** el Recall@5 del set de 40 preguntas (0,275 → 0,200) y el rango medio. No hay evidencia de no-inferioridad; **no se debe aplicar el backfill** tal cual.
- El reindexer existente sí llena los embeddings (T-DSH-135/137), pero llenarlos no mejora este set: el límite no es la cobertura, es el ranking/competición de candidatos.
- Sin cambios de código; producción intacta.

## No verificado

- Una sola corrida A/B; sin intervalo de confianza (n=40, diferencias de 3 aciertos pueden ser ruido, pero la dirección es negativa y consistente con `worse_b=4` vs `better_b=1`).
- El set de 40 preguntas de T-DSH-95 **no está diseñado para memorias de coloquio**: mide hechos de tareas/commits. Un A/B con preguntas sobre contenido de coloquio es necesario para medir el efecto real de la cobertura; no se hizo.
- B quedó al 98 % (32 nodos sin embedding) y su DATA_DIR pasó por dos arranques del kernel (jobs de re-embed de arranque), así que no es un "solo embeddings" perfectamente aislado; A también arrancó una vez.
- No se probó una mitigación de ranking (p. ej. filtrar `coloquio_memory` por tipo en recall, o bajar su peso) para convertir el backfill en no-inferior.
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); kernels de prueba en 47094/47095 parados al terminar.
