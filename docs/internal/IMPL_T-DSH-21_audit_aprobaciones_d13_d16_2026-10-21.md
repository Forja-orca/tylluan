# T-DSH-21 — Matriz de audit de aprobaciones de plan (D-13 + D-16)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `e259b4f` == origin/main (push91; D-13 y D-16 en main; T-DSH-20 en diff local, pendiente de integración).
- **Encargo (turno 1494):** solo tests, sin tocar `src/`: contra el router real, demostrar que aprobar por (a) `approve_action` directo, (b) ruta de intención de `tylluan_do` (D-16) y (c) REST (si existe) deja **exactamente una** fila de audit con el **aprobador real** y `human_intervention` coherente con `operator_gate_allows` (documentando el caso no-operador: la fila dice lo que pasó, no que intervino un humano). Tabla ruta × caso (autor, otro agente, operador, anónimo) × esperado × real; si algún caso miente en el audit → `#[ignore]` con motivo y `archivo:línea`. Informe con `## Impact` y «no verificado».

## Entrega

`crates/tylluan-kernel/tests/approve_audit_matrix.rs` — un test contra el router de producción (`build_router`, `POST /mcp` JSON-RPC), audit aislado en un `TYLLUAN_AUDIT_DB` temporal. **1 passed / 0 failed** tras corregir el criterio de "éxito" (el plan fixture apunta a un guild inexistente a propósito: la ejecución devuelve error MCP pero la aprobación sí ocurrió; el éxito se mide por plan consumido + fila de audit).

```text
rustup run 1.99.1 cargo test -p tylluan-kernel --test approve_audit_matrix
# test result: ok. 1 passed; 0 failed (14,4 s)
```

## Tabla ruta × caso × esperado × real

| ruta | caso | esperado | real | veredicto |
|---|---|---|---|---|
| direct | autor se autoaprueba | rechazo `self_approval`, **0 filas**, plan intacto | refused=true, rows=0, pending=true | OK |
| direct | otro agente aprueba | 1 fila, agente=`t21-other`, `human_intervention=0`, preview con autor | rows=1 `("t21-other", 0, "[plan author: t21-author] …")` | OK |
| direct | **operador** (token maestro) | 1 fila, `human_intervention=1` | rows=1 `("anonymous", 1, …)` | OK (ver O1) |
| direct | anónimo aprueba plan ajeno | 1 fila, agente=`anonymous`, `human_intervention=0` | rows=1 `("anonymous", 0, …)` | OK (ver O2) |
| direct | anónimo se autoaprueba | rechazo `self_approval`, 0 filas, plan intacto | refused=true, rows=0, pending=true | OK |
| intent | autor se autoaprueba | rechazo `self_approval`, 0 filas, plan intacto | refused=true, rows=0, pending=true | OK |
| intent | otro agente aprueba | 1 fila, agente=`t21-other`, `human_intervention=0` | rows=1 `("t21-other", 0, …)` | OK |
| intent | **operador** | 1 fila, `human_intervention=1` | rows=1 `("anonymous", 1, …)` | OK (ver O1) |
| intent | anónimo aprueba plan ajeno | 1 fila, agente=`anonymous`, `human_intervention=0` | rows=1 `("anonymous", 0, …)` | OK (ver O2) |
| intent | anónimo se autoaprueba | rechazo `self_approval`, 0 filas, plan intacto | refused=true, rows=0, pending=true | OK |
| REST | `POST /api/v1/plans/{id}/approve` | no existe | HTTP 404 | OK (ruta inexistente) |
| REST | `POST /api/v1/plan/{id}/approve` | no existe | HTTP 404 | OK |
| REST | `POST /api/v1/approvals/{id}/approve` | no existe | HTTP 404 | OK |

Extra verificado en todas las filas aprobadas: **segunda aprobación del mismo plan no añade filas** (el plan ya está consumido; se mantiene 1 fila exacta) y **los rechazos no escriben audit** (0 filas, plan intacto, gate `self_approval` machine-readable). Las filas `direct` e `intent` son **idénticas** para el mismo caso: D-16 enruta a la misma `approve_action`, compuerta y audit compartidos (no hay dos copias), que es justo lo que su comentario en `handler_do/mod.rs:371-400` afirma.

## Observaciones (no son mentiras del audit; son límites del diseño actual)

- **O1 — La identidad del operador no se materializa en `agent_id`.** Cuando aprueba el operador (token maestro) sin `agent_id`, la fila queda `agent_id="anonymous"` y `human_intervention=1`. La señal honesta es la columna `human_intervention`; el `agent_id` no dice "operador". Mejora recomendada (requiere `src/`): si `operator_gate_allows` es cierto, escribir `"operator"` (o el `agent_id` declarado) en `agent_id` para que la fila sea autoexplicativa.
- **O2 — Un cliente anónimo puede aprobar el plan de OTRO agente.** No es autoaprobación (el autor es distinto) y el audit lo registra honestamente (`anonymous`, `human_intervention=0`), pero sin binding token→identidad el "otro agente" es un string declarable: es la misma familia que T-DSH-15 (recomendación: requerir operador para toda aprobación) y D-13/D-16. Aquí solo se documenta; no hay fila mentirosa.
- **O3 — El spec `approve_action_self_approval` (`#[ignore = "D-13"]`) sigue fallando solo en CASO 2**, como desde T-DSH-15: exige `operator_required` para "otro agente", pero el contrato implementado permite la aprobación cruzada. Decisión de José/TL pendiente; esta matriz documenta el contrato **real** y por eso **no** marca ningún `#[ignore]` propio.
- **O4 — No hay ruta REST de aprobación de plan** (las tres rutas plausibles dan 404; en `api_v1/routes.rs` solo existen `dispatches/{id}/approve` y `federation/peers/{name}/approve`, que son otra feature). Aprobar planes es solo MCP (`/mcp` `tools/call`), por diseño actual.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test --test approve_audit_matrix` | **1 passed / 0 failed** (14,4 s); 13 filas de matriz |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1152 passed / 0 failed / 4 ignored** (sin cambios: todo el diff son tests de integración) |
| `approve_action_self_approval -- --ignored` | sigue fallando **solo CASO 2** (decisión pendiente, ver O3) |

## Impact

- **Diff sin commitear: solo tests** — un fichero nuevo (`tests/approve_audit_matrix.rs`) y este informe. **Cero cambios en `src/`**, cero escrituras en el audit real (`TYLLUAN_AUDIT_DB` temporal), sin kernels arrancados, sin secretos.
- Coste CI: +14,4 s (un test de integración; el fixture no arranca guilds, el error "Plan guild not found" es intencional y hermético).

## No verificado

- **REST**: la ausencia de ruta se demuestra sondeando 3 rutas plausibles (404) y el listado de rutas; si existiera una ruta con otro prefijo no la habría probado (no hay grep de "plan" en `api_v1/routes.rs` que registre aprobación de planes).
- **Transportes distintos de `/mcp`** (stdio/Nexus): no ejecutados; la matriz usa el router HTTP real.
- **Cadena de hashes del audit**: solo se comprueban columnas (`agent_id`, `human_intervention`, `result_preview`), no la verificación de integridad de la cadena (`tylluan-cli audit`).
- **Ejecución real del guild del plan**: no se ejecuta (guild inexistente a propósito); lo auditado es la compuerta + fila, no el efecto de la herramienta.
- **`agent_id` real del operador**: O1 es análisis de código + fila observada; no hay usuario/claims de operador en la fila.
