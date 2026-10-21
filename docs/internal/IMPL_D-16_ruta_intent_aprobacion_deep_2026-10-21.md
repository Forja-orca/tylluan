# D-16 — Segunda superficie de aprobación: la ruta de intent de `tylluan_do`

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` = `043eb71` (detached, diff sin commitear)
**Parche:** `E:\tylluan-patches\deep\D-16.patch` (14.278 B, 2 archivos, +313/−2)

---

## El hallazgo, antes del arreglo

La cola dice: «`tylluan_do` con la intención de aprobar un plan (`handler_do/mod.rs`, ruta de
`approve action for plan`) también resuelve planes y hoy no lleva el guard de `self_approval`
(aviso de Buffy en B-7)».

**Esa ruta no existe.** `handler_do/mod.rs:557-560` es la *frase que el kernel emite* al cliente
en modo plan — una cadena de texto, no un handler. Lo verifiqué con el test escrito antes del
arreglo, contra el router de producción:

```
CASO 1 (autor aprueba por intent sin operador): esperado rechazo `self_approval`,
obtenido HTTP 200 OK:
{"result":{"content":[{"text":"❌ Error: NO_GUILD_MATCH: No guild found for intent:
 'approve action for plan d16planself'. Tried guild-context routing
 (agent_role=agent-author)…","isError":true}}}
```

Es decir: no había ni compuerta ni aprobación. La instrucción que el propio kernel imprime
(`To approve, call: tylluan_do(intent='approve action for plan <id>')`) moría en el router
semántico con `NO_GUILD_MATCH`. B-7 apuntó a la línea del mensaje y la leyó como handler; el
efecto neto es peor que el que describía la cola —una superficie de aprobación documentada que
no aprueba nada— y el arreglo correcto es el mismo que pide el encargo.

## Qué se hizo

Una sola cosa: **enrutar la instrucción al `approve_action` que ya existe**, para que la
compuerta de D-13 y su audit sean uno solo, no dos copias (requisito explícito: «reutiliza la
lógica de D-13, no la dupliques»).

- `plan_id_from_approve_intent()` (`handler_do/mod.rs`): acepta **sólo** `approve action for
  plan <id>` y la variante corta `approve plan <id>`; el id se corta en el primer carácter que
  no sea alfanumérico/`-`/`_`. Cualquier otra frase cae al router semántico como siempre.
- El matcher entra en el bloque determinista ya existente (junto a `whoami`, `undo_last_action`,
  `list_pending_actions`…) y por tanto hereda la misma disciplina: **sólo intents cortos**
  (`SHORT_INTENT_MAX = 60`), para que un mensaje largo que cite la frase no sea secuestrado.
- Al hacer match se rellenan `requestId` (el id extraído) y `approved=true` y se despacha
  `approve_action` con el `agent_id` del llamante. A partir de ahí, **todo** es D-13: autor o
  anónimo sin operador → `{"error":"self_approval"}`, plan intacto, sin audit; operador →
  aprueba, consume el plan y el audit registra al aprobador real con `human_intervention=1`.

Sin cambios en `approve_action`, en la forma del rechazo, en umbrales de gates ni en valores por
defecto.

## Verificación

Test nuevo, un solo test secuencial (mismo patrón que la spec de Buffy, para no pelear por
`TYLLUAN_AUDIT_DB`): `crates/tylluan-kernel/tests/approve_action_intent_route.rs`.

| Caso | Comprobación | Resultado |
|---|---|---|
| 1 | Autor pide aprobar su plan por intent, JWT sin operador | rechazo `self_approval`, plan pendiente, sin audit |
| 2 | Anónimo (sin `agent_id`) sobre plan anónimo por intent | rechazo `self_approval`, plan pendiente |
| 3 | Operador (token maestro) por intent | HTTP 200, plan consumido |
| 4 | Audit de la aprobación del operador | `agent_id='operator-jose'`, `human_intervention=1` |

```
antes:  cargo test -p tylluan-kernel --test approve_action_intent_route
        -> FAILED. 4 caso(s) sin cumplir (NO_GUILD_MATCH en 1 y 2; 3 y 4 sin efecto)
después: -> test result: ok. 1 passed; 0 failed; finished in 2.68s

cargo test -p tylluan-kernel --lib        (x2, en paralelo)
        -> test result: ok. 1132 passed; 0 failed; 4 ignored   (33.45 s / 33.52 s)
cargo clippy -p tylluan-kernel --all-targets -- -D warnings
        -> Finished … EXIT=0
```

## Hallazgo colateral (NO es de D-16, y es pre-existente)

`cargo test -p tylluan-kernel --test approve_action_self_approval -- --ignored` (la spec de
B-7/D-13) **falla también en `origin/main` limpio** — lo comprobé con mis cambios en `git stash`:

```
CASO 2: el plan fue consumido por un agente no operador (debe seguir pendiente)
CASO 4: existe(n) 1 fila(s) de audit con human_intervention=[0] …
CASO 4: la aprobacion del operador no produjo ninguna fila de audit con human_intervention=1
```

Causa: B-7 especificó que «otro agente sin operador» debía rechazarse con `operator_required`;
D-13, tal como se implementó y se aceptó, **sólo** bloquea la auto-aprobación (un agente distinto
puede aprobar). El test y el código no comparten contrato. No lo toco: es decisión del TL si se
alinea la spec a D-13 o se endurece D-13. D-16 no depende de ello (los casos 1, 3 y 4 de mi
prueba son independientes del caso «otro agente»).

## Impact (ADR-013)

- **Superficie pública:** `tylluan_do` gana una ruta determinista; el conjunto de las 6
  herramientas soberanas no cambia (CONTRACT-01 intacto), no hay herramienta nueva.
- **Seguridad:** la Compuerta de aprobación pasa de «una superficie» a «la que el kernel
  documenta» — antes la instrucción impresa no aprobaba nada; ahora aprueba por el mismo camino
  auditado. No se relaja ningún gate existente; se cierra una elusión por instrucción.
- **Comportamiento:** intents cortos con la frase `approve action for plan <id>` dejan de ir al
  router semántico. Un mensaje largo que la cite no se ve afectado (límite de 60 caracteres).
- **Riesgo residual:** un `plan_id` inventado produce el error propio de D-13
  (`Plan guild … not found` / no plan), sin efecto lateral; el plan ajeno no se consulta.
- **No-depredación:** 1 fichero de `src/` (transporte) + 1 fichero de tests; sin cambios en
  `guilds/`, `integrations/` ni `tylluan*.toml`.

## No verificado (declarado)

- No hay verificación en vivo contra un kernel arrancado: el router de producción se ejercita
  en proceso (`build_router`), que es el mismo camino de código. Un kernel de prueba exigiría
  arrancar proceso (bloqueado en este entorno por AV).
- El caso «otro agente aprueba sin operador» del contrato de Buffy: sigue permitido a propósito
  (semántica D-13); no lo he convertido en rechazo sin decisión del TL.
- La visibilidad del nuevo enrutado en logs de producción: no añadí traza propia; se apoya en el
  audit de D-13.
