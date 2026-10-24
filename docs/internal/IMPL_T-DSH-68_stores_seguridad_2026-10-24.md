# T-DSH-68 — `let _ =` / `Err(_) => return` sobre SQLite en stores de seguridad no cubiertos por T-DSH-67 (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` **`97f89a4`** (push143, que integra D-50/D-51): el análisis inicial se hizo sobre `d2e71cf` y **se re-verificaron las 10 filas contra `97f89a4`** tras el push — todas siguen en las mismas líneas (D-50 solo tocó `upsert_activity` de `agent_profile.rs`, no el `new()` citado; D-51 no toca ningún fichero de la tabla). **Sin cambios de código.**
- **Encargo (T1709):** solo lectura; barrer `let _ =` / `Err(_) => return` sobre `rusqlite execute` en stores de seguridad **no cubiertos por T-DSH-67** (`agents_contract`, `journal`, `rate_limiter`, `trust`, `grants`…), tabla ≤10 filas `fichero:línea`, informe con `## Impact` y «No verificado».
- **Método:** barrido con `Select-String` de `let _ =`, `Err(_) =>`, `.ok()` y `if let Ok` en `crates/tylluan-kernel/src` (security/ y stores de estado), lectura del contexto de cada candidato y descarte de lo que está en `#[cfg(test)]`, de lo que propaga el error y de lo ya cubierto por T-DSH-67 (auditoría y peers de federación). Evidencias: `E:\tylluan-targets\dsh-t7-data\t68`.

## Tabla (10 filas, stores de seguridad/estado no cubiertos por T-DSH-67)

| # | Fichero:línea | Patrón | Ruta / impacto si SQLite falla |
|---|---|---|---|
| 1 | `security/trust.rs:186` | `let Ok(mut stmt) = conn.prepare(...) else { return 0; }` | `propagate_untrusted_to_summaries`: un error de SQLite al preparar la query **aborta en silencio** la propagación retroactiva de "no confiable" a resúmenes/digests (defensa anti-envenenamiento de memoria). Devuelve 0 como si no hubiera nada que marcar |
| 2 | `security/trust.rs:206-212` | `let updated = conn.execute("UPDATE nodes SET metadata ..."); if updated.is_ok() { marked += 1 }` | Si el UPDATE del flag `untrusted` falla, no se cuenta ni se loguea: la marca de desconfianza **no se persiste** y nadie se entera (el nodo sigue tratándose como confiable en el siguiente arranque) |
| 3 | `transport/http/api_v1/api_contracts.rs:265, 299, 314, 383, 433, 443, 452, 489` | `let _ = state.contract_db.persist(&entry)` (8 sitios) | Bounded Work Contracts (gobernanza de trabajo en equipo): creación, tick de presupuesto, bloqueo, votos, extensión y cierre se actualizan en memoria y **la persistencia se descarta**; tras un reinicio el contrato puede volver a estado/presupuesto anteriores o perderse |
| 4 | `transport/http/mod.rs:716` | `if let Ok(active) = state.contract_db.load_active()` | En el arranque, un error de lectura de `work_contracts` se traga: los contratos activos **no se restauran** y su presupuesto/estado deja de aplicarse sin aviso |
| 5 | `transport/server/handlers.rs:29` | `let _ = journal.checkin(&agent_id, &format!("tool:{name}"))` | Auto-checkin de **cada** llamada a herramienta soberana (journal de continuidad M40): si falla la escritura, el agente pierde su punto de recuperación sin log |
| 6 | `transport/server/handlers.rs:260` | `let _ = self.journal.as_ref().map(\|j\| j.checkin(&plan.agent_id, "plan:executed"))` | Tras una ejecución aprobada por humano (HITL), el marcador `plan:executed` puede no quedar en el journal (recuperación tras crash) |
| 7 | `memory/silva/recall_feedback.rs:185` | `let _ = self.record_agent_access(&memory_id, now).await` | Signal Loop: el acceso real del agente a una memoria que ha sido útil no se registra → se pierde una fila del dataset con el que se alimenta la defensa CoherenceGate / resolución de feedback |
| 8 | `memory/agent_memory.rs:114` (y `:146`) | `let _ = silva.set_weight(&node_id, 2.0).await` | Tras promover un patrón de fallo repetido a "experiencia", el boost de peso (saliencia) se descarta: el patrón existe pero con el peso por defecto, degradando la señal de memoria |
| 9 | `memory/agent_profile.rs:42, 46, 49, 59` | `conn.execute_batch("ALTER TABLE ... ").ok()` y `CREATE TABLE agent_domain_scores ... .ok()` | Migraciones/creación de esquema del store de perfiles de agente (identidad/competencias que usa el routing) con error descartado: si la DB está bloqueada/corrupta, el esquema queda incompleto y los `upsert_activity` posteriores fallan (y sus callers también descartan) sin aviso |
| 10 | `security/friction_log.rs:350` y `:508-515` (y `:579`, `:603-605`) | `UPDATE ... .ok()` y `Err(_) => return 0.0` / `Vec::new()` | Store de fricción (métricas que consumen Guard/CoherenceGate para señales de riesgo): si falla la escritura de TTFUA o las lecturas de medianas, se devuelven ceros/listas vacías **sin distinguir** de "no hay datos"; el comentario de 506-508 declara la degradación intencional para no panear, pero sigue siendo silenciosa |

## Revisados y correctos (para que no se confundan con la tabla)

- `security/agents_contract.rs`: carga de `.tylluan/agents.toml` con manejo explícito del error (warn/empty), sin escrituras SQLite.
- `security/grants.rs`: `load_plans`/`persist_plans` manejan errores con `tracing::warn!` (plan store de aprobaciones HITL: **bien hecho**, contraste con las filas 3-4).
- `security/rate_limiter.rs`: en memoria, sin SQLite.
- `transport/http/api_v1/api_journal.rs`: `checkin`/`recover`/`cleanup_stale` **propagan** `rusqlite::Result` y el handler devuelve 500; las implementaciones no descartan errores.
- `transport/http/api_v1/api_contracts.rs` (las funciones `ContractDb::persist`/`load_active`): **propagan** el error; el descarte está en los call sites (filas 3-4).
- `security/write_gate.rs`, `security/dispatch_queue.rs`, `security/guard.rs`: revisados, propagan o loguean el error (write_gate loguea los tres casos del `spawn_blocking`).
- `federation/permissions.rs:213` y el resto de auditoría/peers: ya cubiertos por T-DSH-66/67, no se repiten aquí.

## Impact

- **Cierra el hueco que dejó T-DSH-67**: T-DSH-67 cubrió auditoría y peers de federación; aquí quedan documentadas **10 rutas más** de estado de seguridad/gobernanza donde un error de SQLite es invisible, con las tres más sensibles en `trust` (propagación de desconfianza a resúmenes), contratos BWC (presupuesto/estado de gobernanza) y journal/continuidad.
- **Distingue lo bien hecho**: `grants.rs`, `dispatch_queue.rs`, `write_gate.rs` y los stores de journal/contracts (implementación) sí manejan los errores; el patrón dañino es el **call site** que hace `let _ =` sobre un `Result` que el store sí devuelve.
- Da al TL material para una tarea de hardening barata y acotada: loguear (`tracing::warn!`) en los call sites de las filas 1-10, sin cambiar semántica (todos son no-fatales por diseño).

## No verificado

- **Re-verificado tras push143**: comprobé las 10 filas sobre `97f89a4` (push143) y todas siguen en las mismas líneas; D-50 solo modificó `upsert_activity` de `agent_profile.rs` (el `new()` citado en la fila 9 no se movió) y D-51 no toca ficheros de la tabla. Aun así, es una comprobación por línea/contenido, no una ejecución.
- **Tabla estática**: no inyecté fallos de SQLite en cada ruta; la consecuencia es lectura de código, no una prueba en vivo.
- **No barrí** stores de seguridad que no usan `rusqlite` directamente (p. ej. `guild_credential.rs` en memoria, `hooks.rs` regex, `trust` vía Silva para lecturas) ni escrituras de seguridad en otros crates (`tylluan-link`); el encargo era `rusqlite` en stores de kernel.
- **Fila 10**: el comentario del propio código declara la degradación como intencional ("operational error, not a reason to panic"); la marco como silenciosa, no como bug funcional.
- No he tocado código ni ejecutado kernels; todo es `grep`/lectura sobre el worktree.
