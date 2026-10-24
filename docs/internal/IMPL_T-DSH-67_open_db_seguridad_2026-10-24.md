# T-DSH-67 — Otros `open_db` / `log_audit_entry` que descartan errores de SQLite en rutas de seguridad (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `d2e71cf` (reset de worktree con `agent_wt.sh reset dsh` antes de empezar). **Sin cambios de código.**
- **Encargo (T1704):** solo lectura; buscar otros `open_db` / `log_audit_entry` con `let _ =` sobre errores de SQLite **en rutas de seguridad**; tabla fichero:línea (máximo 10), sin tocar nada.
- **Método:** `Select-String` sobre `crates/tylluan-kernel/src` (45 llamadas a `open_db`, 6 llamadas de producción a `log_audit_entry`), lectura del contexto de cada candidato y descarte de los que están en `#[cfg(test)]` o que sí propagan el error. Evidencias: `E:\tylluan-targets\dsh-t7-data\t67` (grep y contexto por fila).

## Tabla (10 filas, rutas de seguridad, commit `d2e71cf`)

| # | Fichero:línea | Llamada / error descartado | Por qué es ruta de seguridad | Consecuencia si SQLite falla |
|---|---|---|---|---|
| 1 | `transport/server/handlers.rs:303` | `let _ = handler_do::audit::log_audit_entry(...)` dentro de `spawn_blocking` (auditoría de toda llamada a herramienta soberana, incluida la evidencia HITL/aprobación) | La auditoría es la evidencia de quién ejecutó qué y si hubo aprobación humana (D-13) | La fila de auditoría se pierde **sin log**; el panel/`/api/v1/audit` puede mostrar una ejecución sin rastro |
| 2 | `transport/server/handlers.rs:609` | `let _ = handler_do::log_audit_entry("", "kernel", ...)` (las 5 herramientas soberanas que no son `tylluan_do`) | Misma evidencia de auditoría | Igual: llamada sin rastro de auditoría |
| 3 | `transport/server/handler_do/outcome.rs:194` | `let _ = log_audit_entry(...)` (auditoría del despacho `tylluan_do`) | Rastro central del motor de despacho | Ejecución sin fila de auditoría |
| 4 | `transport/server/handler_do/external_mcp.rs:188` | `let _ = log_audit_entry(...)` (llamadas a guilds MCP externas) | Superficie externa: lo que sale del kernel hacia servidores MCP | Llamada externa sin evidencia |
| 5 | `transport/http/api_v1/mod.rs:779` | `let _ = ... log_audit_entry("POST /api/v1/silva/edge (D-34 guild exception)", ...)` | Única excepción documentada que escribe en Silva por API | Escritura de grafo sin fila de auditoría |
| 6 | `federation/permissions.rs:213` | `let _ = crate::transport::server::handler_do::log_audit_entry(...)` (denegaciones de permisos de federación) — **ya identificada en T-DSH-66** | Auditoría de denegaciones de espacio/peer | Denegación sin evidencia; explicaba el flaky t15 |
| 7 | `transport/http/api_v1/api_federation.rs:306` | `let _ = state.peer_db.update_ed25519_pubkey(&name, pubkey)` | Identidad de peers: la pubkey se usa para verificar firmas | Si el UPDATE falla, el peer queda con `ed25519_pubkey` vacío; `ingest_from_peer` calcula `has_pubkey=false` en `api_federation.rs:113` y **se salta `verify_envelope`** (fallo abierto de firma para ese peer), y el `tracing::info!` de la línea 307 dice que se guardó |
| 8 | `federation/mod.rs:81-84` | `let _ = conn.execute_batch("ALTER TABLE federation_peers ADD COLUMN ed25519_pubkey/external_address/permissions ...")` | Migraciones de columnas de seguridad de la tabla de peers (identidad y permisos) | En una DB legacy, si el ALTER falla (bloqueo, schema), la columna puede quedar ausente y el error es invisible; los INSERT/SELECT posteriores fallarán o usarán defaults (permisos `{}` = deny-all, pubkey vacía = sin verificación de firma) |
| 9 | `transport/http/mod.rs:707` | `let _ = state.peer_db.insert(p)` (bootstrap TOML→DB de `federation_peers`) | Persistencia de peers aprobados en TOML | Si el INSERT falla, la línea 711 relee la lista **autoritativa desde la DB**: los peers del TOML desaparecen en silencio (fail-closed, pero sin aviso y sin rastro de por qué) |
| 10 | `transport/http/api_v1/mod.rs` (líneas 1712 y 1719) | `let db_path = std::path::Path::new("./data/audit.db")` — **no usa `audit_db_path()`** — y `let _ = conn.execute_batch("CREATE TABLE IF NOT EXISTS guild_audit_log ...")` | Endpoint `GET /api/v1/audit` (consulta del rastro con verificación de cadena, M31-P1) | Con `TYLLUAN_AUDIT_DB` definido (seam de tests / override de operador) o con CWD distinto, el endpoint lee **otra DB** que el escritor (`audit.rs:15-19` es el único dueño de la resolución) → el rastro sale vacío/incorrecto; el fallo de creación de tabla también se descarta |

## Contexto de la búsqueda

- `log_audit_entry` tiene **6 llamadas de producción** y las **6 descartan el `Result`** (`let _ =`): filas 1-6. `audit.rs:25-81` sí devuelve `Err` en open/schema/insert; el problema es el caller.
- `open_db` tiene **45 llamadas**; las que descartan el error en rutas de seguridad son las filas 7-9 (escrituras de identidad/peers) y el fallback silencioso de la fila 10.
- Quedan **fuera de la tabla** (no son `let _ =` sobre SQLite en ruta de seguridad, se listan para que no se confundan):
  - `memory/silva/recall_feedback.rs:125`: `open_db(Err) => return Ok((0,0))` — degradación silenciosa del Signal Loop (0 resoluciones) si el audit DB no se puede abrir.
  - `memory/agent_memory.rs:35/49/52`: `open_db`/`prepare`/`query_map` con `Err(_) => return 0` — el harvest de fallos devuelve 0 sin avisar.
  - `router/scheduler/confusion.rs:222`: degradación **documentada y deliberada** (WS7: tiples a cero con nota "store not created yet", nunca `Err`).
  - `transport/server/handlers.rs:806`: `.ok()?` pero es un helper de **tests** (`#[cfg(test)]`).
  - `security/friction_log.rs:180`: ruta por defecto hardcodeada en producción (`./data/audit.db`), mismo default que el kernel, sin drift en producción; su variante `#[cfg(test)]` usa DB scratch.
  - `.unwrap()` de `open_db` en `memory/night/mod.rs:451` y otros son **código de tests**.

## Impact

- **Cuenta exacta y verificable del agujero**: no es solo la denegación de federación de T-DSH-66; **todos los caminos de auditoría del kernel** (herramientas soberanas, despacho `tylluan_do`, MCP externo, excepción Silva, denegaciones de federación) descartan el error de SQLite, así que cualquier fallo de escritura es invisible para el operador.
- **Hallazgo de seguridad concreto (fila 7)**: un fallo al persistir la pubkey de un peer deja ese peer **sin verificación de firma** (`has_pubkey=false`) y aun así el log dice que se guardó; es el patrón más peligroso de la tabla porque degrada una comprobación de identidad en silencio.
- **Hallazgo funcional/seguridad (fila 10)**: `GET /api/v1/audit` no usa el resolvedor único `audit_db_path()`; con `TYLLUAN_AUDIT_DB` o CWD distinto lee otra base que la que escribe el kernel, así que la verificación de cadena puede consultar un rastro vacío.
- Sin cambios de código; hallazgos listos para que el TL decida qué entra en D-51 (orden de PRAGMAs + warn del error de auditoría) y qué merece tarea propia.

## No verificado

- **No he reproducido fallos** de estas rutas: la tabla es estática (lectura de código), no una prueba de inyección de error de SQLite en cada una.
- **No analicé todas las rutas de seguridad** del kernel: me ceñí a `open_db` y `log_audit_entry`; puede haber escrituras de seguridad con otras APIs (`rusqlite::Connection::open` directo, `execute` con `let _ =` en stores como `agents_contract`, `journal`, `rate_limiter`) que no barrí en esta pasada.
- **Fila 7**: no verifiqué en vivo que un fallo de `update_ed25519_pubkey` deje el `peer.ed25519_pubkey` vacío en la lista recargada (el flujo es `reload_peers_cache` → DB); lo deduzco del código (el UPDATE es la única escritura y el `tracing::info!` incondicional está en la línea 307).
- **Fila 10**: no probé el endpoint con `TYLLUAN_AUDIT_DB` definido; la conclusión es de lectura (resolvedor único en `audit.rs:15-19` vs ruta hardcodeada en `mod.rs`, línea 1712).
- No he tocado código ni ejecutado kernels; solo `grep`/lectura sobre el worktree en `d2e71cf`.
