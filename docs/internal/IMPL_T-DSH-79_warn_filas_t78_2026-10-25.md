# T-DSH-79 — Implementación de las 10 filas de T-DSH-78 (warn sin cambiar retornos; fail-closed solo en la fila 1)

- **Agente:** dsh
- **Fecha:** 2026-10-25
- **Base:** `origin/main` `19e54a8` (`docs(status): sync HEAD citation`), tras `agent_wt.sh reset dsh`. **Diff SIN commitear**; sin commit ni push.
- **Encargo (T1740):** fila 1 (`delete_node`) distinguiendo `QueryReturnedNoRows` de cualquier otro error (warn + **fail-closed** `Ok(false)`); fila 2 (`trust.rs:34`) solo warn si el JSON no parsea y la metadata no es vacía ni `"{}"`, sin cambiar el retorno; fila 3 warn + `continue` sin reescribir metadata ilegible; filas 4-10 solo `tracing::warn!` sin cambiar retornos/flujo (fila 7: warn sin abortar; fila 6: warn una vez). Un test por fila donde sea factible con `capture_warns`. Excluidos `api_contracts.rs`, `transport/http/mod.rs`, `api_federation.rs`.

## 1. Cambios (6 ficheros, +350/−20)

| Fila (T-DSH-78) | Fichero | Cambio |
|---|---|---|
| 1 | `memory/silva/nodes.rs` (`delete_node`) | `protected` con `match`: `Ok` → igual; `QueryReturnedNoRows` → `false` (comportamiento actual, id inexistente); cualquier otro error → `warn!` + `return Ok(false)` (**fail-closed**: no borra). |
| 2 | `security/trust.rs` (`is_untrusted_metadata`) | `match` del parseo: `Ok` → misma lógica; `Err` → `warn!` solo si `trim()` no es vacío ni `"{}"`, y devuelve `false` (retorno intacto; decisión fail-open/fail-closed de José). |
| 3 | `security/trust.rs` (propagación retroactiva) | JSON ilegible → `warn!` + `continue` (ya no se reescribe con `json!({})`). |
| 4 | `memory/silva/nodes.rs` (`upsert_node_with_validity`) | `parsed_meta` con `match`: en `Err` `warn!` (si la metadata no es vacía/`{}`) y `None` (mismo flujo). |
| 5 | `security/trust.rs` (consulta de resúmenes) | `filter_map` con `match`: fila `Err` → `warn!` y se omite (antes se descartaba en silencio). |
| 6 | `security/coherence_gate.rs` (`resolve_self_auth_token`) | Si tras env+fichero no hay token: `warn_missing_self_token_once()` con `Once` estático (**una vez por proceso**); error de lectura distinto de `NotFound` con warn propio; retorno `Option<String>` intacto. |
| 7 | `memory/silva/nodes.rs` (`delete_node`, `fts_rowid`) | `match`: `Ok`/`QueryReturnedNoRows` → igual; otro error → `warn!` y `None` (**no aborta** el borrado). |
| 8 | `transport/http/api_v1/api_security.rs` | Nuevo `feedback_counts(resolved, pending)` que avisa y devuelve `0` en `Err`; el handler usa el helper (contrato de respuesta `0` intacto). |
| 9 | `transport/http/api_v1/api_silva.rs` | `list_profiles()` y `get_domain_reputation()` con `unwrap_or_else`: `warn!` + `Vec::new()` (mismo vacío que antes). |
| 10 | `federation/permissions.rs` (`peer_by_x25519`) | Pubkey no decodificable → `warn!` y `false` (sigue denegando: fail-closed, ahora con rastro). |

Sin cambios de retorno/flujo salvo el fail-closed pedido en la fila 1; el resto solo añade warns.

## 2. Tests nuevos (9; todos con `capture_warns`)

| Fila | Test |
|---|---|
| 1 | `delete_node_protected_check_failure_is_fail_closed_and_warns`: `ALTER TABLE nodes RENAME COLUMN protected …` fuerza el error → `Ok(false)` + warn `fail-closed` + el nodo sigue existiendo (contado por SQL directo). |
| 1 | `delete_node_missing_id_stays_ok_without_failclosed_warn`: id inexistente → `Ok(false)` sin warn de fail-closed (comportamiento actual). |
| 2 | `is_untrusted_metadata_warns_on_unparsable_but_keeps_false`: `"{not-json"` → warn y `false`. |
| 2 | `is_untrusted_metadata_silent_for_empty_and_empty_object`: `""`, `"{}"` no avisan; `{"untrusted_content":true}` sigue devolviendo `true`. |
| 4 | `upsert_node_invalid_metadata_warns`: upsert con `"{not-json"` → warn `metadata JSON inválida` y el upsert no falla. |
| 6 | `missing_self_token_warns_once`: `Once` inyectable → el mensaje se emite exactamente 1 vez en dos llamadas. |
| 8 | `feedback_counts_warns_and_keeps_zero_contract` y `feedback_counts_does_not_warn_on_success`. |
| 10 | `peer_by_x25519_warns_on_undecodable_pubkey`: pubkey `"no-es-hex"` → `None` + warn `no decodificable`. |

**Filas 3, 5, 7 y 9 sin test** (no factibles sin scaffolding pesado): la 3 y la 5 dependen de forzar errores de fila/JSON dentro de la propagación retroactiva sobre `SilvaDB`; la 7 exige romper solo la tabla `nodes` después de la comprobación de `protected` (la 1 ya rompe la columna y no llega a la 7); la 9 requiere un `AgentProfileStore` con fallo inyectable. Quedan cubiertas por revisión de código.

## 3. Verificación (salidas reales)

- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **exit 0** (43.72 s).
- `cargo test -p tylluan-kernel --lib` → **1275 passed; 0 failed; 4 ignored** (49.23 s). Base 1266 + **9 tests nuevos = 1275**.
- `cargo test -p tylluan-kernel --test security_audit` → **40 passed; 0 failed; 0 ignored**.
- `git diff --stat`: `nodes.rs +127`, `trust.rs +78`, `api_security.rs +61`, `coherence_gate.rs +44`, `permissions.rs +44`, `api_silva.rs +16`; total **+350/−20**.
- Primer intento de clippy falló (`x25519_from_ed25519_hex` devuelve `Option`, no `Result`, así que no acepta `map_err`): corregido con `match` (el `warn` se emite solo cuando hay pubkey no vacía pero indescifrable).
- `check_doc_links.py` exit 0; `check_docs_reality.sh` exit 1 solo por el artefacto WSL conocido (`ROADMAP_O3.md`, línea 42). `check_test_count.sh` seguirá fallando hasta que el TL sincronice la cifra (ahora 1275+75+12 = **1362**; README/STATUS en 1353): **no ejecuté `--fix`**.

## Impact

- Cierra las 10 filas de T-DSH-78: el fallo de lectura de `protected` en `delete_node` deja de ser fail-open (un nodo protegido ya **no** se puede borrar si la comprobación falla, y queda warn), y la marca ASI06 `untrusted_content` avisa cuando la metadata es ilegible sin cambiar el retorno (la decisión de tratarla como no confiable queda para José).
- El resto son avisos de una línea sin cambio de flujo: metadata ilegible no se reescribe (fila 3), procedencia no se pierde en silencio (fila 4), filas ilegibles dejan rastro (fila 5), Layer 4 avisa una vez si no hay token (fila 6), limpieza FTS avisa sin abortar (fila 7), métricas de seguridad no bajan a cero en silencio (fila 8), perfiles/reputación (fila 9) y pubkey indescifrable en P2P (fila 10).
- 9 tests nuevos fijan el comportamiento (incluido que el fail-closed no borra y que el warn de token es único).

## No verificado

- **Sin kernel en vivo** ni inyección real de errores de SQLite; la verificación es unitaria (warns capturados) + clippy/lib/security_audit.
- **Fila 1**: el error se fuerza renombrando la columna `protected` en una DB en memoria; no se probó una DB corrupta/bloqueada real ni el caso `JoinError`. El fail-closed no se probó con un nodo realmente protegido (la columna rota impide leerlo; se verifica que el nodo sigue existiendo por SQL directo).
- **Fila 6**: el test cubre el helper `warn_missing_self_token_once` con un `Once` inyectable; no se prueba `resolve_self_auth_token` contra un entorno sin token (cwd/env globales no deterministas en paralelo).
- **Filas 3, 5, 7, 9**: sin test (ver arriba); solo revisión de código.
- **Fila 10**: el warn se emite por peer y por llamada de `peer_by_x25519`; no medí volumen en un mesh grande con muchas pubkeys corruptas (podría ser ruidoso).
- No ejecuté `scripts/verify.sh` completo ni otras suites fuera del criterio; no toqué los ficheros excluidos (D-57/D-58).
- Gates de docs: solo los fallos conocidos (artefacto WSL y recuento de tests pendiente de integración del TL).
