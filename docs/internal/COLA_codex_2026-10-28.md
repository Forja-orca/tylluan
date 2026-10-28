# COLA Codex (2026-10-28) — la lanza José con un mensaje corto

Quién: Codex. Dónde: tu worktree `E:\tylluan-wt-codex` (si tiene un diff ajeno, no lo descartes: apúntalo en el informe y trabaja en una rama limpia). Antes de nada: `git fetch origin` y rebasa a `origin/main`; lee, por este orden, `AGENTS.md`, `STATUS.md` y `REVIEW.md` (el tercero).
Reglas: diff sin commitear más un informe `docs/internal/IMPL_<tarea>_codex_2026-10.md` con `## Impact` y `## No verificado`. Solo el TL commitea y empuja. No toques `E:\tylluan` ni el kernel vivo. Todo lo nuevo, apagado o igual que hoy por defecto (ADR-013).

## Orden

### C-1 — Reindexer configurable (pendiente de H-42, H-43, H-44 y H-45)
- Contexto medido: el reindexer procesa 100 nodos cada 600 s fijos en el código (`crates/tylluan-kernel/src/main.rs`, bloque "Agnostic RE-INDEXING"), no converge (~6.400 pendientes, 4.493 nunca embebidos) y su guard de 120 s salta cada día. Dos tipos, `session_digest` y `routing_anchor`, el recall los excluye (`handler_recall.rs`), así que su vector no sirve.
- Hacer, con valores por defecto idénticos a hoy:
  1. `[memory] reindex_nodes_per_tick` (por defecto 100) y `reindex_tick_secs` (por defecto 600), leídos en `config.rs` y usados en el bucle.
  2. `[memory] reindex_skip_types` (lista de tipos de nodo, por defecto vacía) aplicada como `AND n.type NOT IN (...)` en `get_stale_embeddings` (`memory/silva/nodes.rs`).
  3. Tests: valores por defecto; con la lista vacía la consulta devuelve lo mismo que hoy; con `["session_digest"]` ese tipo no aparece.
- Aceptación: `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → 0; `cargo test -p tylluan-kernel --lib` → 1401 + tus tests, 0 fallos.
- Fuera de alcance: `GuardedTask`, `BackgroundBudget`, `embed_batching_enabled`, el motor de embeddings.

### C-2 — Hacer visible el efecto del filtro de huella de embeddings (auditoría AUD-08)
- Contexto: `SilvaDB::fingerprint_observation()` (`memory/silva/mod.rs`) cuenta cuántos candidatos excluiría el filtro (`embedding_fingerprint_filter`, apagado por defecto), pero ninguna ruta lo expone: hoy no se puede medir antes de activarlo.
- Hacer: añadir los tres contadores (por modelo, por dimensión, excluidos) a un endpoint REST de solo lectura que ya exista (por ejemplo el de señales del sistema), sin ruta nueva. Test del handler. Decide y justifica en el informe si hace falta reflejarlo en MCP (norma de José: toda capacidad en REST y MCP; si no aplica, di por qué).
- Aceptación: mismos comandos que C-1; el endpoint devuelve los contadores y el resto del JSON queda intacto.
- Fuera de alcance: activar el filtro.

## Si te atascas
Escribe en el informe qué paso falla, el comando exacto y su salida, y para.

## Cómo informar
Mensaje `[COLA codex]` en Coloquio con: comandos ejecutados, cifras, ficheros tocados, y qué no verificaste.
