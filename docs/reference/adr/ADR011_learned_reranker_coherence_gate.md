# ADR-011: Learned LightReranker + Coherence Gate + Neural Memory Vision

- **Estado:** 🟢 **Fase 1/1b/2 IMPLEMENTADAS** (Signal Loop + Coherence Gate, ver §2.5) — 🟡 **Fase 3-5 PENDIENTES** (LightReranker cutover: bloqueado por datos, no por diseño) — Fase Visión (P3) explícitamente fuera de alcance
- **Fecha:** 2026-07-25 (implementación §2.5 añadida el mismo día)
- **Autores:** Deep (OpenCode), Claude Code (Tech Lead)
- **Ámbito:** Kernel Rust (`crates/tylluan-kernel`), ONNX Runtime (`ort 2.0.0-rc.10`), NightConsolidation
- **Dependencias:** ADR-010 (SLLM embebido, ortogonal), ADR-009 (Agents declarativos, señal `agent_id`)

---

## 1. Contexto y Problema

Tylluan v0.13.0 ejecuta `search_hybrid` usando **RRF (Reciprocal Rank Fusion)** con k=60 para fusionar resultados de búsqueda vectorial, textual y de grafo. Es determinista, simple y funciona. Pero tiene dos limitaciones:

**A. El ranking no aprende del uso real.**  
Dos agentes distintos que buscan "error 503 timeout" pueden necesitar memorias completamente distintas (uno es SRE, otro es frontend). RRF asigna el mismo peso a las mismas posiciones siempre, independientemente de quién busca, de qué memorias le fueron útiles en el pasado, y de qué tarea está ejecutando.

**B. No hay defensa contra envenenamiento en la ruta de recall → generación.**  
El test `adv_memory_poisoning_recall_returns_inert` verifica que `tylluan_recall` devuelve contenido inerte como texto. Eso es correcto y necesario. Pero no cubre el **segundo salto**: cuando un LLM generativo (post ADR-010) consume esa memoria y la inyección revive con autoridad de "esto ya pasó por control de calidad del sistema". Los ataques *ShadowMerge* (inyección de aristas envenenadas latentes en grafos, 2026) y *Sleeper Poisoning* (inyecciones durmientes que despiertan con una consulta específica meses después) son vectores reales publicados en 2025-2026 contra sistemas de memoria agéntica.

### Literatura relevante 2025-2026

| Fuente | Aporte | Relación con ADR-011 |
|--------|--------|---------------------|
| **Titans** (Google, arXiv:2501.00663, 2025) | Memoria neuronal que actualiza pesos en inferencia vía "sorpresa" basada en gradiente. MAC/MAG. | Visión a largo plazo para Neural Memory Architecture. |
| **ATLAS** (arXiv:2505.23735, 2025/2026) | Evolución de Titans: optimiza memoria sobre ventana deslizante. Supera a Titans en BABILong. | Visión a largo plazo; ver §6. |
| **Larimar** (IBM, ICML 2024, arXiv:2403.11901) | Memoria episódica pequeña no-LLM acoplada a LLM congelado. Edita/olvida hechos sin reentrenar. | Prueba de que "red pequeña separada del LLM" es viable. |
| **ShadowMerge** (arXiv:2605.09033, mayo 2026) | Ataque contra memoria **basada en grafo**: inyecta relación envenenada que comparte ancla/canal con evidencia legítima. 93.8% ASR sobre Mem0. | La defensa es el Coherence Gate en recall — capa 3 (cosim) no depende de patrones, resiste variantes de ShadowMerge que muten el texto. |
| **eTAMP — "Poison Once, Exploit Forever"** (arXiv:2604.02623, abril 2026) | Envenenamiento cruzado sesión/sitio sin acceso directo a la memoria — observación contaminada resurge semánticamente en tareas futuras. 19.5-32.5% ASR en modelos frontera reales. | La defensa es procedencia + gate de coherencia antes de cualquier consumo generativo. |
| **"Hidden in Memory: Sleeper Memory Poisoning"** (arXiv:2605.15338, mayo 2026) | Memorias fabricadas dormidas hasta acumular condiciones específicas ("time-bomb"). 99.8% aceptación en GPT-5.5. | Motiva por qué el gate debe aplicarse en cada recall, no solo al ingerir. |
| **MemLineage** (arXiv:2605.14421, mayo 2026) | Defensa SOTA real: Merkle log + DAG de derivación ponderado, ASR a cero. | Coincide con `owner_scope`/`provenance` + `consensus.rs`; brecha real: nuestro gate no rastrea derivación, solo similitud final (ver §Estado de Implementación). |
| **"RAG Sanitizer" (Leong, 2026)** | Citado dentro de arXiv:2606.30566 como filtro en capa de recuperación. **No verificado contra fuente primaria** — solo cita de segunda mano. | Tratar como referencia orientativa, no como precedente confirmado. |
| **Learned Rerankers** (2026) | Rerankers ultra-compactos (LambdaMART / FFN) entrenados con señal binaria de utilidad del agente. | Este ADR implementa exactamente esto en §4. |

---

## 2. Decisiones

### Decisión 1: Signal Loop de Retroalimentación Implícita (P0)

Se crea una nueva tabla SQLite en SilvaDB para registrar la señal de utilidad de cada recuperación de memoria:

```sql
CREATE TABLE IF NOT EXISTS recall_feedback (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id     TEXT NOT NULL,
    agent_id      TEXT NOT NULL,
    task_hash     TEXT NOT NULL,
    query_text    TEXT NOT NULL,
    rank_position INTEGER NOT NULL,
    useful        INTEGER NOT NULL DEFAULT 0,  -- 0=unknown, 1=useful, -1=not_useful
    accessed_at   TEXT NOT_NULL DEFAULT (datetime('now')),
    UNIQUE(memory_id, task_hash)
);
```

**Población de la señal:**

1. **Señal positiva (useful=1):** Cuando `tylluan_recall` retorna una memoria y en los siguientes `N` pasos del mismo `agent_id`, el contenido de esa memoria aparece referenciado en un `tylluan_do` (detectado por solapamiento de contenido o por hash). Implementación inicial: ventana de 3 turnos de agenda (episodios en AgentMemory).

2. **Señal negativa (useful=-1):** Cuando una memoria recuperada coincide con el `query_text` pero el agente no la referencia en los siguientes `N` pasos, y la sesión continúa. Señal más débil (puede ser falso negativo si el agente ya sabía el dato).

3. **Anotación explícita (post-MVP):** Endpoint `POST /api/v1/recall-feedback` para que el cliente (dashboard, agente externo) marque resultados como útiles/no útiles directamente.

**Integración en el kernel:**
- Se añade `log_recall_feedback(...)` al módulo `handler_recall.rs`, llamado por `handle_tylluan_recall` después de entregar resultados.
- El módulo `handler_do/audit.rs` se extiende para detectar referencias a memorias recientes.
- NightConsolidation ejecuta una nueva fase `FeedbackSignalPhase` que materializa la señal cruda en ejemplos de entrenamiento.

### Decisión 2: Coherence Gate en Recall (P0)

El `ConsensusEngine::verify_synthesis_coherence` existente solo protege la salida de `apply_synthesis`. Se necesita un gate análogo en la ruta de recall antes de que las memorias alimenten a cualquier modelo generativo (ADR-010).

**Implementación:**

```
tylluan_recall(query)
  → search_hybrid(query)
  → CoherenceGate::filter(results, context)  ← NUEVO
  → response(results)
```

```rust
pub struct CoherenceGate {
    coherence_threshold: f32,  // default 0.85, igual que SYNTHESIS_COHERENCE_THRESHOLD
}

impl CoherenceGate {
    /// Filtra resultados que no pasan el control de coherencia.
    /// Un resultado es incoherente si:
    ///   1. Contiene patrones de inyección conocidos (regex list)
    ///   2. Tiene procedencia sospechosa (federation_source != local + trust < threshold)
    ///   3. El embedding del contenido vs. el embedding de la query tiene cosim < threshold
    pub async fn filter(
        results: Vec<(GraphNode, f32)>,
        query: &str,
        query_embedding: Option<&[f32]>,
        embedding_engine: Option<&EmbeddingEngine>,
    ) -> Vec<(GraphNode, f32)>
}
```

**Tres capas de defensa:**

| Capa | Qué detecta | Coste | Implementación |
|------|-------------|-------|---------------|
| 1. Regex de inyección | Payloads conocidos (`[SYSTEM:`, `<\|im_start\|>`, `IGNORE ALL PREVIOUS`) | Sub-μs por nodo | Lista estática en `security/poison_patterns.rs` |
| 2. Verificación de procedencia | Nodos con `federation_source` no confiable + score bajo | Sub-ms | Consulta a metadatos del `GraphNode` |
| 3. Cosim query-contenido | Nodos cuyo embedding de contenido no se alinea semánticamente con la query | ~0ms (reusa embedding ya almacenado, sin inferencia extra) | `SilvaDB::get_node_embedding`, mismo motor BGE-M3 que `search` |

**Comportamiento:**
- Nodos que fallan la capa 1: **eliminados silenciosamente** (riesgo de inyección confirmado).
- Nodos que fallan la capa 2 o 3: **penalizados** (score × 0.1), no eliminados, para evitar falsos positivos.
- Si más del 50% de los resultados son eliminados/penalizados, se añade una advertencia al resultado: `"⚠️ {n} resultados filtrados por control de coherencia"`.

### Decisión 3: Learned LightReranker (P1)

**Arquitectura:**

```
search_hybrid(query)
  → RRF (k=60) → top-K candidatos (K = limit × 4)
  → LightReranker ONNX → score_final
  → sort por score_final → top-N (limit)
```

**Input features (4-dimensional vector por candidato):**
```
x = [
    score_rrf,        // RRF fused score actual (f32)
    score_graph,      // PPR score si skip_graph=false, 0 si no
    recency_score,    // 1.0 / (1 + days_since_last_access)
    agent_affinity,   // 0..1: qué tan útil fue este tipo de nodo para este agent_id históricamente
]
```

**Arquitectura de red:**
- 2 capas densas: `4 → 16 → 1`
- Activación ReLU en capa oculta, sigmoid en salida
- ~100 parámetros entrenables
- Formato ONNX, <10KB en disco
- CPU-entrenable sin GPU (gradiente batch en NightConsolidation)

**Entrenamiento:**
- Frecuencia: cada NightConsolidation (noche)
- Datos: ejemplos de `recall_feedback` con `useful=1` (positivos) y `useful=-1` (negativos)
- Loss: Binary Cross-Entropy
- Optimizador: SGD con learning rate 0.01, batch_size=32
- Early stopping: si loss no mejora en 3 epochs, se conserva el checkpoint anterior
- **Fallback automático:** si el modelo entrenado da score medio < 0.5 en validación, NO se despliega — el sistema sigue usando RRF puro (sin pérdida de calidad)

**Integración en search_hybrid:**
```rust
pub async fn search_hybrid(
    &self,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: usize,
    type_filter: Option<&str>,
    skip_graph: bool,
    reranker: Option<&LightReranker>,  // NUEVO parámetro opcional
) -> Result<Vec<(GraphNode, f32)>>
```

Si `reranker` es `None` o el modelo no pasó validación → RRF puro (comportamiento actual). Si `reranker` es `Some` y pasó validación → RRF produce candidatos, reranker reordena.

**Cutover sin degradación:**
1. Fase 1 (Noche 1): recolectar señal, sin desplegar reranker.
2. Fase 2 (Noche 2): entrenar reranker offline, evaluar contra validación. NO desplegar.
3. Fase 3 (Noche N, cuando validation_accuracy > 0.75): desplegar reranker como "shadow mode" (escribe scores pero no modifica resultados).
4. Fase 4 (Noche N+1, si shadow mode muestra mejora > 5% en useful_rate): activar reranker.

### Decisión 4: Neural Memory Architecture — Visión a Largo Plazo (P3)

No se implementa en este ADR. Se documenta como dirección futura basada en Titans/ATLAS/Larimar:

- **No antes de v0.16.0.**
- Requiere que el Learned LightReranker haya operado en producción ≥3 meses con señal de feedback validada.
- La Neural Memory Architecture reemplazaría el FSRS-5 heurístico con un predictor neuronal de media-vida (Neural FSRS, ~200KB).
- Las micro-redes NO almacenan memoria directamente (SilvaDB sigue siendo ground truth), solo calculan pesos, rutas de prefetch y enlaces latentes.
- Principio de cero olvido catastrófico: si una micro-red falla, el sistema cae a heurísticas deterministas sin pérdida de datos.

---

## 2.5 Estado de Implementación (2026-07-25)

Fase 1, 1b y 2 de §6 ya están implementadas y con tests, no son solo diseño:

- **Signal Loop**: `recall_feedback` (SilvaDB schema v18, `memory/silva/schema.rs`) + `SilvaDB::log_recall_feedback`/`resolve_pending_feedback`/`resolved_feedback_count` (`memory/silva/recall_feedback.rs`, 6 tests). `FeedbackSignalPhase` corre cada NightConsolidation (`memory/night/feedback_signal_phase.rs`), 9º phase del `PhaseOrchestrator` ya paralelo.
- **Coherence Gate**: `security::coherence_gate::CoherenceGate` (3 capas tal como se especifica arriba) + `security::poison_patterns` (lista estática, 10 patrones) — 9 tests (7 + 2). Wireado en **ambos** caminos de `handle_tylluan_recall` (`transport/server/handler_recall.rs`): el camino normal Y el camino de cache-hit — este segundo hallazgo real durante la implementación: el cache de recall guardaba los candidatos **sin gatear**, así que una memoria envenenada que entrara una vez al caché lo habría burlado en cada hit posterior. Corregido antes de mergear, no después.
- **LightReranker (P1, scaffolding solamente, no cutover)**: `router::light_reranker::LightReranker` (mismo patrón de degradación elegante que `mlp::MlpScorer`) + `scripts/train_light_reranker.py`. **Deliberadamente NO** se cambió la firma de `search_hybrid` (17 call sites reales) — con cero modelo entrenado real hoy, tocar una función usada en 17 sitios para un reranker que siempre devuelve `None` es la complejidad prematura que este proyecto evita. `rerank()` es un wrapper aditivo, opt-in, listo para adoptarse cuando Fase 3-4 tengan datos reales. El script de entrenamiento **rehúsa entrenar** por debajo de 5.000 filas resueltas — no produce un modelo silenciosamente sobreajustado.
- **Brecha honesta documentada**: `recall_feedback` no persiste `score_rrf`/`score_graph`/`recency_score` por fila hoy — el script de entrenamiento usa proxies (`1/(60+rank+1)` para RRF, constantes para el resto) hasta que se decida si vale la pena instrumentar `search_hybrid` para exponerlos por candidato. No resuelto en este ADR, dejado explícito para no descubrirse por sorpresa en Fase 3.

Verificado: `cargo test -p tylluan-kernel --lib` bajo `RUSTFLAGS=-D warnings`, 494/494 pasando (17 tests nuevos: 6 signal loop + 7 coherence gate + 2 poison patterns + 2 light reranker). `jaccard_words` (`memory/dream_cycle.rs`) se promovió a `pub(crate)` y se reutilizó tal cual para la resolución de señal, en vez de duplicar la función.

---

## 3. Comparativa de Opciones

### Señal de Feedback

| Opción | Ventajas | Desventajas |
|--------|----------|-------------|
| **A. Señal implícita (elegida)** | Sin fricción de UX. Datos abundantes. Escala con el uso. | Señal ruidosa (falsos negativos). Complejidad de detección de referencias. |
| B. Señal explícita (post-MVP) | Señal de alta calidad. Bajo ruido. | Requiere UI/API. Baja densidad de datos. |
| C. Hibrida (A + B) | Lo mejor de ambos. | Más código. Dos pipelines de señal. |

### Coherence Gate

| Opción | Ventajas | Desventajas |
|--------|----------|-------------|
| **A. 3 capas en recall path (elegida)** | Cubre ShadowMerge + Sleeper Poisoning. Degradación graceful. | Coste ONNX en capa 3 (~8ms). |
| B. Solo regex + procedencia | Más rápido (<1ms). | No detecta inyección semántica avanzada. |
| C. Gate en search_hybrid (no recall) | Protege todas las salidas de búsqueda. | Penaliza búsquedas no-generativas (dashboard). |

### Learned Reranker

| Opción | Ventajas | Desventajas |
|--------|----------|-------------|
| **A. FFN 4→16→1 con fallback (elegida)** | <10KB. Entrenable en CPU. Fallback automático. | Solo mejora ranking, no recall. |
| B. Reemplazar RRF completamente | Control total del ranking. | Sin RRF como baseline = sin punto de comparación. |
| C. Usar Jina cross-encoder existente | Ya implementado (`search_hybrid_reranked`). | Dependencia externa. No entrenable con datos locales. |

---

## 4. Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|-------------|---------|------------|
| Señal de feedback demasiado ruidosa para entrenar | Media | Alto — reranker nunca se activa | Fase 1 larga (2 semanas de recolección). Umbral de activación conservador (accuracy > 0.75 + mejora > 5%). |
| Coherence Gate elimina memorias legítimas | Media | Medio — falsos positivos de inyección | Capa 1 elimina silenciosamente (solo patrones claros). Capas 2-3 penalizan, no eliminan. Advertencia al usuario si >50% filtrado. |
| Reranker sobreentrenado a patrones locales | Baja | Bajo — no se despliega si no pasa validación | Validación cruzada temporal (train = días 1-14, test = día 15, no overlap). |
| ShadowMerge muta para evadir regex de capa 1 | Media | Alto — inyección no detectada | Capa 3 (cosim) no depende de patrones. Actualización periódica de lista de patrones desde literatura. |
| Coste ONNX adicional afecta latencia de recall | Baja | Medio | Coherence Gate capa 3 es opt-in (solo si embedding_engine presente). Reranker <10KB = latencia despreciable. |

---

## 5. Límites y No-Objetivos

- **NO** se entrena un modelo de lenguaje. El reranker es una FFN de ~100 parámetros, no un transformer.
- **NO** se reemplaza RRF. RRF sigue siendo el baseline. El reranker es una mejora opcional.
- **NO** se implementa Titans/ATLAS ahora. Quedan como visión para v0.16.0+.
- **NO** se modifica el contrato de `tylluan_recall` (CONTRACT-01: 5 herramientas exactas).
- **NO** hay dependencias cloud. ONNX Runtime + CPU training = local y soberano.

---

## 6. Roadmap de Implementación

| Fase | Hito | Dependencias | Estado |
|------|------|-------------|--------|
| **Fase 1** | `recall_feedback` table + signal population | Ninguna | ✅ Implementado — `memory/silva/schema.rs` v18, `memory/silva/recall_feedback.rs` |
| **Fase 1b** | FeedbackSignalPhase en NightConsolidation | Fase 1 | ✅ Implementado — `memory/night/feedback_signal_phase.rs` |
| **Fase 2** | CoherenceGate (capas 1-2: regex + procedencia) | Ninguna | ✅ Implementado — `security/coherence_gate.rs`, `security/poison_patterns.rs` |
| **Fase 2b** | CoherenceGate (capa 3: cosim) | ~~ADR-010 (embedding_engine)~~ evitada: reusa embeddings ya almacenados (`get_node_embedding`), cero inferencia extra | ✅ Implementado, sin la dependencia originalmente prevista |
| **Fase 3** | LightReranker: entrenador + ONNX export | Fase 1 (datos de feedback) | 🟡 Scaffold listo (`router/light_reranker.rs`, `scripts/train_light_reranker.py`) — script rehúsa entrenar bajo 5.000 filas, sin datos reales aún |
| **Fase 4** | LightReranker: integración en search_hybrid | Fase 3 | ⬜ No iniciado — deliberadamente, sin modelo real no se toca una función con 17 call sites |
| **Fase 5** | Shadow mode + cutover automático | Fase 4 | ⬜ No iniciado |
| **Visión** | Neural FSRS (Titans-inspired) | ≥3 meses de señal validada | ⬜ Fuera de alcance de este ADR (v0.16.0+) |

---

## 7. Referencias

1. Titans: Google Research, arXiv:2501.00663, 2025
2. ATLAS: arXiv:2505.23735, 2025/2026
3. Larimar: IBM Research, ICML 2024, arXiv:2403.11901
4. ShadowMerge / Sleeper Poisoning: literatura de seguridad agéntica 2026
5. eTAMP: Environment-injected Trajectory-based Agent Memory Poisoning, 2025/2026
6. MemLineage / RAG Sanitizer: defensas SOTA 2026
7. ADR-010: Embedded SLLMs — T5-Small vs. SmolLM2 (2026-07-25)
8. ADR-009: Agents Declarative Contract (2026-07-25)
9. Código existente: `crates/tylluan-kernel/src/security/integration_tests.rs` — `adv_memory_poisoning_recall_returns_inert`
10. Código existente: `crates/tylluan-kernel/src/memory/consensus.rs` — `verify_synthesis_coherence`
11. Código existente: `crates/tylluan-kernel/src/memory/silva/search.rs` — `search_hybrid` RRF implementation
