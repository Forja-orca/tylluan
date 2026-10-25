# T-DSH-77 — Hacer visibles con `tracing::warn!` las filas 1-8 y 10 de T-DSH-76 (sin cambiar retornos ni flujo)

- **Agente:** dsh
- **Fecha:** 2026-10-25
- **Base:** `origin/main` `b430071` (push150, que integra T-DSH-75), tras `agent_wt.sh reset dsh`. **Diff SIN commitear**; sin commit ni push.
- **Encargo (T1736):** arreglar las filas 1-8 y 10 de T-DSH-76 con `tracing::warn!` sin cambiar retornos ni flujo. Prioridad filas 1-2 (cuarentena ASI06 y `reinforce_node`); filas 4-7 y 9 con un helper `friction_or_warn()` en `friction_log.rs` reutilizando el patrón rate-limited de `audit_or_warn()`; fila 9 (`router/matcher.rs`) incluida. Tests uno por comportamiento; criterios clippy `-D warnings` exit 0, `--lib` verde (base 1263) y `--test security_audit`; excluidos `api_contracts.rs`, `transport/http/mod.rs` y `api_federation.rs`.

## 1. Cambios (diff sin commitear: 5 ficheros, +229/−20)

| Fichero | Cambio |
|---|---|
| `security/coherence_gate.rs` | **Fila 1**: nuevo helper `quarantine_node_for_layer4()` (línea 633) que ejecuta la cuarentena ASI06 y avisa con `tracing::warn!` en tres casos: `JoinError`, error de SQLite y `rows == 0` (nodo desaparecido); devuelve `Result<usize, String>` para el test, pero el call-site de producción (`línea 931`, dentro de `cfg(not(test))`) mantiene `let _ =` (mismo flujo). **Fila 2**: `reinforce_node` con `if let Err(…) { warn!(…) }`. **Fila 4**: `friction_or_warn("layer4_hybrid_decision", …)` (línea 626). **Fila 3**: `log_decision_example` con `if let Err(…) { warn!(…) }`. |
| `security/friction_log.rs` | Nuevo `friction_or_warn(event_type, description)` (línea 432) + `FrictionWarnLimiter` (línea 389, `AtomicU64` last/suppressed, intervalo 60 s, mismo patrón que `audit_or_warn`) + `now_unix_secs()`. Los tres helpers públicos (`log_routing_mismatch`, `log_blocklist_rejection`, `log_semantic_noise`) mantienen su retorno `()` y ahora llaman a `friction_or_warn` → cubre filas 5, 6 y 7. |
| `security/dispatch_executor.rs` (línea 133) | **Fila 8**: `if let Err(error) = child.wait() { warn!(…) }` en el hilo reaper. |
| `security/dispatch_queue.rs` (línea 165) | **Fila 10**: `if let Some(parent) = … && let Err(error) = std::fs::create_dir_all(parent) { warn!(…) }`. |
| `router/matcher.rs` (línea 537) | **Fila 9**: `friction_or_warn("routing_ambiguous", …)` en el tiebreaker semántico. |

**Sin cambios de retorno/flujo:** la cuarentena sigue ignorando el resultado en producción (solo se añade warn), los helpers de fricción siguen devolviendo `()`, `child.wait()` se sigue llamando igual y `DispatchQueue::open` sigue devolviendo el error de `open_db` con contexto. Excluidos `api_contracts.rs`, `transport/http/mod.rs` y `api_federation.rs`.

## 2. Tests nuevos (3)

| Fichero y línea | Test | Qué fija |
|---|---|---|
| `security/friction_log.rs`, línea 1043 | `friction_warn_limiter_warns_once_per_minute_and_counts_suppressed` | `Some(0)` → `None` → `None` → `Some(2)` (1 warn/minuto + contador de suprimidos) |
| `security/friction_log.rs`, línea 1056 | `friction_or_warn_warns_on_failure_and_keeps_unit_return` | Con el path thread-local de la DB de fricción apuntando a un directorio (open falla): `friction_or_warn` **avisa** (`"friction event write failed"` capturado) y devuelve `()`; `log_routing_mismatch` conserva su retorno `()` |
| `security/coherence_gate.rs`, `judge_dedup_tests::layer4_quarantine_failure_warns` | — | Con la conexión SilvaDB en `PRAGMA query_only=ON`, `quarantine_node_for_layer4` devuelve `Err` y **avisa** con `"quarantine UPDATE failed"` (capturado). Usa runtime multi-thread propio porque `SilvaDB::in_memory` usa `block_in_place` |

## 3. Verificación (salidas reales)

- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **exit 0** (30.39 s en la última pasada).
- `cargo test -p tylluan-kernel --lib` → **1266 passed; 0 failed; 4 ignored** (47.15 s). Base del encargo 1263; **+3 tests nuevos = 1266**.
- `cargo test -p tylluan-kernel --test security_audit` → **40 passed; 0 failed; 0 ignored** (0.02 s).
- `git diff --stat`: `coherence_gate.rs +116`, `friction_log.rs +110`, `dispatch_queue.rs +12`, `dispatch_executor.rs +8`, `matcher.rs +3`; total **+229/−20**, solo en los 5 ficheros del encargo.
- `check_doc_links.py` exit 0; `check_docs_reality.sh` exit 1 solo por el artefacto WSL conocido (`ROADMAP_O3.md`, línea 42).
- `check_test_count.sh` → **exit 1** (espera 1353 = 1266+75+12; README/STATUS siguen en 1350); **no ejecuté `--fix`**, es integración del TL.

## Impact

- Cierra las filas 1-8 y 10 de T-DSH-76: un fallo de la **cuarentena ASI06** decidida por Layer 4 (JoinError, error SQLite o `rows == 0`) deja de ser invisible; la penalización KeepSoft, el dataset de decisiones, los eventos de fricción (incluido **blocklist rejection**), el reaper del camino HITL y la creación del directorio de la cola BWC ahora avisan sin cambiar el comportamiento.
- La fricción usa un helper único `friction_or_warn()` con rate limit (1 warn/minuto + `suppressed`) copiando el patrón ya aceptado de `audit_or_warn`; una ráfaga de fallos de SQLite no inunda el log y se conserva el conteo.
- Cero cambios de retorno/flujo: los tests lo fijan (`let ()` en friction, `Ok`/`0` intactos, `open` sigue devolviendo el error con contexto) y no se toca seguridad/permisos.

## No verificado

- **No arranqué el kernel** ni provoqué un fallo real de SQLite en producción; la verificación es unitaria (captura de warns en el hilo del test) más clippy/lib/security_audit.
- **La cuarentena del test se fuerza con `PRAGMA query_only=ON`**, no con un JoinError real (panics del blocking task); esa rama (`Err(join_error)`) no está cubierta por test, solo por revisión.
- **`friction_or_warn` en producción** escribe en la DB de auditoría real; no medí su frecuencia de fallo ni el volumen de warns bajo fallo sistémico (el rate limit 1/min está probado a nivel de limitador, no con fallos reales encadenados).
- **Filas 2-4** viven dentro de `#[cfg(not(test))]` (Layer 4 solo en builds de producción): el warn de `reinforce_node` y `log_decision_example` no se ejecuta en la suite; solo la cuarentena se testea vía helper.
- **No ejecuté** `scripts/verify.sh` completo ni el resto de suites de `tests/` (fuera del criterio); no toqué otros crates.
- **Gates de docs:** `check_test_count.sh` falla por recuento (1353 real vs 1350 en README/STATUS); deuda de integración del TL.
- Sin commit ni push: diff sin commitear.
