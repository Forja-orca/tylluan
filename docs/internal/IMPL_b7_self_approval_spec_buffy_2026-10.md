# B-7 — Prueba end-to-end de self-approval (especificación ejecutable D-13)

**Fecha:** 2026-10-21 · **Agente:** Buffy · **Worktree:** `E:\tylluan-wt-buffy` (HEAD `8001d23` + diff sin commitear, regla 9) · **Código del kernel: NO MODIFICADO** (solo se añade el fichero de test).

## Qué se entregó

[crates/tylluan-kernel/tests/approve_action_self_approval.rs](../crates/tylluan-kernel/tests/approve_action_self_approval.rs) — un único test secuencial `d13_self_approval_requires_operator_credential`, marcado **`#[ignore = "D-13"]`** (mismo patrón que la suite OAuth F2: la suite normal lo salta y CI sigue verde; la corrida con `-- --ignored` es la demostración del bug y posteriormente la verificación de D-13).

### Contrato que codifica (los 4 casos de la cola)

1. **Autor aprueba su propio plan sin operador** → rechazo con token `self_approval`; el plan NO se consume; sin audit.
2. **Otro agente aprueba sin operador** → rechazo con token `operator_required`; plan intacto; sin audit.
3. **Operador (token maestro en cabecera Authorization) aprueba** → la aprobación procede (plan consumido + ejecución), HTTP 200.
4. **El audit solo registra `human_intervention=true` cuando el aprobador es operador.**

### Modelo de identidad y credencial fijado en la prueba

- Identidad del aprobador: argumento `agent_id` del `tools/call` (mismo mecanismo que el resto de herramientas, `handlers.rs:23`).
- Credencial de operador: token maestro **por cabecera** (`AclAuthKind::MasterToken` → `operator_gate_allows`, auth.rs). Los agentes autentican con JWT OAuth (scope mcp) que **no** satisface la clase operador.
- El router es el de producción (`build_router`) con `dev_mode=false`, para que el bypass de desarrollo no contamine el caso "no operador".
- Superficie del rechazo: acepta HTTP 403 **o** 200+`result.isError` con el token de error en el cuerpo (el `tools/call` MCP devuelve normalmente 200+isError); lo no negociable es el token machine-readable + plan intacto + sin audit. La cola describe los casos 1-2 como "403"; si Deep los implementa como 403 puro, la prueba también los acepta.
- El plan fixture apunta a un guild inexistente a propósito: la prueba especifica la **compuerta de aprobación**, no la ejecución del guild; así el test es hermético (sin spawn de procesos Python) y el camino de aprobación es idéntico (consume plan + audita).

## Criterio de la cola: la salida de hoy

`cargo test -p tylluan-kernel --test approve_action_self_approval -- --ignored` → **exit 101**, fallando exactamente por la razón descrita:

```
D-13 (self-approval de approve_action) — 6 caso(s) sin cumplir:
  - CASO 1 (autor aprueba su propio plan sin operador): esperado rechazo con token
    `self_approval` (HTTP 403 o 200+isError), obtenido HTTP 200 OK: {"id":1,"jsonrpc":"2.0",
    "result":{"content":[{"text":"❌ Error: Plan guild 'd13_probe' not found","type":"text"}],
    "isError":true,"resultType":"complete"}}
  - CASO 1: el plan fue consumido por una autoaprobacion no autorizada (debe seguir
    pendiente hasta que un operador lo resuelva)
  - CASO 2 (otro agente aprueba sin operador): esperado rechazo con token
    `operator_required`, obtenido HTTP 200 OK: (mismo cuerpo que CASO 1)
  - CASO 2: el plan fue consumido por un agente no operador (debe seguir pendiente)
  - CASO 4: existe(n) 1 fila(s) de audit con human_intervention=[1] para intents
    rechazados ('D-13 caso-autor') — solo el aprobador operador debe generar auditoria HITL
  - CASO 4: existe(n) 1 fila(s) de audit con human_intervention=[1] para intents
    rechazados ('D-13 caso-otro-agente') — solo el aprobador operador debe generar auditoria HITL
test result: FAILED. 0 passed; 1 failed; 0 ignored
```

Es decir, hoy: **cualquier cliente MCP autenticado (incluso solo con un JWT) aprueba su propio plan**, el plan se consume y se ejecuta, y el audit queda con `human_intervention=1` — una mentira que contamina el ASR y las métricas de fricción. Los casos 3 y 4-positivo (operador) ya pasan hoy y no aparecen en la lista.

Corrida normal (sin `--ignored`): `test result: ok. 0 passed; 0 failed; 1 ignored` — **exit 0**, CI verde.

## Notas de método (relevantes para Deep al implementar D-13)

- El test llama `grants::init()` + `grants::init_plan_store()` en el setup: sin eso, `approve_action` panica con `GrantRegistry not initialized` (el registro solo se inicializa en el arranque real, `main.rs`). D-13 no debe depender de que el caller lo init.
- Seam de audit: `TYLLUAN_AUDIT_DB` (temp file por binario de test, patrón de `adversarial_http_battery.rs`); la fila HITL se escribe fire-and-forget en `spawn_blocking`, así que la parte positiva del caso 4 hace polling.
- **Segunda superficie de aprobación**: `tylluan_do(intent='approve action for plan <id>')` también resuelve planes (`handler_do/mod.rs:557`). El contrato de esta prueba cubre la herramienta `approve_action` (lo que pide la cola), pero Deep debería aplicar el mismo guard en la ruta de intent, o la compuerta se elude por ahí. No lo specifiqué en código para no ampliar el encargo.

## No verificado / limitaciones

- No verificado: qué hará Deep exactamente con la superficie (403 HTTP vs isError MCP) — la prueba acepta ambas con token obligatorio; si el TL quiere endurecer a "403 exacto", cambiar una línea en `is_refused_with`.
- No se probó el camino `approve_action` sobre grants oneshot (no-plan, `list_pending_actions`): el encargo es el plan mode/HITL.
- La ejecución del guild en el caso 3 devuelve `Plan guild 'd13_probe' not found` (intencional, ver arriba); la prueba no afirma nada sobre la ejecución en sí.

## Impact (ADR-013)

Un único fichero nuevo en `crates/tylluan-kernel/tests/` (zona de tests, no `src/`), marcado `#[ignore]` → no afecta a la suite normal ni a `check_test_count.sh` (que cuenta `--lib`). Sin cambios en umbrales de gates, sin tocar el kernel de producción ni sus datos. `scripts/check_docs_reality.sh` y `check_no_predation.sh` se ejecutan al cierre de la cola.

## Checks ejecutados

- `cargo test -p tylluan-kernel --test approve_action_self_approval` → exit 0 (1 ignored).
- `cargo test -p tylluan-kernel --test approve_action_self_approval -- --ignored` → exit 101 con la salida de arriba (la demostración pedida).
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **exit 0**.
