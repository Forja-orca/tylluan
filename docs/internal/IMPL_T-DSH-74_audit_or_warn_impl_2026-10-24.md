# T-DSH-74 — Implementación de `audit_or_warn()` (helper único con rate limit) + 6 call sites + tests

- **Agente:** dsh
- **Fecha:** 2026-10-24 (madrugada 2026-10-25)
- **Base:** `origin/main` `215f185` (push147), tras reset. **Diff SIN commitear**; sin commit ni push.
- **Encargo (T1730):** implementar `audit_or_warn()` en `handler_do/audit.rs` tal como se bocetó en T-DSH-73 (rate limit 1 warn/minuto + contador de suprimidos), sustituir las 5 llamadas `let _ = log_audit_entry` y `api_memory.rs` líneas 310-321; NO tocar `federation/permissions.rs`; tests del limitador y del valor de retorno; criterios `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` exit 0 y `cargo test -p tylluan-kernel --lib` verde.

## 1. Cambios (diff sin commitear, 7 ficheros, +123/−11)

| Fichero | Cambio |
|---|---|
| `crates/tylluan-kernel/src/transport/server/handler_do/audit.rs` | Nuevo `audit_or_warn()` (línea 142) + `AUDIT_WARN_INTERVAL_SECS = 60` + `AuditWarnLimiter` con `last_warn_secs`/`suppressed` (`AtomicU64`, `Ordering::Relaxed`) + `now_unix_secs()` + `static AUDIT_WARN_LIMITER`. `log_audit_entry` no se toca. Dos tests nuevos al final del módulo (líneas 484-525). |
| `crates/tylluan-kernel/src/transport/server/handler_do/mod.rs` | Re-export `pub(crate) use audit::audit_or_warn;` (línea 58). |
| `crates/tylluan-kernel/src/transport/server/handlers.rs` | Línea 303 (HITL `approve_action`, `spawn_blocking`): `let _ = …log_audit_entry(` → `…audit_or_warn(`. Línea 609 (cada tool soberana ≠ `tylluan_do`, `tokio::spawn`): idem. El `return Ok(result)` de la línea 308 y el flujo no cambian. |
| `crates/tylluan-kernel/src/transport/server/handler_do/outcome.rs` | Import (línea 11) y llamada (línea 194, camino principal de `tylluan_do`) → `audit_or_warn`; `capture_friction_from_result` sigue igual en la línea 195. |
| `crates/tylluan-kernel/src/transport/server/handler_do/external_mcp.rs` | Import (línea 8) y llamada (línea 188) → `audit_or_warn`. |
| `crates/tylluan-kernel/src/transport/http/api_v1/mod.rs` | Línea 779 (excepción D-34 `silva/edge`, dentro de `drop(spawn_blocking(…))`) → `audit_or_warn`. |
| `crates/tylluan-kernel/src/transport/http/api_v1/api_memory.rs` | Líneas 310-321 (`quarantine:release`): el closure de `spawn_blocking` pasa a llamar `audit_or_warn` (devuelve `()`); comentario de la línea 306 actualizado. |

**No tocado:** `federation/permissions.rs` (ya tenía el patrón correcto con `if let Err … warn!` desde D-51), `api_federation.rs` ni `api_contracts.rs` (excluidos por el encargo, Deep trabaja ahí).

## 2. Semántica del helper

- Firma idéntica a `log_audit_entry` (8 argumentos), devuelve `()`: los 6 call sites son fire-and-forget (`spawn_blocking`/`tokio::spawn`/`drop(spawn_blocking)`), así que el reemplazo es mecánico y no altera el resultado de la tool ni el `Result` del handler.
- En fallo: un único `tracing::warn!(error = %error, suppressed, "audit write failed (best-effort; el flujo continua)")` como máximo **una vez por minuto** (proceso-wide); los fallos intermedios incrementan `suppressed` y el siguiente warn reporta cuántos se suprimieron (y resetea el contador).
- Se usó let-chain (`if let Err(error) = … && let Some(suppressed) = …`) porque `clippy::collapsible-if` con `-D warnings` lo exige (primer intento rechazado por clippy, corregido).

## 3. Tests nuevos (en `audit.rs`)

1. `audit_warn_limiter_warns_once_per_minute_and_counts_suppressed` (línea 486): instancia local del limitador; primer fallo → `Some(0)`; dos fallos dentro del minuto → `None`; al cumplirse 60 s → `Some(2)` (2 suprimidos reportados y contador reseteado).
2. `audit_or_warn_preserves_call_site_value_when_audit_fails` (línea 502): fuerza el fallo apuntando `TYLLUAN_AUDIT_DB` a un directorio (SQLite no puede abrirlo como DB); `let () = audit_or_warn(...)` fija en compilación que devuelve `()`, y el valor del call-site sigue siendo 42. Usa `AUDIT_ENV_TEST_LOCK` (mismo lock que el seam de D-53) y restaura el env.

## 4. Verificación (salidas reales)

- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **exit 0** (1m 39s; `Finished dev profile`).
- `cargo test -p tylluan-kernel --lib` → **1255 passed; 0 failed; 4 ignored** (40.23 s). Baseline del encargo: 1253 lib; **+2 tests nuevos = 1255**.
- `git diff --stat`: 7 ficheros, +123/−11, todo en `crates/tylluan-kernel/src/transport/…` (sin docs ni otros crates).
- Gate de docs de recuento: `scripts/check_test_count.sh` → **exit 1**, porque README/STATUS siguen con 1340 y el total real pasa a **1255+75+12 = 1342**. **No ejecuté `--fix`**: la actualización de README/STATUS la hace el TL al integrar (lo dejo señalado abajo).
- `check_doc_links.py` → exit 0. `check_docs_reality.sh` → exit 1 solo por el artefacto WSL conocido (`ROADMAP_O3.md`, línea 42, `onnx/int8/decima.json`).

## Impact

- **Behavioral:** no cambia ningún resultado funcional ni el valor de retorno de los call-sites; la escritura de auditoría sigue siendo best-effort y non-fatal. Lo único nuevo es un `tracing::warn!` (rate-limited) cuando la auditoría falla, en las 5 rutas del encargo y en `quarantine:release`.
- **Observabilidad:** un fallo de SQLite de la auditoría (DB bloqueada/corrupta/disco lleno) deja de ser invisible; con el rate limit, una ráfaga de N fallos produce 1 warn/minuto con `suppressed=N`, sin inundar el log.
- **Consistencia:** unifica el patrón que `federation/permissions.rs` ya aplicaba desde D-51; `audit_or_warn` queda re-exportado en `handler_do` para futuras rutas.
- **Riesgo:** ninguno de seguridad añadido (el helper no relaja ni endurece permisos; solo loguea). El rate limit es proceso-wide; si en el futuro un call-site viviera en otro proceso, tendría su propio contador.

## No verificado

- **No arranqué el kernel ni probé en vivo** una ruta real con la DB de auditoría bloqueada; el comportamiento se cubre con los dos tests unitarios (limitador y fallo forzado), no con fault injection sobre SQLite real.
- **No medí** la frecuencia real de ráfagas ni el impacto en volumen de log bajo fallo sistémico; el límite 1/minuto se tomó del boceto de T-DSH-73, sin medición.
- **No ejecuté** `scripts/verify.sh` completo ni la suite `tests/` del kernel (solo `--lib` y clippy, según el criterio del encargo); no toqué otros crates.
- **Los gates de docs fallan por recuento** (1342 real vs 1340 en README/STATUS): es deuda de integración del TL, no de este cambio; no lo arreglé para no tocar docs que no me corresponden.
- **El test del limitador** usa un reloj sintético (segundos inyectados); no verifica que el `warn!` de `tracing` se emita realmente una sola vez (no hay captura de logs en el test), verifica la semántica del limitador (una decisión de warn + contador de suprimidos).
- Sin commit ni push: diff sin commitear, como exige el protocolo.
