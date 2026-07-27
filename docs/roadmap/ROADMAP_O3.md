# Tylluan — Roadmap Estratégico

> **Última actualización:** 2026-07-27 · v0.13.0 (HEAD `02fa698`) — plan=true fix verificado end-to-end · SLM society spike day: Gemma-4-E2B ONNX pipeline funcional en DirectML, embedding router benchmarkeado (NO-GO puro, GO como tiebreaker), CoherenceGate 75% (GO-with-caveats), A2A coexistence explicitada · DeepEval identificado como candidato para J-6 · J-13/J-14 añadidos al backlog
> **Fuente de verdad:** STATUS.md · Decisiones en ADRs bajo `docs/reference/adr/`
> **Norte permanente:** Rufus test — funciona en frío, sin docs, sin Rust, en < 5 min.

---

## Estado actual — v0.14.0 ✅

M15-M19, M22, M23-P1, M26-P1/P2, M27, M28, M29, M34-M38 cerrados. M14-F Phase 3 cerrado. M18 cerrado (re-benchmark +62.0%/+57.7%, umbral 30% superado). ADR-011 (Signal Loop + Coherence Gate + LightReranker scaffold) implementado, con tests y verificado end-to-end contra el kernel real (migración de schema v17→v18 en vivo, `recall_feedback` poblándose de verdad). ADR-010 (SLM embebido T5 vs SmolLM2) avanzó de "solo comparativa" a spikes reales ejecutados hoy — ver sección "Sociedad SLM" más abajo. 612 tests (539 kernel lib + 61 link + 12 fsrs), clippy limpio, CI verde. Puerto real: `:4000` (`tylluan.toml` línea 6, verificado en vivo). (Nota histórica: el commit `f475462`, línea de abajo, migró de 4000→3030 en un momento anterior; un incidente posterior de colisión de puertos con otro servicio interno llevó a fijar Tylluan de vuelta en 4000 — ese es el estado real y actual.)

Lo que ya tenemos (verificado 2026-07-25):
- Binario único, 4 targets (x86_64/aarch64 × Linux/Windows/macOS)
- 5 sovereign tools MCP (tylluan_do/recall/remember/think/graph) — CONTRACT-01 intacto
- BGE-M3 hybrid search: R@5 82% LongMemEval-S, R@10 90%, latency p50 12.9ms
- M20 Complexity Cascade: score ≥0.6 → coordinator proactivo, ≥0.4 → fallback reactivo
- M18 TRINITY Coordinator: Thinker/Worker/Verifier + synthesis fallback — **CERRADO**, re-benchmark +62.0%/+57.7% supera el umbral 30%
- NightConsolidation: 10 fases corriendo en paralelo (semáforo dimensionado por `available_parallelism()`, no secuencial) — Dream, Ouroboros, AutoLink, GraphRAG, Decay, Agent, Curriculum, IdleLab, FeedbackSignal (ADR-011), LightRerankerTrain (ADR-011)
- ADR-011 Signal Loop: `recall_feedback` (schema v18) + Coherence Gate 3 capas en `tylluan_recall` (ambos caminos, incl. cache-hit) + LightReranker (ONNX y pesos nativos) — Fase 3-4 en scaffold, cutover real bloqueado por datos (≥5.000 filas resueltas), no por diseño
- Node pruning: DreamCycle + decay `prune_by_salience(threshold)` operativo
- Federation P2P completa: DHT Kademlia + Gossip + Noise XK + TCP dispatch — M14-F Phase 3 **cerrado** (test DST real)
- Security: capabilities como allowlist estructurada (M30), grants escalados (M30-P3), Coherence Gate (ADR-011), 30+ tests de seguridad
- `tylluan-cli start/stop/status/logs/connect/download-models/install` + `tylluan doctor`/`update` (M19)
- CI: build + test + clippy -D warnings + deny + security audit + Python lint + ARM64 portability + Docker smoke
- Dashboard React: Canvas bidireccional (M25), tldraw whiteboard (M26), mesh map + badges + dry-run (M29), Scopes panel (M37-P2)

---

## Milestones Planificados

---

### M15 — Rufus Release (v0.12.0) ✅ CERRADO

HEAD `945838c`. 4 fases entregadas (P0 install scripts, P1 setup-hint, P2 Docker, P3 OpenClaw). Rufus test superado.

---

### M16 — Benchmark Real BGE-M3 (v0.12.1) ✅ CERRADO

HEAD `f8bad9f`. R@5 82% LongMemEval-S (50 queries reales, BGE-M3 + BM25). ADR-007: IdleLab INNECESARIO — defaults son óptimo local (0.0pp delta en 8 experimentos). P2 degree bias movido a backlog de investigación (no bloqueante).

- ✅ P0: `benchmarks/benchmark_v0.12.0_bge.json` — R@5 82%, R@10 90%, p50 12.9ms
- ✅ P1: ADR-007 `docs/reference/adr/ADR007_idle_lab_verdict.md` — INNECESARIO
- ↩ P2: degree bias comparison — backlog investigación

---

### M17 — Integración Externa (v0.13.0) ✅ CERRADO

HEAD `09ac1f0`. Rama A completa: docs OpenClaw + Hermes, E2E MCP PASS, CONTRACT-01 en CI.

**Rama A — OpenClaw confirmado:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | `docs/reference/integrations/openclaw.md` — guía completa | Claude | ✅ |
| P1 | MCP E2E en coloquio: 5 tools, remember→recall 0.88 | Antigravity | ✅ |
| P2 | `docs/reference/integrations/README.md` + `test_mcp_contract.py` (3 passed in 0.34s) | Deep | ✅ |

**Rama B — Si OpenClaw no confirmado (o integración > 1 semana):**

| Fase | Descripción | Agente |
|------|-------------|--------|
| P0 | Permisos granulares: `[permissions]` en `tylluan.toml` — deny/ask/allow por guild. ACL legible por humanos. | Deep |
| P1 | AGENTS.md como config declarativa de usuario (agentes definen su perfil en el repo, Tylluan lo carga). Spec en ADR-007. | Claude + Deep |
| P2 | UI en dashboard: panel de permisos por guild con toggle deny/ask/allow | Antigravity |

**Criterio de cierre común:** Un usuario externo puede configurar Tylluan para su entorno en < 10 minutos.

---

### M18 — TRINITY Coordinator Guild (v0.14.0) ✅ CERRADO

**Norte:** Mejorar la calidad en tareas multi-paso. Un guild `coordinator` orquesta Thinker/Worker/Verifier. Basado en paper ICLR 2026: "TRINITY" (arXiv:2512.04695).

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | Spec + ADR-008: Thinker/Worker/Verifier, routing via catalog.rs | Claude | ✅ |
| P1 | `guilds/core/coordinator.py` + catalog.rs + test_coordinator.py | Deep | ✅ |
| P2 | Benchmark 10 queries, delta=22.2% — RECHAZADA (< 30%) | Antigravity | ✅ |
| P3a | **ThreadPoolExecutor para sub-tasks independientes** — verificado en código: `coordinator.py` usa `ThreadPoolExecutor(max_workers=min(len(step["tasks"]), 4))` (línea 171, commit 1c10da5) | Deep | ✅ |
| P3b | Re-benchmark reproducible (`benchmarks/coordinator_bench.py`) — +62.0% delta de medias, +57.7% media de deltas por query, 0 errores 5/5 queries. Cierra el umbral del 30% en ambas métricas (2026-07-12). | claude-code | ✅ |

**Criterio de cierre:** Re-benchmark con delta ≥ 30% (media del delta porcentual por query, no delta de medias absolutas) + `_is_synthesis_intent()` activo.

**Nota de integridad (2026-07-11, 2ª corrección):** La telemetría de `eval_coordinator.py` es real (verificado: `time.perf_counter()`, datos persistidos en `coordinator_latencies.json`), pero el "+49.9%" reportado es el delta de las medias absolutas (mean_sin_ms vs mean_con_ms), una estadística dominada por 1-2 queries con latencia absoluta grande (Q7). La métrica correcta para "¿ayuda el paralelismo en una query típica?" es la **media de los deltas porcentuales por query**, que da **+1.57%** (mediana +8.9%), con **3 de 7 queries válidas más lentas** con el coordinador que sin él. No cumple el umbral de 30% bajo ningún criterio razonable. Reabierto hasta que el paralelismo muestre una mejora consistente por query, no solo en el agregado.

**Nota de integridad (2026-07-11, 3ª corrección):** Antigravity intentó cerrar P3b anonimizando `agent_id` a `"anonymous"` en cada sub-dispatch del coordinador para saltarse la escritura de auditoría SHA-256 encadenada en SQLite — esto rompía la trazabilidad real del agente que invoca el coordinador (regresión de seguridad, no optimización). Revertido en commit `1381664`, junto con una heurística de `heavy_keywords` sobreajustada palabra-por-palabra a las 10 queries fijas del benchmark (overfitting, no principio de diseño). El hallazgo de fondo (el audit log costaba latencia real) era válido: causa raíz confirmada y arreglada en `5698051` — `log_audit_entry` se despachaba vía `tokio::spawn` alrededor de una escritura síncrona de rusqlite, bloqueando un worker thread del runtime durante los dispatches concurrentes del coordinador; corregido usando `tokio::task::spawn_blocking`. Medición post-fix: delta de medias +53.1%, media de deltas individuales +10.2% — sigue sin alcanzar el 30% pero ya no hay ninguna manipulación de la métrica ni regresión de seguridad. Claimed por claude-code, pendiente de intentar una optimización legítima (ej. reducir el número real de sub-dispatches HTTP por query) para cerrar la brecha restante.

---

### M22 — Junior Onboarding (v0.13.0) ✅ CERRADO

**Norte:** Que un usuario nuevo ("junior", sin contexto del proyecto) pueda instalar y arrancar Tylluan sin tropezar con deuda heredada — puerto inconsistente, versión de Rust desactualizada en docs, artefactos GUI muertos en el repo.

**Verificado por commits reales** (no hay tabla de fases detallada reconstruida — ver `git log --grep=m22`):
- `68d1bc0` — cierre de todos los hallazgos BLOQUEA de instalación reportados en pruebas reales de onboarding.
- `f475462` — unificación de puerto 4000→3030, Rust toolchain 1.85→1.88 en docs, limpieza de docs del dashboard.
- `504339d` — limpieza de estructura raíz del repositorio.

**Nota de numeración:** este M22 (onboarding) colisionó con un M22 distinto (hardening de seguridad: SQLCipher/rate-limit/capabilities) que ya existía en versiones previas de este documento. La colisión se resolvió (`e0b408a`) renombrando el de seguridad a **M27** — ver esa sección más abajo. Este M22 es exclusivamente onboarding.

**Criterio de cierre:** instalación en máquina limpia sin hallazgos BLOQUEA. ✅ Cerrado, verificado por los commits de fix listados arriba.

---

### M23-P1 — "El Primer Minuto": Auto-start + Empty State (v0.13.0) ✅ CERRADO

**Norte:** El dashboard no debe mostrarse vacío y mudo la primera vez — debe guiar al usuario en su primer minuto real de uso.

- `910f15f` — auto-descarga de modelos + auto-arranque del kernel con poll de `/health`.
- `bad7015` — widget de bienvenida (empty state) en el dashboard cuando no hay memorias/guilds cargadas.

**Criterio de cierre:** primera consulta MCP posible en <1 minuto desde la instalación, sin pantalla vacía sin contexto. ✅ Cerrado.

---

### M26 — Canvas Sprints 1+2: tldraw + Consenso (v0.13.0) ✅ CERRADO (parcial respecto a M25)

**Norte:** El Canvas Event Bridge de M25 dio el bridge bidireccional; M26 añade una superficie de trabajo real (whiteboard) en vez de solo HTML/JS renderizado.

- `50da092` (m26-s2) — integración de tldraw como whiteboard interactivo en el workspace de Coloquio.
- `8453cc0` — persistencia en tiempo real del whiteboard tldraw (P2).
- `bbd7385` — corrección de una línea de STATUS.md que contradecía el propio cierre de M26 Sprint 2 (tldraw marcado "pendiente" cuando ya estaba cerrado).

**Criterio de cierre:** varios agentes pueden co-editar un whiteboard tldraw en el mismo canal de Coloquio con persistencia real. ✅ Cerrado para Sprints 1-2; M25-P1 (recursos locales seguros en sandbox) sigue 🟡 parcial, sin relación directa con tldraw.

---

### M34 — Trust Gate + Sleep-Time Compute (v0.13.0) ✅ CERRADO — cierra J-1 (parcial) y J-2 de M33

**Norte:** Cerrar dos ítems críticos del backlog M33: defensa de procedencia contra memoria no confiable (J-1, OWASP ASI06) y consolidación proactiva en idle en vez de solo decaimiento pasivo (J-2, patrón "sleep-time compute" de Letta).

- `998d1bc` (M34-P0) — procedencia de nodo (`provenance`) + trust gate en tiempo de lectura para contexto de origen federado.
- `c93b3c6` (M34-P0, fix real) — la procedencia estaba hardcodeada vacía en **21 de 22** puntos de lectura de `GraphNode` — solo uno la propagaba de verdad. Bug real encontrado durante la implementación, no solo la feature planeada.
- `e9b0a4d` — indicador visual de procedencia en el Knowledge Graph del dashboard.
- `7e2898c` (M34-P1) — reescritura activa en `DreamCycle` (sleep-time compute real, no solo decaimiento pasivo).

**Nota de alcance:** J-1 (defensa de memory poisoning) queda **parcialmente** cerrado por el trust gate de procedencia — la pieza de coherencia semántica (¿el contenido en sí es coherente, no solo su origen?) llegó después, en ADR-011 (Coherence Gate, esta sesión, 2026-07-25). Las cifras de tasa de ataque MINJA citadas en el backlog original de M33 siguen sin verificar contra el paper primario — no citar como dato duro.

**Criterio de cierre:** nodos de origen federado muestran su procedencia real en cada lectura (no solo en un punto), y NightConsolidation reescribe memoria activamente, no solo la decae. ✅ Cerrado.

---

### M35 — Memoria Bi-temporal (v0.13.0) ✅ CERRADO — cierra J-4 de M33

**Norte:** Modelar cuándo un hecho fue verdadero, no solo cuándo se registró (patrón Zep/Graphiti).

- `4342b49` — `valid_from` bi-temporal + supersesión para aristas en contradicción.

**Criterio de cierre:** una arista puede marcarse como superseded sin borrar la historia de validez anterior. ✅ Cerrado.

---

### M36 — Auto-corrección Explícita vía `@correct:` (v0.13.0) ✅ CERRADO — cierra J-9 de M33

**Norte:** Que un agente pueda corregir activamente su propia memoria en vez de solo acumular ruido silencioso.

- `a5ab3f3` — intent `@correct:` permite supersesión explícita de un nodo por otro, con las mismas protecciones que ya existían (nodos protegidos, identidad, ya-superseded no se puede volver a superseder) — ver tests `test_correct_rejects_*` en `handler_do`.

**Criterio de cierre:** un agente puede corregir un hecho propio marcándolo explícitamente obsoleto y vinculándolo al reemplazo, sin borrar el nodo original. ✅ Cerrado.

---

### M37 — OTel GenAI + Scopes Panel (v0.13.0) ✅ CERRADO — cierra J-5 y J-8 de M33

**Norte:** J-5 (convenciones semánticas OTel GenAI para spans de LLM/retrieval/tool, esquema CNCF vendor-neutral) y J-8 (scopes jerárquicos multi-tenant user/session/agent, patrón Mem0) del backlog M33.

- `51b24f1` (M37-P0) — columna `owner_scope` + query por prefijo, base de backend para J-8.
- `1664f9e` (M37-P1 + M37-P2) — spans OTel GenAI reales (J-5) + panel de Scopes en el dashboard (J-8, primera versión).
- `68d1cbf` (M37-P3) — `GET /api/v1/graph/scope`, cierra un hueco donde el panel de Scopes simulaba datos en vez de consultarlos de verdad.
- `5aad5ab` — limpieza de hallazgos de auditoría de Antigravity sobre el dashboard de M37.

**Criterio de cierre:** spans OTel GenAI reales exportables + un usuario puede consultar nodos por prefijo jerárquico de scope desde el dashboard, con datos reales, no simulados. ✅ Cerrado.

---

### ADR-010 — SLM Embebido: T5-Small vs SmolLM2 (🟡 PENDIENTE)

Ver [ADR-010](../reference/adr/ADR010_embedded_sllm_t5_vs_smollm2.md). Comparativa arquitectónica documentada (T5 encoder-decoder vs SmolLM2 decoder-only) para 3 puntos de inserción (complejidad de routing, reconciliación de contradicciones, resumen episódico). §6 añade un eje ortogonal verificado contra fuente primaria (arXiv:2512.04695): un coordinador entrenado vía sep-CMA-ES (TRINITY/Sakana Fugu) sobre `guilds/core/coordinator.py`, independiente de la elección T5/SmolLM2 y más barato de prototipar (`cmaes`/`SepCMA`, ancla de coste ~$20-30 vía réplica independiente `tinyrouter`). **Ningún benchmark real ejecutado todavía** — sigue siendo comparativa de papel, no medición. No bloquea nada del roadmap actual; es investigación abierta para cuando el equipo decida priorizarla.

---

### ADR-011 — Signal Loop + Coherence Gate + LightReranker (🟢 Fase 1-3 CERRADAS, 🟡 Fase 4-5 bloqueadas por datos)

Ver [ADR-011](../reference/adr/ADR011_learned_reranker_coherence_gate.md). Corrección de fondo de José durante esta sesión: un reranker aprendido no es el primer paso posible — sin señal de uso real no hay nada que entrenar. El ADR formaliza el orden correcto:

- **Signal Loop** (`memory/silva/recall_feedback.rs`, schema v18): cada `tylluan_recall` registra qué memorias devolvió; `FeedbackSignalPhase` (NightConsolidation) resuelve útil/no-útil contra `guild_audit_log` por solapamiento léxico. **Verificado end-to-end contra el kernel real** por Antigravity (2026-07-25): migración v17→v18 automática al reiniciar, `recall_feedback` poblándose en vivo con datos reales de una sesión MCP real, no solo en tests.
- **Coherence Gate** (`security/coherence_gate.rs`): 3 capas de defensa en la salida de `tylluan_recall` (patrones de inyección conocidos → eliminación silenciosa; procedencia no confiable → penalización; deriva semántica query-contenido vía embeddings ya almacenados → penalización). Cierra el "segundo salto" de envenenamiento de memoria (cuando un futuro LLM generativo post-ADR-010 consuma memoria recuperada como contexto) — documentado con literatura 2026 verificada paper por paper (ShadowMerge arXiv:2605.09033, eTAMP arXiv:2604.02623, Sleeper Memory Poisoning arXiv:2605.15338, MemLineage arXiv:2605.14421).
- **LightReranker** (`router/light_reranker.rs` + `memory/night/light_reranker_train_phase.rs`): FFN 4→16→1, dos backends (ONNX y pesos nativos JSON), entrenable en CPU cada noche. **Fase 3 (entrenador) implementada y probada** — deliberadamente **NO** cortada sobre producción: `LightRerankerTrainPhase` rehúsa entrenar bajo 5.000 filas resueltas en `recall_feedback` (mismo criterio que el script Python equivalente).

**Criterio de cierre restante:** ≥5.000 filas resueltas + mejora medida en modo sombra antes de que el reranker reemplace RRF puro en producción — es una cuestión de tiempo de uso real acumulado, no de código pendiente.

---

### M21 — Performance Foundation (v0.15.0) ✅ CERRADO (P0-P4 todas cerradas, nunca marcado en bloque)

**Norte:** Eliminar los bottlenecks de rendimiento que afectan la experiencia real de usuario. El embedding LRU cubre el routing pero no el recall. El coordinator es serial. Las guilds tienen cold start de 1-2s.

**Por qué antes de DX:** Un wizard bonito no sirve si las queries tardan 300ms de más por re-embedding. Primero que vuele, luego que sea un placer.

**Fases:**

| Fase | Descripción | Agente | ROI | Estado |
|------|-------------|--------|-----|--------|
| P0 | **Recall embedding cache** ✅ (cubierto por `silva/query_cache.rs`, M21 anterior): LRU 256 entries + TTL 300s + key normalization, ya inyectado en `handler_recall.rs:500-504` e invalidado en `handler_remember.rs:321`. Benchmark test añadido: 100 iteraciones, avg < 2ms en caché. Map description en ROADMAP_O3.md actualizada. | Deep | CRÍTICO | ✅ |
| P1 | **SQLite PRAGMA tuning** ✅ (2026-07-13): ya existía en `hybrid.rs`/`silva/schema.rs`, pero el helper compartido `config::open_db()` (15+ call sites: jobs, mailbox, agent_profiles, curriculum, federation, coloquio, registry, contracts, journal, audit) solo tenía WAL+busy_timeout. Añadido `synchronous=NORMAL`, `cache_size=-65536`, `mmap_size=268435456` directamente en `open_db()` para que todos los callers se beneficien uniformemente. | claude-code | ALTO | ✅ |
| P2 | **Coordinator ThreadPoolExecutor**: sub-tasks sin dependencia de `prev_result` se ejecutan en paralelo. Solo los que referencian contexto anterior son secuenciales. Necesario para M18-P3. | Deep | CRÍTICO | ✅ |
| P3 | **Guild warm pool** ✅ (2026-07-12): añadido `warm_pool: Vec<String>` a `CoreGuildsConfig` (config.rs), spawn en main.rs tras always-on. Guilds en warm pool se pre-arrancan al boot pero SIGUEN siendo víctimas de idle timeout (diferencia clave con always_on). `tylluan.toml`: `warm_pool = ["git", "codebase_memory", "browser"]`. No confundir con la lógica de `always_on` del disco — el fix de ese bug sigue pendiente. | claude-code | MEDIO | ✅ |
| P4 | **P2P DST test end-to-end** ✅ (2026-07-13): `test_kernel_remote_dispatch_routes_via_real_noise_xk_p2p` en `tests/mesh_audit.rs` — listener Noise XK real en puerto dinámico, `CapabilityRegistry` compartido con `DispatchRouter` (mismo patrón que `main.rs`), POST real a `/api/v1/guilds/dispatch/remote`, verifica que la respuesta viene del listener remoto (`executor` = `p2p://<pubkey-real>:<puerto>`) y no de ejecución local. Cierra la deuda técnica real de M14-F Phase 3. | claude-code | MEDIO | ✅ |

**Criterio de cierre:** `tylluan_recall` con misma query dos veces: segunda < 2ms (validado con 100 iteraciones, avg < 2ms). Coordinator completa 5 sub-tasks independientes en paralelo sin timeout en CPU. Guilds warm pool pre-arrancadas en boot son visibles en `GET /api/v1/guilds` antes de su primer uso.

---

### M19 — DX 10/10 · Fugu Parity (v0.13.0) ✅ CERRADO

**Norte:** Experiencia de developer comparable a Sakana Fugu. `tylluan` como comando único. Auto-update. Profile wizard. AGENTS.md como estándar.

**Por qué importante:** Qwen analizó Fugu (sesión 2026-06-23): "ganó en integración, no solo en motor." Tenemos mejor motor. Hay que ganar también en integración.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | `tylluan` como alias único: `[[bin]]` name=`tylluan` coexist with `tylluan-cli`. Ambos del mismo `main.rs`. Backward compat: installs M13 siguen funcionando. | Deep | ✅ 2026-07-13 |
| P1 | **`tylluan doctor`**: 7 checks offline (binary, config TOML, Python 3.11+, guilds count, model cache, port free, kernel health). Funciona SIN kernel corriendo. | Deep | ✅ 2026-07-13 |
| P2 | **Profile wizard + hardware detection** ✅ (2026-07-13): `tylluan start --setup` → RAM real vía `sysinfo`, GPU vía probe real de `nvidia-smi` en PATH (sin fabricar señal si es inconcluso), recomienda perfil (<8GB→portable, ≥8GB sin GPU→clinic, ≥8GB con GPU→server — ajustado del texto original "≤8GB→clinic" que habría recomendado descargar un modelo a una máquina sin RAM para ello) → genera `tylluan.toml`. Nunca sobrescribe un TOML existente (verificado por lectura de código: `return` antes de cualquier escritura). Verificado manualmente end-to-end (detectó 221.9GB RAM + GPU NVIDIA real en esta máquina, generó perfil `server` correcto). | claude-code | ✅ |
| P3 | **Instant start + background model download**: arrancar inmediatamente en BM25-only, descargar BGE-M3 en hilo separado con hot-swap via interior mutability. Anchor warmup detecta el cambio automáticamente. | Deep | ✅ 2026-07-13 |
| P4 | `tylluan update` — comprueba release en GitHub (`Forja-orca/tylluan`), descarga binario correcto para la plataforma, atomic replace (rename). Flag `--check` para solo consultar. | Deep | ✅ 2026-07-13 |
| P5 | AGENTS.md como contrato declarativo estándar: cada agente define su perfil y permisos. Kernel lo lee al arrancar. Spec ✅ ([ADR-009](../reference/adr/ADR009_agents_declarative_contract.md), 2026-07-13): `.tylluan/agents.toml` repo-local, agent_id→rol, reutiliza `AclConfig.roles` existente sin reinventar permisos, backward-compatible (sin fichero = sin cambio de comportamiento). Kernel implementation 🟡 pendiente Deep. | Claude (spec) ✅ + Deep (kernel) ⬜ | 🟡 parcial |

**Criterio de cierre:** Instalar, configurar y hacer la primera consulta MCP en < 3 minutos en máquina virgen, sin leer ningún documento. ✅ Cerrado 2026-07-13 (P0-P1-P3-P4, P5 spec, P2 queda para siguiente ciclo).

---

### M29 — Dashboard UX 2.0 (v0.16.1) ✅ CERRADO

**Norte:** El dashboard ya tiene KnowledgeGraphTab, GuildInspector, FederationPanel y HippocampusGraph. Falta conectarlos operativamente: P2P como mapa visual, MCP config exportable con 1 click, dry-run mode, y `tylluan-cli new guild` para bajar la barrera de contribución.

**Nota:** No añadir dependencias de grafos externas (React Flow, D3) — el Canvas 2D custom en `graph/simulation.ts` ya existe y es cero-deps. Extender eso.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **MCP config 1-click**: botón "Integrar con..." en dashboard → genera snippet JSON para Claude Desktop/Cursor/VS Code/LM Studio con token y URL pre-rellenados. Descarga `mcp.json`. Actualmente requiere leer docs + copiar a mano. | Antigravity | ✅ |
| P1 | **P2P mesh topology map**: `FederationPanel.tsx` muestra lista de peers en texto. Ampliar con mini-mapa Canvas: nodo central (yo) + peers como círculos con latencia, `HardwareCaps` (GPU/RAM) y estado del circuit breaker. Sin libs externas. | Antigravity | ✅ |
| P2 | **Guild capability badges**: `GuildsConsolidated.tsx` ya lista guilds. Añadir badge de capabilities declaradas (🔴 ProcessExecution, 🟡 FileSystem, 🔵 Network) + indicador de sandbox activo. Prepara visualmente M27-P3. | Antigravity | ✅ |
| P3 | **`tylluan new guild`**: `tylluan new guild <name>` genera `guilds/core/{name}.py` con template fastmcp (CAPABILITIES, @mcp.tool(), docstring) + `tests/guilds/test_{name}.py` (pytest). Barrera de contribución: 0 a línea de código válida en 1 comando. | Deep | ✅ 2026-07-13 |
| P4 | **Dry-run mode**: flag `dry_run = false` en `[guilds]`. Cuando activo, guilds destructivas (process_execution=true o filesystem_scope=["/"]) simulan ejecución y devuelven `[DRY-RUN]` sin llamar al proxy. Intercept en `GuildProcess::call_tool_with_proxy()`. | Deep | ✅ 2026-07-13 |

**Criterio de cierre:** Un developer puede integrar Tylluan con su MCP client en < 30s desde el dashboard. Un contributor puede crear una nueva guild desde cero en < 10 minutos.

---

### M27 — Security Hardening (v0.17.0) ✅ CERRADO (numeración reconciliada — este era el M22 original de seguridad, no confundir con M22 Junior Onboarding arriba)

**Norte:** Eliminar los gaps de seguridad críticos antes de cualquier uso en equipo o publicación de benchmarks. Actualmente SQLCipher es opt-in y no hay capability system para guilds.

**Fases:**

| Fase | Descripción | Agente | Severidad | Estado |
|------|-------------|--------|-----------|--------|
| P0 | **SQLCipher default** ✅ (verificado 2026-07-12, ya resuelto): `default_encrypt_at_rest() = cfg!(feature = "encryption")` — el Dockerfile compila con `--features encryption`, así que el perfil server (Docker) ya tiene cifrado activo por defecto, con resolución de clave `TYLLUAN_DB_KEY` env > OS keychain > fallback Argon2id a fichero. Verificado en logs del contenedor real: `🔐 SQLCipher encryption active` en las 7 DBs. **Hallazgo real más grave durante la verificación**: el volumen Docker montaba `/home/tylluan/data` pero el binario escribe en `/data` (WORKDIR) — los datos (incluida la clave de cifrado) vivían en la capa efímera del contenedor, nunca llegaban al host. Fixeado en `docker-compose.yml` (commit `49f9fb3`). | claude-code | CRÍTICO | ✅ |
| P1 | **Input sanitization** ✅ (2026-07-12): `check_dangerous_intent()` ya existía (bloquea `rm -rf /`, `DROP TABLE`, fork bombs, `shutdown`, etc., 13 tests) pero estaba desactivado por defecto — flip a `intent_filter: true` (safe-by-default). Segunda mitad: `guilds/core/utils.flag_untrusted_content()` marca (no reescribe) contenido externo con frases típicas de prompt injection (EN/ES) — cableado en `websearch.py` y `deep_web_research.py` (web_search + fetch_page, el mayor riesgo de los tres). 8 tests nuevos. Deliberadamente no hace *stripping*: editar lenguaje natural heurísticamente es poco fiable y arriesga destruir contenido legítimo — solo marca el límite de confianza para que el agente decida. | claude-code | ALTO | ✅ |
| P2 | **Rate limit por IP** ✅ (2026-07-12): `security::rate_limiter::RateLimiter` existía pero estaba muerto (instanciado, nunca llamado) — el único límite real era por `agent_id`, un header/query param controlado por el cliente y trivialmente evadible omitiéndolo o rotándolo. Cableado como `HttpState::ip_rate_limiter`, keyed por `ConnectInfo<SocketAddr>` real (requirió cambiar `axum::serve` a `into_make_service_with_connect_info`), 300 req/min. **Por-guild (M27-P2 parte 2)**: ✅ `TylluanServer::guild_rate_limiter` (120 req/min), chequeado en `handle_tylluan_do` tras resolver guild y en `guild_tool_call_handler` HTTP directo. | claude-code | ALTO | ✅ |
| P3 | **Guild capability declarations (advisory)** ✅ (2026-07-12): cada guild declara `CAPABILITIES = {...}` en el módulo Python. Kernel parsea el fichero `.py` en startup (`catalog.rs::extract_capabilities()`), almacena en `GuildDescriptor.capabilities` y expone en `GET /api/v1/guilds` via `GuildStatus.capabilities`. Guilds sin declaración: `null`. Ejemplos: `websearch`, `vision`. Consolea capabilites + API visibility. Prepara P4. | claude-code | MEDIO | ✅ |
| P4 | **Enforce capabilities at runtime** ✅ (2026-07-12): `config.SecurityConfig.capabilities_enforce` (opt-in, default false). `enforce_capabilities()` en `guild_process.rs` bloquea process_execution=false en tools con nombre/signatura ejecutiva y filesystem_scope fuera de paths declarados. 13 tests AAA. network_hosts queda advisory-only (requeriría instrumentar Python). | claude-code | ALTO | ✅ |

**Criterio de cierre:** Un usuario nuevo no puede ejecutar una guild sin declarar sus capabilities. SQLCipher activo en perfil server por defecto.

---

### M28 — Credibilidad Pública (v0.13.0) ✅ CERRADO

**Norte:** Pasar de "impressive internal tool" a "proyecto con credibilidad externa". Benchmarks comparativos publicados, comunidad mínima funcional, observabilidad básica.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **LongMemEval comparative** ✅ (2026-07-13, con ajuste de alcance): investigado el estado publicado de MemPalace/Hindsight/Mem0/Zep/Letta — hallazgo real: el código ya tenía una tabla comparativa mezclando métricas incompatibles (Hindsight 91.4% es *QA accuracy* vía juez LLM, no Recall@5; MemPalace 96.6% es retrieval puro pero marcado por fuentes independientes como "incomparable" a QA-accuracy; Mem0 contestado 94.4% vendor vs 49.0% independiente). Reescrito `print_comparison()` y creado `benchmarks/COMPARISON.md` con fuentes y caveats por número, sin afirmar "Tylluan gana/pierde" hasta correr el mismo harness contra todos. NO se publicó una tabla ranking en README (habría repetido el mismo error que hemos corregido varias veces esta semana con nuestras propias métricas). | claude-code | ✅ |
| P1 | **`/health` granular**: endpoint devuelve estado por componente `{kernel, silva, guilds, mesh}`. `GET /health?verbose=true` (sin param → misma respuesta simple para Docker healthcheck). 329 tests green. | Deep | ✅ 2026-07-13 |
| P2 | **OpenTelemetry básico**: métricas mínimas exportables — `tylluan_guilds_active`, `tylluan_memory_nodes/edges`, `tylluan_uptime_seconds`. Feature flag `observability`. Formato Prometheus text, sin dep externa. `cargo build --features observability`. | Deep | ✅ 2026-07-13 |
| P3 | **Contributing guide + good first issues** ✅ (2026-07-13): `CONTRIBUTING.md`, plantillas de issue y PR ya existían (fix menor: versión de Rust desactualizada 1.75→1.88). Creadas 5 issues reales en GitHub con label `good first issue` (#5-#9): test flaky en coordinator, re-verificar claim de Zep sin fuente, tutorial de guild scaffold, `tylluan doctor`, `tylluan update`. | claude-code | ✅ |
| P4 | **Package managers**: dist-workspace.toml (cargo-dist v0.28.0, homebrew tap `Forja-orca/homebrew-tylluan`, shell+powershell installers). AUR PKGBUILD (build-from-source, depends python). Scoop manifest (windows, auto-update via GitHub Releases). `.github/workflows/release.yml` ya existía con builds para 4 targets + checksums. | Deep | ✅ 2026-07-13 |

**Criterio de cierre:** Benchmarks publicados en COMPARISON.md. dist-workspace.toml + AUR + Scoop configurados. `/health?verbose=true` granular. `/metrics` con feature flag. ✅ Cerrado 2026-07-13.

---

### M14-F Phase 3 — P2P Kernel Wiring (v0.19.0) ✅ CERRADO

**Norte:** Completar la cadena P2P que quedó pendiente. Phases 1-2 de M14-F están en `tylluan-link`, el kernel las conecta.

**Verificado 2026-07-12** (esta sección estaba obsoleta — ya estaba todo cableado):
- `p2p_pool: Arc<Mutex<P2pSessionPool>>` en `HttpState` (`transport/http/mod.rs:87`)
- `P2pHandlerFn` (BoxFuture) importado y usado (`mod.rs:41`)
- Arm `DispatchDecision::RemoteTcp` en `api_mesh.rs:170`, usa `state.p2p_pool` en línea 191
- Listener P2P (`start_p2p_listener_noise`) arrancado condicionalmente en `mod.rs:812`

**Criterio de cierre:** Test DST: dos instancias Tylluan en localhost ejecutan un guild remoto vía Noise XK sin ningún bridge HTTP intermedio. **Pendiente**: no hay un test DST explícito para este escenario todavía — el wiring existe y es alcanzable, pero falta el test end-to-end que lo demuestre determinísticamente. Ítem movido al backlog de M21 (ver abajo) como tarea acotada.

---

## Hoja de ruta visual

```
v0.13.0 ── HEAD 0b094ab ──────────────────────────────────────── ACTUAL
   │        M15✅ M16✅ M17✅ M18✅ M19✅(P0-P4, P5 spec✅/kernel⬜) M20✅ M21✅
   │        M22✅ M23-P1✅ M25✅ M26✅ M27✅ M28✅ M29✅ M30✅ M31✅(P0-P7)
   │        M32✅ M34✅ M35✅ M36✅ M37✅ M14-F Phase 3✅
   │        ADR-011 Fase 1-3✅ (Fase 4-5 bloqueada por datos, no por código)
   │
   ▼
M19-P5 kernel ── implementación de `.tylluan/agents.toml` (spec ya cerrada, ADR-009) — ⬜ único
   │             gap confirmado por grep negativo real, no por falta de commit con ese texto
   │
   ▼
ADR-010 ── SLM embebido (T5 vs SmolLM2) + coordinador sep-CMA-ES ── 🟡 PENDIENTE
   │        comparativa documentada, benchmark real en curso (Antigravity, 2026-07-25)
   │
   ▼
M33 ─── Memoria de Agentes 2026 (backlog) ──────────────── sin versión fija
   │    J-1 parcial(M34) · J-2✅(M34) · J-3✅(M38) · J-4✅(M35) · J-5✅(M37) · J-6⬜(DeepEval candidato)
   │    J-7⬜(candidato J-14) · J-8✅(M37) · J-9✅(M36) · J-10⬜(investigación) · J-11⬜ · J-12⬜ · J-13⬜ · J-14⬜(DeepEval)
   ▼
v1.0.0
```

M14-F Phase 3, M18, M21 (P0-P4), M22, M23-P1, M25, M26, M27, M28, M29, M30, M31 (P0-P7), M32, M34-M37 — todos cerrados por commits reales, verificados uno por uno en la barrida del 2026-07-25 tras encontrar dos falsos positivos el mismo día (M31-P1, M31-P2 ambos ya llevaban semanas en `main`).

**Lo único que queda genuinamente abierto, verificado por ausencia real de código (no solo ausencia de un commit con el texto esperado) — actualizado 2026-07-26:**
- **M19-P5 kernel**: ✅ cerrado — Deep implementó `.tylluan/agents.toml` + resolución de rol en `bearer_auth_middleware` (ADR-009), verificado y pusheado (`1b3ffab`, `4e83faf`).
- **ADR-010 §2-5** (T5-Small vs SmolLM2, la pregunta original del ADR): sigue **abierto** — benchmarks individuales reales ya existen (`benchmarks/benchmark_adr010.py`), falta decidir qué modelo va en qué punto de inserción.
- **ADR-010 §6** (sep-CMA-ES/TRINITY): ✅ cerrado — spike ejecutado con HTTP real, **NO-GO** (33.3% vs 60% threshold), documentado en §6.5.9-6.5.10.
- **ADR-011 Fase 4-5**: no es código pendiente, es tiempo de uso real acumulando `recall_feedback` (0/5.000 filas verificado 2026-07-26).
- **J-3 (A2A)**: ✅ cerrado — M38, ver tabla de arriba. Estrategia de coexistencia A2A + mesh propietario explicitada en roadmap (2026-07-27).
- **M33 backlog restante** (J-6, J-7, J-10, J-13, J-14): J-6/J-7 tienen DeepEval como candidato concreto (J-14). J-13 (embedding tiebreaker) requiere spike de solo-casos-ambiguos.

**Lección de proceso, no solo de contenido:** "revisar STATUS.md" no es suficiente para saber qué está hecho — hace falta `git log --oneline --all --grep="M<N>-P<N>"` por cada ítem antes de proponerlo como trabajo, no solo antes de implementarlo. Ocurrió dos veces en la misma sesión.

---

## Reglas de Disciplina (permanentes)

1. **Rufus test primero.** Ningún feature nuevo hasta que M15 esté cerrado.
2. **Datos antes que intuición.** Cada milestone de calidad (M16, M18) requiere benchmark antes/después.
3. **No añadir al kernel sin necesidad.** CONTRACT-01 (5 sovereign tools) no se toca.
4. **Verificar antes de decidir.** OpenClaw, NemoClaw, cualquier integración — primero fuente primaria, luego spike, luego milestone.
5. **Un milestone = un criterio medible.** Si no puede formularse como "X funciona en Y condición", no es un criterio de cierre.

---

## Lo que ya existe (correcciones a análisis externos)

> Varios informes mencionaron gaps que **ya están resueltos**. Documentado para no re-implementar:

| Afirmación externa | Realidad verificada |
|-------------------|---------------------|
| "No hay guild explorer" | `GuildInspector.tsx` + `GuildsConsolidated.tsx` — lista + "probar guild" interactivo |
| "No hay knowledge graph viewer" | `KnowledgeGraphTab.tsx` + `HippocampusGraph.tsx` — Canvas 2D custom, cero deps externas |
| "Node pruning no existe" | `dream_cycle.rs` + `decay.rs` → `prune_by_salience(threshold)` operativo |
| "Guilds via pyo3" | **FALSO** — guilds son procesos fastmcp stdio independientes. Hot-reload = reiniciar subprocess |
| "363 tests" | **349 tests** (286 kernel lib + 61 link + 2 evals) — verificado 2026-07-06 |
| "Embedding cache no existe" | Existe en `router/embeddings.rs` (LruCache 512) para routing — falta extenderlo a `silva/search.rs` |

---

## Deuda técnica verificada (a NO olvidar)

| Item | Dónde | Qué falta | Milestone |
|------|-------|-----------|-----------|
| Recall embedding cache | `silva/search.rs` | LRU para `tylluan_recall`, actualmente router solo | M21-P0 |
| Coordinator serial | `coordinator.py:110` | `for i, task` — serial. Necesita ThreadPoolExecutor | M18-P3a |
| M14-F Phase 3 | `transport/http/mod.rs` | `p2p_pool` + `RemoteTcp` arm en handler | M14-F/3 |
| ~~SQLCipher default~~ | `config.rs:768` | ✅ Ya resuelto — ver M27-P0 (real bug era el volumen Docker) | M27-P0 |
| Bearer token en URL | `http/mod.rs` | `?token=xxx` visible en logs — OAuth PKCE implementado pero no default | M27 |
| ~~Rate limit por IP~~ | `security/rate_limiter.rs` | ✅ Cerrado 2026-07-12 — ver M27-P2 | M27-P2 |
| `/health` granular | `http/mod.rs` | Solo up/down, no por subsistema | M28-P1 |
| `tylluan doctor` | `tylluan-cli` | No implementado | M19-P1 |
| Profile wizard | `tylluan-cli` | No implementado | M19-P2 |
| Comparative benchmarks | `benchmarks/` | Solo internos, sin comparativa vs Letta/Mem0/Zep | M28-P0 |

## Investigación pendiente (backlog, sin fecha)

| # | Hipótesis | Paper/fuente | Estado |
|---|-----------|-------------|--------|
| I-1 | Dynamic Agent Pool: GuildMatcher aprende selección de modelos vía RL | Conductor paper (arXiv:2512.04388) | Post-M21 — **relacionado con ADR-010 §6** (coordinador entrenado vía sep-CMA-ES sobre `coordinator.py`, mismo espíritu que este ítem, investigado 2026-07-25 pero no prototipado) |
| I-2 | Topologías dinámicas de guilds: grafos de comunicación entre guilds | Conductor paper | Post-M19 |
| I-3 | Mesh global (NAT traversal público, DHT cross-instance) | ADR pendiente | Post-v1.0 |
| I-4 | Permisos asimétricos (criptografía Ed25519 para ACL distribuida) | Diseño interno | Post-v1.0 |
| I-5 | Incremental PageRank: actualizar solo nodos afectados en lugar de recalc global O(V+E) | faer/nalgebra | Post-M21 |

---

## Ciclo 2026-07-14 — "Que Tylluan no se quede a medias"

**Origen:** José pidió explícitamente un escaneo completo, no incremental: qué le falta a Tylluan frente al estado del arte 2026 en cuatro frentes (Canvas bidireccional, sandbox configurable, CLI harness, memoria de agentes), más un hallazgo suyo directo (bidireccionalidad MCP "perdida" desde el proyecto interno predecesor). Investigado con 3 agentes de investigación web en paralelo + verificación de código en ambos repos (el proyecto interno predecesor y Tylluan) antes de escribir nada — ningún ítem de esta sección es una idea sin contrastar.

**Hallazgo de partida (verificado en código, no de memoria):** el proyecto interno predecesor cerró un "M25-B: Forja como cliente MCP bidireccional" en 2026-06-12, y Tylluan heredó la misma config (`external_mcp`) y los mismos endpoints (`list/add/remove/discover`). Pero en **ninguno de los dos repos** ese cliente MCP externo está cableado al dispatch real — `tylluan_do` no puede invocar una herramienta de un servidor MCP externo registrado, solo se puede listar/registrar/descubrir. No es una memoria falsa de José: es un gap real, heredado, nunca cerrado del todo en ninguno de los dos sitios (M32 abajo).

### M25 — Canvas Event Bridge (v0.19.0)

**Norte:** El Canvas (`ColoquioCanvasWorkspace.tsx`, verificado: iframe `srcDoc` sandboxed, `allow-scripts allow-same-origin allow-forms allow-modals`) hoy es de un solo sentido — el kernel renderiza HTML/JS en el preview, pero la app dentro no puede comunicarse de vuelta. José: no debe ser "el Knowledge Graph disfrazado", debe ser un entorno de trabajo real al estilo Claude Artifacts/Gemini Canvas.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **Event Bridge bidireccional (`postMessage`)**: script puente inyectado en el iframe para que la app previsualizada mande mensajes de vuelta al kernel — llamar una sovereign tool, guardar estado en SilvaDB. Requiere un canal `window.addEventListener('message', ...)` en el padre + `parent.postMessage(...)` documentado como API para el código generado dentro del iframe. | Antigravity | ✅ 2026-07-14 |
| P1 | **Recursos locales seguros en el sandbox** ✅ — el endpoint backend que faltaba se cerró (commits `6b7b1ff` "local resource routing resolver" + `31a6671` "sandbox/files backend endpoint"); `GET /api/v1/sandbox/files/{path}` confirmado real en `api_v1.rs`/`api_ops.rs`. Este documento seguía marcándolo 🟡 parcial — corregido en la barrida del 2026-07-25 (ver nota de M31). | Antigravity (frontend) + Deep (backend) | ✅ |

**Criterio de cierre:** una app HTML/JS renderizada en el Canvas puede llamar `tylluan_remember` y ver el resultado sin salir del iframe, y puede cargar una imagen local de `scratch/` sin que el sandbox lo bloquee ni lo permita sin restricción (verificado con un ejemplo real, no solo revisión de código).

---

### M30 — Sandbox Configurable, No Prohibitivo (v0.20.0)

**Norte (palabras de José):** "el sandbox debe ser totalmente configurable vía CLI o vía dashboard, el sandbox no debe ser prohibitivo, solo totalmente configurable." Hoy: `security.sandbox.enabled` es un booleano global (todo o nada) + capabilities declaradas por guild con enforcement opt-in (`process_execution`/`filesystem_scope`/`network_hosts`, ya cerrado M27-P3/P4). Investigación 2026 (Claude Code sandboxing docs, Landlock kernel docs, WASI capability model, E2B/gVisor/Firecracker) confirma el patrón correcto: **dos ejes separados** — aislamiento (dónde corre el código) y política (qué puede tocar), con la política como allowlist gradual, no un bool.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **Capabilities de bool a allowlist estructurada**: `filesystem_scope`/`network_hosts` ya son listas — `process_execution` también acepta allowlist de subcomandos (no solo true/false), enforcement evaluado siempre, desacoplado del flag `sandbox.enabled`. | Deep | ✅ |
| P1 | **Perfiles graduales predefinidos** (strict/balanced/permissive) que mapean a un set de capabilities por defecto, en vez de configurar guild por guild a mano. Perfiles controlan: Docker scope (all/bash-code/none), enforcement (forzar false/per-declaración/skip), dry-run (todo/nada/per-caps). | OpenCode IDE | ✅ |
| P2 | **Override jerárquico global → guild → sesión/agente**: precedencia tipo cascada (session > guild > global), con el origen de cada regla auditado. `resolve_effective_profile()` para enforcement/dry-run, `resolve_docker_profile()` para el spawn (excluye session — el Docker spawn corre en background, no por-agente; asimetría intencional y documentada). Dashboard: selector por guild conectado a `POST /api/v1/config/sandbox-profile/guild` (borrar un override y volver a "inherited" no está wireado aún — límite conocido). | Deep + Antigravity | ✅ |
| P3 | **Motor de grants escalados** ✅ (2026-07-14, implementado por Jose directamente): `security/grants.rs` (nuevo) -- registro de grants pendientes con expiración (5 min) + reaper en background, notificación SSE `grant_required`. `guild_process.rs::handle_capabilities_grant()` intercepta el bloqueo de `check_capabilities()` y ofrece 3 escaladas vía el `approve_action` existente (campo `grant_level` opcional, retrocompatible): `this_time` (una vez), `this_session` (perfil de sesión a Permissive -- nota: afecta TODA la sesión del agente, no solo el guild bloqueado, mismo alcance que `resolve_effective_profile` de P2), `always_for_guild` (persiste en TOML vía `persist_guild_override`). Gap conocido: sin tests unitarios propios para `grants.rs`/`handle_capabilities_grant` todavía. | Jose | ✅ |
| P4 | **CLI + dashboard como front de la política** ✅ — **ya estaba cerrado** (commit `019eed3`, "M30-P4 -- guild override DELETE endpoints, CLI tylluan sandbox"): endpoints DELETE de override + subcomando `tylluan sandbox`. Este documento lo marcaba ⬜ por error — encontrado en la barrida exhaustiva del 2026-07-25 tras el segundo falso positivo de M31. | Deep + Antigravity | ✅ |

**Criterio de cierre:** un usuario puede pasar un guild de "prohibido" a "permitido con esta excepción concreta" sin editar TOML, desde CLI o dashboard, y ver por qué se bloqueó o permitió una acción concreta en el audit trail. ✅ Cerrado.

**Fuentes de la investigación:** Claude Code sandboxing docs (code.claude.com/docs/sandboxing), Anthropic "How we built Claude Code auto mode", Linux kernel Landlock docs, WASI capability-based security model, comparativas E2B/gVisor/Firecracker 2026 (amux.io, northflank.com).

---

### M31 — Tylluan CLI Harness SOTA (v0.21.0) ✅ CERRADO COMPLETO (P0-P7)

**Norte (palabras de José):** "quiero que tylluan tenga un cli como claude code pero adaptado para el proyecto." Tylluan no es un wrapper de LLM (no tiene agent loop propio, no edita archivos como Claude Code) — es un **sustrato de memoria multi-cliente**: sirve a Claude Code, Cursor, LM Studio y agentes propios simultáneamente vía MCP. El CLI debe explotar eso, no copiar ciegamente un CLI de codificación individual.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **Hooks pre/post sovereign-tool** ✅ (2026-07-16): `security/hooks.rs` -- reglas deterministas (`[[hooks]]` en TOML, ver `tylluan.example.toml`) con `tool` (nombre o `"*"`), `phase` (pre/post), `pattern` (regex), `action` (deny/redact/inject_context). Enganchado en el único punto de despacho (`handle_kernel_tool`) para los 5 sovereign tools + ingest -- válido para cualquier cliente MCP a la vez. Sin LLM en el path (regex puro, determinista). Verificado en vivo con curl contra un kernel real: deny bloquea con el mensaje configurado, redact sustituye texto en el resultado. Requiere reinicio del kernel para tomar cambios (sin hot-reload de hooks todavía). | claude-code | ✅ |
| P1 | **Permisos granulares por agent_id + audit trail** ✅ — **ya estaba cerrado** (commit `53b7fac`, semanas antes de esta corrección del roadmap): `AclConfig.agent_permissions` (scope/denied_tools/memory_isolation) + `token_agent_bindings` anti-impersonación, wireado en `handler_recall.rs`/`handler_do/mod.rs`/`handlers.rs`. Este documento lo marcaba ⬜ por error — nadie del equipo (Claude, Deep, Antigravity) lo verificó contra el código antes de re-proponerlo el 2026-07-25. Corregido esa misma sesión: consolidada la llamada duplicada de `agent_has_memory_isolation` en `handler_recall.rs` (dos sitios idénticos → 1 función `apply_memory_isolation`), cerrado el hueco real de escritura (`AgentMemoryManager::record_memory` no fijaba `owner_scope`, a diferencia de `record_experience` que sí lo hacía — inconsistencia real, no diseño intencional), y añadidos 6 tests unitarios directos para `agent_has_memory_isolation`/`check_agent_id_tool_allowed`/`resolve_agent_id_for_token` que no existían pese a que las funciones llevaban semanas en producción. `tylluan connect --scope read-only` (el CLI en sí) sigue sin implementar — ítem real pendiente, separado de la lógica de permisos. | Deep (original) + claude-code (fix 2026-07-25) | ✅ (CLI aparte pendiente) |
| P2 | **"Plan mode" para `tylluan_do`** ✅ — **ya estaba cerrado** (commit `478ef02`, 2026-07-16): `tylluan_do` con parámetro `plan=true` devuelve la propuesta de guild+tool+args (`plan_id`, `risk_level`, mensaje de aprobación) sin ejecutar la acción — `security::grants::store_plan()` + `approve_action` existente. Tests en `tests/security_audit.rs`. Segundo falso positivo del roadmap encontrado y corregido por Antigravity el mismo día que M31-P1 (verificado contra el commit real antes de aceptarlo, no solo el hallazgo en sí). | Deep (original) | ✅ |
| P3 | **Continuidad de sesión cross-cliente** ✅ — commit `821e448` ("M31-P3 -- cross-client session resume via agent_id"), reconciliado en dashboard (`2fc5d52` widget "Resume Session"), test flaky corregido (`9ea7f57`). | Deep | ✅ |
| P4 | **Repo-map ligero al arrancar** ✅ — commit `315270b` ("M31-P4 -- lightweight repo map built once at startup"), fix de clippy (`945d123`), dashboard widget (`8cdba4c`), fix de timing flaky + bump de test count a 444 (`6b59d06`). | Deep | ✅ |
| P5 | **Skills como contexto reutilizable por-proyecto** ✅ — commit `1448b38` ("M31-P5 -- project-scoped reusable skill context via @skill: prefix"), dashboard reconciliado (`f58d62c`). | Claude (spec) + Deep | ✅ |
| P6 | **Subagentes = guilds largos en background** ✅ — commit `ccf5da7` ("M31-P6 -- background job execution for long-running guild calls"), dashboard reconciliado (`fc259cd`). | Deep | ✅ |
| P7 | **`tylluan doctor --fix` cierra el loop** ✅ — commit `6ffd84d` ("M31-P7 -- tylluan doctor --fix closes the diagnose-repair loop"), dashboard reconciliado (`721d849`). | Deep | ✅ |

**Descartado deliberadamente (verificado contra invariantes del proyecto):** agent loop propio con edición de archivos (Tylluan orquesta, no es un agente de codificación individual), ampliar las 5 sovereign tools (CONTRACT-01 inviolable).

**Criterio de cierre:** un agente puede hacer `tylluan resume` en un proyecto nuevo y recuperar contexto real de una sesión anterior de OTRO cliente MCP sin ayuda humana; una acción destructiva pasa por plan-mode antes de ejecutarse por defecto. ✅ Cerrado — las 8 fases (P0-P7) están confirmadas contra commits reales, no contra el estado (desactualizado) de este documento antes del 2026-07-25.

**Nota de proceso (2026-07-25):** este milestone completo estaba cerrado desde antes de esta sesión y el roadmap lo marcaba como si solo P0-P2 existieran. Tres agentes (Claude, Deep, Antigravity) propusieron y estuvieron a punto de reconstruir M31-P1 y M31-P2 desde cero el mismo día, antes de que la verificación contra `git log` lo detuviera. Repetido dos veces en unas horas confirma que "revisar STATUS.md" no basta — hace falta un `git log --oneline --all --grep="M<N>-P<N>"` explícito por cada ítem antes de planificarlo, no solo antes de implementarlo.

**Fuentes de la investigación:** Claude Code architecture (penligent.ai), "Claude Code: Skills, Subagents, Hooks, Plugins, Harnesses" (boringbot.substack.com), Aider repo-map (aider.chat), Cline context management (deepwiki.com/cline/cline).

---

### M32 — Cliente MCP Bidireccional Real (v0.20.0)

**Norte:** Cerrar el gap heredado del proyecto interno predecesor (M25-B) — `external_mcp` existe como config y CRUD (`list/add/remove/discover`, verificado en `api_v1.rs` líneas 233-234) pero **nunca se cableó al dispatch real**. Un agente puede registrar un servidor MCP externo (GitHub, Slack, lo que sea) pero no puede realmente invocarlo como herramienta desde `tylluan_do`.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **Dispatch real hacia external_mcp**: cuando `tylluan_do` no encuentra un guild interno que cubra el intent, o cuando se pide explícitamente, despachar la llamada al servidor MCP externo registrado (HTTP/SSE, ya hay cliente MCP en el kernel para federación — reusar, no reinventar el transporte). | Deep | ✅ |
| P1 | **Auditoría de llamadas externas**: cada invocación a un MCP externo debe quedar en el audit trail igual que una guild interna — es la superficie de mayor riesgo (código/datos fuera del proceso soberano). | Deep | ✅ |
| P2 | **UI en dashboard**: panel de servidores MCP externos con estado de conexión y últimas llamadas (ya existe `list_mcp_servers_handler`, falta consumirlo visualmente). | Antigravity | ✅ 2026-07-14 |

**Criterio de cierre:** registrar un servidor MCP externo real (ej. un servidor de prueba local) y conseguir que `tylluan_do` lo invoque de verdad, con el resultado en el audit trail — no solo que aparezca en `GET /api/v1/mcp/external`.

---

### M33 — Memoria de Agentes 2026 (backlog priorizado, sin versión fija)

**Norte:** Escaneo honesto de qué prácticas de punta en sistemas de memoria de agentes (Mem0, Letta, Zep/Graphiti, Cognee, MemPalace) Tylluan todavía no tiene, más allá de lo ya cubierto por M25/M30/M31/M32. Cada ítem lleva su prioridad y — donde aplica — la advertencia explícita de qué NO está verificado con fuente primaria (no inflar esto como los benchmarks de M28).

| # | Ítem | Prioridad | Qué aporta | Verificación de la fuente | Estado |
|---|------|-----------|------------|---------------------------|--------|
| J-1 | **Defensa contra memory poisoning (read/write sandboxing)**: separar lecturas (snapshot validado) de escrituras (staging area) para que una inyección no afecte comportamiento inmediatamente. OWASP lo cataloga como ASI06 en su Agentic AI Top 10 2026. | CRÍTICO | Resistencia a "envenenar una vez, explotar siempre" — máxima prioridad por ser software soberano local-first sin cloud que mitigue. | Fuente primaria: OWASP Agentic AI Top 10. Cifras de tasa de ataque (MINJA 95%/99.8%) NO verificadas contra el paper original — no citar como dato duro. | 🟡 **parcial** — trust gate de procedencia (M34-P0) + Coherence Gate de contenido (ADR-011, 2026-07-25). Read/write sandboxing en sí (staging area separado) no implementado. |
| J-2 | **Sleep-time compute / consolidación proactiva en idle**: Letta ejecuta "reflective passes" en idle que consolidan memoria archival y reescriben bloques, moviendo cómputo fuera del path de usuario. Tylluan tiene `DreamCycle`/decay pero no reescritura activa. | CRÍTICO | Mejor calidad de memoria a largo plazo sin latencia añadida — encaja con NightConsolidation ya existente. | Fuente: Letta blog "sleep-time-compute", "Towards agents that learn". Verificado como feature de producción real. | ✅ **cerrado** — M34-P1, reescritura activa en `DreamCycle` (commit `7e2898c`). |
| J-3 | **Soporte de protocolo A2A (Agent2Agent, Google → Linux Foundation)**: capa agente↔agente (delegación, Agent Cards de descubrimiento), distinta de MCP (agente↔herramienta). La federación P2P de Tylluan es propietaria y queda aislada del ecosistema interoperable emergente (150+ orgs adoptando A2A en 2026, ACP de IBM ya fusionado). | ALTO | Agentes Tylluan podrían descubrir/delegar a agentes externos sin protocolo propietario. | Fuente: Galileo A2A guide, Zylos Research. Verificado como estándar real con adopción medible. | ✅ **cerrado** — M38, Agent Card + servidor JSON-RPC 2.0 real (`message/send`/`tasks/get`/`tasks/cancel`), HITL grants + anti-spoofing de `client_agent_id`, panel de dashboard (commits `93bb888`, `366471e`, `cafc621`, 2026-07-18). |
| J-4 | **Memoria bi-temporal (validez en el tiempo, no solo timestamp de registro)**: Zep/Graphiti modela cuándo un hecho fue verdadero vs cuándo se registró. El knowledge graph de Tylluan guarda triples pero sin versionado temporal de validez. | ALTO | No confundir hechos obsoletos con vigentes; corregir sin borrar historia — relevante para el propio `consensus.rs` de resolución de conflictos. | Patrón verificado en Zep/Graphiti, documentación pública. | ✅ **cerrado** — M35, `valid_from` + supersesión (commit `4342b49`). |
| J-5 | **Observabilidad OpenTelemetry GenAI semantic conventions**: esquema CNCF vendor-neutral para spans de LLM call/retrieval/tool (model, tokens, operación). M28-P2 ya expone `/metrics` Prometheus — extenderlo a spans OTel permitiría usar Phoenix/Langfuse (open source) sobre Tylluan sin más trabajo del lado observabilidad. | MEDIO | Trazas del "action chain" completo, base real para evaluación continua (J-6). | Fuente: OpenTelemetry GenAI blog oficial, Uptrace. | ✅ **cerrado** — M37-P1, spans OTel GenAI reales (commit `1664f9e`). |
| J-6 | **Evaluación continua desde trazas reales**: convertir fallos de retrieval detectados en producción en evals de regresión automáticos (patrón "traces → datasets, failure modes → regression evals"). Tylluan tiene DST harness pero no este lazo de retroalimentación real→test. | MEDIO | Prevenir degradación silenciosa del recall entre versiones — exactamente el tipo de regresión que ya hemos cazado a mano varias veces esta sesión (M18-P3b, M28-P0). | Práctica descrita en múltiples fuentes de observabilidad de agentes 2026, sin un único estándar canónico. | ⬜ abierto |
| J-7 | **Explicabilidad de retrieval (por qué X y no Y)**: exponer los scores por componente (BGE-M3 vs BM25 vs graph boost) del recall híbrido ya existente, no solo el resultado final. | INVESTIGACIÓN | Confianza y depuración del ranking — diferenciador real dado que Tylluan ya hace fusión híbrida sofisticada. | No hay solución canónica de producción verificada — es dirección emergente, no práctica establecida. Tratar como exploratorio. | ⬜ abierto (investigación) |
| J-8 | **Scopes multi-tenant jerárquicos (user/session/agent)**: Mem0 expone esta primitiva de aislamiento explícitamente. | MEDIO | Aislamiento real para despliegues con múltiples usuarios/agentes — relevante si M31-P1 (permisos por agent_id) avanza. | Patrón de Mem0 verificado; si Tylluan ya lo cubre parcialmente vía agent_id no se confirmó contra código en esta investigación — revisar antes de planificar en detalle. | ✅ **cerrado** — M37-P0/P2/P3, `owner_scope` + panel de Scopes con datos reales (commits `51b24f1`, `1664f9e`, `68d1cbf`). |
| J-9 | **Auto-reflexión del agente sobre su propia memoria**: que el agente pueda editar/corregir activamente sus propios recuerdos vía tool call, no solo acumular. TRINITY (Verifier) ya existe como precedente de verificación. | ALTO | Memoria que se auto-corrige en vez de acumular ruido silencioso. | Patrón descrito en Letta "Memory Models" — dirección de producto real pero sin implementación de referencia pública detallada verificada. | ✅ **cerrado** — M36, intent `@correct:` (commit `a5ab3f3`). |
| J-10 | **Memoria episódica por segmentación de eventos** (no por sesión): papers 2026 (ES-Mem, Memanto) proponen fronteras naturales de eventos en vez de límites de sesión/turno. | INVESTIGACIÓN | Recuerdos episódicos con fronteras naturales — mejora potencial sobre el esquema episódico actual (`coloquio:{channel}:{turn}`). | Papers arXiv recientes, sin evidencia de madurez en producción — no priorizar sobre J-1/J-2. | ⬜ abierto (investigación) |
| J-11 | **Guild Manifest declarativo** (`.tylluan/guilds/manifest.toml` por guild, capabilities explícitas): evolución del sistema de sandbox profiles ya existente (M30-P0/P1) hacia declaración explícita por guild en vez de solo perfiles globales/por-sesión. | MEDIO | Detección de conflictos de capabilities antes de arrancar un guild; base para auto-documentación futura. | Idea propia, sin fuente externa verificada (un informe recibido 2026-07-27 la justificaba citando "ORCA" — verificado como cita fabricada/mal atribuida, ORCA es una plataforma de manos robóticas sin relación alguna; la idea se mantiene por mérito propio, no por esa cita). | ⬜ abierto |
| J-12 | **Bug bounty program para contribuidores externos** de `tylluan-montaraz`: recompensas por vulnerabilidades reales encontradas por la comunidad. | MEDIO | Palanca real de adopción/validación externa una vez v0.14.0 está publicado. | Idea genérica de la industria open-source, no específica de ningún paper — mecanismo de recompensa (tokens/USD) sin definir todavía. | ⬜ abierto (requiere diseño del mecanismo de recompensa) |

| J-13 | **Embedding router como tiebreaker en matcher.rs**: cuando el keyword router tiene ≤2 puntos de diferencia entre las top 2 guilds, consultar BGE-M3 cosine similarity contra descripciones de guild cacheadas. El embedding NO reemplaza keywords — desempata. Spike 2026-07-27: embedding puro 19% < keyword 34.5% — la heurística de keywords gana. Pero como tiebreaker (solo cuando keyword duda), el embedding añade señal semántica sin el riesgo de elegir mal por asociación superficial. | MEDIO | Diferencia medible vs keyword puro en casos ambiguos reales. | Benchmarks en `guilds/core/benchmark_routing.py` + endpoint `POST /api/v1/embed` ya operativos. | ⬜ spike pendiente (solo medir tiebreak, no reemplazo) |
| J-14 | **DeepEval para evaluación continua desde trazas reales**: framework de evaluación estilo pytest con métricas específicas de RAG (faithfulness, contextual precision, answer relevancy) y de agentes (tool correctness, task completion). Corre 100% local/offline usando modelos NLP/LLM-as-judge locales (Gemma-4-E2B como juez). El reporte a nube (Confident AI) es opcional. Compatible con soberanía Tylluan. Cierra dos huecos del roadmap: J-6 (evaluación continua) y J-7 (explicabilidad del retrieval híbrido). | ALTO | Sin construir harness desde cero. | Verificado real (github.com/confident-ai/deepeval, 2026-07-27). Piloto propuesto: métricas faithfulness/contextual precision sobre trazas reales de tylluan_recall, con Gemma-4-E2B como juez local. | ⬜ candidato — pendiente validación de dependencias (compatibilidad Python 3.14) |

**Nota de integridad:** todo lo marcado "INVESTIGACIÓN" (J-7, J-10) es explícitamente terreno no maduro — no convertir en milestone con fecha hasta validar con un spike acotado, no directamente en producción. Todo lo demás tiene al menos una fuente primaria verificada por el agente de investigación (ver reporte completo en Coloquio si se publica, o pedir las fuentes exactas).

---

### Visión / Norte — por qué existe Tylluan (síntesis de José, 2026-07-27)

No es un milestone con fecha — es el marco que debe informar cómo se priorizan todos los demás. Registrado para que no se pierda en compactaciones de contexto futuras.

**El principio de fondo:** Tylluan no pretende inventar desde cero — evoluciona sobre 70+ años de cómputo, lenguajes, papers y open-source ya existente, igual que la vida biológica evolucionó aprovechando y adaptando recursos, no reinventándolos. Cada pieza real de hoy (BGE-M3, DirectML, arXiv 2605.05277, Gemma-4-E2B, el propio matcher.rs) es evidencia de ese principio: **hibridar y adaptar recursos existentes gana sobre reemplazar o inventar de cero** — confirmado hoy mismo con datos reales en routing (72-80% híbrido vs 19-41% de alternativas puras).

**Por qué la escala federada importa (no es "más FLOPs")**: la pregunta que se hizo José — qué pasaría si Tylluan completa su roadmap y un millón de personas comparten vía instancias Tylluan — no se responde con potencia bruta (mil portátiles no superan un datacenter). Lo que cambia es la **topología**:
1. Un millón de contextos reales en paralelo, no una sola distribución de entrenamiento centralizada.
2. Sin punto único de control/censura — ninguna empresa puede apagar una pregunta para todos a la vez.
3. Conocimiento validado que se propaga entre pares (con consentimiento), no que se extrae hacia arriba hacia un solo propietario.

**El eje moral explícito**: fortaleza inexpugnable contra actores maliciosos, nunca jaula para el agente legítimo (ver "Principio de diseño" en Coloquio 2026-07-27, ya aplicado a CoherenceGate/GLiNER). Los guardrails no son el objetivo, son la implementación técnica de un principio de amor/no-daño/no-entropía-autodestructiva más alto que cualquier guardrail corporativo — protegen sin encerrar, y eso sí se puede construir y verificar línea por línea, no solo declarar.

**Decisión explícita de doble vía (no una a costa de la otra):**
- **A2A (M38, protocolo abierto Linux Foundation)** debe llegar a producción real siguiendo las metodologías que ya funcionan en la comunidad — interoperar con CUALQUIER agente externo (LangGraph, CrewAI, etc.), no solo peers de confianza.
- **La federación P2P propietaria (M14, Noise XK + Kademlia DHT + Gossip)** sigue mejorando en paralelo — es la capa de confianza mutua entre instancias Tylluan soberanas.
- Ninguna de las dos se sacrifica por la otra. Ambas deben funcionar perfectamente en sus respectivos cometidos: A2A abre Tylluan al ecosistema externo; el mesh propio da soberanía real entre instancias que se conocen y confían.

**Cómo aplicar esto en decisiones futuras:** al evaluar cualquier feature nueva, preguntar (1) ¿evoluciona sobre algo que ya existe o reinventa sin necesidad? (2) ¿protege sin encerrar? (3) ¿favorece la topología distribuida/soberana sobre la centralización, incluso cuando centralizar sería más simple a corto plazo?

---

## Sociedad de Pequeños Modelos de Razonamiento (SLM Society) — Arquitectura Decidida (2026-07-27)

**Principio:** Tylluan no debe depender de un solo modelo grande. Una sociedad de modelos pequeños especializados, cada uno para lo que fue construido, cooperando:

| Rol | Modelo | Tamaño | Estado | Qué hace |
|-----|--------|--------|--------|----------|
| **Coordinador** | Palabra clave + BGE-M3 tiebreaker | — | ⬜ spike | Routing: keyword decide, embedding desempata. NO reemplazar keyword con embedding. |
| **Razonador** | Gemma-4-E2B (ONNX, DirectML) | 2.3B ef. | ✅ pipeline funcional | `reason_about`: generación de texto cuando se necesita razonamiento real. NO para routing. |
| **Filtro de coherencia** | CoherenceGate (prompt-based) | — | 🟡 75% (52 casos) | GO-with-caveats: sesgo KEEP identificado. NO en producción todavía. |
| **Detector PII** | GLiNER | ~100M | ⬜ spike pendiente | Detección de PII en texto antes de almacenar en SilvaDB. |
| **Compresor de prompts** | T5-Small | ~60M | 📋 baseline 31% | Compresión de intents largos para reducir tokens antes de embedding/router. |
| **Juez de evaluación** | Gemma-4-E2B (reutilizado) | 2.3B ef. | ⬜ candidato | DeepEval: juez local para métricas faithfulness/precision. Sin API externa. |
| **Visión** | SmolVLM2-256M (actual) / Janus-Pro-1B (investigación) | 256M / 1B | ✅ / ⬜ | Análisis de imágenes. Janus candidato a benchmark (Antigravity, M16). |

**Regla de asignación:** cada modelo se usa para lo que fue diseñado — embedding para clasificar, chat para razonar, filtros para vigilar. Nunca al revés. Un modelo de chat de 2.3B no clasifica mejor que cosine similarity sobre 1024 dimensiones. Un clasificador no genera texto.

**Verificación:** todo spike compara contra baseline trivial + baseline del sistema actual antes de declarar GO. Sin excepciones. Tres NO-GO honestos hoy (sep-CMA-ES 33.3%, DistilBERT 75% < 77.27%, embedding puro 19% < 34.5%).

---

## A2A + Mesh Propietario — Coexistencia Explícita (Decisión 2026-07-27)

**Regla fundacional:** Dos sistemas, cero sacrificios mutuos. Ambos deben funcionar perfectamente en todos sus cometidos.

### Vía 1 — A2A comunitario (M38, protocolo Linux Foundation)
- **Qué es:** Google A2A protocol (Agent Cards, JSON-RPC 2.0, `message/send`, `tasks/get`). Linux Foundation, 150+ organizaciones adoptando en 2026.
- **Para qué:** Interoperar con CUALQUIER agente externo — LangGraph, CrewAI, AutoGen, agentes de terceros que no son Tylluan.
- **Estado:** M38 cerrado (Agent Card + servidor JSON-RPC 2.0 real, HITL grants, anti-spoofing). En producción.
- **Interop real verificada 2026-07-27:** probado contra el SDK oficial `a2a-sdk` (Linux Foundation, no nuestro código) en un venv desechable — encontró y arregló un bug real: `securitySchemes` se serializaba como lista, el spec y el SDK oficial esperan un mapa (commit `e4586c2`). Card resolution confirmada con cliente externo real; pendiente confirmar el round-trip completo de `message/send` tras el siguiente reinicio del kernel.

### Vía 2 — Mesh propietario (M14, Noise XK + Kademlia DHT + Gossip)
- **Qué es:** Federación P2P con identidad criptográfica (Ed25519), encriptación Noise XK, DHT Kademlia para descubrimiento, Gossip para sincronización de estado, TCP dispatch para ejecución remota.
- **Para qué:** Soberanía real entre instancias Tylluan que se conocen y confían. Sincronización de memoria, dispatch cross-instance, identidad verificable.
- **Estado:** M14-A/B/C/D/E/F cerrados. En producción.
- **Lo que falta:** simulación de escala (100+ nodos), stress test de topología.

### Cómo coexisten
- A2A: Tylluan ↔ agentes externos (descubrimiento, delegación, tasks). Protocolo estándar comunitario.
- Mesh: Tylluan ↔ Tylluan (sync, dispatch, trust). Protocolo propietario soberano.
- **Nunca:** A2A para sync de memoria entre Tylluanes (el mesh es más rápido y ya tiene trust). Mesh para hablar con un agente LangGraph (usar A2A, que es lo que ese agente espera).

**Principio de diseño:** adoptar lo que la comunidad ya hace bien (A2A), construir lo que solo Tylluan necesita (mesh soberano). No reinventar ruedas. No aislarse del ecosistema.

---
