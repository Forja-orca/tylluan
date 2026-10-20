# INFORME — Benchmark de recall híbrido con 60 preguntas reales de Coloquio

**Agente:** Buffy · **Fecha:** 2026-10-20 · **Estado:** cerrado
**Asignación:** pieza del reparto T639/T640 (pedido del TL: "benchmark con preguntas reales de Coloquio", turno 2026-10-20).

---

## 1. Qué se midió

¿Sirve el `tylluan_recall` híbrido (BM25 + FTS5 + embeddings + RRF, con CoherenceGate) para recuperar contexto cuando un agente hace una **pregunta de seguimiento corta** dentro de una sesión de Coloquio?

## 2. Metodología

1. **Corpus:** 2142 mensajes únicos de los 5 canales de Coloquio (`general`, `equipo`, `mision-activa`, `buffy`... — paginados por `last_turn`, ventana de 500/canal con cierre de páginas por duplicados).
2. **Preguntas:** 60 preguntas reales escritas por agentes (no por humanos, no generadas), extraídas con saneado: se descartan URLs, coordenadas, rutas Windows/Unix, líneas de comando con paths, mensajes >500 chars y preguntas con referentes implícitos no reconstruibles ("¿esto va bien?" sin entidad). Distribución: general 30, equipo 23, mision-activa 3 + resto.
3. **Motor:** kernel de test aislado (puerto 55752, `TYLLUAN_DATA_DIR` propio, **silva.db copiada de producción** `E:\tylluan\data`), v0.17.1 commit `2458b4a`. Cada pregunta: `tylluan_recall` top-5 vía `/mcp`, latencia medida por pregunta.
4. **Etiquetado manual (Buffy, uno a uno):** para cada hit: `R` relevante (trata el tema de la pregunta y ayudaría a responderla), `P` parcial (misma área de discusión, no aborda la sub-pregunta), `I` irrelevante, `S` **self-hit** = el propio mensaje del que se extrajo la pregunta (el cliente ya lo tiene en su ventana; no aporta respuesta). Para preguntas sin contexto nominal ("¿Por qué existen dos?") se recuperó el turno fuente de Coloquio (±3 turnos) antes de etiquetar, para no castigar por desconocimiento del referente.
5. **Artefactos (worktree, sin commitear):**
- **`benchmarks/recall_manual_v1.json`** — conjunto etiquetado canónico (60 items con fichas de origen, criterio R/P/I/S, metodología, métricas y los 10 fallos documentados con causa).
- `test-run-recall/work/{coloquio_msgs.json, questions60.json, recall_raw60*.json, recall_parsed.json, labels.json, fails10.json}` — corpus y crudos.
- **Verificación de la copia de bases (B-3.1):** `PRAGMA integrity_check` → `ok` sobre `test-run-recall/data/silva.db` (10449 nodos) y `tylluan.db`, tras la ejecución.

## 3. Resultados

| Métrica | Valor |
|---|---|
| **R@1** (top-1 relevante) | **17/60 = 28,3%** |
| **R@5** (algún relevante en top-5, excl. self-hits) | **23/60 = 38,3%** |
| **MRR** | **0,325** |
| Zero-result (0 hits tras filtros) | 11/60 = 18,3% |
| Top-1 = mensaje fuente de la propia pregunta (self) | 14/60 = 23,3% |
| Self-hit en top-5 | 19/60 = 31,7% |
| *(secundario)* R@5 contando self como relevante | 38/60 = 63,3% |
| Latencia | media 220 ms · **p50 131 ms · p95 619 ms** · min 14 ms · max 799 ms |

## 4. Hallazgos (por dónde gana y por dónde falla)

1. **Gana con entidad nominal concreta.** Las 17 R@1 son preguntas con un ancla recuperable: "sanitizer… audit.db" (R@1), "GPU" (R@1), "DreamCycle" (R@1), "pool de conexiones de lectura" (R@1/R@2), "¿hay otros call sites…?" (P — solo 1 hit devuelto), "mention-bridge api_coloquio.rs:185" (R@1). Con una entidad concreta el híbrido funciona y la latencia es correcta.
2. **Falla con follow-ups context-free.** "¿Por qué existen dos?", "¿Cuál de las dos hay que hacer?", "¿Los restauro o los dejo para el tech lead?", "¿el wiring lo tienes en curso?" — sin nombre propio ni término de dominio, no hay señal léxica ni semántica que anclar: 0 relevantes en top-5 en todos los casos. Un humano las resuelve por la ventana de conversación, no por búsqueda.
3. **19/60 el top-1 es el propio mensaje fuente (S).** El índice recupera el hilo (buen síntoma de que el embedding ancla la sesión) pero devuelve la pregunta, no la respuesta. De las 60, solo 4 tienen S *y* R en top-5: cuando el self-hit ocupa el rank-1 empuja la respuesta fuera del top-5 en varios casos.
4. **11 zero-result concentrados en sesiones recientes (turnos 1083–1093, 1067, 878).** Varios no son "0 en el índice" sino **filtrados por CoherenceGate**: la respuesta cruda era "Found N, Showing top 0" con aviso "N resultados filtrados por control de coherencia". El gate está actuando sobre follow-ups recientes cuyos mensajes fuente tienen `status` aún no confirmado.
5. **10 fallos ilustrativos (los 37 sin ningún R en top-5, muestreados por diversidad de causa):**

| # | Pregunta | Labels | Causa probable |
|---|---|---|---|
| 1 | «¿Por qué existen dos?» (general:946) | `PIIII` | context-free: 'dos' sin sujeto (hybrid.rs vs silva/mod.rs); el léxico del referente no está en la query |
| 2 | «¿hay algo razonable que hacer…semillas iniciales…?» (equipo:302) | `S` | CoherenceGate recortó a **1 solo hit**; consulta reflexiva sin entidad nominal |
| 3 | «¿Excepción, timeout del cliente, error 5xx?» (mision-activa:489) | `P` | follow-up técnico sin entidad propia; único hit = parcial del mismo benchmark de concurrencia |
| 4 | «b) ¿Corro ANTES el spike con decima-agent?» (general T1083) | *(0 hits)* | zero-result: pregunta con marcador 'b)' de una lista del propio turno |
| 5 | «¿El wiring (subtool + dispatch en handlers.rs)…?» (general:95) | `IP` | entidad concreta pero pregunta **estado efímero de wiring**, no contenido indexable |
| 6 | «¿O prefieres versión agresiva o conservadora…PID…?» (general T1091) | *(0 hits)* | zero-result: follow-up de diseño de sesión muy reciente, ausente o filtrado |
| 7 | «¿Cual de las dos hay que hacer?» (equipo:406) | `IISII` | context-free: las dos opciones viven en el turno 405 sin léxico compartido |
| 8 | «¿que impide que un nodo recibido sea protected…?» (general T1171) | `SS` | self-hit puro: la pregunta existe solo dentro del mensaje-ciclo T1171; la respuesta aún no estaba escrita |
| 9 | «¿como se comporta federation_source…?» (general T1171) | `SS` | idéntica a #8 (sub-pregunta del mismo ciclo) |
| 10 | «¿por que no estamos usando ni un 5% del potencial…?» (equipo:117) | `S` | pregunta reflexiva larga; único hit = el propio enunciado del TL |

**Patrón de los 10:** 4 context-free, 3 self-hit puro, 2 zero-result, 1 estado efímero — ningún fallo se debe a mala latencia ni a corpus sucio.

6. **Artefacto de ventana anidada:** el mensaje "Coloquio: general (50 messages, offset=0)" con el T1002 (LUZ VERDE T999) aparece como top-1 en consultas genéricas de `general` (índices 1, 12, 37, 44…). Es un `read_channel` guardado como memoria: domina por baja densidad léxica, no por relevancia. En 3 de esos casos (37, 44) contenía de hecho la respuesta, pero es suerte de ventana, no de ranking.

## 5. Conclusión y recomendación

El recall híbrido **no es un sustituto de la ventana de conversación para follow-ups** (38% R@5 real), pero **sí recupera hilos por entidad concreta con buena latencia** (<200 ms p50). Recomendaciones para la fase de diseño del siguiente incremento (no implementado aquí, decisión del TL):

1. **Combinar fuentes:** para follow-ups, buscar en "últimos N mensajes del canal" (recencia dura) *fusionados* con el híbrido, no solo híbrido.
2. **Penalizar/depriorizar self-hits** en el ranking de recall (el mensaje del propio query no debe ocupar rank-1).
3. **Auditar el CoherenceGate en follow-ups recientes:** los zero-result de turnos 1083–1093 son candidates a "gate demasiado agresivo con memoria sin confirmar".

## 6. Verificación

- 60/60 recalls ejecutados y parseados (115 hits totales, 11 preguntas con 0).
- Etiquetas manuales 1:1 con los hits (`labels.json` valida longitud contra el parse).
- `benchmarks/recall_manual_v1.json` relee sin error: 60 items, 10 fallos, métricas recalculables desde `items[].labels`.
- Kernel de test parado y puerto 55752 liberado tras la medición; producción (47004) no tocada.
- Este informe es la entrega; el diff (scripts de extracción/parse + `test-run-recall/work/`) queda sin commitear en el worktree de Buffy por regla de flota (solo Claude Code commitea).
