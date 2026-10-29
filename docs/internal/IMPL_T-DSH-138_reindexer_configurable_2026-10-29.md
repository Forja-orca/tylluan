# T-DSH-138 — Reindexer configurable (C-1 de COLA Codex)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = facf490` (push200), `git rev-parse --short HEAD` = `facf490` al empezar.
- Diff sin commitear: `config.rs`, `main.rs`, `memory/silva/nodes.rs`, `memory/silva/tests.rs`, `api_v1/api_memory.rs`, `tests/sparse_cascade_bench.rs`.
- Encargo (T2105, C-1 de `docs/internal/COLA_codex_2026-10-28.md`): `[memory] reindex_nodes_per_tick` (default 100), `reindex_tick_secs` (default 600) y `reindex_skip_types` (default vacía) aplicada como `AND n.type NOT IN (...)` en `get_stale_embeddings`; **defaults idénticos a hoy**; tests de defaults, lista vacía = comportamiento actual y `["session_digest"]` excluido.

## Cambio

### `config.rs` (`MemoryConfig`)

```rust
#[serde(default = "default_reindex_nodes_per_tick")] pub reindex_nodes_per_tick: usize, // 100
#[serde(default = "default_reindex_tick_secs")]    pub reindex_tick_secs: u64,          // 600
#[serde(default)]                                   pub reindex_skip_types: Vec<String>, // []
```

- Añadidos al `impl Default`; un `tylluan.toml` sin estas claves se comporta exactamente como hoy.

### `main.rs` (bucle "Agnostic RE-INDEXING")

- Antes del `bg_spawn`, captura `reindex_nodes_per_tick`, `reindex_tick_secs` y `reindex_skip_types` de la config.
- `interval_at(now + reindex_tick_secs, reindex_tick_secs)` (antes 600 fijo).
- `stale_nodes.iter().take(reindex_per_tick)` (antes 100 fijo).
- `get_stale_embeddings(..., &skip_types_inner)` (clon por iteración para el closure).

### `nodes.rs` (`get_stale_embeddings`)

- Nueva firma: `get_stale_embeddings(current_model, current_hash, skip_types: &[String])`.
- Si `skip_types` está vacía, **SQL idéntico** al histórico (mismo resultado); si no, añade `AND n.type NOT IN (?4, ?5, ...)` con parámetros ligados.
- Callers actualizados: `main.rs`, `api_memory.rs` (lee la lista de la config) y los tests/bench existentes (`&[]`).

### `api_memory.rs`

- Lee `reindex_node_delay_ms` y `reindex_skip_types` en una sola lectura de config antes de calcular la lista stale; pasa la lista a `get_stale_embeddings`.

## Tests

- `t138_reindex_config_tests::reindex_defaults_are_identical_to_today`: 100 / 600 / lista vacía en `MemoryConfig` y `TylluanConfig`.
- `t138_reindex_config_tests::reindex_keys_parse_from_toml`: `reindex_nodes_per_tick = 7`, `reindex_tick_secs = 30`, `reindex_skip_types = ["session_digest","routing_anchor"]` se leen bien.
- `stale_embeddings_skip_types_filters_only_listed_types`: con lista vacía los 3 tipos (`session_digest`, `routing_anchor`, `episode`) aparecen stale; con `["session_digest"]` solo se excluye ese tipo.
- Test existente `stale_embeddings_detects_real_hash_mismatch_but_not_pre_hash_sentinel` actualizado a la firma nueva (semántica intacta).

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib t138                     → 2 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib stale_embeddings_skip    → 1 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1425 passed; 0 failed; 4 ignored
```

Lib 1425 = 1422 de main + **3 tests nuevos**.

## Impact

- El reindexer periódico deja de tener 100 nodos/600 s fijos: `[memory] reindex_nodes_per_tick` y `reindex_tick_secs` permiten ajustar la convergencia sin tocar código, con **defaults idénticos a hoy** (100/600) para producción.
- `[memory] reindex_skip_types` permite excluir tipos cuyo vector no usa el recall (`session_digest`, `routing_anchor`) y así concentrar el reindex en lo útil; **default vacío = consulta histórica exacta**.
- Sin cambios de contrato REST/MCP; el endpoint `/memory/reindex` usa la misma lista stale (ahora también respeta `reindex_skip_types`).

## No verificado

- No se midió en test-run el efecto de cambiar `reindex_nodes_per_tick`/`reindex_tick_secs` (solo se verifican defaults y parseo); no se comprobó la convergencia del reindexer con valores distintos.
- No se probó `reindex_skip_types` sobre una BD real grande (el test es en memoria con 3 nodos); no se verificó que excluir `session_digest`/`routing_anchor` no rompa ningún consumidor del recall (el COLA dice que `handler_recall.rs` ya los excluye, no se re-verificó aquí).
- La consulta con lista vacía se comparó por construcción (misma SQL sin el `AND`) y por el test, no con un `EXPLAIN`/diffs sobre la BD de producción.
- No se probó el cambio en caliente (la config se lee al arrancar; no hay recarga en vivo).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); solo clippy + lib en dev.
