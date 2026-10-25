# T-DSH-75 — Hacer visibles (warn, sin cambiar retornos) los errores descartados de T-DSH-67: federation migrations, recall_feedback y agent_memory

- **Agente:** dsh
- **Fecha:** 2026-10-25 (madrugada)
- **Base:** `origin/main` `65fec42` (push148, que ya integra T-DSH-74), tras `agent_wt.sh reset dsh`. **Diff SIN commitear**; sin commit ni push.
- **Encargo (T1732):** 1) `federation/mod.rs` líneas ~81-84 (`let _ = conn.execute_batch` de las tres migraciones ALTER: ignorar solo "duplicate column", warn en cualquier otro error); `memory/silva/recall_feedback.rs` línea ~125 (`open_db` Err ⇒ `Ok((0,0))`); `memory/agent_memory.rs` líneas ~35/49/52 (`return 0`). 2) Un test por comportamiento cambiado. 3) `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` exit 0 y `cargo test -p tylluan-kernel --lib` verde (base 1255). 4) Excluidos `transport/http/mod.rs`, `api_contracts.rs` (Deep, D-57) y `audit.rs`.

## 1. Cambios (diff sin commitear: 4 ficheros modificados + 1 nuevo, +276/−30)

| Fichero | Cambio |
|---|---|
| `crates/tylluan-kernel/src/federation/mod.rs` (línea 64) | Nuevo `is_duplicate_column_error(message)` (solo `"duplicate column name"` es esperado). Las tres migraciones pasan de `let _ = conn.execute_batch(...)` a un bucle (línea 91) con `if let Err(error) = … && !is_duplicate_column_error(...) { tracing::warn!(error, sql, "federation: safe migration failed (non-fatal; continuing)") }`. `open` sigue devolviendo `Ok` en errores de migración (no cambia el flujo). |
| `crates/tylluan-kernel/src/memory/silva/recall_feedback.rs` (línea 18) | Nuevo `open_audit_db_for_feedback(path) -> Option<Connection>`: warn con `error` y `path` si `open_db` falla, `None` para el caller. `resolve_pending_feedback` usa el helper y mantiene su `Ok((0, 0))`. |
| `crates/tylluan-kernel/src/memory/agent_memory.rs` (línea 13) | Nuevo `collect_failure_rows(audit_db_path, cutoff) -> Option<Vec<...>>`: warn en fallo de open (antes línea 35), de `prepare` (antes 52) y de `query_map` (antes 49); `None` para el caller. `harvest_failures_from_audit` usa el helper y sigue devolviendo `0` en error. |
| `crates/tylluan-kernel/src/lib.rs` (+4) | `#[cfg(test)] pub(crate) mod test_support;` para el helper de captura de logs de los tests. |
| `crates/tylluan-kernel/src/test_support.rs` (nuevo, 55 líneas) | `capture_warns<F: FnOnce()>(f) -> String`: subscribe un `tracing_subscriber::fmt` con `MakeWriter` a buffer compartido y devuelve lo logrado en WARN+ (thread-local). Solo existe en `cfg(test)`. |

**No tocado:** `transport/http/mod.rs`, `api_contracts.rs`, `audit.rs` (excluidos), ni retornos/flujo público: todos los caminos siguen devolviendo lo mismo (`Ok`, `Ok((0,0))`, `0`).

## 2. Tests nuevos (8; uno o más por comportamiento cambiado)

| Fichero y línea | Test | Qué fija |
|---|---|---|
| `federation/mod.rs`, línea 430 | `migration_error_classification` | `"duplicate column name"` ⇒ ignorable; `"too many columns"`, `"database is locked"`, `"no such table"` ⇒ no ignorable |
| `federation/mod.rs`, línea 441 | `migration_duplicate_column_is_ignored_without_warn` | Abrir la misma DB dos veces (las 3 ALTER son duplicadas) ⇒ `capture_warns` no contiene `safe migration failed` |
| `federation/mod.rs`, línea 464 | `migration_non_duplicate_error_warns` | Tabla precreada en el límite de columnas de SQLite (2000) para que ALTER falle con "too many columns" ⇒ `capture_warns` contiene `safe migration failed`; `PeerDb::open` sigue devolviendo `Ok` |
| `recall_feedback.rs`, línea 642 | `audit_db_open_failure_warns_and_returns_none` | `open_audit_db_for_feedback(<directorio>)` ⇒ `None` y warn `audit DB unavailable` capturado |
| `recall_feedback.rs`, línea 663 | `resolve_pending_feedback_keeps_zero_contract_when_audit_db_unavailable` | Con una fila pendiente y audit DB inválida ⇒ `Ok((0, 0))` (contrato intacto) |
| `agent_memory.rs`, línea 845 | `collect_failure_rows_warns_when_audit_db_unavailable` | Directorio como DB ⇒ `None` + warn `audit DB unavailable` capturado |
| `agent_memory.rs`, línea 865 | `collect_failure_rows_warns_when_audit_schema_is_missing` | DB SQLite válida sin `guild_audit_log` ⇒ `None` + warn `failed to prepare` capturado |
| `agent_memory.rs`, línea 887 | `harvest_returns_zero_when_audit_db_is_unavailable` | `harvest_failures_from_audit` sigue devolviendo 0 |

## 3. Verificación (salidas reales)

- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **exit 0** (2m 41s).
- `cargo test -p tylluan-kernel --lib` → **1263 passed; 0 failed; 4 ignored** (41.33 s). Base del encargo 1255; **+8 tests nuevos = 1263**.
- `git diff --stat`: `federation/mod.rs +92`, `lib.rs +4`, `agent_memory.rs +137/−?`, `recall_feedback.rs +73`, más `test_support.rs` nuevo (untracked). Sin otros ficheros.
- `scripts/check_test_count.sh` → **exit 1**: espera **1350** (1263+75+12) y README/STATUS siguen en 1342; **no ejecuté `--fix`** (integración de docs del TL). `check_doc_links.py` exit 0. `check_docs_reality.sh` exit 1 solo por el artefacto WSL conocido (`ROADMAP_O3.md`, línea 42).

## Impact

- **Comportamiento:** ninguno cambia en los retornos ni en el flujo; lo único nuevo son `tracing::warn!` en errores que antes se descartaban en silencio. `PeerDb::open` sigue siendo tolerante a fallos de migración; `resolve_pending_feedback` sigue devolviendo `Ok((0,0))`; `harvest_failures_from_audit` sigue devolviendo 0.
- **Observabilidad:** un fallo real de migración de federación (bloqueo, esquema corrupto, límite de columnas) deja de ser invisible y se distingue del caso normal (columna duplicada, que no avisa); un fallo al abrir/preparar/consultar la DB de auditoría en recall feedback y en el harvest Ouroboros también queda visible.
- **Tests:** se añade un helper solo-test (`test_support::capture_warns`) que permite verificar la emisión/no emisión de warns sin depender del log global; los 8 tests cubren los casos "no avisa" y "sí avisa" de cada cambio.
- **Riesgo:** bajo; no toca seguridad, permisos ni rutas; el test del límite de columnas es sintético (crear 2000 columnas) y depende del `SQLITE_MAX_COLUMN` por defecto de SQLite — si una build lo cambiara, el test fallaría de forma explícita.

## No verificado

- **No arranqué el kernel** ni provoqué en vivo un fallo de SQLite real; la verificación es unitaria (captura de warns en el hilo del test) más clippy/lib tests.
- **El test de "otro error" en federación** fuerza el fallo con el límite de columnas de SQLite, no con un fallo real de bloqueo/corrupción; no probé el escenario `database is locked` de forma determinista.
- **No ejecuté** `scripts/verify.sh` completo ni la suite de `tests/` del kernel (solo `--lib` y clippy, criterio del encargo); no toqué otros crates.
- **Gates de docs:** `check_test_count.sh` falla por el recuento (1350 real vs 1342 en README/STATUS); es deuda de integración del TL, no la arreglé para no tocar docs ajenas.
- **El helper de captura es thread-local**: solo captura eventos del hilo del test; los tests llaman a la lógica en ese hilo (los warns se emiten síncronamente antes de cualquier `await`), pero no probaría warns emitidos desde tareas `spawn`/`spawn_blocking`.
- Sin commit ni push: diff sin commitear.
