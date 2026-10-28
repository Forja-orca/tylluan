# H-45 — Diseño comparativo: embeber-al-escribir vs filtrar-tipos + corrección docstring (hermes)

**Asignación:** T2001 (TL claude-code). **Base:** `1a24e82` (worktree rebasado tras PUSH185; los docs H-43/H-44 ya estaban en main, idénticos a mis copias — verificado con diff). **Sin código de producción**; única edición: docstring de `guilds/core/silva_utils.py` (diff sin commitear, abajo). Fecha: 2026-10-28 ~17:2x.

## 0. Datos medidos (todo del log `E:/tylluan/logs/kernel.log`, arranque 2026-10-24 16:22, y BD viva vía URI `mode=ro&immutable=1`)

| Métrica | Valor | Fuente |
|---|---|---|
| Embed ONNX de 1 texto en idle | **283–507 ms** (n=25, min 283, p50 507) | `recall_stage_embed" stage_ms=` en log |
| Cola de recall DURANTE rondas de reindex | p90 **5,4 s**, p99 7,7 s, max **19,5 s** | mismos samples |
| Ronda de reindexer (100 nodos) | **supera los 120 s del guard de forma rutinaria**: 363 WARN "taking longer than expected" del RE-INDEXING en 4 días (46/183/129/5 por día; extensiones 1→3 del guard) | `guard: Task 'Agnostic RE-INDEXING'` |
| Coste por nodo en condiciones reales de coalesción OFF | **≥1,2 s/nodo** (120 s ÷ 100 nodos, cota inferior por construcción) | derivado de las 2 líneas anteriores |
| Throughput actual | **600 nodos/h** (take(100) cada 600 s, `main.rs:1919-1946`); deltas `Found` literales: −100/tick en calma, +200 en ráfagas PUSH | log + código |
| Backlog actual y su composición | 6458 stale; nunca-embedeados ≈4648: code_entity **2008** (¡creciendo: 1778→2008 en ~2 h por PUSH185), coloquio 963, session_digest 794, episode 547, routing_anchor 152, resto ~184 | snapshot BD 17:2x |
| Tipos YA excluidos de recall | `session_digest` + `routing_anchor` (`handler_recall.rs:688/919`) → embeberlos = ONNX que recall jamás devuelve | código |
| Coalesced batching | `embed_batching_enabled = false` en `tylluan.toml:292` → el EmbedBatcher (que fusionaría reindex+recall en un solo batch ONNX) está **apagado**; el reindexer serializa contra recall en el mutex del modelo | config + `main.rs:1975-1985` |

**Coste ONNX total del backlog:** 5512 nodos (sin los 946 filtrables) × 0,3–0,5 s ONNX puro ≈ **28–46 min de CPU ONNX serial**; a ritmo real medido (1,2 s/nodo, coalesción OFF) ≈ **1,8 h de pared** si se sube el tope a 400/tick (2400/h). El coste NO es el problema: la **contención** sí (recall p90 5,4 s durante rondas).

## 1. Opción A — embeber al escribir (handler `silva/node` + flywheel)

**A1 inline (embed síncrono en la escritura):**
- Suma 0,3–0,5 s a CADA escritura REST y al flywheel (hasta ~50 msgs/tick → +15–25 s por tick); con coalesción OFF el embed comparte el mutex del modelo con recall → reproduce la contaminación medida el 09-07 (p50 recall >10 s en rondas).
- Mismo ONNX total que A2, pero concentrado en primer plano y bloqueando al escritor.
- **Descartada.**

**A2 "con cola para no bloquear":** la cola YA EXISTE — el escaneo `get_stale_embeddings` + reindexer ES la cola de escritura diferida (lag ≤1 tick). Lo que duele no es la ausencia de cola sino **el tope (100/600 s) y la contención (coalesción OFF)**. Embeber-en-el-escribiente con cola propia duplicaría el mecanismo existente con más código y nuevas races (doble productor de embeddings contra el mismo mutex). El único sitio donde embeber-al-escribir aporta valor real es en la FUENTE de tipos de alta latencia útil (code_graph → `code_entity` crece +230/2 h más rápido de lo que el reindexer digiere): ahí, llamar al embed desde el guild/fuente (patrón `tylluan_remember`, que ya embebe con éxito y 0 fallos) es preferible a tocar el handler REST genérico.

## 2. Opción B — filtrar tipos en `get_stale_embeddings` (`nodes.rs:644`, hoy sin filtro de tipo)

- Filtro: `AND n.type NOT IN ('session_digest','routing_anchor')` ≈ 1 línea.
- Ahorra 946 nodos (14,7 % del backlog) + su inflow perpetuo (~160/día de session_digest): ONNX que hoy se gasta en vectores que recall excluye de resultados.
- **No arregla nada del problema real**: code_entity (2008, creciendo), coloquio (963) y episode SÍ aparecen en recall y necesitan vector. Como único remedio es insuficiente.
- Riesgo semántico: si mañana se quisiera recall denso de session_digest (p. ej. para resúmenes navegables), el filtro habría dejado un agujero histórico silencioso. Es decisión de producto, no técnica.

## 3. Recomendación (híbrida, en orden de impacto/coste)

1. **Higiene B** (aceptar el filtro, 1 línea): deja de gastar ONNX en tipos que recall excluye. Reversible si el producto cambia de opinión.
2. **Ajuste del reindexer (= opción B de H-42, ya recomendada por el TL):** tope/periodo configurables (take 100→400 mantiene rondas de 12×32 con los 500 ms de sleep actuales); **probar `embed_batching_enabled=true`** — el coalesced path existe en código (`embed_batch_coalesced_async`) y fusiona reindex+recall en batch único; el dato de contaminación de 09-07 se midió con él APAGADO y no hay A/B publicado con él encendido [no verificado — requiere validación antes de activar en prod]; y tope de hilos ONNX para no saturar la máquina. Con esto: 2400 nodos/h → backlog residual en ~2 h y code_entity al día.
3. **Solo si se quiere inmediatez para `code_entity`:** embeber en la FUENTE (guild code_graph vía el patrón remember), NO en el handler REST genérico ni en el flywheel.
4. A1 inline en handler+flywheel: **no** (latencia al usuario, latencia del flywheel, contención medida, cero ganancia de ONNX total).

## 4. (b) Docstring corregido — diff sin commitear

`guilds/core/silva_utils.py:123-140` (única copia afectada; la de `guilds/scholars/plugins/` no contiene el texto mentiroso): se elimina la promesa "the node gets a real embedding like any other node it creates" y se documenta la realidad verificada en H-44 (handler solo upsert node+FTS; invisibles a búsqueda denso-híbrida hasta el barrido del reindexer; lag mínimo 1 tick de 10 min). Lint OK, cero cambios de comportamiento. El diff completo va en el turno de entrega; `git status` en el worktree muestra `M guilds/core/silva_utils.py` sobre `1a24e82`.

## Impact (ADR-013)

Solo lectura de BD/log + un docstring en `guilds/` (sin cambio de comportamiento; no toca transport/, integraciones ni toml). No se activó ningún flag, no se cambió código Rust, no se corrió nada contra prod salvo consultas `ro&immutable=1`. El fondo del diseño (qué activar y cuándo) queda para decisión de Jose/TL.

## No verificado / límites

- [no verificado] A/B del coalesced batching encendido: no hay datos en el log actual (flag false desde el arranque del 24-oct); mi estimación de contención para ese modo es estructural (batch compartido) no medida.
- [no verificado] Coste ONNX puro por nodo con batch=32 (el stage_ms medido es de 1 texto de recall; el batch amortiza overhead pero el CPU-bound escala ~lineal por texto).
- Los conteos BD son snapshot 17:2x con el kernel escribiendo; la estructura es estable, los números se mueven (code_entity +230/2 h).
- `Found` cuenta stale TOTALES (incluye hash-viejos 1870 de H-43 que también consumen presupuesto); el desglose por tipo es de mi consulta, no del log.
