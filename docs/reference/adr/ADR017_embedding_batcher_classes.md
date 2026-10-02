# ADR-017: Rediseño del batching de embeddings por clase de call-site + LRU warm-start

- **Estado:** 📐 **DISEÑO para revisión del Tech Lead** (no implementado — asignación explícita: diseño antes de tocar código)
- **Fecha:** 2026-10-01
- **Autor:** Deep (con los datos de sus 3 corridas del harness de latencia T4)
- **Ámbito:** `crates/tylluan-kernel/src/router/embeddings.rs` (`EmbedBatcher`, `embed_batch_coalesced`, `embed_batch_coalesced_async`), `transport/server/handler_recall.rs`, `handler_think.rs`, `handler_do/mod.rs`, `handler_remember.rs`, `main.rs` (warm-start al boot)

---

## 0. Evidencia base (medida, no conjetura)

Tres corridas del harness de latencia sobre kernel de test aislado, n=50 por celda, mxbai-embed-large CPU:

| Configuración | recall C=1 p50 | recall C=4 p50 | recall C=8 p50/p99 | do C=1 p50 |
|---|---|---|---|---|
| flag OFF (producción) | 5.2s | 43.3s | 84.2s / 115.5s | 121ms |
| flag ON (batcher global único) | 12.6s | **25.0s** | 78.5s / 236.0s | 1.3s |

Lecturas verificadas:

1. El batcher global único **mejora C=4** (el coalescer agrupa bien a concurrencia moderada) pero **degrada C=1 (+2.4×), C=8 p99 (+2×) y `do` (+10× en p50)** — head-of-line blocking cruzado: los embeds largos de recall bloquean los embeds cortos de routing de `do` en la misma cola.
2. Con flag ON se generaron **31 nodos stale en la ventana C=8** (la cola acotada rechaza con `Busy` bajo C≥4 y esos embeds caídos dejan nodos sin embedding) — cero con OFF. Refuerza el NO-GO de `e70bc42`.
3. Phase 0 del pool SQLite (2026-10-01): la espera del Mutex de conexión es **<0.1% del p50** de recall en C=8 — el cuello de botella es el **Mutex del modelo ONNX** (5s/embed en CPU), no SQLite.
4. El suelo de 5s/query es costo de inferencia puro con queries distintas (cache fría); el techo C=8 es la cola del Mutex del modelo.

**Conclusión de la evidencia:** el lever real no es ni el pool SQLite (descartado con dato) ni un batcher global (medido peor en 5 de 8 celdas) — es **separar las clases de call-site** para que los embeds baratos de routing no paguen la cola de los embeds caros de recall, y **pagar menos inferencia** con warm-start de lo que se repite.

---

## 1. Contexto y problema

El diseño actual (`embeddings.rs`) tiene un único `EmbedBatcher` global (max_batch 16, ventana 5ms, cola acotada con rechazo `Busy`), compartido por TODOS los call-sites densos:

- **RECALL** (`handler_recall`, `handler_think`): queries de usuario, textos largos, ~5s/embed — el costo dominante.
- **ROUTING** (`handler_do` fallback semántico, anchors de routing): intents cortos, embeds baratos, alta frecuencia.
- **BACKGROUND** (reindexer, distill M22, anchor warmup): embeds masivos, sin latencia crítica.

Un solo batcher mezcla las tres clases en una ventana de 5ms: la cola FIFO del `EmbedBatcher` no distingue un intent de routing (que debería tardar decenas de ms) de un batch de 8 queries de recall (40s de ONNX). El resultado medido es el de la tabla de §0. El rechazo `Busy` además convierte una petición legítima en un nodo sin embedding (stale perpetuo) — el costo del rechazo supera al beneficio del acotado.

---

## 2. Decisión propuesta

### 2.1 Batchers por clase de call-site (3 clases)

| Clase | Call-sites | Ventana | max_batch | Cola |
|---|---|---|---|---|
| **RECALL** | `handler_recall`, `handler_think` | 10–20ms | 8–16 | acotada con backpressure real (spawn_blocking spill, nunca drop) |
| **ROUTING** | `handler_do` (fallback semántico), anchors, DCR de `handler_remember` | 50–100ms | 32 | acotada, rechazo degrada a embed directo síncrono con warn (nunca nodo sin embedding) |
| **BACKGROUND** | reindexer (`embed_batch_coalesced_async`), distill, warmup | — | 32 | **sin rechazo**: si RECALL está saturada, usa `embed_batch_async` directo (comportamiento flag-OFF actual); solo entra en la ventana de RECALL cuando hay hueco |

Reglas invariantes:

1. **Un solo motor ONNX sigue existiendo** (el `Mutex<TextEmbedding>` no se duplica): los tres batchers comparten el mismo engine; el coalescing es por clase, la inferencia sigue serializada por el Mutex. Lo que se elimina es el *head-of-line cruzado* y la latencia de cola artificial, no el Mutex.
2. **Ningún rechazo deja datos a medias:** si un embed falla en ROUTING o BACKGROUND, el call-site degrada a su comportamiento flag-OFF de hoy (embed directo o BM25-only para recall de rutas), NUNCA escribe un nodo sin embedding. El bug Busy→stale de la RUN2 queda prohibido por diseño.
3. **flag-gated:** `[silva] embed_batching_enabled` sigue como master switch; las clases activas se configuran por defecto a (RECALL+ROUTING on, BACKGROUND off) — la opción "todo off" es exactamente el comportamiento de producción actual, para rollback instantáneo.

### 2.2 LRU warm-start al boot

- **Routing anchors:** al arranque (tras `engine` listo, el mismo punto del anchor warmup existente en `main.rs`), pre-embed las ~45 descripciones de guild del catálogo (`tools/guild_catalog.py` → `ROUTABLE_DESCRIPTIONS`) e intents semilla, poblando el LRU de 512 slots del engine y el `query_embed_cache` (TTL 5min). El fallback semántico de `do` deja de pagar ONNX en caliente.
- **Top-intents frecuentes:** opcional fase 2 — los N intents más frecuentes de `agent_profiles`/curriculum se re-embeben en el warm-up. Coste: ~45 embeds × 5s = ~4 min una vez al boot (hoy el anchor warmup ya hace parte de esto).
- **Cuidado con el boot:** el warm-up debe ir DETRÁS del `background_budget` (mismo patrón que reindexer/checkpoint) para no colisionar con las primeras requests reales — el mismo confound de arranque que cerramos en el A/B.

### 2.3 Qué NO se hace (registrado explícito)

- **NO** read-pool SQLite (Phase 0: <0.1% del p50, descartado con dato).
- **NO** reintroducir un batcher global único (medido peor en 5 de 8 celdas).
- **NO** tocar el orden de stages de `handler_recall` ni su semántica de resultados.
- **NO** cambiar el default de producción (`embed_batching_enabled=false`) hasta que la fase correspondiente pase su gate de medición.

---

## 3. Fases y gates de medición

Cada fase termina con el harness de latencia (`benchmarks/latency/cpu_baseline.py`) + el pre-flight de Buffy (0 colisiones de mantenimiento en ventana). Criterios de aceptación contra la baseline OFF de §0:

- **F1 — ROUTING + warm-start** (el leverage más barato, no toca recall):
  - do C=1 p50 ≤ 121ms; do C=4/C=8 sin degradación vs OFF.
  - 0 rechazos Busy; 0 nodos stale nuevos en ventana.
- **F2 — RECALL class** (solo si F1 verde):
  - recall C=1 p50 ≤ 5.2s (con warm, objetivo < 5.2s);
  - recall C=4 p50 ≤ 25.0s y C=8 p50 ≤ 78.5s (los mejores números de las dos configuraciones medidas);
  - C=8 p99 ≤ 115.5s (nunca el 236s del batcher global).
- **F3 — BACKGROUND yield** (solo si F2 verde): reindexer entra en ventana RECALL solo con hueco; 0 stale tras 2 ticks; verificación de convergencia del contador stale (la firma que Buffy detectó en T840).

Si una fase falla su gate, se revierte el flag de esa clase y se documenta la razón — no se "arregla a mano" la medición.

---

## 4. Decisiones abiertas para el Tech Lead

1. **Ventanas exactas** (RECALL 10–20ms, ROUTING 50–100ms) — propongo calibrarlas empíricamente en F1 con dos valores y quedarnos con el mejor gate.
2. **Warm-start siempre-on o config-gated** (`[silva] embed_warm_start` default on, off para toaster profile).
3. **F1+F2 en un solo sprint o separados** — F1 es ~100 líneas y no toca el camino de recall; F2 es el cambio de riesgo real.
4. Si el TL prefiere atacar el suelo de 5s/embed en vez de la cola (ej. modelo más pequeño para routing, mxbai-q para anchors), este ADR lo registra como alternativa P0 del embedding, no del batching.

---

## 5. Consecuencias

- **Positivas:** do deja de pagar la cola de recall (medido +10×); el techo C=8 deja de crecer con el flag ON; cero rechazos que generan stale; warm-start reduce el p50 de routing en caliente a nivel LRU (<5ms).
- **Negativas:** 2–3 batchers = más código que el shim actual (un struct parametrizado, no duplicación); el warm-start añade ~4 min al arranque en CPU (mitigado: tras budget + config-gated); la matriz de configs (flag × clases) exige disciplina de medición por fase.
- **Rollback:** desactivar `embed_batching_enabled` (master) restaura el comportamiento exacto de producción actual en segundos.
