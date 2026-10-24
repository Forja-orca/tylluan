# T-DSH-66 — Causa raíz probable del flaky `federation_audit::t15_denials_are_audited` (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `d2e71cf` (reset de worktree con `agent_wt.sh reset dsh` antes de empezar). **Sin cambios de código.**
- **Encargo (T1699):** solo lectura; comparar el log del run fallido (`gh run view` del run de `d2e71cf`, intento 1) con el código de `federation_audit.rs` (`test_state` / `TYLLUAN_AUDIT_DB`, fix `b7ba7e7`) y de `log_audit_entry`; tabla **hipótesis → fichero y línea → cómo falsearla**.
- **Evidencias:** `E:\tylluan-targets\dsh-t7-data\t66` (`job120903699110.log`, `wal_race_sim.py`/`.log`, `inspect_audit.py`, `repro_loop*.log`, logs de compilación del test).

## 1. Log real del fallo (CI `d2e71cf`, run 40104647726, **attempt 1**, job `Rust — security audit tests` 120903699110)

```
2026-10-24T14:56:40.4968573Z test t15_denials_are_audited ... FAILED
...
2026-10-24T14:56:40.7320963Z ---- t15_denials_are_audited stdout ----
thread 't15_denials_are_audited' (2677) panicked at crates/tylluan-kernel/tests/federation_audit.rs:548:5:
a permission denial must leave a guild_audit_log row (got 0)
...
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s
```

Datos del entorno del intento 1: Ubuntu 24.04, runner 4 vCPU, `cargo test -p tylluan-kernel --test federation_audit` (sin `--test-threads`, 12 tests en paralelo, `.github/workflows/ci.yml` línea 397 y siguientes). Attempt 2 (mismo commit) verde; local 12/12 (TL) y 12/12 en 12 vueltas frescas propias (ver §4).

## 2. Código implicado (leído, no modificado)

- `crates/tylluan-kernel/tests/federation_audit.rs`:
  - `test_state()` líneas 26-66: fija `TYLLUAN_AUDIT_DB` **una sola vez** con `std::sync::Once` (56-66) a un fichero en `std::env::temp_dir()` (`tylluan_audit_fed_{contador}.db`).
  - `t15_denials_are_audited` líneas 530-550: registra peer `paudit` con permisos `{}` → `ingest_from_peer(..., "receive")` → assertion `cnt >= 1` sobre `guild_audit_log WHERE intent LIKE 'peer paudit:%'` (línea 548).
- `crates/tylluan-kernel/src/federation/permissions.rs` líneas 210-216: `audit_permission_denied()` formatea `intent = "peer {peer}: {flow} denied — {reason}"` y llama a `log_audit_entry(...)` **sincrónicamente** con `let _ =` (**el error se descarta**).
- `crates/tylluan-kernel/src/transport/http/api_v1/api_federation.rs` líneas 107-138: `ingest_from_peer()`; la denegación por espacio (134-137) llama a `audit_permission_denied(&peer.name, ...)` y devuelve `IngestOutcome::Denied`.
- `crates/tylluan-kernel/src/transport/server/handler_do/audit.rs`:
  - `audit_db_path()` líneas 15-19: `TYLLUAN_AUDIT_DB` o, si no está, `./data/audit.db`.
  - `log_audit_entry()` líneas 25-81: `open_db(path)` (línea 32), `CREATE TABLE IF NOT EXISTS` (33-46), 3 `ALTER TABLE ADD COLUMN` con error ignorado (54-56), `SELECT hash ... LIMIT 1` (62-66), `INSERT` (76-80). Devuelve `Err` en open/schema/insert; **el caller de permissions.rs no lo propaga ni lo loguea**.
- `crates/tylluan-kernel/src/config.rs`: `open_db()` empieza en la línea 1488; el batch de PRAGMAs está en 1529-1535:
  ```
  1530: "PRAGMA journal_mode=WAL; \
  1531: PRAGMA busy_timeout=5000; \
  ```
  **El busy_timeout se aplica DESPUÉS del cambio a WAL.**

## 3. Tabla hipótesis → fichero y línea → cómo falsearla

| # | Hipótesis | Fichero:línea | Cómo falsearla / estado |
|---|---|---|---|
| **H1 (principal)** | En CI el fichero de auditoría es **nuevo** (runner limpio) y varios tests lo abren a la vez; `PRAGMA journal_mode=WAL` corre **antes** de `busy_timeout`, así que una conexión puede fallar con *database is locked*; `open_db`/`log_audit_entry` devuelven `Err` y `audit_permission_denied` lo **descarta** (`let _ =`): la fila de t15 no llega a insertarse. En local el fichero `%TEMP%\tylluan_audit_fed_*.db` **persiste entre runs** (ya en WAL) y la carrera no aparece. | `config.rs` 1530-1531 (orden de pragmas); `audit.rs` 32 y 76-80 (error propagado); `permissions.rs` 213-215 (`let _ =`); `federation_audit.rs` 56-66 (fichero único compartido) | **Medio probado**: simulación con 12 hilos sobre SQLite nuevo y el orden actual → **278/360 errores "database is locked"**; con `busy_timeout` antes de WAL → **11/360** (`wal_race_sim.log`). En local, 84 vueltas (12+12+60) borrando el fichero antes de cada run → **84/84 pasan** (probabilidad baja con solo 4 escrituras concurrentes; ver §4). Falsearla del todo: repetir en CI con `TYLLUAN_AUDIT_DB` apuntando a un fichero preexistente en WAL (debería dejar de fallar), o inyectar un lock artificial; y comprobar si `crates/tylluan-kernel/data/audit.db` aparece en el workspace del run fallido (ver H2). |
| **H2** | La fila va a `./data/audit.db` (fichero por defecto) y t15 lee el fichero del seam: requiere que `TYLLUAN_AUDIT_DB` no estuviera visible en el hilo que escribe (env es global; `set_var` es `unsafe` en edition 2024). | `audit.rs` 15-19 (fallback); `federation_audit.rs` 56-66 (`set_var` en `Once`); `permissions.rs` 213 | **Parcialmente descartada**: en local el `crates/tylluan-kernel/data/audit.db` tiene **0 filas** con `intent LIKE 'peer %'` (solo d13/d28 etc. del binario de tests lib), así que las denegaciones federadas van siempre al seam. En CI se falsearía mirando el workspace del job fallido: si existiera `crates/tylluan-kernel/data/audit.db` con filas `peer paudit:`, H2 sería la causa; si no aparece, H2 cae. |
| **H3** | El intent auditado no empieza por `peer paudit:` (p. ej. resolución por x25519 → `peer x25519:...`), así que la fila existe pero el `LIKE` no la encuentra. | `permissions.rs` 212 (formato del intent); `api_federation.rs` 134-137 (`peer.name`); `federation_audit.rs` 543 (`LIKE 'peer paudit:%'`) | **Descartada localmente**: en los `%TEMP%\tylluan_audit_fed_*.db` todas las filas de denegación llevan `peer paudit: receive denied — space 'x' ...` (consultado con `inspect_audit.py`). Se falsearía en CI listando `DISTINCT intent` del fichero del seam tras un fallo (no hay artefacto de ese run). |
| **H4** | La fila se escribe **después** de la query (auditoría asíncrona). | `permissions.rs` 210-216 + `api_federation.rs` 134-138 | **Descartada por código**: la llamada es síncrona dentro de `ingest_from_peer`, antes de devolver `Denied`; no hay `tokio::spawn` en esa ruta. Se falsearía con un grep de `spawn` en la ruta o viendo que con `--test-threads=1` tampoco cambia (los 12/12 locales ya lo apuntan). |
| **H5** | Visibilidad WAL: t15 lee con otra conexión y no ve la fila recién insertada. | `config.rs` 1530 (WAL); `federation_audit.rs` 540 (`open_db` para leer) | **Descartada en la práctica**: WAL publica los commits al abrir la conexión de lectura; 12/12 locales con fichero persistente y 12/12 frescos no muestran este patrón. Se falsearía con `PRAGMA journal_mode=DELETE` temporal (scratch) o leyendo la misma conexión. |
| **H6** | Carrera en la cadena de hash: dos escritores leen el mismo `prev_hash` y un INSERT falla. | `audit.rs` 62-66 (SELECT last) y 76-80 (INSERT) | **Descartada como causa de cnt=0**: no hay constraint único sobre `prev_hash`/`hash`, ambos INSERT pueden convivir; la cadena quedaría "rota" (detectable con `verify_audit_chain`) pero la fila existiría. Se falsearía contando filas vs `verify_audit_chain` en un DB con escrituras concurrentes. |
| **H7** | Carrera de esquema: los `CREATE TABLE IF NOT EXISTS` + 3 `ALTER TABLE ADD COLUMN` concurrentes cambian el schema y el `INSERT` de t15 recibe `SQLITE_SCHEMA` (error no reintentado por rusqlite). | `audit.rs` 33-46 (DDL) y 76-80 (INSERT) | **Descartada en la simulación**: con `busy_timeout` activo antes de WAL, 360 conexiones emulando CREATE+3×ALTER+SELECT+INSERT dieron **0 errores en CREATE/ALTER/SELECT/INSERT** (solo 36/360 en el paso de pragmas). Si la causa fuera de esquema, los errores aparecerían en esos pasos. |

## 4. Reproducción local intentada (honesta)

- Compilado el binario de test (`cargo test -p tylluan-kernel --test federation_audit --no-run`, 6 m 54 s) y ejecutado directamente con `--test-threads=12`.
- **12 vueltas borrando los `%TEMP%\tylluan_audit_fed_*.db` antes de cada una (condición CI): 12/12 pasan**.
- **12 vueltas con el fichero persistente (condición local): 12/12 pasan**.
- **60 vueltas adicionales con fichero borrado antes de cada run: 60/60 pasan** (`repro_loop60.log`). En total **84 vueltas locales sin un solo fallo**.
- Conclusión: la carrera de H1 es plausible por el orden de PRAGMAs y la simulación, pero **no la he reproducido** con la suite real en esta máquina (solo 4 escrituras concurrentes por run y disco rápido); el flaky de CI necesita o bien una ventana más amplia (runner lento/4 vCPU, `/tmp` sobre overlayfs) o una causa que no he aislado. Lo dejo explícito como límite del análisis, no como causa probada.

## Impact

- **Señala el punto exacto donde una fila de auditoría puede perderse en silencio**: `log_audit_entry` devuelve `Err` y `audit_permission_denied` lo tira con `let _ =` (`permissions.rs` 213-215), así que un fallo de SQLite (bloqueo, open, insert) es invisible en los logs; el test solo ve `got 0`.
- **Aporta una causa concreta y medible del "solo en CI"**: el fichero del seam es nuevo en CI y persistente en local (el `Once` usa un nombre estable `tylluan_audit_fed_N.db`), y `open_db` cambia a WAL **antes** de activar el busy_timeout (`config.rs` 1530-1531), donde una simulación con 12 conexiones da 278/360 bloqueos frente a 11/360 con el orden inverso.
- Deja descartadas por código tres hipótesis alternativas (fila tardía/asíncrona, nombre del intent por x25519, visibilidad WAL) y una comprobación local que acota el problema.

## No verificado

- **No he reproducido el fallo con la suite real**: 24 vueltas locales (12 frescas + 12 persistentes) pasan; la hipótesis H1 está apoyada por la simulación y el orden de pragmas, no por un fallo observado aquí.
- **No tengo el fichero de auditoría del run fallido de CI** (no se publica como artefacto): no puedo confirmar si la fila se perdió por error de SQLite (H1) o fue a otro fichero (H2).
- **No he tocado código** (encargo solo lectura): las propuestas de arreglo (poner `busy_timeout` antes de `journal_mode`, no descartar el error de auditoría, o que t15 reintente/use fichero propio) no están implementadas ni probadas.
- La simulación es Python/SQLite genérica, no `rusqlite` con el busy handler exacto de `open_db`; sirve para mostrar la ventana, no para cuantificar la tasa real del test.
- No he analizado si otros tests de la suite (d13, d28) arrastran el mismo patrón de "escribir mientras el seam cambia"; solo el caso t15 pedido.
