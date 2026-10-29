# T-DSH-124 — AUD-04/05: roster, quórum y persistencia en `api_contracts.rs`

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = d995340` (push190). Diff sin commitear: 2 ficheros + 1 test nuevo.
- Encargo (T2025): AUD-04/05 en `api_contracts.rs` — entregas y votos por participante único validado contra el roster; quórum sobre el roster (hoy 1 voto de 1 = mayoría); `persist_contract` devuelve 5xx si falla (hoy `let _ =` en 5 sitios). Tests a nivel handler: no miembro, entrega duplicada, voto parcial, empate, fallo de BD.

## Cambios (diff sin commitear)

### AUD-04 — roster y quórum (`api_contracts.rs`)

- **`contract_deliver_handler`**: solo un `agent_id` del `team` (roster) puede entregar; si no es miembro → **403** `agent not in contract roster`. Una sola entrega por participante; duplicada → **409** `duplicate delivery` (antes se aceptaba cualquier agente y los duplicados contaban como entregas distintas).
- **`contract_vote_handler`**: solo vota un miembro del roster (si no → **403**); se mantiene un voto por participante (revotar reemplaza). **Quórum sobre el roster**: `majority = roster > 0 && approves * 2 > roster` (antes `approves > total_votos/2`, así que 1 voto de 1 era mayoría). Empate y voto parcial dejan `majority: false`.
- Los handlers `create`, `tick`, `deliver`, `vote` y `close` pasan a devolver el tipo concreto `(StatusCode, Json<serde_json::Value>)` (antes `impl IntoResponse`): mismo contrato HTTP, y permite test a nivel handler sin parsear la respuesta.

### AUD-05 — persistencia con 5xx (`api_contracts.rs`)

- Nuevo helper `persist_error_response(entry_id, e)` → **500** `contract_persist_failed` con `contract_id` y detalle.
- Sustituidos los **6** `let _ = persist_contract(...)` por `if let Err(e) = ... { return persist_error_response(&id, e); }` en `create`, `tick` (2), `deliver`, `vote` (3 ramas) y `close`. Antes un fallo de SQLite se perdía en el log y el cliente recibía 200 aunque el contrato no estuviera guardado.

### Tests nuevos (nivel handler)

- `crates/tylluan-kernel/tests/contracts_audit.rs` (7 tests, runtime multi-thread, harness `support::test_state`):
  1. `deliver_rejects_agent_not_in_roster` → 403.
  2. `deliver_rejects_duplicate_participant` → 409 en la segunda entrega del mismo agente.
  3. `partial_vote_is_not_majority_over_roster` → 1 voto de 2 no es mayoría.
  4. `tie_is_not_majority` → 1 approve + 1 reject no es mayoría.
  5. `vote_rejects_agent_not_in_roster` → 403.
  6. `persist_failure_returns_500` → se fuerza el fallo dropeando `work_contracts` desde otra conexión a la misma DB; el handler devuelve 500 `contract_persist_failed` con el `contract_id`.
  7. `close_by_non_consolidator_is_forbidden` → 403 (regresión del gate existente).
- `tests/support/mod.rs`: nuevo `test_state_with_contract_db(identity_tag, db_path)` (refactor interno `build_state` con path de contract DB) para poder inyectar una DB fallable; `test_state_custom` conserva su firma y comportamiento (`:memory:`).

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1403 passed; 0 failed; 4 ignored
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test contracts_test          → 5 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test contracts_audit         → 7 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test blackboard_e2e          → 8 passed (sin regresión)
```

## Impact

- Un contrato ya no admite entregas de agentes fuera del roster ni entregas duplicadas, y el quórum de extensión se calcula sobre el roster completo: 1 voto de 1 **no** desbloquea nada. Cierra AUD-04 (BWC con 1 participante no podía ser mayoría de sí mismo).
- Un fallo de SQLite al persistir ya no se traga: el handler devuelve **500** con el `contract_id`, así que el cliente sabe que el contrato no quedó guardado (AUD-05).
- Los tests cubren los cinco casos pedidos y el gate de cierre por consolidador.

## No verificado

- No se probó contra el kernel arrancado por HTTP real: los tests llaman a los handlers directamente con `HttpState` de test (sin middleware/auth). El comportamiento de rutas/ACL no cambia, pero no se hizo una petición HTTP end-to-end.
- `persist_failure_returns_500` fuerza el fallo con `DROP TABLE` desde otra conexión: es un fallo de SQLite real, pero no cubre otros modos (disco lleno, DB bloqueada, cifrado).
- El contrato en memoria queda mutado aunque el persist falle y el handler devuelva 500 (misma semántica que antes en cuanto a memoria; no se revierte el cambio en el `DashMap`). No se probó reconciliación posterior.
- No se midió concurrencia real (dos entregas simultáneas del mismo agente podrían pasar ambas la comprobación antes de insertar; el `get_mut` del `DashMap` serializa por contrato, pero no se testea con hilos).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004).
