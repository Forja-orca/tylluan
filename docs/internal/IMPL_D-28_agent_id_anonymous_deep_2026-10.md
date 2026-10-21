# IMPL D-28 — `agent_id` sin argumento deja de auditarse como cadena vacía

**Item:** D-28 de `COLA_deep_2026-10-22.md` (parte de código de D-24, **sin migración**).
**Fecha:** 2026-10-22. **Agente:** Deep. **Diff sin commitear.**
**Base:** `514058e` + D-22b + D-22c (parches previos de esta tanda).
**Parche verificado sobre:** `origin/main` `b70d9a9` aplicando antes `D-22b.patch` +
`D-22c.patch`: `git apply` de los tres OK y `cargo check --tests` aislado **exit 0** (6m12s).
**Parche:** `E:\tylluan-patches\deep\D-28.patch` — 4.243 B, 2 ficheros, **+65/−2**.

## 1. El cambio

Una sola línea de comportamiento, la que D-24 §1.1 señaló:

```diff
-    let audit_agent = agent_id.clone().unwrap_or_default();
+    let audit_agent = agent_id.clone().unwrap_or_else(|| "anonymous".to_string());
```

Está en `handler_do/outcome.rs:182` dentro de `post_process_outcome` (Stage 3). En el briefing
aparecía como `handler_do/mod.rs:1305`; D-22a/D-22b movieron ese bloque a `outcome.rs`, así que
se localizó por símbolo (`audit_agent`), no por línea. Es la única fila de audit que escribe
`tylluan_do`; el resto de rutas de entrada ya usaban `"anonymous"` (`mod.rs:457`,
`mod.rs:581`, `outcome.rs:123`, `outcome.rs:164`) o `"unknown"` donde el contrato es otro.

**No se canonicaliza nada**: `"Deep"` y `"deep"` siguen siendo agentes distintos, no se toca
ninguna fila histórica con `agent_id=''`, y no hay migración. Eso queda para José.

## 2. El test (`d28_tylluan_do_without_agent_id_audits_anonymous_not_empty`)

Va en `handler_do/tests.rs`. Qué hace:

1. Construye un `intent` único con PID + nanos (no un literal fijo: el `audit.db` es compartido
   por toda la suite).
2. Llama a `post_process_outcome(..., &None, ...)` — la función que `handle_tylluan_do` invoca
   como Stage 3 y la única que escribe la fila — con `agent_id = None`, que es exactamente lo que
   produce un `tylluan_do` sin ese argumento.
3. Sondea el `audit.db` (hasta 10 s, cada 25 ms) buscando la fila por ese intent — el insert es
   fire-and-forget en `spawn_blocking`, así que el test espera el efecto observable, no un sleep
   fijo (mismo patrón que los tests D-8 de esta carpeta).
4. Assert: `agent_id == "anonymous"`, con el mensaje de fallo nombrando la cadena vacía.

**Por qué no una llamada end-to-end `handle_tylluan_do` completa (declarado):** con el harness de
test ningún guild arranca (`test_registry()` no tiene procesos), el fallo cae en Stage 1 y Stage 1
devuelve antes de post-procesar. **Un Stage-1 fallido no genera fila de audit en absoluto** — ni
con `agent_id` ni sin él — así que un assert sobre la fila sería imposible de satisfacer por esa
ruta. Lo dejo anotado como observación separada (§5), fuera del alcance de D-28.

**Control negativo (el test vale algo):** revertida la línea a `unwrap_or_default()` el test falla:

```
assertion `left == right` failed: sin agent_id el audit debe registrar 'anonymous',
nunca cadena vacia (got "")
test result: FAILED. 0 passed; 1 failed
```

Restaurada la línea, pasa en 0,15 s.

## 3. Verificación

| Criterio | Resultado |
|---|---|
| **Test demuestra `anonymous` y no `""`** | Pasa con el fix (0,15 s) y **falla con `""`** al revertir (control negativo, §2) |
| **Clippy `-D warnings`** | `cargo clippy -p tylluan-kernel --tests -- -D warnings` → **exit 0** (1m41s) |
| **Lib dos veces sin fallos** | **1177 passed / 0 failed / 4 ignored** en ambas (59,01 s y 54,33 s) — 1.176 previos + el test nuevo |
| **Parche** | 4.243 B, 2 ficheros, +65/−2 |
| **Parche apilable y compilable** | sobre `b70d9a9`: D-22b + D-22c + D-28 aplican OK y `cargo check --tests` aislado **exit 0** (6m12s) |

Las dos pasadas de la lib son **secuenciales** (dos `cargo test` simultáneos chocan en `link.exe`
1104 y ese fallo no dice nada del código).

## 4. Riesgos considerados

- **Consultas por agente**: quien filtre `agent_id = ''` para "llamadas sin agente" deja de ver las
  nuevas; pasan a `'anonymous'`. Es el objetivo del ítem, pero es un cambio observable en datos,
  no solo en logs — declarado aquí.
- **Filas históricas**: intactas. La base de un usuario con filas `''` mostrará una discontinuidad
  (`''` antes, `'anonymous'` después). La migración es decisión de José.
- **Colisión de nombre**: si un cliente real se identifica como `"anonymous"`, sus filas se
  mezclan con las anónimas. Ya era así en las otras cuatro rutas de entrada; D-28 las alinea.
- **Test y `audit.db`**: el test escribe en el `audit.db` que resuelva `audit_db_path()`
  (`./data/audit.db` por defecto; `TYLLUAN_AUDIT_DB` si otro test lo fijó *en el mismo proceso* —
  los que lo fijan son tests de integración, procesos aparte). No borra ni trunca nada: inserta
  una fila más en la cadena, que sigue verificable.

## 5. Observación para el TL (no es D-28)

Un fallo de Stage 1 (ACL, rate-limit, sin guild, guild que no arranca) **no deja rastro en
`guild_audit_log`**: `handle_tylluan_do` devuelve el `CallToolResult` de error antes de
`post_process_outcome`. Un intent que el router no resuelve no aparece en el audit en absoluto.
No lo he tocado (cambiaría el contrato del audit y no está en la cola); si interesa, sería un
ítem propio: auditar también los rechazos de Stage 1 con `status='error'` y el prefijo ya
existente (`ROUTING_FAILED:`, `RATE_LIMITED:`, …).

## Límites

- No verificado en vivo (sin procesos): ningún `tylluan_do` real ejecutado contra un kernel
  arrancado en esta sesión.
- El parche **depende de D-22b y D-22c** (los dos ficheros que toca los crea/modifica esa serie).

## Impact

`crates/tylluan-kernel/src/transport/server/handler_do/{outcome.rs,tests.rs}`. Cambia un valor por
defecto de datos de audit (`""` → `"anonymous"`) en la ruta de `tylluan_do`; ninguna firma
pública, ruta de herramienta, timeout, gate ni dependencia. No toca `dev_mode`, ACL, embeddings,
federation ni el contrato de las 6 herramientas. No arranca procesos. No migra datos.
