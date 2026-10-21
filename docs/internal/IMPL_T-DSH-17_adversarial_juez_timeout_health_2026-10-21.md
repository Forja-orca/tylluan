# T-DSH-17 — Auditoría adversarial del juez ASI06 tras D-19 (timeout 90 s, semáforo y health)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `e022299` == origin/main (push88; T-DSH-16 integrada; lib 1151).
- **Encargo (turno 1485):** solo tests contra el código real: (1) ninguna guild fuera de `CPU_INFERENCE_GUILDS` recibe 3600 s por alguna vía (tabla del catálogo completo, falla si cambia); (2) una llamada colgada a `llama_backend` no retiene el semáforo del juez más allá de `JUDGE_TIMEOUT` (90 s) ni bloquea nuevas escrituras de memoria (la capa 2 es fire-and-forget); (3) `/health?verbose=true` cuenta el timeout como fallo. Si algo falla: `#[ignore]` + hallazgo con archivo:línea. Informe con tabla caso × esperado × real, `## Impact` y «no verificado».

## Entrega (solo tests)

- **Vía 1** — test unitario en `src/registry/guild_process.rs` (módulo de tests, mismo fichero del registry): `only_cpu_inference_guilds_get_the_unlimited_proxy_ceiling`. Recorre **el catálogo completo** (45 guilds) con el fixture real de categorías (15/60/180 s, plano 3600) y exige: techo 3600 s **si y solo si** la guild está en `CPU_INFERENCE_GUILDS`; ninguna otra > 180 s; `timeouts_for()` (la vía que se entrega al spawn) coincide con `effective_tool_call_secs()` para todas; y un **guard estructural** de que `main.rs` sigue cableando `.with_guild_timeouts(...)` (sin ese cableado la rama de fallback da el plano 3600 s a TODAS las guilds, `bash` incluida).
- **Vías 2 y 3** — dos binarios de integración nuevos con un `llama_backend` falso que **acepta la conexión y nunca responde**:
  - `tests/asi06_judge_hang_writes.rs` (rápido, corre en la suite normal): escrituras de memoria contra el router real con el juez colgado.
  - `tests/asi06_judge_hang_health.rs` (**`#[ignore]` por coste**: espera el `JUDGE_TIMEOUT` real de 90 s; `-- --ignored` lo ejecuta en ~95 s).

## Tabla caso × esperado × real

| # | caso | esperado | real | veredicto |
|---|---|---|---|---|
| V1a | guild del catálogo **fuera** de `CPU_INFERENCE_GUILDS` | techo ≠ 3600 s y ≤ 180 s | 39 guilds a 15/60/180 s, ninguna a 3600 s | OK |
| V1b | guild **dentro** de `CPU_INFERENCE_GUILDS` | techo plano 3600 s | las 6 a 3600 s (vision, deep_analysis, knowledge, comfy_ui, n8n_bridge, llama_backend) | OK |
| V1c | conjunto de guilds con 3600 s | **exactamente** la const | coincide (equality de conjuntos ordenados) | OK |
| V1d | vía `timeouts_for` (lo que recibe `Guild::spawn`) | == `effective_tool_call_secs` | igual en las 45 | OK |
| V1e | cableado de producción en `main.rs` | `.with_guild_timeouts(...)` presente | presente (`src/main.rs:1025`) | OK |
| V1f | fallback **sin** cableado (embedders/tests) | plancha: TODAS a 3600 s | `bash` = 3600 s (comportamiento previo documentado, **no** una vía por categoría) | OK (documentado) |
| V2a | 2 escrituras con el juez colgado | respuesta inmediata (fire-and-forget) | ambas HTTP 200 en **< 3 s** | OK |
| V2b | ¿la rapidez es por no llamar al juez? | juez lanzado e in-flight | `judge_spawn_count` +2 y `in_flight ≥ 1` a los 300 ms | OK |
| V3a | tras 90 s de backend colgado | timeout del juez, no cuelgue eterno | el juicio termina solo y `in_flight` vuelve a **0** | OK |
| V3b | `/health?verbose=true components.judge.failures` | +1 con el timeout | +1 | OK |
| V3c | `by_kind.timeout` | ≥ 1 (clasificado como timeout) | ≥ 1 | OK |
| V3d | `timeout_ms` / `max_concurrency` publicados | 90000 / 1 | 90000 / 1 | OK |
| V3e | permiso del semáforo liberado tras el timeout | una escritura posterior vuelve a entrar al juez | nuevo spawn y `in_flight ≥ 1` otra vez | OK |

**Ningún vector encontró un fallo**: no hizo falta marcar tests como `#[ignore]` por fallo; el único `#[ignore]` es por coste (~95 s), con motivo escrito.

## Observaciones / riesgos (no bypasses)

1. **Mientras el juez está colgado, las escrituras de memoria no se bloquean pero tampoco se juzgan** (`busy_skip`): una segunda escritura espera hasta `JUDGE_QUEUE_WAIT` (60 s) por la única ranura y, si no la consigue, se salta el juicio (contabilizado en `components.judge.by_kind.busy_skip`). Es coherente con el modo observación de ASI06 (capa 2 nunca bloquea), pero implica hasta ~90 s de escrituras sin veredicto L2 si `llama_backend` se cuelga. No lo fuerzo como fallo: está documentado en `coherence_gate.rs` (D-14/C2).
2. **La rama de fallback es la única "vía" que da 3600 s a guilds normales** y solo existe si un embedder/tests construyen el registry sin `.with_guild_timeouts`. Producción lo cablea; el guard V1e lo vigila a nivel de texto (`include_str!("../main.rs")`, mismo patrón que el test de T-DSH-16), con la fragilidad inherente a un guard textual.
3. **El timeout del juez es de petición completa** (`reqwest` `.timeout(JUDGE_TIMEOUT)`), no de generación del modelo: un `llama_server` que responda gota a gota pero tarde >90 s se corta igual, que es el comportamiento pedido por D-14/C1.

## Verificación de criterios

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test asi06_judge_hang_writes` | **1 passed** (5,8 s) |
| `cargo test -p tylluan-kernel --test asi06_judge_hang_health -- --ignored` | **1 passed** (95,2 s) |
| test unitario V1 | **1 passed** |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1152 passed / 0 failed / 4 ignored** (1151 + 1 test nuevo) |
| suite normal de `asi06_judge_hang_health` | 0 passed, **1 ignored** (verde) |

## Impact

- **Diff sin commitear: solo tests** — 1 test unitario añadido al módulo de tests de `registry/guild_process.rs` y 2 ficheros de integración nuevos. **Cero cambios en código de producción**, cero escrituras en el kernel vivo (servidor falso en loopback dentro del test), sin secretos.
- Coste en CI: +5,8 s (el binario lento queda `#[ignore]`); lib +1 test (1152).

## No verificado

- **`llama_backend` real**: el "colgado" es un socket TCP que nunca responde, no un `llama-server` con una ranura ocupada; el comportamiento de encolado interno del modelo no se reproduce.
- **Un solo pase del camino de 90 s** (no repetido): si el timeout fuese flaky por contienda, una sola ejecución no lo demuestra.
- **`busy_skip` bajo saturación real**: observación 1 se deduce del código (`acquire_judge_permit` con 60 s de espera) y no la medí con dos juicios concurrentes.
- **Embedders externos** que construyan `GuildRegistry` sin `with_guild_timeouts`: no auditados; en ese caso todas las guilds heredan el plano 3600 s por la rama de fallback (documentado).
- **Kernel vivo**: no se arrancó ni se consultó el de producción (regla de flota); `/health?verbose=true` se ejercita en el router en proceso.
