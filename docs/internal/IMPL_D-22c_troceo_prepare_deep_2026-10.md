# IMPL D-22c — Trocear `handler_do/mod.rs`: `prepare.rs` (cierre de la serie D-22)

**Item:** D-22c de `COLA_deep_2026-10-22.md` (último corte de la serie abierta en D-22a §6).
**Fecha:** 2026-10-22. **Agente:** Deep. **Diff sin commitear.**
**Base:** `514058e` + D-22b (parche previo de esta misma serie).
**Parche verificado sobre:** `origin/main` `776e576` **aplicando antes `D-22b.patch`**:
`git apply --check` 0 para ambos y `cargo check --tests` aislado **exit 0** (8m32s).
**Parche:** `E:\tylluan-patches\deep\D-22c.patch` — 47.288 B, 3 ficheros, +465/−446.
**Tipo:** movimiento puro. Cero cambios de comportamiento.

## 1. Corte aplicado

| Antes (tras D-22b) | Después |
|---|---|
| `handler_do/mod.rs` **1.181** líneas | `mod.rs` **739** líneas · nuevo `handler_do/prepare.rs` **459** líneas |

**Criterio de parada de D-22c cumplido:** `mod.rs` queda por debajo del techo blando de 800
líneas, así que la serie D-22 se cierra aquí (no hay D-22d).

Movido tal cual, en el orden del fichero original:

| Bloque | Líneas origen (post-D-22b) | Líneas movidas |
|---|---|---|
| `ResolvedToolCall` (+ doc) | 685–692 | 8 |
| `resolve_and_prepare_tool_call` (Stage 1, + doc de 4 líneas) | 712–1025 | 314 |
| `execute_guild_call_with_progress` (Stage 2, + doc de 1 línea) | 1027–1142 | 116 |

`mod.rs` queda con `mod prepare;` y `use prepare::{execute_guild_call_with_progress,
resolve_and_prepare_tool_call};` (privado: `tests.rs` es hijo de `handler_do`, y un `use` privado
del padre sí es visible para los descendientes — mismo criterio que en D-22b).

`ResolvedToolCall` **no** se importa en `mod.rs`: `handle_tylluan_do` solo usa valores
(`resolved.guild_name`, …), no el nombre del tipo; importarlo daba `unused import` y `-D warnings`
fallaba. Los campos sí son `pub(super)`.

## 2. Cambios permitidos por el criterio (solo `use` y visibilidad)

- Visibilidad: `pub(super) async fn` en `resolve_and_prepare_tool_call` y
  `execute_guild_call_with_progress`; `pub(super) struct ResolvedToolCall` con sus **5 campos**
  también `pub(super)` (los lee `handle_tylluan_do` en el padre). Es el único bloque donde la
  verificación byte a byte muestra algo además de la firma, y es exactamente eso: 6 prefijos
  `pub(super)`.
- `prepare.rs` importa: `rmcp::model::*`, `tracing::{info, warn}`, `super::TylluanServer`,
  `super::error_prefixes`, `super::external_mcp`, `super::coloquio_utils::{_parse_coloquio_pagination,
  parse_coloquio_intent}`, `super::is_bash_state_intent`, `super::outcome::penalize_lesson_fn`,
  `super::routing::resolve_guild_name`, `crate::registry::proxy::error_result` y
  `crate::transport::server::utils::{extract_command_from_intent, extract_path_from_intent,
  extract_url_from_intent}`.
- `mod.rs` pierde cuatro imports que ya solo usaban los bloques movidos (`tracing::{info, warn}`,
  `super::utils::{extract_*}`, `use routing::resolve_guild_name;` y
  `use coloquio_utils::{parse_coloquio_intent, _parse_coloquio_pagination};`), y poda
  `ResolvedToolCall` del `use prepare::…` y `penalize_lesson_fn` del `use outcome::…`.
- **`tests.rs` gana dos imports** (`use super::coloquio_utils::parse_coloquio_intent;`,
  `use super::routing::resolve_guild_name;`): antes los veía por los `use` privados de `mod.rs`,
  y al retirarlos de allí daban `E0425` en ~20 sitios del test. Es cambio de `use`, no de cuerpo
  de test; los nombres de test no cambian.

## 3. Comprobación de "solo se mueve código"

Script `C:\Users\FoRJa\AppData\Local\Temp\opencode\d22c_split.py` (corta por firma exacta +
llaves balanceadas) y verificación con rangos explícitos contra el estado **D-22b**
(`git show :…/mod.rs`, el índice ya tenía D-22b):

```
ResolvedToolCall: 7 lines, identical_save_visibility=True   (8 con doc; diff = 6x pub(super))
resolve_and_prepare_tool_call: 310 lines, identical_save_visibility=True
execute_guild_call_with_progress: 115 lines, identical_save_visibility=True
ALL_MOVED_IDENTICAL: True
```

Los docs de Stage 1 y Stage 2 viajan con sus bloques (comprobado por presencia literal en
`prepare.rs`). El diff del struct es exactamente:

```
-pub struct ResolvedToolCall {      +pub(super) struct ResolvedToolCall {
-    guild_name: String,            +    pub(super) guild_name: String,
… (5 campos) …
```

Nada más cambia dentro de los bloques.

**Incidentes durante el corte (declarados):**
1. Primer `cargo check`: `super::utils` no resuelve desde `prepare.rs` (el módulo `utils` está en
   `transport::server`, no en `handler_do`) y faltaban 5 símbolos del padre
   (`penalize_lesson_fn`, `is_bash_state_intent`, `parse_coloquio_intent`,
   `_parse_coloquio_pagination`). Se corrigieron en una sola pasada de `use` (`d22c_fix.py`).
2. El generador escribió `prepare.rs` con finales de línea mixtos (cabecera `\n`, cuerpo `\r\n`);
   se normalizó a `\r\n` antes de tocar imports. El diff final no tiene líneas mezcladas.
3. `tests.rs` rompió al retirar de `mod.rs` los dos `use` que heredaba (ver §2). Arreglado con dos
   imports explícitos; `--list` sigue dando los mismos 1.180 nombres.

## 4. Verificación (criterio estricto de D-22)

| Criterio | Resultado |
|---|---|
| **Mismos tests, mismos nombres** | `--list` antes = **1.180**, después = **1.180**; `Compare-Object` → **diff_count = 0** |
| **Lib dos veces sin fallos** | **1176 passed / 0 failed / 4 ignored** en ambas (48,93 s y 46,15 s) |
| **Clippy `-D warnings`** | `cargo clippy -p tylluan-kernel --tests -- -D warnings` → **exit 0** (44,66 s) |
| **Solo movimientos** | §3 (438 líneas movidas verbatim: 310 + 115 + 8, más cabecera/`use`/separadores hasta 459) |
| **Parche independiente** | 47.288 B, 3 ficheros, +465/−446 |
| **Parche apilable** | sobre `776e576`: `D-22b.patch` check 0 / apply 0 → `D-22c.patch` check 0 / apply 0 → `cargo check --tests` aislado **0** (8m32s) |

Las dos pasadas de la lib son **secuenciales** (dos `cargo test` simultáneos chocan en `link.exe`
1104 y ese fallo no dice nada del código). El `--tests` del árbol aislado tardó 8m32s por compilar
desde cero (`tylluan-link` + kernel + tests), no por el parche.

## 5. Estado de la serie D-22

| Fase | Resultado | `mod.rs` |
|---|---|---|
| D-22a | `embedding`, `tests`, `base_test_server` | 1.531 |
| D-22b | `outcome.rs` | 1.181 |
| **D-22c** | **`prepare.rs`** | **739** |

`mod.rs` conserva ahora solo el orquestador (`handle_tylluan_do`), `plan_id_from_approve_intent`,
`error_prefixes`, los `mod`/`use` y `base_test_server`. Ningún fichero de la serie supera las 500
líneas salvo `mod.rs` (739) y `outcome.rs` (369), ambos por debajo del techo de 800.

## Límites

- No hay test nuevo: el criterio prohíbe cambiar el recuento. Cobertura = suite existente de
  `handler_do` (mismos nombres) + `check` aislado del parche apilado.
- No verificado en vivo (sin procesos): ningún `tylluan_do` real ejecutado en esta sesión.
- El parche **depende de D-22b** (mismo fichero base): aplicar D-22c sin D-22b falla a propósito.

## Impact

Solo `crates/tylluan-kernel/src/transport/server/handler_do/`. Sin cambios de comportamiento,
firmas públicas, rutas de herramienta, timeouts, defaults ni dependencias. No toca `dev_mode`,
ACL, embeddings, federation ni el contrato de las 6 herramientas. No arranca procesos.
