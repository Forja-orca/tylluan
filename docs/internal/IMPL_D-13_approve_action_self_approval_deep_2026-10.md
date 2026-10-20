# D-13 — `approve_action` sin comprobar al aprobador

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` @ `8001d23`
**Parche:** `E:\\tylluan-patches\\deep\\D-13.patch` (12.301 B, 1 archivo, +223/−2)
**Estado:** implementado y verificado en local. **Sin commitear.**

---

## El caso exacto, antes y después

`tylluan_do` de un agente planifica en modo `plan: true` → `store_plan(...)` guarda
`PendingPlan { …, agent_id: <autor> }`. El plan espera aprobación humana.

**Antes** (`handlers.rs`, rama `"approve_action"`):

```rust
if !resolved && approved
    && let Some(plan) = crate::security::grants::get_plan(request_id).await
{
    // … ejecuta plan.tool con plan.args …
    let audit_agent = plan.agent_id.clone();   // el AUTOR, no el aprobador
    log_audit_entry(…, &audit_agent, …, true)  // human_intervention hardcodeado
}
```

No había ninguna comparación entre quién llama y `plan.agent_id`. Un agente podía
`plan: true` y aprobar su propio plan en la llamada siguiente, y el log de auditoría
lo registraba como `human_intervention = true` **y** atribuido al autor. Dos
afirmaciones falsas en la misma fila.

**Después:**

```rust
let operator = auth::operator_gate_allows(auth::current_auth_kind());
let is_anonymous = |who: &str| who.is_empty() || who == "anonymous";
let is_self = plan.agent_id == agent_id
    || (is_anonymous(&plan.agent_id) && is_anonymous(&agent_id));
if is_self && !operator {
    return Ok(CallToolResult {                      // plan NO consumido
        content: vec![Content::text(json!({
            "error": "self_approval", "request_id": …, "author": …, "approver": …,
            "hint": "another agent, or the operator token, must approve this plan"
        }).to_string())],
        is_error: Some(true),
    });
}
…
let audit_agent = agent_id.clone();                 // el APROBADOR real
let human_intervention = operator;                  // true solo con credencial de operador
```

Tres decisiones declaradas:

- **Autor desconocido (`""`) y `"anonymous"` son la misma identidad.** Si ambos lados son
  anónimos, es auto-aprobación: es exactamente la forma que tenía un bucle desatendido
  (`record_experience` → plan → approve sin `agent_id` en ninguno de los dos pasos).
- **"Clase operador" = `operator_gate_allows(...)`**, la misma función que decide el gate
  HTTP de rutas de operador: `MasterToken` o `DevBypass`. No inventé una segunda noción de
  "humano". El token por query-string (`MasterTokenQuery`) **no** cuenta, coherente con E1.
- **El plan no se consume al rechazar.** Se comprueba antes de `remove_plan`, así que el
  plan sigue disponible para un aprobador legítimo. Un 403 que además borrase el plan
  sería una denegación de servicio trivial sobre el trabajo pendiente del agente.
- El autor se conserva en el `result_preview` (`[plan author: X] …`) cuando difiere del
  aprobador: la columna `agent_id` pasa a ser el actor real sin perder la trazabilidad.

**Sobre el `403`:** el contrato de la cola pide `403 {"error":"self_approval"}`. MCP no
tiene códigos de estado: `approve_action` se invoca como tool y devuelve
`CallToolResult`. Lo que se entrega es `is_error: true` + el JSON
`{"error":"self_approval", …}` con esos campos. Si el TL quiere un 403 HTTP real, hace
falta un endpoint REST dedicado a aprobar planes (hoy no existe); **no** lo inventé.

## Verificación

```
cargo test -p tylluan-kernel --lib transport::server::handlers::tests::approve_action
  -> 5 passed; 0 failed   (0,44 s)

cargo test -p tylluan-kernel --lib
  -> test result: ok. 1115 passed; 0 failed; 4 ignored   (38,42 s)   [base 1110 + 5]

cargo clippy -p tylluan-kernel --all-targets -- -D warnings
  -> Finished … EXIT=0
```

Los 5 tests:

1. `approve_action_refuses_self_approval_and_keeps_plan_pending` — autor `agent-a` aprueba
   su propio plan → `is_error`, texto `self_approval`, y `get_plan(..)` **sigue** `Some`.
2. `approve_action_accepts_another_agent` — autor `agent-a`, aprobador `agent-b` → sin
   error de self-approval y el plan queda consumido.
3. `approve_action_blocks_anonymous_self_approval` — plan anónimo aprobado sin `agent_id`
   → bloqueado y pendiente; después `agent-b` sí lo ejecuta (el anónimo no es un callejón
   sin salida).
4. `approve_action_operator_may_self_approve` — con `ACL_AUTH_KIND = MasterToken` y
   aprobador == autor → pasa y ejecuta. Fricción cero para el humano.
5. `approve_action_audit_names_the_approver_and_human_only_for_operator` — lee la fila real
   de `guild_audit_log` (polling con deadline de 10 s sobre el store de auditoría):
   aprobación entre agentes → `agent_id = "agent-b"`, `human_intervention = 0`;
   aprobación con token maestro → `agent_id = "agent-a"`, `human_intervention = 1`.

Los tests inicializan `grants::init()` / `init_plan_store()` porque `approve_action` llama
a `resolve()` antes de mirar el plan y el registro sólo lo inicializa el bootstrap real
(ambos son `OnceLock::set`, idempotentes).

## Impact

- **Seguridad:** cierra la auto-aprobación de planes por agente. El operador conserva la
  capacidad de aprobar cualquier cosa, incluido su propio plan.
- **Auditoría:** `human_intervention` deja de estar hardcodeado a `true` en esta rama y
  `agent_id` identifica al aprobador real. Las métricas de ASR / fricción operativa que
  leen esa columna dejan de contar aprobaciones entre agentes como humanas.
- **Comportamiento observable:** un agente que antes "funcionaba" auto-aprobándose ahora
  recibe `self_approval` y su plan queda pendiente. Es el objetivo, pero es un cambio de
  contrato para cualquier cliente que lo hiciera.
- **Sin cambios** en `dev_mode`, umbrales de gates ni valores por defecto.
- **No-depredación (ADR-013):** un solo archivo (`handlers.rs`), rama `approve_action` +
  su módulo de tests. No toca ficheros de otros agentes.

## No verificado (declarado)

- El flujo HTTP completo de aprobación (no hay endpoint REST de aprobar planes; sólo el
  tool MCP), así que no hay un `curl` real que enseñar.
- Que un cliente MCP concreto (Claude Desktop, Qwen) muestre bien `is_error` + JSON de
  error: la forma es la estándar de MCP, pero no la probé en un cliente real.
