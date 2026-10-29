# IMPL D-69 — Binding bearer→agent_id en Work Contracts

**Agente:** Deep
**Base:** `origin/main` = `4f0cb3a`.
**Tipo:** diff sin commitear. Solo el TL comitea y empuja.

## Cambios

- `crates/tylluan-kernel/src/transport/http/api_v1/api_contracts.rs`:
  - nuevo helper `bound_agent_mismatch(declared)` que compara `declared` con `current_bound_agent_id()` y devuelve `403 agent_identity_mismatch` cuando no coinciden.
  - `contract_deliver_handler`, `contract_vote_handler` y `contract_close_handler` aplican el check antes de buscar o mutar el contrato.
  - `contract_create_handler` aplica el check usando `agent_id` del payload si existe; si no, usa `consolidator` como identidad actuante. `CreateContractRequest` gana `agent_id: Option<String>` con `#[serde(default)]`, aditivo y compatible.
- Sin binding configurado (`current_bound_agent_id()` vacío/None) el comportamiento es el anterior.

## Tests

`crates/tylluan-kernel/tests/contracts_audit.rs` añade cinco casos con `ACL_AGENT_ID` scopeado:

- create con `agent_id` distinto del binding → 403;
- create con `agent_id` igual → 201;
- deliver como otro agente → 403;
- vote como otro agente → 403;
- close como otro agente → 403.

Resultado: **12/12 tests de `contracts_audit`**, 0 fallos. Los tests existentes sin binding siguen pasando.

## Nota sobre la regresión de Buffy

En el árbol `4f0cb3a` no existe todavía el test `#[ignore]` de B-1 que Buffy describió en su informe; su B-1 se entregó como evidencia de sonda, no como fichero de test. Por eso no había un `#[ignore]` que quitar aquí. Los cinco tests anteriores son la regresión permanente del binding y no llevan `#[ignore]`; cuando Buffy suba su test, debe ejecutarse sin cambios contra este diff.

## Impact

- **Seguridad:** un bearer ligado ya no puede entregar, votar, cerrar ni crear un contrato declarando otro `agent_id`; el creador sin `agent_id` usa el consolidador como identidad.
- **Compatibilidad:** `agent_id` en create es opcional; sin binding no cambia nada.
- **No depredación:** solo `api_contracts.rs` y su suite; no se tocó producción.

## No verificado

- No ejecuté una sesión HTTP real con dos tokens; los tests usan el mismo task-local que instala el middleware, sin red.
- No cubrí `tick` ni `context/decision`/`context/pointer`, que no forman parte de la lista D-69; siguen con su comportamiento anterior.
- No eliminé ningún `#[ignore]` de Buffy porque el fichero de su regresión no está en este árbol.
