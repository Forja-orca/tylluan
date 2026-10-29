# T-DSH-136 — `[memory] reindex_node_delay_ms` configurable (default 200 ms)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = 3060839` (push198), `git rev-parse --short HEAD` = `3060839` al empezar.
- Diff sin commitear: `config.rs` (+34) y `transport/http/api_v1/api_memory.rs` (+5/−1).
- Encargo (T2099): en `POST /memory/reindex` el `sleep(200 ms)` por nodo pasa a clave de config `[memory] reindex_node_delay_ms` con **default 200** (comportamiento idéntico por defecto) + test unitario de lectura de la clave; clippy `-D warnings`, lib test; informe con `## Impact` y `## No verificado`.

## Cambio

### `config.rs`

- Nuevo campo en `MemoryConfig`:
  ```rust
  /// T-DSH-136: pausa por nodo del bucle de `POST /memory/reindex`.
  /// Default 200 ms = comportamiento historico; 0 = sin pausa.
  #[serde(default = "default_reindex_node_delay_ms")]
  pub reindex_node_delay_ms: u64,
  ```
- `default_reindex_node_delay_ms() -> 200` y añadido al `impl Default for MemoryConfig`.
- Un `tylluan.toml` sin la clave se comporta exactamente igual que antes (200 ms).

### `api_memory.rs` (`reindex_handler`)

- Antes del `tokio::spawn`, lee la clave una vez: `let reindex_delay_ms = state.config.read().await.memory.reindex_node_delay_ms;`
- El bucle usa `tokio::time::sleep(Duration::from_millis(reindex_delay_ms))` en lugar del literal `200`.
- Sin más cambios en el handler (misma lista stale, mismo `save_embedding`, mismos eventos de progreso).

## Tests

- `t136_reindex_delay_tests::reindex_node_delay_default_is_200ms`: `MemoryConfig::default()` y `TylluanConfig::default()` dan 200.
- `t136_reindex_delay_tests::reindex_node_delay_parses_from_toml`: `[memory] reindex_node_delay_ms = 5` se lee como 5; sin la clave sigue en 200.

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib t136                     → 2 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1422 passed; 0 failed; 4 ignored
```

Lib 1422 = 1420 de main + **2 tests nuevos**.

## Impact

- La pausa por nodo del reindex deja de ser un literal: `[memory] reindex_node_delay_ms` permite acelerar el backfill en test-run (p. ej. 0) sin tocar código, y el **default 200 ms mantiene el comportamiento idéntico** para producción (T-DSH-135 midió ~1 s/nodo y ~9,7-13,6 s por lote de 10 con ese sleep).
- Sin cambio de contrato ni de semántica del endpoint: solo el valor de la pausa.

## No verificado

- No se ha medido en test-run el efecto de `reindex_node_delay_ms=0` sobre el ritmo real del reindex (solo se verificó la lectura de la config y que el default es 200); T-DSH-135 midió con 200.
- No se probó el endpoint con la clave cambiada en caliente (la config se lee al entrar en el handler; no hay recarga en vivo).
- No se comprobó que todos los consumidores de `MemoryConfig` (serde) sigan funcionando con configs antiguas más allá de los tests unitarios (el `#[serde(default)]` cubre el caso sin clave).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); solo clippy + lib en dev.
