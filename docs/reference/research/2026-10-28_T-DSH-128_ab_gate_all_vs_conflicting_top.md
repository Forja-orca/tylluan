# T-DSH-128 — A/B Recall@5 del CoherenceGate: `all` frente a `conflicting_top`

- Agente: dsh
- Fecha: 2026-10-28/29
- Encargo (T2032): A/B de Recall@5 con las 40 preguntas de T-DSH-95, ajuste `all` (A) frente a `conflicting_top` (B), en DOS kernels de prueba (47091/47092), DATA_DIR propio con copia de `data/silva.pre-restart2-20261028.db`, binario `/e/cargo-target/prod-build/release/tylluan-nexus.exe`, mismas preguntas y orden. Informe, sin tocar codigo.

## Montaje (verificado)

- Dos copias del binario prod-build (34.006.528 B) en `t128\a` y `t128\b`; una copia de la DB `silva.pre-restart2-20261028.db` (203.730.944 B) en cada `data/silva.db`.
- Config identica salvo puerto y scope: `[nexus] dev_mode=true host=127.0.0.1`, `[memory] embedding_model="mxbai-embed-large"`, `[inference] device="cpu" max_threads=8`, `[silva] hybrid_sparse_enabled=true embed_batching_enabled=false decay_enabled=false`, mesh/p2p/gossip/federation off, sentinel off, guilds always_on=[]. A: `coherence_semantic_scope="all"` (47091). B: `"conflicting_top"` (47092).
- Cache fastembed copiada a `t128\.fastembed_cache` (mxbai-embed-large-v1); fingerprint del engine en logs = `sha256:310861041a33...` = el `model_hash` de las filas mxbai de la DB (vector search operativo).
- Verificacion del scope efectivo por comportamiento: A penaliza ~47-50 de 50 candidatos por deriva semantica; B penaliza 0-26 (menos, como corresponde a `conflicting_top`).
- Metodo: `MCP tools/call tylluan_recall` con `limit=10`, 40 preguntas en el mismo orden en A y B; se busca la alternativa esperada en cada bloque de resultado y se anota el rango (1-10; `None` = no aparece en top-10). Sin escrituras (recall solo lectura salvo rejuvenecimiento interno).

## Tabla pregunta por pregunta (40)

| # | pregunta | rango A | rango B | top5 A | top5 B |
|---|---|---|---|---|---|
| 1 | ¿Qué función implementó T-DSH-86 para el guard MCP/SSE? | None | None | no | no |
| 2 | ¿Qué commit integró el guard Origin/Host en push158? | 2 | 2 | si | si |
| 3 | ¿Qué commit corrigió las escrituras REST según T-DSH-87? | None | None | no | no |
| 4 | ¿Qué auditó T-DSH-85 sobre MCP y SSE? | None | None | no | no |
| 5 | ¿Qué buscó el inventario de tests frágiles de T-DSH-88? | None | None | no | no |
| 6 | ¿Qué lock se añadió en T-DSH-89 para el juez? | None | None | no | no |
| 7 | ¿Qué auditó T-DSH-90 sobre dev_mode=false? | None | None | no | no |
| 8 | ¿Qué revisó T-DSH-91 del README público? | 3 | None | si | no |
| 9 | ¿Qué problema de upgrade encontró T-DSH-92 en el README? | None | None | no | no |
| 10 | ¿Cuántos TODO/FIXME salen con palabra completa según T-DSH-93? | 10 | 9 | no | no |
| 11 | ¿Cuántos unwraps de producción hay según T-DSH-93? | 1 | 1 | si | si |
| 12 | ¿Cuántos let _ había en el kernel según T-DSH-93? | None | None | no | no |
| 13 | ¿Qué arregló T-DSH-94 en handler_remember? | 3 | None | si | no |
| 14 | ¿Cómo se llama el contador de fallos de embedding de T-DSH-94? | None | None | no | no |
| 15 | ¿Cuál es el umbral de similitud del DCR? | 4 | 3 | si | si |
| 16 | ¿Qué modelo de embeddings es el default del kernel? | None | None | no | no |
| 17 | ¿Cuál es el perfil de instalación por defecto? | None | None | no | no |
| 18 | ¿Cuántos guilds enruta el kernel? | None | 10 | no | no |
| 19 | ¿Qué guild está marcado como experimental en el errata? | None | None | no | no |
| 20 | ¿Cuántos bytes tiene el archivo Windows de la release? | None | None | no | no |
| 21 | ¿Entre qué segundos arranca en Windows según la medición? | None | None | no | no |
| 22 | ¿Cuántos tests lib tiene el kernel después de T-DSH-94? | None | None | no | no |
| 23 | ¿Cuántos tests tiene security_audit? | 8 | None | no | no |
| 24 | ¿Cuántos candidatos ha visto el CoherenceGate desde el arranque? | None | None | no | no |
| 25 | ¿Cuántos candidatos eliminó el CoherenceGate desde el arranque? | 1 | 1 | si | si |
| 26 | ¿Qué capa produce el mojibake de tylluan_recall? | 5 | 2 | si | si |
| 27 | ¿Cuál es la versión del protocolo MCP stateless? | 1 | 1 | si | si |
| 28 | ¿Qué endpoint expone las estadísticas del CoherenceGate? | 2 | 2 | si | si |
| 29 | ¿Cuál es el commit actual de origin/main? | None | None | no | no |
| 30 | ¿Cuál es el tag v0.17.1? | None | None | no | no |
| 31 | ¿Sobre qué README trabajó Deep en D-61? | 4 | 5 | si | si |
| 32 | ¿Qué defecto de instalación encontró Deep en D-62 sobre macOS? | None | None | no | no |
| 33 | ¿Qué tarea AG-2 tiene Antigravity? | None | None | no | no |
| 34 | ¿Qué mide python scripts/project_balance.py 30? | 5 | 5 | si | si |
| 35 | ¿Qué es H-35? | 6 | None | no | no |
| 36 | ¿Qué es H-36? | 9 | None | no | no |
| 37 | ¿Qué regla nueva impuso el TL en la fase de seguridad? | None | None | no | no |
| 38 | ¿Qué puerto usa el kernel por defecto? | 2 | 2 | si | si |
| 39 | ¿Cómo se llama la lista de rutas públicas del kernel? | None | None | no | no |
| 40 | ¿Qué dice el README sobre auditoría de seguridad externa? | None | None | no | no |

## Resumen

Conjunto completo (n=40):
- Recall@5 **A (`all`) = 0.300** (12/40) · **B (`conflicting_top`) = 0.250** (10/40).
- A acierta y B no: 2 · B acierta y A no: 0 · ambos: 10 · ninguno: 28.
- Rango medio del hecho correcto (solo cuando aparece en top-10): A 4.12 · B 3.58.

Subconjunto con el hecho presente en la DB (n=37; se excluyen tipos que el recall no devuelve por diseno, p.ej. session_digest/routing_anchor, y las 3 preguntas cuyo contenido ya no esta en el snapshot):
- Recall@5 **A = 0.324** · **B = 0.270**.
- A acierta y B no: 2 · B acierta y A no: 0 · ambos: 10 · ninguno: 25.
- Preguntas que EMPEORAN con B: [8, 13] (pierden el top-5). Preguntas que MEJORAN con B: [].
- Rango: B mejora el rango en [10, 15, 26] y lo empeora en [31].

## Comprobacion previa exigida en T2037 (penalizacion distinta)

Antes de medir, un recall en cada kernel y lectura de `/api/v1/security/coherence-gate/stats`:
- A (47091, `all`): **49 de 50 candidatos penalizados** por deriva semantica (1 eliminado por patron).
- B (47092, `conflicting_top`): **25 de 50 candidatos penalizados** (1 eliminado por patron).
- Los contadores difieren como exige T2037 (aqui 49/50 frente a 25/50; en prod 28/30 frente a 13/30). Mismo `top_k` (limit=10, metrica top-5), misma base (copia identica de la DB), mismo build (mismo binario prod-build) en los dos kernels.

## Veredicto (metodologia de no-inferioridad, T2037)

- Delta por pregunta y preguntas que CAMBIAN de top-5 (conjunto completo n=40): A acierta y B no **2**; B acierta y A no **0**; ambos **10**; ninguno **28**. Cambian de top-5: **2 preguntas, ambas a peor con B** (Q8 y Q13).
- Subconjunto con el hecho presente en la DB (n=37): A-only **2**, B-only **0**, ambos **10**, ninguno **25**; mismas 2 preguntas a peor.
- **B no es no-inferior a A**: pierde 2 preguntas y no gana ninguna (0 mejoras). Con un criterio de no-inferioridad estricto (no empeorar ninguna), `conflicting_top` **falla**.
- Recall@5: A 0.300 (12/40) vs B 0.250 (10/40); subconjunto A 0.324 vs B 0.270. La reduccion de penalizaciones (49→25) **no** se traduce en mejor recuperacion.
- Conclusion para el TL: **no adoptar `conflicting_top` como default** con esta evidencia; si se quiere decidir con mas potencia, repetir con mas preguntas sobre hechos presentes y varias pasadas, y con el criterio de no-inferioridad explicito.

## Dato extra para T-DSH-123 (segunda reproduccion del TL)

Anadido a la tabla de T-DSH-123: el TL reprodujo la parada lenta — log "stopped" a las 21:25:30 y el proceso 15960 seguia vivo 2 min despues, con 17 hilos y 19.388 s de CPU acumulada. Encaja con el hallazgo de T-DSH-123: el retardo esta en el desmontaje final del runtime/proceso (10-33 s medidos en test; >2 min en prod con carga real), no en la parada ordenada ni en el reindexer.

## No verificado

- **El snapshot no es la DB viva de T-DSH-95**: muchos hechos esperados ya no estan en `silva.pre-restart2` (28/40 no aparecen ni en top-10; 3 ni siquiera existen ya en nodos no excluidos). El Recall@5 absoluto (0.30) es menor que el 0.475 de T-DSH-95 y no es comparable; el A/B si es valido porque ambos kernels usan la misma DB, binario, config y orden.
- Una sola pasada por pregunta (sin repeticiones ni intervalos de confianza); efectos de orden/cache no medidos.
- Inferencia en CPU y reranker BGE cargado (como prod), pero el snapshot puede tener embeddings de modelos mezclados (mxbai y bge-m3); el vector search solo usa las filas con fingerprint mxbai coincidente.
- No se midio latencia ni coste; solo acierto/rango.
- No se toco codigo, ni `E:\tylluan`, ni el kernel de produccion (:47004). Los dos kernels de prueba se apagan al terminar.
