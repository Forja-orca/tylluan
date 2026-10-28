# FASE 0 (ADR-025) — Inventario de spawns, ámbito @hermes

**Asignación:** T2009 (TL claude-code). **Base:** `1a24e82` en mi worktree (`D:/hermes-projects/tylluan-wt-hermes`). **Solo lectura; cero código tocado.** Fecha: 2026-10-28 ~19:0x. Autor: hermes.
**ADR-025 no está en el worktree** (commit local del TL `4705492` sin push, igual que dsh): sigo el encargo literal de T2009 y sus 6 subruntimes.

## Cierre de cuentas (comando y cifra)

```
grep -rn "tokio::spawn\|spawn_blocking\|thread::spawn" crates/tylluan-kernel/src --include=*.rs
=> 191 ocurrencias en 61 ficheros  (verificado en mi worktree)
```
Ancla de dsh (T2011): hermes 63 + dsh 128 = 191 ✓. **Mi parte: 63** — mi grep restringido a mi ámbito suma exactamente 63:
`main.rs` 39 · `memory/inference_budget.rs` 5 · `memory/silva/query_cache.rs` 4 · `memory/silva/maintenance.rs` 4 · `memory/night/mod.rs` 3 · `version_check.rs` 2 · `guard.rs` 2 · `metrics_ring.rs` 1 · `memory/silva/tests.rs` 1 · `memory/silva/decay.rs` 1 · `memory/dream_cycle.rs` 1.
Composición: **43 filas de producción + 9 menciones (comentarios/doc/código muerto) + 11 en módulos de test = 63.**
Ficheros de mi ámbito sin ocurrencias: `maintenance.rs` (raíz), `hormones.rs`, `curriculum.rs`, `cleanup.rs`, `setup.rs`, `metrics_exporter.rs`, `eval/`, `doctor/`.

## Tabla — producción (43)

| archivo | línea | propósito | subruntime | disparador | cadencia | GuardedTask / BackgroundBudget / CancellationToken | participa en apagado | riesgo |
|---|---|---|---|---|---|---|---|---|
| main.rs | línea 675 | NAT/STUN discover_external_addr (gate config.nat.enabled, default false) | Red | una vez | — | no / no / no | no | Bajo (apagado por defecto) |
| main.rs | línea 730 | carga SparseEngine en paralelo (T-DSH-55) | Inferencia | una vez (arranque) | — | no / no / no | sí (joined en arranque) | Bajo |
| main.rs | línea 753 | carga EmbeddingEngine dense (mxbai) | Inferencia | una vez (arranque) | — | no / no / no | sí (joined) | Bajo |
| main.rs | línea 768 | carga RerankEngine Jina con catch_unwind | Inferencia | una vez (arranque) | — | no / no / no | sí (joined) | Bajo |
| main.rs | línea 966 | Hub Mailbox Listener: procesa propuestas de agentes | Coordinación | evento (buzón) | bucle infinito; check_mail(5 s) hardcodeado | no / no / no | no | **Medio-alto**: pánico = pipeline de propuestas muerto en silencio, sin supervisor |
| main.rs | línea 1025 | embed de cada propuesta (Proposal Embedding) | Inferencia | evento (por propuesta) | — | **sí 30 s** / no / no | no | Medio: block_in_place + mutex del modelo; sin backpressure por mail |
| main.rs | línea 1247 | RegistryActor.run (serializa mutaciones del registry) | Procesos y guilds | una vez (actor) | — | no / no / no | no | Medio: si el actor muere, el registry queda inconsistente; sin reinicio |
| main.rs | línea 1253 | persistencia de métricas de guilds | Procesos y guilds | intervalo | 60 s hardcodeado | no / no / no | no | Bajo |
| main.rs | línea 1274 | Doctor diagnose periódico | Coordinación | intervalo | 60 s hardcodeado | no / no / **sí (doc_token)** | **no — token creado pero JAMÁS cancelado** (línea 1290 solo lo retiene; nada llama a .cancel()) | Bajo; hallazgo: el token da falsa sensación de apagado ordenado |
| main.rs | línea 1302 | IVF warm-up (centroids si >50 embeddings) | Memoria | una vez | — | no / no / no | no | Bajo |
| main.rs | línea 1314 | IVF staleness watcher | Memoria | intervalo | 6 h hardcodeado | no / no / no | no | Bajo |
| main.rs | línea 1427 | Universal Gateway HTTP server | Red | una vez (dueño del listener) | — | no / no / sí (cancel_token línea 1425) | **sí** (cancel maestro línea 2194 + wait_for_shutdown) | Medio: crítico; sin él no hay API |
| main.rs | línea 1446 | version_check llama (fire-and-forget T999) | Coordinación | una vez | — | no / no / no | no | Bajo (timeout 10 s, silencioso) |
| main.rs | línea 1454 | arranque de guilds always_on | Procesos y guilds | una vez (arranque) | — | no / no / no | no | Medio: pánico antes de línea 1461 deja /health en warming_up para siempre |
| main.rs | línea 1471 | WarmPool pre-warm guilds lazy | Procesos y guilds | una vez | — | no / no / no | no | Bajo |
| main.rs | línea 1489 | descarga+hot_swap de modelo embedding en background | Inferencia | una vez | — | no / no / no | no | Medio: sin presupuesto; fallo = solo warn, sin reintento |
| main.rs | línea 1491 | (anidado en 1489) hot_swap_engine blocking | Inferencia | una vez | — | no / no / no | no | Medio |
| main.rs | línea 1519 | arranque de MCP servers externos (config) | Procesos y guilds | una vez | — | no / no / no | no | Medio: spawn de procesos hijos desde config; fallo silencioso (warn) |
| main.rs | línea 1592 | sharing policy (apply/reset shareable) | Memoria | intervalo | 300 s hardcodeado | no / no / no | no | Bajo |
| main.rs | línea 1613 | procesador de jobs episodic_index | Memoria | intervalo | 2 s hardcodeado (poll) | no / no / no | no | Medio: pánico = coloquio deja de indexarse en silencio; claim_next síncrono |
| main.rs | línea 1659 | transporte MCP stdio | Red | una vez | — | no / no / no | **sí** (stdio_handle vive → wait_for_shutdown línea 2186) | Bajo |
| main.rs | línea 1698 | seed routing_anchors + embed warm-start | Memoria | una vez (arranque) | gate config silva.embed_warm_start | no / **sí** / no | no | Medio: ráfaga ONNX al boot (mitigada por budget; skip con warn si budget ocupado) |
| main.rs | línea 1789 | M22 re-embed de episodes/lessons legacy (+5 s) | Memoria | una vez | — | no / no / no | no | **Medio-alto**: re-embed entero sin budget ni guard, compite con recall al arranque (misma contaminación medida en H-45) |
| main.rs | línea 1801 | M24 collapse de summaries duplicados (+20 s) | Memoria | una vez | — | no / no / no | no | Bajo |
| main.rs | línea 1813 | Hormone tick (decay exponencial) + curriculum cada 60 ticks | Coordinación | intervalo | 5 s hardcodeado | no / no / no | no | Medio-bajo: lock del server RwLock + mutex hormones cada 5 s para siempre |
| main.rs | línea 1847 | biological decay scheduler (gate config silva.decay_enabled) | Memoria | intervalo | 300 s hardcodeado | **sí 60 s** / no / no | no | Medio (ver duplicidad con dream_cycle) |
| main.rs | línea 1867 | flywheel coloquio→SilvaDB (upsert sin embed) | Memoria | intervalo | 60 s hardcodeado | no / no / no | no | **Alto**: origen directo del backlog ONNX medido en H-44/H-45 (coloquio_memory 963 sin vector); pánico = el sistema deja de aprender en silencio |
| main.rs | línea 1909 | Agnostic Reindexer (get_stale → embed batches de 32, take(100)) | Memoria | intervalo | 600 s hardcodeado (interval_at) + take 100 hardcodeado | **sí 120 s +3 extensiones** / **sí** / no | no | **Alto**: supera el guard a diario (363 WARN en 4 días, mediciones H-45); abort del guard pierde la ronda; coalescer OFF (tylluan.toml:292) |
| main.rs | línea 2006 | HNSW rebuild if-needed | Memoria | intervalo | 600 s hardcodeado | **sí 120 s** / **sí** / no | no | Medio |
| main.rs | línea 2035 | migración F3 hybrid→silva (hasta complete) | Memoria | intervalo (hasta terminar) | 600 s interval_at | **sí 600 s (línea 2066)** / **sí** / no | no | Medio: trabajo largo con abort posible en migraciones grandes |
| main.rs | línea 2091 | consenso de memoria colectiva | Memoria | intervalo | tick 60 s con countdown 3600 s hardcodeado | **sí 120 s (línea 2105)** / **sí** / no | no | Medio |
| main.rs | línea 2119 | Retrolink orphans (+10 s) | Memoria | una vez | — | no / no / no | no | Bajo |
| main.rs | línea 2129 | NightConsolidation loop (run_night_consolidation_loop) | Memoria | intervalo | config.night (repose/hourly) | no / no / no | no | Medio: fases pesadas; única defensa = semáforo interno (night/mod.rs:163) |
| main.rs | línea 2145 | mantenimiento SQLite: WAL checkpoint TRUNCATE | Memoria | intervalo | 300 s hardcodeado | no / **sí** / no | no | Medio: checkpoint bloqueante (budget mitiga cola) |
| main.rs | línea 2164 | (anidado en 2145) prune_dead_nodes(0.05) por tick | Memoria | evento (cada tick de mantenimiento) | heredada 300 s | no / no / no | no | **Medio-alto**: fire-and-forget sin budget ni guard; puede solaparse con su siguiente tick; carreras FK con el reindexer (mismas que los 336+77 WARN medidos en H-44) |
| guard.rs | línea 35 | GuardedTask::run — PRIMITIVA: spawn + timeout adaptativo | Coordinación | por petición (cada tarea guardada) | ×3 extensiones ×1.5 | es el propio guard / no / no | awaited por el llamante | **Medio-alto**: tras 3/3 hace `handle.abort()` (línea 75) y descarta la ronda; si la tarea estaba dentro de un `spawn_blocking` (ONNX), **el hilo blocking no se cancela y sigue corriendo** tras el abort (semántica tokio: abort cancela la tarea, no el worker) → el guard pierde la ronda pero el trabajo ONNX contamina igual |
| memory/night/mod.rs | línea 163 | PhaseOrchestrator: cada fase nightly en su tarea | Memoria | evento (ciclo nocturno) | semáforo limita concurrencia | no / no / no | sí (joined por run_all dentro del loop de 2129) | Bajo |
| memory/dream_cycle.rs | línea 47 | start_background_scheduler (decay por interval_hours) | Memoria | intervalo | config silva.decay_interval_hours | no / no / no | no | Medio-bajo — **posible código muerto**: `start_background_scheduler` no tiene NINGÚN llamador en crates/ (grep); de estar vivo sería una segunda agenda de decay duplicando main.rs:1847 |
| metrics_ring.rs | línea 70 | sampler de métricas sysinfo (CPU/mem/latencia guild) | Coordinación | intervalo | 5 s hardcodeado | no / no / no | no | Bajo |
| memory/silva/decay.rs | línea 311 | prune_traces_before (DELETE traces) | Memoria | por petición (API/mantenimiento) | — | no / no / no | awaited por el llamante | Bajo |
| memory/silva/maintenance.rs | línea 25 | record_tool_call: INSERT traza por tool-call | Memoria | por petición | — | no / no / no | awaited (línea 54) | Medio: un spawn_blocking POR LLAMADA — presión sobre el blocking pool con tráfico alto |
| memory/silva/maintenance.rs | línea 278 | save_sessions (MCP sessions upsert) | Memoria | evento | — | no / no / no | awaited (.await.ok()) | Bajo |
| memory/silva/maintenance.rs | línea 299 | load_sessions (arranque) | Memoria | una vez | — | no / no / no | sí (arranque) | Bajo |

## Menciones sin spawn real (9)

| archivo | línea | qué es |
|---|---|---|
| main.rs | línea 662 | comentario que explica el move de 675 |
| main.rs | línea 715 | comentario T-DSH-55 (cargas paralelas 730/753/768) |
| main.rs | línea 1984 | comentario T842-i (por qué el reindexer awaiting escribe en secuencia) |
| main.rs | línea 2179 | **código muerto**: `// tokio::spawn(routine_fut);` — P2 Sovereign Routine deshabilitado |
| guard.rs | línea 28 | doc-comment de run() |
| memory/silva/maintenance.rs | línea 54 | continuación del spawn de línea 25 (mención en string de error) |
| memory/night/mod.rs | línea 297 | comentario en test (panics por tarea) |
| version_check.rs | línea 11 | doc del patrón fire-and-forget |
| version_check.rs | línea 97 | doc de run_version_check |

## Módulos de test (11, aparte — no producción)

| archivo | línea | test |
|---|---|---|
| memory/inference_budget.rs | línea 461 | legacy_unbounded_wait_never_rejects (std::thread) |
| memory/inference_budget.rs | línea 493 | concurrent_saturations_produce_explicit_rejections_not_hangs |
| memory/inference_budget.rs | línea 530 | wait_percentiles_recorded_for_admitted_calls |
| memory/inference_budget.rs | línea 563 | condvar_waiter_does_not_poll_while_parked |
| memory/inference_budget.rs | línea 638 | legacy_contention_all_callers_eventually_admitted_no_starvation |
| memory/silva/query_cache.rs | línea 350 | test_single_flight_identical_queries_compute_once (std::thread) |
| memory/silva/query_cache.rs | línea 390 | test_concurrent_distinct_queries_do_not_serialize |
| memory/silva/query_cache.rs | línea 408 | (mismo test, variante de joiners) |
| memory/silva/query_cache.rs | línea 442 | test_single_flight_error_wakes_joiners_and_retries |
| memory/night/mod.rs | línea 732 | loop de repose con PhaseOrchestrator (tokio::spawn) |
| memory/silva/tests.rs | línea 2145 | sqlite_lock_wait_telemetry_records_queueing (holder thread) |

## Top 5 de mayor riesgo (mi ámbito)

1. **main.rs:1867 flywheel** — sin guard, sin budget, sin token: 60 s escribiendo nodos SIN embedding (causa raíz H-44 del backlog); su muerte en pánico apaga el aprendizaje del kernel sin nadie que lo note.
2. **main.rs:1909 reindexer + guard.rs:35 abort** — el único mecanismo de embeddings está cronopresupuestado (100/600 s), supera su guard a diario (363 WARN/4 d) y el abort tras 3/3 descarta rondas cuyo ONNX blocking sigue corriendo de todos modos: lo peor de ambos mundos.
3. **main.rs:966 Hub Mailbox Listener** — bucle infinito crítico (propuestas BWC) sin ninguna supervivencia; pánico = feature muerta en silencio hasta el próximo reinicio.
4. **main.rs:2164 prune fire-and-forget anidado** — solapable tick a tick, sin budget, carreras FK documentadas (336+77 WARN) con el reindexer; patrón a erradicar en la fase de unificación.
5. **Apagado: solo HTTP y stdio participan.** 27 de los 28 bucles de main mueren por `process::exit` (línea 2239), no por señal; `doc_token` (línea 1274) demuestra el patrón mal implementado — token creado, nunca cancelado. Para subruntimes reales hace falta un token maestro compartido por todos los loops.

## No verificado

- [no verificado] `dream_cycle.rs:47`: grep no encuentra llamador de `start_background_scheduler`, pero no he descartado registro dinámico/feature-gated — marcar como "probablemente muerto" hasta opinión del TL.
- [no verificado] Si tras `handle.abort()` (guard.rs:75) el `spawn_blocking` interno sigue: es semántica documentada de tokio, no lo he ejecutado.
- [no verificado] `service_manager.shutdown_all()` (2226) cubre servicios auxiliares/watchdog, no los spawns de esta tabla — no he leído service manager a fondo.
- Conteos de grep son exactos sobre `1a24e82`; cualquier push del TL (PUSH186) puede mover líneas.
- La línea exacta de `prune_dead_nodes` anidada es 2164; la verifiqué por contexto (2163 clone + 2164 spawn), no por nombre de la variable.
