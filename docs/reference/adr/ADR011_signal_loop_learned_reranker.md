# ADR-011: Signal Loop + Learned LightReranker — Memoria Entrenada, No Generativa

- **Estado:** 🟡 **PENDIENTE DE DECISIÓN (DECISION PENDING)**
- **Fecha:** 2026-07-25
- **Autores:** Flota de Agentes Soberanos (José, Claude Code, Deep)
- **Ámbito:** `crates/tylluan-kernel` (`transport/server/handler_recall.rs`, `transport/server/handler_do/audit.rs`, `memory/silva/search.rs`, `memory/night/`), ONNX Runtime (`ort`).
- **Relación con otros ADRs:** Paralelo a [ADR-010](ADR010_embedded_sllm_t5_vs_smollm2.md) (§6), no lo sustituye ni compromete su ejecución. ADR-010 decide qué modelo **genera** texto; este documento decide cómo la memoria **aprende a rankear** — son ejes ortogonales, ninguno bloquea al otro.

---

## 1. Contexto y Problema

La filosofía fundacional de Tylluan es que un agente = LLM (razonamiento) + un sistema de memoria separado, con sus propios modelos y subagentes — no una base de datos vectorial pegada a un LLM. Hoy ese sistema de memoria (SilvaDB) es enteramente determinista: BM25 + FTS5 + BGE-M3 + RRF (Reciprocal Rank Fusion, k=60) + un cross-encoder pretrenado congelado (Jina Reranker Turbo, ~37M parámetros, `router/embeddings.rs::RerankEngine`) para el camino opcional `search_hybrid_reranked`.

Ninguno de esos componentes **aprende de cómo se usa Tylluan realmente**. El RRF pesa igual una memoria que resultó útil que una que nunca se volvió a tocar. El reranker Jina es un modelo congelado entrenado por terceros sobre datos genéricos — no sabe nada sobre los patrones de uso específicos de un despliegue de Tylluan concreto.

**Corrección de fondo (José, 2026-07-25):** un reranker aprendido no es el primer paso posible. Sin una señal de entrenamiento real, no hay nada que entrenar. El primer paso es el **loop de recolección de señal implícita** — sin eso, cualquier red aprendida (reranker, prefetcher, proyector multimodal, o cualquier propuesta futura) carece de datos. Este documento formaliza ambas piezas, en el orden correcto: primero el loop, después su primer consumidor real.

---

## 2. Parte A — Signal Loop: `audit_recall_feedback`

### 2.1 El gap real (verificado contra el código, no asumido)

`handle_tylluan_recall` (`handler_recall.rs`) hoy **no registra en ningún sitio** qué `node_id`s devolvió una llamada de recall. `log_audit_entry` (`handler_do/audit.rs`) sí registra cada llamada a `tylluan_do` en `guild_audit_log`, con hash-chaining SHA-256 — pero solo cubre acciones, no recuperaciones de memoria. No existe correlación entre "esta memoria fue recuperada" y "esta memoria fue efectivamente usada después". Sin esa correlación, no hay señal de entrenamiento — es la corrección de José y es literalmente cierta en el código actual.

### 2.2 Esquema propuesto

```sql
CREATE TABLE IF NOT EXISTS audit_recall_feedback (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id TEXT NOT NULL,       -- node_id devuelto por tylluan_recall
    agent_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,       -- momento de la recuperación
    task_hash TEXT NOT NULL,       -- hash determinista del query/intent original (agrupa por tarea)
    useful INTEGER,                -- NULL = aún sin resolver, 0 = negativo, 1 = positivo
    resolved_at TEXT               -- cuándo se resolvió useful (NULL mientras pendiente)
);
```

Vive en `./data/audit.db`, junto a `guild_audit_log` — mismo archivo, misma disciplina de apertura vía `crate::config::open_db`.

### 2.3 Cómo se puebla (mecanismo honesto, no ground truth perfecto)

**Escritura (positiva por defecto, con ventana de resolución):**

1. Cada `node_id` devuelto por `handle_tylluan_recall` se inserta como fila con `useful = NULL`, `task_hash` derivado del query normalizado (mismo patrón de hash que `routing_failure_id` en `audit.rs`).
2. El kernel observa las siguientes **3 llamadas** de ese `agent_id` a `tylluan_do` (vía el mismo `guild_audit_log` que ya existe, sin tabla nueva para esa mitad). Si el `intent` de alguna de esas 3 llamadas comparte términos significativos (overlap de tokens no triviales, mismo umbral tipo Jaccard que ya usa `dream_cycle.rs` para pre-filtrar antes del coseno real) con el `memory_id` recuperado, se marca `useful = 1`.
3. Si pasan 3 llamadas del mismo agente sin overlap, o transcurre un timeout (p. ej. 1h), se marca `useful = 0`.

**Esto es una señal heurística proxy, no verdad fundacional perfecta** — igual que Ouroboros ya opera sobre "ground truth aproximado, no juicio LLM": el audit chain dice qué pasó, no por qué. Documentarlo así evita que el equipo trate `useful` como una etiqueta infalible. Es deliberadamente el mismo nivel de honestidad que ya aplica `adv_cross_scope_leakage_agent_filtered` (test añadido esta sesión): documenta el comportamiento real, no aspira a una garantía que el sistema no tiene.

### 2.4 Coste de implementación

Cero modelos nuevos, cero dependencias nuevas. Es SQL + una comparación de tokens ya usada en otro sitio del código (`jaccard_words` en `dream_cycle.rs`, reutilizable tal cual). Bloqueante para todo lo demás en este ADR — sin esto poblado durante semanas, no hay §3.

---

## 3. Parte B — Learned LightReranker

### 3.1 Qué es y qué NO es

Una red densa de 2 capas, exportada a ONNX, **<2MB**, que toma como entrada las señales ya calculadas por `search_hybrid` para cada candidato y produce un score final de reordenamiento:

```
entrada (4 features, ya calculadas hoy, cero coste de inferencia extra):
  - score_rrf            (search_hybrid, ya existente)
  - score_graph           (local_query_graph / PPR con degree penalty, ya existente)
  - recency_score         (derivable de updated_at, half-life decay ya existente)
  - cross_encoder_approx  (opcional: score de Jina si search_hybrid_reranked está activo, o 0.0 si no)

salida: score final de reordenamiento (reemplaza o combina con el orden RRF)
```

No reemplaza al reranker Jina Turbo (`RerankEngine`, ~37M parámetros, cross-encoder pretrenado genérico) — son complementarios. Jina aporta comprensión semántica genérica que un modelo de 2 capas dense no puede replicar. LightReranker aporta lo que Jina no puede: **aprender de los patrones de uso reales de este despliegue concreto**, algo que un modelo congelado pretrenado por terceros nunca podrá hacer sin reentrenarlo.

### 3.2 Entrenamiento

Se reentrena en cada pulso de *NightConsolidation* (fase nueva del `PhaseOrchestrator` ya existente, `memory/night/`, ejecutando en paralelo con las 8 fases actuales vía el mismo semáforo dimensionado por `available_parallelism()`), usando las filas de `audit_recall_feedback` con `useful` ya resuelto (no `NULL`) desde la última ejecución. Igual que `scripts/train_complexity_mlp.py` del experimento MLP de esta sesión: exportación a ONNX, carga vía `ort::Session`, mismo patrón de `MlpScorer` (`mlp/mod.rs`) con degradación elegante (`Option<f64>`, `None` si no hay modelo entrenado aún o falla la carga).

### 3.3 Pregunta explícita 1 — ¿Qué decide que el reranker reemplaza al RRF estándar?

**No lo reemplaza nunca por decreto — se gana el reemplazo con evidencia, con un umbral cuantitativo, no con "se ve mejor".**

Criterios, todos obligatorios antes de activar LightReranker como default en `search_hybrid`:

1. **Volumen mínimo de señal**: ≥5.000 filas resueltas (`useful` no NULL) en `audit_recall_feedback` — mismo umbral que Deep propuso para el prefetcher, por la misma razón: una red de este tamaño con menos datos sobreajusta y genera falsos positivos que contaminan resultados.
2. **Evaluación offline con held-out**: sobre un 20% de las filas resueltas apartado y nunca usado para entrenar, medir NDCG@5 (o precision@5, más simple de implementar primero) del ranking de LightReranker vs. el ranking RRF actual sobre el mismo conjunto. LightReranker debe superar a RRF por un margen no trivial (a definir empíricamente al llegar aquí — no fijamos una cifra arbitraria hoy sin datos reales que la respalden).
3. **Modo sombra (shadow mode) antes de cutover real**: LightReranker corre en paralelo a RRF durante un periodo de quemado (burn-in) — se calcula su ranking, se loguea, pero **no se usa** para servir resultados reales. Se compara contra el ranking que RRF sirvió de verdad y contra las etiquetas `useful` que llegan durante ese periodo.
4. Solo si 1-3 se cumplen, LightReranker pasa a producción.

### 3.4 Pregunta explícita 2 — ¿Cuándo y cómo se hace el cutover sin degradación?

**Gradual, con fallback automático, nunca un flag binario de golpe:**

1. **Blend, no reemplazo instantáneo** — mismo patrón ya construido y probado esta sesión en `router/complexity.rs::blend_with_mlp` (60/40 heurística/aprendido, degradación elegante a `None`). LightReranker entra como una señal más en la fusión, con peso inicial bajo (ej. 10-20%), no sustituyendo RRF de un día para otro.
2. **Incremento de peso gradual** solo si el modo sombra de §3.3 sigue confirmando mejora en ventanas sucesivas de NightConsolidation — no una sola noche buena, varias consecutivas.
3. **Fallback automático a RRF puro** si `LightReranker::score()` devuelve `None` (modelo no cargado, fallo de inferencia) — mismo patrón exacto que `MlpScorer` ya usa hoy. Cero riesgo de que un modelo roto tumbe el recall.
4. **Reentrenamiento continuo**, no un cutover de una sola vez y listo — cada noche el modelo se reentrena con datos frescos; si el rendimiento cae, el modo sombra lo detecta antes de que afecte producción real (porque el peso de blend solo sube si el sombra confirma mejora, nunca baja de golpe sin que el sombra lo muestre primero).

---

## 4. Parte C — Gate de Coherencia en Generación (defensa del "segundo salto")

Identificado en la discusión de esta sesión: el primer salto de una cadena de inyección de memoria (payload malicioso almacenado, ver `adv_memory_poisoning_recall_returns_inert`, commit `0f81cc1`) ya está cubierto — hoy la memoria se devuelve como texto inerte, sin ejecución. El **segundo salto** — el riesgo real y aún no materializado — ocurre el día en que un SLM generativo (post ADR-010) lea nodos recuperados de SilvaDB como contexto de entrada. En ese momento, un payload de inyección almacenado hace meses puede reactivarse con la autoridad implícita de "esto ya pasó por el sistema de memoria".

### 4.1 Que esto no es hipotético — literatura 2026 verificada contra arXiv real

No es una preocupación teórica nuestra: es exactamente la superficie de ataque que la literatura de seguridad de 2026 ya está documentando, con evaluación empírica. Verificado directamente contra abstract/HTML de cada paper (no una lista de nombres sin comprobar):

- **ShadowMerge** (arXiv:2605.09033, mayo 2026) — ataque contra memoria **basada en grafo** (exactamente la forma de SilvaDB): inyecta una relación envenenada que comparte el mismo "ancla" y canal de relación que evidencia legítima, pero con un valor en conflicto. 93.8% de tasa de éxito de ataque medida sobre Mem0 + PubMedQA/WebShop/ToolEmu. Es el ataque más directamente aplicable a nuestro grafo de conocimiento (`memory/silva/graph.rs`).
- **eTAMP — "Poison Once, Exploit Forever"** (arXiv:2604.02623, abril 2026) — envenenamiento cruzado de sesión/sitio **sin acceso directo a la base de memoria**: una única observación contaminada (ej. una página web manipulada) se ingiere pasivamente en la trayectoria del agente, y resurge semánticamente en tareas futuras no relacionadas. Tasas de éxito 19.5-32.5% en modelos frontera reales (GPT-5-mini/5.2, GPT-OSS-120B).
- **"Hidden in Memory: Sleeper Memory Poisoning"** (arXiv:2605.15338, mayo 2026) — memorias fabricadas que quedan dormidas hasta que condiciones específicas se acumulan a través de múltiples interacciones ("time-bomb attacks"). 99.8% de aceptación de memoria envenenada en GPT-5.5, 95% en Kimi-K2.6 — la tasa de aceptación es alarmantemente alta precisamente porque nada en el pipeline estándar verifica coherencia contra la fuente original.
- **MemLineage** (arXiv:2605.14421, mayo 2026) — defensa real, no la nuestra: adjunta procedencia criptográfica (Merkle log, firmas Ed25519 por principal) **y** un DAG de derivación ponderado que rastrea qué entradas recuperadas influyeron en cada memoria nueva. Lleva la tasa de éxito de ataque a cero en sus tres cargas de trabajo modeladas, con overhead sub-milisegundo. Conceptualmente muy cercano a nuestro `owner_scope` + `provenance` ya existentes en el schema de `nodes` — la diferencia es que MemLineage rastrea *derivación* (qué influyó en qué), no solo origen estático.
- **"RAG Sanitizer" (Leong, 2026)** — citado dentro de otro paper de 2026 (`arXiv:2606.30566`) como mecanismo de filtrado en la capa de recuperación, previo a la inyección en contexto. **No pude verificar el paper primario directamente** (solo la cita dentro de otro trabajo) — tratar como referencia de segunda mano, no como fuente primaria confirmada, a diferencia de los cuatro anteriores.

### 4.2 Mitigación ya construida, no una promesa a futuro

El gate de verificación cruzada BGE-M3 añadido en `memory/consensus.rs::apply_synthesis` esta sesión (commit `63e3073`) — re-embebe el texto generado y lo compara vía coseno contra el embedding almacenado de cada fuente, con el mismo umbral (0.85) que usa Ouroboros para agrupar fallos por clúster — es, en espíritu, nuestra versión mínima de lo que ShadowMerge/eTAMP explotan y lo que RAG Sanitizer describe: comprobar que el contenido generado sigue siendo semánticamente fiel a sus fuentes antes de confiarlo. Hoy protege únicamente la síntesis de Consensus. Este ADR formaliza que **el mismo patrón, ya probado, se reutiliza sin diseño nuevo** en cualquier punto futuro donde un SLM generativo consuma nodos de SilvaDB como contexto — incluyendo el eje de coordinación entrenada de ADR-010 §6, si evoluciona de enrutamiento puro a generación real.

Lo que nuestro gate actual **no cubre** y MemLineage sí: procedencia de derivación (qué memorias influyeron en la creación de cuáles otras, no solo similitud de contenido final). Es una brecha real, documentada aquí, no resuelta por este ADR — candidata a un ADR futuro si el equipo decide que vale la pena el coste de un DAG de derivación sobre el grafo existente.

No es una pieza nueva a construir ahora — es una decisión de arquitectura: **ningún punto de inserción de generación futura se activa sin pasar primero por `average_cosine_to_sources` o su equivalente**. Se deja registrado aquí para que nadie lo reinvente ni lo olvide cuando llegue ese punto.

---

## 5. Horizonte de largo plazo (verificado contra fuente primaria, no extrapolación)

Investigación de esta sesión, verificada contra abstracts reales, no resúmenes de terceros:

- **Titans — "Learning to Memorize at Test Time"** (arXiv:2501.00663, Google Research, Behrouz/Zhong/Mirrokni). Módulo de memoria neuronal que actualiza sus propios pesos **en inferencia**, vía una métrica de "sorpresa" basada en gradiente con momento, más olvido adaptativo vía weight-decay. Maneja contextos >2M tokens.
- **ATLAS — "Learning to Optimally Memorize the Context at Test Time"** (arXiv:2505.23735). Evolución directa de Titans; optimiza memoria sobre ventana deslizante en vez de token a token ("Omega rule"), supera a Titans en BABILong 10M tokens.
- **Larimar** (arXiv:2403.11901, IBM Research, ICML 2024, código real en `github.com/IBM/larimar`). El más cercano conceptualmente a lo que este ADR persigue: memoria episódica pequeña, **no un modelo de lenguaje**, acoplada a un LLM congelado, permite editar/olvidar hechos sin reentrenar el modelo base.

**Por qué no se persigue ahora:** Titans/ATLAS requieren actualización de pesos en tiempo de inferencia — un salto de ingeniería mayor (kernels fusionados a medida, implementaciones de referencia hoy orientadas a GPU, ej. `lucidrains/titans-pytorch`) que no encaja con el perfil CPU-first de Tylluan ni con el hardware modesto que el proyecto explícitamente quiere soportar (Raspberry Pi 4 hasta workstations de gama alta, sin dependencia de GPU cluster). El signal loop + LightReranker de este ADR es el paso mínimo real, con precedente publicado (línea de investigación de reranking aprendido sobre señal de uso — verificado a nivel de abstract, no de metodología completa, para arXiv:2607.00017), construible con el cómputo que el equipo tiene hoy. Titans/ATLAS/Larimar quedan como el horizonte de referencia, no como el próximo sprint.

---

## 6. Qué falta verificar antes de tocar código

- Umbral exacto de "overlap de tokens no trivial" para resolver `useful` en §2.3 — necesita calibrarse contra datos reales de Coloquio/audit log, no fijarse arbitrariamente.
- Margen exacto de mejora en NDCG@5/precision@5 que justifique subir el peso del blend en §3.3 — mismo caso, sin datos reales no hay cifra defendible.
- Si el propio esquema `audit_recall_feedback` debe federarse entre peers del mesh (M14) o quedarse estrictamente local por nodo — no evaluado en este documento, fuera de alcance.
- Brecha de MemLineage (§4.2): nuestro gate BGE-M3 verifica similitud de contenido final, no derivación — no sabemos qué memorias concretas "influyeron" en la creación de cuáles otras. Cerrar esa brecha (DAG de derivación tipo MemLineage) no está evaluado en este documento — candidato a ADR futuro, no a implementación ahora.
- "RAG Sanitizer" (§4.1) solo verificado por cita de segunda mano — antes de referenciarlo como precedente en cualquier decisión de diseño real, localizar y leer el paper primario.

---

## 7. Próximos Pasos (orden estricto — cada uno bloquea al siguiente)

1. **§2**: Implementar `audit_recall_feedback` + población desde `handle_tylluan_recall` + resolución por overlap desde `guild_audit_log`. Sin esto, nada más en este documento es ejecutable.
2. Dejar correr el signal loop en producción real (José, equipo) durante semanas — no hay atajo, la sección 3.3 lo exige explícitamente.
3. **§3**: Una vez ≥5.000 filas resueltas, construir y evaluar LightReranker en modo sombra.
4. **§3.4**: Cutover gradual solo si el modo sombra lo confirma.
5. **§4** queda como decisión de arquitectura ya vigente, no requiere trabajo adicional hasta que ADR-010 produzca un punto de inserción generativo real.

---

> *Este documento no compromete ejecución inmediata. Formaliza, con el mismo rigor que ADR-010, el orden correcto de un trabajo que sin esta secuencia (señal antes que modelo) se construiría al revés y fallaría por falta de datos, no por diseño defectuoso.*
