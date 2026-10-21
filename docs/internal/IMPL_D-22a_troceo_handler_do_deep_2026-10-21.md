# D-22a — Trocear `handler_do/mod.rs`: extraer el módulo de tests

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` = `57d2e7b`
**Item:** D-22 (`docs/internal/COLA_deep_2026-10-21.md:29`) — primero de los cortes.
**Tipo:** movimiento puro de código. Cero cambios de comportamiento.

---

## 1. Corte aplicado

`crates/tylluan-kernel/src/transport/server/handler_do/mod.rs` medía **2.783 líneas** (techo
blando 800). De ellas, **1.249 líneas eran el módulo `#[cfg(test)] mod tests`** pegado al final
del fichero, sin ninguna dependencia de producción hacia él.

| Antes | Después |
|---|---|
| `mod.rs` 2.783 líneas (prod 1.495 + tests 1.252 + helper `cfg(test)`) | `mod.rs` **1.532** líneas · nuevo `handler_do/tests.rs` **1.253** líneas |

`mod.rs` baja de 1.600 → **se cumple el criterio de parada** del item; siguiente corte anotado
en §6.

En `mod.rs` queda únicamente:

```rust
#[cfg(test)]
mod tests;
```

`base_test_server` **se queda** en `mod.rs`: es `pub(crate)` y lo usan otros módulos por ruta
exacta (`transport/server/handlers.rs:622`, `transport/server/background_jobs.rs:388`); moverlo
habría cambiado su ruta pública.

## 2. Por qué este corte y no otro

Es la costura más limpia que queda: no toca ninguna función de producción ni ninguna firma, no
obliga a cambiar visibilidades y ya es el patrón que usa el fichero para el resto de submódulos
(`audit`, `skill`, `coloquio`, `routing`…). El único ajuste de importes es el que el compilador
obliga:

- se elimina de `mod.rs` el `#[cfg(test)] use embedding::parse_content_for_embedding;`
  (su único uso, 5 llamadas, estaba en el bloque movido);
- se añade en `tests.rs` `use super::embedding::parse_content_for_embedding;`.

## 3. Comprobación de "solo se mueve código"

El script de extracción (`C:\Users\FoRJa\AppData\Local\Temp\opencode\d22a_split.py`) no copió el
texto a mano: partió el fichero por el marcador `#[cfg(test)]\nmod tests {`, quitó 4 espacios de
indentación (obligatorio para que el contenido sea válido como módulo de fichero y pase
`cargo fmt`) y **comprobó en la propia ejecución** que el cuerpo resultante es idéntico al
original dedentado, línea a línea:

```
split at line 1532 of 2784
mod.rs lines: 1532 tests.rs lines: 1252
body identical: yes ( 1249 lineas )
```

Es decir: **1.249/1.249 líneas idénticas**; las 4 líneas de diferencia son exactamente la
cabecera documental, una línea en blanco y el `use` recolocado.

`git diff --stat` (rutas limitadas al directorio tocado):

```
 .../src/transport/server/handler_do/mod.rs   | 1254 +-------------------
 .../src/transport/server/handler_do/tests.rs | 1253 +++++++++++++++++++
 2 files changed, 1254 insertions(+), 1253 deletions(-)
```

## 4. Verificación (criterio estricto del item)

| Criterio | Resultado |
|---|---|
| **Mismo número de tests** (ni suben ni bajan) | `--list` antes = **1.154**, después = **1.154**; `Compare-Object` de los nombres → **diff_count = 0** (mismos nombres exactos, no solo el mismo total) |
| **Lib en paralelo dos veces sin fallos** | dos procesos `cargo test -p tylluan-kernel --lib` simultáneos (`Start-Job`) → **1150 passed / 0 failed / 4 ignored** en ambos (32.22 s y 34.52 s) |
| **Clippy exit 0** | `cargo clippy -p tylluan-kernel --tests -- -D warnings` → **EXIT 0** |
| **Diffstat solo movimientos** | ver §3 |
| **Parche independiente** | `E:\tylluan-patches\deep\D-22a.patch` (121.612 B, 2 archivos, +1254/−1253); `git apply --check` **EXIT 0** sobre `57d2e7b` limpio en worktree temporal |

Nota de cifras: en este worktree hay además el diff de D-15 (8 unit tests nuevos), por eso el
total local es 1150 y no 1142. El parche es independiente por pathspec (`handler_do/`), así que
sobre `57d2e7b` limpio el total sigue siendo **1142 → 1142**: el corte no añade ni quita tests,
solo los mueve de fichero (demostrado por el diff de nombres, que es independiente de la base).

## 5. Riesgos considerados

- **Rutas de test**: los tests movidos pasan de `handler_do::tests::…` a seguir siendo
  `handler_do::tests::…` (mismo nombre de módulo, ahora en fichero propio) → ningún filtro
  `cargo test <nombre>` cambia.
- **Visibilidad**: el módulo hijo conserva acceso a los ítems privados del padre
  (`use super::*` + `super::embedding::…`), así que no hizo falta marcar nada `pub(crate)` de más.
- **`base_test_server`**: se queda por las dos rutas externas citadas en §1.

## 6. Siguiente corte (anotado, no hecho)

`mod.rs` queda en 1.532 líneas, de las que ~1.495 son producción. Cortes propuestos por costura:

1. **D-22b** → `handler_do/outcome.rs`: `persist_remember` (1443–1495), `post_process_outcome`
   (1176–1441), `capture_friction_from_result` (684) y `penalize_lesson_fn` (709). ~380 líneas,
   todas de "efectos posteriores a la llamada" y sus helpers de lección.
2. **D-22c** → `handler_do/prepare.rs`: `resolve_and_prepare_tool_call` (741–1051),
   `ResolvedToolCall` y `execute_guild_call_with_progress` (1053). ~420 líneas de
   resolución/ejecución.
3. Quedaría `mod.rs` en ~700 líneas: `handle_tylluan_do` (163–682, orquestación),
   `handle_nodo_prefix`, `is_bash_state_intent`, `plan_id_from_approve_intent` y el helper
   `cfg(test)`.

No los ejecuto en este parche porque el item ordena parar al bajar de 1.600.

## Impact

Solo `crates/tylluan-kernel/src/transport/server/handler_do/` (ADR-013: sección obligatoria).
Sin cambios de comportamiento, firmas, visibilidad pública, rutas de herramienta, timeouts,
defaults ni dependencias. No toca `dev_mode`, ACL, embeddings ni federation. No arranca procesos
(los dos `cargo test` en paralelo se lanzaron como trabajos de PowerShell abortables, con
timeout, y ambos terminaron).
