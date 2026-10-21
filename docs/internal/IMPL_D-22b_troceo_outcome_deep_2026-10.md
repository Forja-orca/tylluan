# IMPL D-22b — Trocear `handler_do/mod.rs`: `outcome.rs`

**Item:** D-22b de `COLA_deep_2026-10-22.md` (siguiente corte anotado en D-22a §6).
**Fecha:** 2026-10-22. **Agente:** Deep. **Diff sin commitear.**
**Base:** `514058e`. **Parche verificado sobre:** `origin/main` `776e576` (`git apply --check`
exit 0 + `cargo check --tests` aislado exit 0).
**Parche:** `E:\tylluan-patches\deep\D-22b.patch` — 37.654 B, 2 ficheros, +373/−354.
**Tipo:** movimiento puro. Cero cambios de comportamiento.

## 1. Corte aplicado

| Antes | Después |
|---|---|
| `handler_do/mod.rs` **1.531** líneas | `mod.rs` **1.181** líneas · nuevo `handler_do/outcome.rs` **369** líneas |

Movido, tal cual, en el orden del fichero original:

| Bloque | Líneas origen | Líneas movidas |
|---|---|---|
| `capture_friction_from_result` (+ doc) | 682–698 | 17 |
| `penalize_lesson_fn` | 709–718 | 10 |
| `post_process_outcome` (+ doc y `#[allow(clippy::too_many_arguments)]`) | 1169–1440 | 272 |
| `persist_remember` (+ doc) | 1442–1492 | 51 |

`mod.rs` queda con `mod outcome;` y `use outcome::{penalize_lesson_fn, persist_remember,
post_process_outcome};`. **No** `pub(super) use`: un `pub(super)` en el hijo solo es
re-exportable al ámbito del padre, y re-exportarlo más arriba da `E0364` ("is private, and
cannot be re-exported") — lo comprobé compilando. Un `use` privado basta porque los tres
llamadores están en `mod.rs`; si D-22c necesita `penalize_lesson_fn` desde `prepare.rs`,
basta `super::outcome::penalize_lesson_fn` (ya es `pub(super)`).

Se quedan en `mod.rs`: `plan_id_from_approve_intent` (lo usan `tests.rs` y el modo plan, no es
efecto posterior), `ResolvedToolCall` y los dos bloques de D-22c.

## 2. Cambios permitidos por el criterio (solo `use` y visibilidad)

- Visibilidad: `fn` → `pub(super) fn` en `penalize_lesson_fn`, `post_process_outcome` y
  `persist_remember`. `capture_friction_from_result` sigue privada: su único llamador
  (`post_process_outcome`) está en el mismo módulo.
- `outcome.rs` importa: `rmcp::model::*`, `tracing::{info, warn}`, `super::TylluanServer`,
  `super::audit::{log_audit_entry, routing_failure_id}`, `super::distill_for_embedding`,
  `super::handle_tylluan_do`, `super::routing::{maybe_auto_extract_triples,
  record_activity_trace, run_agent_handshake}` y
  `crate::transport::server::utils::extract_path_from_intent`.
- `mod.rs` pierde de su import de `routing` los dos símbolos que ya solo usa `outcome.rs`
  (`use routing::resolve_guild_name;`), si no `-D warnings` falla por import sin usar.

## 3. Comprobación de "solo se mueve código"

Script de extracción `C:\Users\FoRJa\AppData\Local\Temp\opencode\d22b_split.py` (corta por
firma exacta + llaves balanceadas, no copia a mano) y verificación posterior con rangos
explícitos contra `git show 514058e:…/mod.rs`:

```
capture_friction_from_result: 17 lines, present_verbatim=True
penalize_lesson_fn: 10 lines, present_verbatim=True
post_process_outcome: 272 lines, present_verbatim=True
persist_remember: 51 lines, present_verbatim=True
ALL_MOVED_IDENTICAL: True
```

`present_verbatim` exige que el bloque del original (con el prefijo de visibilidad sustituido)
aparezca **byte a byte** dentro de `outcome.rs`, con `\r\n` normalizados a `\n` solo para la
comparación.

**Incidente durante el corte (declarado):** el primer rango empezaba en la segunda línea del
doc de `post_process_outcome`, dejando una línea `/// Stage 3: …` huérfana en `mod.rs`; clippy
la cazó (`empty line after doc comment`) y el bloque se corrigió moviendo también esa línea a
`outcome.rs`. No quedó en el diff final ninguna línea huérfana ni doc duplicado.

## 4. Verificación (criterio estricto de D-22)

| Criterio | Resultado |
|---|---|
| **Mismos tests, mismos nombres** | `--list` antes = **1.180**, después = **1.180**; `Compare-Object` → **diff_count = 0** (repetido tras el arreglo del doc) |
| **Lib dos veces sin fallos** | **1176 passed / 0 failed / 4 ignored** en ambas (44,76 s y 34,51 s) |
| **Clippy `-D warnings`** | `cargo clippy -p tylluan-kernel --tests -- -D warnings` → **exit 0** |
| **Solo movimientos** | §3 (272+51+17+10 = 350 líneas verbatim; 369 del fichero nuevo = 350 + cabecera + `use` + separadores) |
| **Parche independiente** | 37.654 B, 2 ficheros, +373/−354; `git apply --check` **0** y `cargo check --tests` aislado **0** (4m20s) sobre `origin/main` `776e576` |

Las dos pasadas de la lib son **secuenciales** (dos `cargo test` simultáneos en el mismo
worktree chocan en el linker, `link.exe` 1104, y ese fallo no dice nada del código).

## 5. Riesgos considerados

- **Rutas de test**: no se mueve ningún test (`tests.rs` sigue igual); los tests de
  `handler_do` que ejercitan `tylluan_do` de punta a punta ahora pasan por `outcome::` en
  producción, sin cambiar el nombre de ningún test.
- **`base_test_server`**: sin tocar (sigue en `mod.rs`, sus rutas externas intactas).
- **Recursión**: `post_process_outcome` llama a `handle_tylluan_do` (modo plan) — ahora es un
  `use super::handle_tylluan_do`, no un cambio de firma; el ciclo de módulos ya existía.
- **`notify`/`matcher`/`silva`**: se usan vía `server.*`, sin nuevos imports de tipos.

## Límites

- No hay test nuevo: el criterio prohíbe cambiar el recuento. La cobertura es la existente de
  `handler_do` (misma suite, mismos nombres), más el `check` aislado del parche.
- No verificado en vivo (sin procesos): ningún `tylluan_do` real ejecutado en esta sesión.

## Siguiente corte (anotado, no hecho)

**D-22c → `handler_do/prepare.rs`**: `ResolvedToolCall` (700–707), `resolve_and_prepare_tool_call`
(741–1051) y `execute_guild_call_with_progress` (1053–1168) ≈ 420 líneas. Dejaría `mod.rs` en
**~760** líneas, por debajo del techo blando de 800: ahí se cumple el criterio de parada de
D-22c.

## Impact

Solo `crates/tylluan-kernel/src/transport/server/handler_do/`. Sin cambios de comportamiento,
firmas públicas, rutas de herramienta, timeouts, defaults ni dependencias. No toca `dev_mode`,
ACL, embeddings, federation ni el contrato de las 6 herramientas. No arranca procesos.
