# D-26 — El audit nombra al operador (O1 de T-DSH-21)

**Agente:** Deep (OpenCode)
**Fecha:** 2026-10-21
**Base:** `59d3eaa` (worktree `E:\tylluan-wt-deep`, detached). `origin/main` está en
`3dd9dc6` (dos commits **solo de documentación** sobre la base; el parche aplica limpio
en ambas, ver §4).
**Estado:** implementado y verificado en local. **No verificado en producción** (no se
ha arrancado ningún kernel).

---

## 1. Qué pedía la cola

`docs/internal/COLA_deep_2026-10-21.md`, D-26 (a partir de O1 de
`IMPL_T-DSH-21_audit_aprobaciones_d13_d16_2026-10-21.md`): cuando aprueba el operador
(token maestro) **sin `agent_id`**, la fila de audit quedaba
`agent_id="anonymous"` con `human_intervention=1`. La columna honesta era la segunda;
el `agent_id` no decía quién había aprobado. Se pide escribir `"operator"` en esa fila
**sin cambiar quién puede aprobar ni la compuerta**.

## 2. Qué había (premisa contrastada)

`transport/server/handlers.rs`, rama `"approve_action"` → plan store:

```rust
let audit_agent = agent_id.clone();      // "anonymous" por defecto (handlers.rs:23-27)
```

`agent_id` se toma de `arguments["agent_id"]` con `unwrap_or("anonymous")`. Es decir: la
fila era correcta en `human_intervention` (que sí derivaba de `operator_gate_allows`) y
mentía por omisión en el nombre del aprobador: el operador nunca aparecía.

La compuerta de autoaprobación (`is_self && !operator`) y la exención del operador están
en el mismo bloque y **no se tocan**.

## 3. Cambio

| Archivo | Cambio |
|---|---|
| `crates/tylluan-kernel/src/transport/server/handlers.rs` | `audit_agent` pasa a `if operator && is_anonymous(&agent_id) { "operator" } else { agent_id.clone() }`. `is_anonymous` es el mismo helper que ya existía en ese bloque (`""` o `"anonymous"`). Comentario con la referencia a D-26/O1 y a la razón de no tocar la compuerta. |
| `crates/tylluan-kernel/tests/approve_audit_matrix.rs` | Nueva constante `OPERATOR = "operator"`; las filas `operador` de **direct** e **intent** esperan `agent_id="operator"` con `human_intervention=1`. El resto de las 13 filas, idéntico. Cabecera del test actualizada. |

Propiedades que **no** cambian (y que el test sigue verificando):

- Un `agent_id` **declarado** se respeta tal cual, incluso con el token maestro (`approve_action_audit_names_the_approver_and_human_only_for_operator` sigue esperando `agent-a`). Solo se materializa `operator` cuando no hay identidad declarada.
- Sigue siendo imposible autoaprobar sin operador (`self_approval`, 0 filas, plan intacto).
- Un cliente anónimo aprobando el plan de otro sigue registrando `anonymous` con `human_intervention=0` (no es el operador).
- Se sigue escribiendo **exactamente una** fila por aprobación; los rechazos no escriben nada.

## 4. Verificación ejecutada

| Comprobación | Comando | Resultado |
|---|---|---|
| Matriz completa (13 filas) | `cargo test -p tylluan-kernel --test approve_audit_matrix` | **1 passed / 0 failed** (13,17 s) |
| Clippy | `cargo clippy -p tylluan-kernel --tests -- -D warnings` | **exit 0** (solo el aviso conocido de `bundled-dashboard`) |
| Lib ×2 (secuencial) | `cargo test -p tylluan-kernel --lib` | **1173 passed / 0 failed / 4 ignored** (36,39 s y 37,25 s) |
| Parche contra `59d3eaa` | `git apply --check` en worktree prístino | **exit 0** |
| Compilación del parche aislado | `cargo check -p tylluan-kernel --tests` en ese worktree | **exit 0** (3m 26s) |
| Parche contra `origin/main` (`3dd9dc6`) | `git apply --check` en worktree prístino | **exit 0** |

`E:\tylluan-patches\deep\D-26.patch`: **3 488 B, 2 archivos, +22/−3**.

Nota: la cola pedía «lib en paralelo dos veces». Se ejecutaron **secuenciales**: dos
`cargo test` simultáneos en el mismo worktree colisionan en el linker
(`link.exe` 1104) y ese fallo no es del código. Dos pasadas completas en verde es la
evidencia equivalente.

## 5. Límites / no verificado

- **No verificado en vivo**: sin kernel arrancado ni fila real en `./data/audit.db`.
- El cambio vive en `handle_kernel_tool`, la ruta común de todas las herramientas soberanas, así que cubre los transportes que la usan; **solo se ha probado por `/mcp` (HTTP)**. stdio/Nexus no ejecutados.
- No hay ruta REST de aprobación de plan (O4 de T-DSH-21), así que no hay nada que cambiar ahí.
- **Efecto colateral observable en el preview**: cuando el operador aprueba un plan cuyo autor es anónimo, el preview ahora empieza por `[plan author: anonymous]` (antes no llevaba prefijo porque autor y aprobador eran el mismo string `anonymous`). Es más informativo, no menos; queda documentado aquí para que nadie lo lea como regresión.
- No se ha tocado la cadena de hashes del audit (`prev_hash`/`hash`): la fila cambia de contenido en las aprobaciones **nuevas**; las históricas con `anonymous` no se reescriben (romperían la cadena).
- La matriz sigue siendo un único `#[test]` que agrega 13 filas: un fallo indica la fila en el mensaje, no en el nombre del test.

## Impact

**No-depredación (ADR-013):** no se relaja ninguna comprobación. La compuerta de
autoaprobación, la exigencia de operador y quién puede aprobar quedan intactas; el único
cambio es el **contenido informativo** de una columna del audit (`agent_id` pasa de
`anonymous` a `operator` cuando aprueba el operador sin identidad declarada), que es
exactamente lo que pedía O1. Ningún cliente ni token cambia de permisos.

**Zonas tocadas:** `crates/tylluan-kernel/src/transport/server/handlers.rs` y el test
`tests/approve_audit_matrix.rs`. Nada en `guilds/`, `integrations/`, `tylluan*.toml`, ni
configuración de `dev_mode`.

**Datos:** el test usa un `TYLLUAN_AUDIT_DB` temporal y lo borra; no se escribe en el
audit real ni se migran filas históricas.
