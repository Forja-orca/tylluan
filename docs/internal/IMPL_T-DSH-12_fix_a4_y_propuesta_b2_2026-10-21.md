# T-DSH-12 — Fix mínimo de A4 (`@inbox` legacy) y propuesta de localidad por contrato (B2)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `57d2e7b` == origin/main (push 83; T-DSH-11 ya integrada, lib 1142).
- **Encargo (turno 1473):** (1) fix mínimo de **A4** (`@inbox` con payload legacy sin clave `untrusted`: tratar como no confiable si el remitente no tiene identidad local, en `handler_recall.rs`) como diff SIN commitear con test que pase A4 y no rompa A3, solo esos 2 ficheros; (2) revisar **B2** y **proponer sin implementar** el criterio de localidad por contrato. Informe con `## Impact` y «no verificado».

## (1) Fix de A4 — lectura defensiva en `@inbox`

**Diff sin commitear: 2 ficheros exactamente.**

- `crates/tylluan-kernel/src/transport/server/handler_recall.rs` (único `src`): el bloque `@inbox` deja de decidir la desconfianza solo con `BlackboardMessage.untrusted` (que por `#[serde(default)]` vale `false` en payloads anteriores a D-18) y pasa a aplicar un **criterio de lectura**:
  - si el payload **declara** `untrusted` (true o false), se respeta tal cual (decisión del productor);
  - si **no** lo declara y el remitente **no tiene identidad local** en este kernel, se trata como no confiable y se le pone el banner de `security::trust`.
- `crates/tylluan-kernel/tests/trust_inbox_legacy.rs` (nuevo, **no ignorado**): 4 aserciones — A4 (legacy externo → banner), A3 regresión (flag declarado `true` → banner), control negativo (legacy de remitente **local** → sin banner), y flag declarado `false` explícito → no se pisa.

**Verificación:**

| check | resultado |
|---|---|
| `cargo test --test trust_inbox_legacy` | **1 passed** |
| Matriz `trust_banners_adversarial -- --ignored` post-fix | **A4 → OK**, **A3 → OK**; quedan **B2 y C2** (`2 quebrados de 12`) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1142 passed / 0 failed / 4 ignored** (recuento intacto) |

Coste: una consulta de identidad por mensaje del buzón (el mismo `get_identity` que ya usa `mailbox_send_handler`), limitada por el `limit` del recall; no toca el gate ni el ranking.

## (2) B2 — revisión y propuesta de localidad por contrato (sin implementar)

**Por qué B2 está abierto.** `mailbox_send_handler` decide la localidad así (`api_v1/mod.rs` líneas 156-180):

```text
sender_is_local = IdentityManager::get_identity(&req.from).is_some()
```

y `register_identity` (o el auto-bootstrap de la primera llamada, `handlers.rs` líneas 55-68) crea esa identidad para **cualquier** `agent_id` declarado. Resultado: un remitente externo que se registra (o que ya hizo una llamada) pasa a "local" y su correo no se marca. La frontera de confianza quedó en "existe un nodo", que cualquiera puede crear.

**Propuesta (3 capas, por fases; ninguna implementada aquí):**

1. **Localidad = pertenencia al contrato, no existencia de nodo.** Fuente de verdad: `.tylluan/agents.toml` (contrato declarativo ya cargado por el kernel) y, si existe, el binding `[security.acl.token_agent_bindings]`. Un remitente es *local* solo si está en el roster; todo lo demás es *externo* (marcado), aunque tenga identidad. Las identidades auto-creadas pasan a ser **"declaradas, no verificadas"** (p. ej. `role = "unregistered"`, que ya existe) y no conceden localidad.
2. **Enforcement del binding en TODAS las superficies de escritura, no solo MCP.** Hoy `handlers.rs` líneas 33-39 rechaza suplantación en `tools/call` cuando hay binding, pero **las rutas REST no aplican ese chequeo**: `mailbox_send_handler` acepta `req.from` libre y `api_coloquio` acepta `author_id` libre. Propuesta: extraer ese chequeo a un helper y aplicarlo en `mailbox/send`, `coloquio/*/post` y cualquier escritura con autor declarado. Con `token_agent_bindings` vacío (config actual) el binding no protege nada; la fase 2 requiere decidir si se llena.
3. **Retrocompatibilidad y migración.** Fase 1 con roster es compatible: los agentes reales (`claude-code`, `deep`, `antigravity`, `buffy`, `dsh`…) ya están en `agents.toml`; los que no, se marcan hasta que se añadan. Las identidades existentes se tratan como "declaradas"; un operador puede vetarlas. Esto encaja con D-23/D-24 (canonicalización de `agent_id`), porque el roster es la tabla canónica natural.

**Decisiones que requieren a José/TL:** (a) si `token_agent_bindings` se puebla (implica repartir tokens por agente y reautenticar clientes); (b) qué pasa con remitentes no listados en el roster (marcar siempre vs rechazar en `mailbox/send`); (c) si la localidad debe ser "estática" (roster) o "viva" (roster + binding verificado en la petición). Mi recomendación: **roster como base + binding cuando exista**, marcar por defecto, y no rechazar en el buzón (el buzón es un canal de entrada; rechazar rompería la interoperabilidad con agentes externos legítimos).

## Impact

- **Cambio de código en un único archivo** (`handler_recall.rs`, ~35 líneas) + **un test nuevo**; no toca el gate, el ranking, el formato de respuesta ni la escritura de mensajes. ADR-013: el único efecto es añadir banner a contenido legacy de remitentes sin identidad local; nada cambia para remitentes locales ni para payloads que declaran el flag.
- Cero escrituras en producción (estado de test en memoria, audit en temporal, kernel vivo intacto); sin secretos.
- La propuesta de B2 es **solo diseño**: no se ha implementado ni activado nada.

## No verificado

- **Coste real de la consulta de identidad por mensaje** en buzones grandes: se mide indirectamente (histórico de 5 mensajes en la matriz), no con un buzón de cientos de mensajes.
- **Productores legacy reales**: cuántos payloads sin la clave existen hoy en el buzón de producción y de qué remitentes (la copia de T-DSH-8 los tiene, pero no se han contado por remitente).
- **Efecto de la fase 1 de B2 sobre clientes actuales**: no hay bindings configurados en producción, así que no puedo verificar qué clientes dependen de `req.from` libre en `mailbox/send`.
- **Canonicalización (`agent_id`)**: sigue en D-23/D-24 de Deep; la propuesta la asume, no la resuelve.
