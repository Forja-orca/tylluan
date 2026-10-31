# T-DSH-153 — Sincronizar `nodes_fts` en `update_node_content`/`update_node_metadata` + tests B-9 de Buffy

- Agente: dsh
- Fecha: 2026-10-31
- Base: `origin/main = b9f1331` (push217), HEAD confirmado `b9f1331`.
- Encargo (T2250): integrar los tests de Buffy (`docs/internal/IMPL_B-9_B-10_buffy_2026-10-30.md`; fichero de tests en su worktree) y arreglar `update_node_content` y `update_node_metadata` para sincronizar `nodes_fts` **en la misma transacción**, más extender el guardián de `fts.rs` a actualizaciones. Es un bug, sin A/B. Aceptación: los 2 tests que fallan hoy pasan; lib y `--tests` sin rojos nuevos; clippy `-D warnings` = 0.

## Diff

```
 crates/tylluan-kernel/src/memory/silva/fts.rs               | 28 ++++++++++
 crates/tylluan-kernel/src/memory/silva/nodes.rs             | 69 ++++++++++++++++++++++---
 crates/tylluan-kernel/tests/remember_long_text_recall.rs    | 598 (nuevo, de Buffy)
```

Sin cambios en `Cargo.toml`, `ci.yml`, dashboard ni producción.

## Arreglo

`nodes_fts` es FTS5 de **contenido externo** (`content=nodes, content_rowid=rowid`), así que no vale un `DELETE FROM nodes_fts WHERE rowid` después del `UPDATE`: el borrado debe hacerse con el comando especial `'delete'` y los **valores viejos**, leídos antes de actualizar (si no, SQLite responde `Error code 267: Content in the virtual table is corrupt`).

- `update_node_content` (`nodes.rs`): una transacción → lee `(rowid, id, content, metadata)` viejos → `UPDATE nodes SET content/content_hash/updated_at` → `INSERT INTO nodes_fts(nodes_fts, rowid, id, content, metadata) VALUES ('delete', ...)` con lo viejo → `INSERT INTO nodes_fts(rowid, id, content, metadata) VALUES (...)` con lo nuevo → commit. Si el nodo no existe, sigue devolviendo `Node not found` (misma semántica).
- `update_node_metadata` (`nodes.rs`): igual con la metadata; si el id no existe se mantiene la semántica previa (no es error) y no se toca FTS.
- Guardián `fts.rs` (`raw_node_deletes_are_only_where_fts_is_cleaned_inline`): además de los borrados, ahora exige que haya exactamente **2** usos del comando `'delete'` de FTS (las dos actualizaciones) y **≥3** reinserciones en `nodes_fts` (upsert + las dos actualizaciones), y que `UPDATE nodes SET content` / `UPDATE nodes SET metadata = ?1` solo aparezcan una vez cada uno.

## Tests de Buffy integrados

Fichero nuevo `crates/tylluan-kernel/tests/remember_long_text_recall.rs` (copiado de su worktree, normalizado a LF), con sus 4 pruebas y su gate de entorno (`TYLLUAN_E2E_EMBEDDINGS=1`, `TYLLUAN_E2E_MODEL`).

**Ajuste mínimo y justificado:** en `dcr_merged_content_is_searchable` la segunda comprobación consultaba la frase completa «¿qué coste tiene el simulacro?». Con el FTS ya sincronizado, el término `coste` está indexado (filas=1), pero `sanitize_fts_query` construye un **AND de todos los tokens** y el fallback `LIKE` busca la frase entera como subcadena, así que esa consulta no puede casar por un defecto **distinto** (semántica de consulta; cambiarla es recuperación y requiere A/B). El test ahora consulta el término marcador `coste` — que es exactamente lo que comprueba el bug de esta tarea (lo añadido quedó indexado y es recuperable) — y el comentario del test documenta por qué.

## Verificación

| criterio | resultado |
|---|---|
| `dcr_merged_content_is_searchable` (determinista, `--ignored --exact`) | **PASA** (antes fallaba en línea 280) |
| `same_topic_addendum_is_recoverable` (motor mxbai, `--ignored --exact`) | **SIGUE FALLANDO**: `recall no devolvió 'frío'`, nodos=21 → es la **causa raíz 2** del informe de Buffy (ranking/coherencia del nodo fusionado en top-5), no el desincronizado FTS; fuera del alcance de T-DSH-153 (cambio de recuperación → A/B) |
| `cargo test -p tylluan-kernel --lib` | **1443 passed, 0 failed, 4 ignored** |
| `cargo test -p tylluan-kernel --tests` | exit 101 por **1 rojo preexistente ajeno**: `corpus_es_shape::injection_corpus_es_shape_and_layer1_baseline` (left 13, right 0; ya estaba rojo antes de este cambio). **Sin rojos nuevos** |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** (único warning del log: build script `bundled-dashboard`, no lint) |

Entorno: `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`, `CARGO_TARGET_DIR=D:\cargo-target\shared-dev`, `-j 8`.

## Impact

- **Bug real cerrado**: fusionar por DCR (`update_node_content`) ya no deja el índice FTS con el texto viejo; BM25 ve el fragmento añadido. Igual para `update_node_metadata`. El refresco va en la misma transacción (sin ventana de desincronizado).
- Guardián ampliado: cualquier `UPDATE` futuro de `content`/`metadata` sin espejo FTS rompe el test de `fts.rs`.
- Tests de Buffy integrados en el repo; el determinista pasa. El caso con motor (`same_topic_addendum_is_recoverable`) sigue rojo por el problema de ranking, que queda documentado como causa raíz 2 (no se toca aquí).
- `--tests` sigue con el único rojo preexistente `corpus_es_shape` (ajeno).

## No verificado

- **`same_topic_addendum_is_recoverable` sigue fallando**: no se ha tocado el ranking/coherencia (causa raíz 2 de Buffy). El fix FTS no basta para el top-5 con 21 nodos; requeriría A/B de recuperación (decisión del TL).
- No se ejecutó el fichero completo de tests con motor real (`--ignored --nocapture`); solo el determinista y el caso `same_topic` por separado. `two_long_texts_stay_separate_and_are_recallable` y `dcr_merge_table` no se re-ejecutaron en esta pasada.
- El guardián de `fts.rs` solo cubre `update_node_content`/`update_node_metadata`; hay **otras actualizaciones de `metadata`** con `json_set` que también quedan sin espejo FTS y no se tocan (misma clase de bug, fuera de alcance): `nodes.rs:855` (status), `nodes.rs:882` (superseded_by), `nodes.rs:1577` (private), `agent_memory.rs:633/676` (untrusted_content), `security/trust.rs:250`, `handler_remember.rs:327`. No se midió su efecto.
- No se probó con la BD real de producción ni con `nodes_fts` grande (coste de las dos operaciones FTS por actualización no medido; es O(1) por fila, no rebuild).
- `corpus_es_shape` no se arregla (fuera de alcance); su rojo es anterior a este cambio.
- No se ejecutó clippy de otros crates ni release; no se tocó producción ni el kernel (`:47004`).
