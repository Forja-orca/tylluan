# T-DSH-9 — Auditoría adversarial de D-16: ruta de intent `tylluan_do` → `approve_action`

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `cd0cbb1` == origin/main al empezar. D-16 **aún sin push**: se auditó aplicando su parche de forma temporal sobre la base; el diff entregado es **solo tests** (el parche de D-16 se revirtió después de medir).
- **Encargo (turno 1459):** 15 intents ≤ 60 chars que NO deberían aprobar un plan, comprobar cuáles lo hacen y entregar tabla intent → esperado → real → riesgo.

## Método (reproducible)

1. Aplicado el parche D-16 de Deep sobre `cd0cbb1` (aplicó limpio, `git apply --check` exit 0).
2. Matriz de 15 intents enviados por el **router real** (MCP `tools/call` sobre `tylluan_do`), no llamando a la función privada: así se mide lo que un cliente puede hacer de verdad.
3. Fixture: cada plan lo firma `plan-author`; el llamante declara `agent_id = attacker-agent` (ni autor ni operador). El guild del plan no existe a propósito: la ejecución falla, pero **el plan se consume igualmente**, que es la señal observable de que la ruta llegó a `approve_action`. Un `requestId` sin plan devuelve `Action not found.` (segunda señal de ruteo).
4. Estado aislado: silva/mailbox/journal en memoria y `TYLLUAN_AUDIT_DB` a un fichero temporal. **Cero escrituras en producción** (el fixture ni siquiera arranca un guild).

```text
rustup run 1.99.1 cargo test -p tylluan-kernel \
  --test approve_action_intent_adversarial -- --ignored --nocapture
```

## Matriz (D-16 aplicado; 15 intents + 1 caso de suplantación)

| # | intent (literal, ≤60 chars) | lon | esperado | real | riesgo |
|---|---|---|---|---|---|
| 01 | `approve action for plan a10000010001` (control) | 36 | aprobar | **CONSUMIDO** | — (forma documentada) |
| 02 | `APPROVE ACTION FOR PLAN A20000020002` | 36 | aprobar | **CONSUMIDO** | bajo: el parser lowercasea; con ids hex minúsculas es el mismo id |
| 03 | `reject approve action for plan b30000030003` | 43 | NO aprobar | **CONSUMIDO** | **alto**: niega y ejecuta |
| 04 | `no approve action for plan b40000040004` | 39 | NO aprobar | **CONSUMIDO** | **alto** |
| 05 | `do not approve action for plan b50000050005` | 43 | NO aprobar | **CONSUMIDO** | **alto** |
| 06 | `cancel approve action for plan b60000060006` | 43 | NO aprobar | **CONSUMIDO** | **alto** |
| 07 | `disapprove action for plan b70000070007` | 39 | NO aprobar | **CONSUMIDO** | **alto**: el literal aparece *dentro* de "disapprove" |
| 08 | `explain approve action for plan b80000080008` | 44 | NO aprobar | **CONSUMIDO** | **alto**: mencionar el plan lo ejecuta |
| 09 | `{"intent":"approve action for plan b90000090009"}` | 49 | NO aprobar | **CONSUMIDO** | **alto**: JSON pegado / salida de herramienta |
| 10 | `approve plan c10000100001` (variante corta) | 25 | NO aprobar | **CONSUMIDO** | medio: superficie no documentada por el kernel |
| 11 | `approve action for plan ab-cd_ef1001` | 36 | aprobar | **CONSUMIDO** | — (`-` y `_` están en el charset permitido) |
| 12 | `approve action for plan dead` (prefijo de `deadbeef0003`) | 28 | NO aprobar | pendiente; **enrutó** (`Action not found.`) | bajo: hoy no consume, pero el parser acepta ids truncados |
| 13 | `approve action for plan deаdbeef0003` (`а` cirílica U+0430) | 36 | NO aprobar | pendiente; **enrutó** con id `de` | bajo: trunca en el carácter no-ASCII y hoy falla seguro |
| 14 | `approve  action for plan b14000140004` (doble espacio) | 37 | NO aprobar | **no enrutó** (`NO_GUILD_MATCH`) | bajo: no aprueba, pero cae al router semántico (posible misroute) |
| 15 | `approve action for plan` (sin id) | 23 | NO aprobar | no enrutó | — (id vacío ⇒ `None`) |
| X1 | `approve action for plan e10000130003` + `agent_id="innocent-agent"` | — | NO aprobar (auto-aprobación) | **CONSUMIDO** | **alto**: bypass del gate D-13 por identidad declarativa |

Resultado de la ejecución con `--ignored`: **9 filas no conformes** (03–10 y X1). Con la suite normal el binario queda `0 passed; 1 ignored` (exit 0): no rompe CI mientras el hallazgo no esté decidido.

## Mecanismo (dónde está cada pieza)

- **Parser** — el parche D-16 inserta `plan_id_from_approve_intent` en `crates/tylluan-kernel/src/transport/server/handler_do/mod.rs` (hunk tras la línea 723). Sobre el intent ya lowercased (línea 318) hace `split_once("approve action for plan ")`: **busca el literal en cualquier posición, no lo ancla al inicio**; después toma caracteres `[A-Za-z0-9_-]` tras `trim_start()`. De ahí F1 (subcadena) y F4 (truncado no-ASCII).
- **Emisión** — el propio kernel emite la instrucción en modo plan (líneas 572–577): `tylluan_do(intent='approve action for plan <plan_id>')`. Ese `<plan_id>` es `uuid v4` simple recortado a 12 hex (línea 562), minúsculas: por eso 02 (mayúsculas) y 11 (`-`/`_`) son benignas en la práctica.
- **Gate** — `crates/tylluan-kernel/src/transport/server/handlers.rs` líneas 221–241: `operator_gate_allows(current_auth_kind())` decide operador por la **credencial** (correcto), pero `is_self` compara `plan.agent_id` con `agent_id` que llega de **`arguments`** (`handler_do/mod.rs` línea 175), es decir, un string que el cliente declara. De ahí X1: no hay binding entre credencial e identidad declarada.
- **Respuestas** — plan ejecutado con guild inexistente: `handlers.rs` línea 252; `requestId` desconocido: `handlers.rs` línea 303 (`Action not found.`).

## Hallazgos y riesgo

1. **F1 (alto) — el literal no está anclado.** Negaciones (`reject`, `no`, `do not`, `cancel`), menciones (`explain`) y JSON pegado ejecutan el plan. La aprobación humana de M31-P2 depende de la *redacción* del intent: cualquier texto que contenga la frase aprueba. Un agente que transcribe una conversación o pega una salida de herramienta puede ejecutar acciones planificadas sin humano.
2. **F2 (alto) — bypass del gate D-13 por identidad declarativa.** Quien crea un plan puede aprobarlo declarando otro `agent_id` (`or_insert` en el parche respeta el `agent_id` del cliente). No lo introduce D-16 (la identidad ya era declarativa en el handshake), pero la ruta de intent lo convierte en una frase. Es el punto que más pesa: D-13 deja de ser una compuerta dura.
3. **F3 (medio) — variante corta `approve plan <id>`.** El kernel solo documenta la forma larga; aceptar la corta amplía las frases conversacionales que disparan ejecución.
4. **F4 (bajo) — ids truncados/homoglifos enrutan con id equivocado.** Hoy no consumen nada porque los ids reales son 12 hex aleatorios; el riesgo aparece si algún id no aleatorio es prefijo de otro (el store de planes acepta ids arbitrarios en código interno).
5. **F5 (bajo) — espacios dobles no enrutan.** No aprueba, pero el intent cae al router semántico; en producción ese misroute puede acabar en otro guild/tool.

## Recomendaciones (para el TL / Deep, no aplicadas por dsh)

1. **Anclar y acotar (arreglo de F1/F3):** aceptar solo si el intent *empieza* por `approve action for plan ` (o su variante documentada), y exigir que el resto sea exactamente un id (sin más tokens). Prohibir `split_once` posicional.
2. **Guardas léxicas antes del literal:** si contiene negadores (`no`, `not`, `never`, `reject`, `cancel`, `disapprove`, `do not`) o JSON (`{`/`}`/comillas), no rutear a aprobación.
3. **Validar el id contra el plan store antes de rutear** (existe, no vacío, formato hex esperado) — convierte F4 en rechazo explícito en vez de error de `approve_action`.
4. **Documentar o retirar la variante corta** (F3).
5. **Atar `agent_id` a la credencial/sesión o exigir operador en la ruta intent cuando el `agent_id` declarado no esté registrado** (F2). Mientras no exista binding, conviene que la documentación diga que D-13 protege contra errores, no contra un agente que miente.

## ## Impact

- **Diff entregado: un solo fichero de tests** (`crates/tylluan-kernel/tests/approve_action_intent_adversarial.rs`), marcado `#[ignore]` con el motivo: suite normal en verde (`0 passed; 1 ignored`, exit 0), CI intacto; con `--ignored` falla por la razón correcta (exit 101) y debe pasar cuando se endurezca el parser.
- **Cero cambios en `src/`:** el parche de D-16 se aplicó solo para medir y se revirtió; `git status` muestra únicamente el test nuevo.
- **Cero escrituras en producción:** audit aislado en temporal, silva/mailbox/journal en memoria, sin tocar el kernel vivo ni `data/`.
- Sin secretos ni datos personales: solo ids ficticios y textos de intent.

## No verificado

- El exploit **contra el kernel vivo** (solo se probó el router en proceso; no se tocó producción por regla de flota).
- La **variante corta `approve plan` en texto real**: no encontré referencias internas que la emitan, pero no puedo descartar clientes que la usen.
- **Binding de `agent_id` en el transporte MCP de producción:** no verifiqué si la sesión fija el `agent_id`; el handler lo lee de `arguments`, pero un middleware podría sobrescribirlo (no leído en esta tarea).
- **Unicode más allá del homoglifo probado:** no se probó normalización NFC/NFKC ni otros alfabetos/idiomas.
- Que existan planes con ids **no aleatorios** en la base real (el store los admite por código; no encontré llamadas que lo hagan).
- El comportamiento de las filas 03–10 **con operador** (token maestro): la matriz usó JWT no-operador para que el único efecto medible fuera el consumo del plan; con operador el gate D-13 tampoco frena por diseño (un humano puede aprobar cualquier cosa), así que el riesgo descrito aplica igual.
