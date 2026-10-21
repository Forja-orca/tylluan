# D-23 — Índice FTS huérfano: un solo punto de borrado, contador de desincronización y rebuild sin ejecutar

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` = `59d3eaa`
**Estado:** implementado y verificado en este worktree, **sin commitear** (solo el TL comitea).
**Fuente:** `docs/internal/IMPL_T-DSH-8_fts_anonymous_identidades_2026-10-21.md` (a).
**Parche:** `E:\tylluan-patches\deep\D-23.patch` — 7 archivos, **+619 / −44**, 37.723 B.
**`git apply --check` sobre `59d3eaa` limpio: EXIT 0** (y el patch aplicado y compilado en un worktree aparte sobre `57d2e7b`: `cargo check` EXIT 0).

> Nota de sincronización: el trabajo empezó sobre `57d2e7b`; a mitad, `origin/main` avanzó a
> `59d3eaa` (donde ya están mergeados D-15 `16a7da4` y D-22a). El worktree se resincronizó, el
> parche se reaplicó y **toda la verificación de la §3 se repitió sobre `59d3eaa`**.

---

## 1. Qué estaba roto (dos causas, no una)

El informe de DSH decía "casi todos los borrados no tocan la FTS". Es cierto, pero al
implementar el punto único aparecieron **dos** fallos independientes, y el segundo explica
por qué los sitios que *sí* limpiaban la FTS también dejaban huérfanos.

**(a) El contador mentía.** `maintenance.rs::index_freshness` publicaba `fts_rows` con
`SELECT COUNT(*) FROM nodes_fts`. Sobre una tabla FTS5 de contenido externo (`content=nodes`)
esa consulta lee la **tabla de contenido**, no el índice: devuelve el número de nodos vivos.
Por eso `fts_rows` "cuadraba" siempre mientras el índice real tenía 13.497 docids sin nodo.

**(b) Borrar del índice después de borrar el nodo no hace nada.** `DELETE FROM nodes_fts
WHERE rowid = ?1` sobre contenido externo necesita leer la fila de `nodes` para desindexar.
Si el nodo ya no existe (borrado en una transacción previa ya commiteada), FTS5 no encuentra
el contenido y la sentencia **no borra nada, sin error**. Todos los sitios que limpiaban
"después" eran un no-op silencioso; el que limpiaba "después" con `.ok()` era además
inobservable. El orden correcto es: borrar del índice **antes** de borrar el nodo, en la
misma transacción cuando la haya.

**(c) Corolario encontrado al escribir el test de idempotencia:** `DELETE` sobre un docid que
**no** está en el índice aborta con error de corrupción de FTS5. Cualquier limpieza a ciegas
sobre filas en estado "missing" habría reventado el borrado del nodo. El punto único
comprueba primero contra `nodes_fts_docsize`.

Además, la guarda anti-deriva descubrió **tres caminos de borrado que no estaban en la lista
del informe original** y también generaban huérfanos: `merge_node_into` (`nodes.rs:884`),
`forget_agent` (`nodes.rs:1381`) y `prune_superseded` (`nodes.rs:1474`).

---

## 2. Qué se implementó

### 2.1 Nuevo módulo `memory/silva/fts.rs` — el único punto de borrado

| Elemento | Qué hace |
|---|---|
| `FtsDesync { orphan, missing }` + `total()` | Contadores; `Default` = sincronizado |
| `fts_delete_rowids(&Connection, &[i64])` | Borra docids **solo si están indexados** (`nodes_fts_docsize`), idempotente |
| `delete_nodes_where(conn, where_sql, params)` | Selecciona rowids, limpia FTS y borra de `nodes` en ese orden |
| `fts_clear_all(conn)` | `INSERT INTO nodes_fts(nodes_fts) VALUES('delete-all')` |
| `fts_desync_counts(conn)` | Huérfanos y ausentes contados contra `nodes_fts_docsize` |
| `fts_rebuild(conn)` | `VALUES('rebuild')` — **disponible, NO ejecutado** |
| `SilvaDB::fts_desync()` (`pub(crate)`) | Versión async para health/tests |
| `SilvaDB::rebuild_fts_index()` (`#[allow(dead_code)]`) | Envoltorio async, sin endpoint ni llamada |

`where_sql` es SQL de confianza compuesto desde constantes del crate; los valores van siempre
por `params` (`&[&dyn ToSql]`).

### 2.2 Sitios migrados al punto único

| Archivo | Sitio | Antes | Ahora |
|---|---|---|---|
| `maintenance.rs` | `apply_cleanup` | delete sin FTS | `delete_nodes_where` |
| `maintenance.rs` | `prune_dead_nodes` | delete sin FTS | `delete_nodes_where` |
| `maintenance.rs` | `purge_deprecated_lessons` | delete sin FTS | `delete_nodes_where` |
| `maintenance.rs` | `cleanup_orphan_nodes` | delete sin FTS | `delete_nodes_where` |
| `maintenance.rs` | `index_freshness` | `COUNT(*) FROM nodes_fts` | docsize + `fts_desync/orphans/missing` |
| `decay.rs` | `apply_decay` (poda FSRS) | delete sin FTS | `delete_nodes_where` |
| `decay.rs` | `prune_by_salience` | delete sin FTS | rowids + `fts_delete_rowids` en la tx |
| `decay.rs` | `prune_cold_nodes` | delete sin FTS | rowids + `fts_delete_rowids` en la tx |
| `nodes.rs` | `delete_node` | FTS después, best-effort | FTS **dentro** de la tx, antes del nodo; error aborta |
| `nodes.rs` | `merge_node_into` | sin FTS (**nuevo hallazgo**) | idem |
| `nodes.rs` | `forget_agent` | sin FTS (**nuevo hallazgo**) | tx + `delete_nodes_where` |
| `nodes.rs` | `prune_superseded` | sin FTS (**nuevo hallazgo**) | `delete_nodes_where` |
| `nodes.rs` | `purge_legacy_nested_graphrag_summaries` | FTS después, best-effort | FTS dentro de la tx, antes del nodo |
| `api_v1/api_admin.rs` | `maintenance_purge_handler` | `DELETE FROM nodes` masivo | `fts_clear_all` antes del batch |

### 2.3 Health (`/health?verbose=true`)

Bloque nuevo `components.silva_index`:

```json
"silva_index": { "ok": true, "fts_desync": 0, "fts_orphans": 0, "fts_missing": 0 }
```

`ok` es cierto **solo** si nodos e índice coinciden. Es aditivo: ningún consumidor existente
cambia de forma.

### 2.4 Guarda anti-deriva

Test `raw_node_deletes_are_only_where_fts_is_cleaned_inline` inspecciona el fuente
(`include_str!`) y fija: `maintenance.rs` con **0** `DELETE FROM nodes` crudos, `decay.rs` con
2 (ambos con `fts_delete_rowids`), `nodes.rs` con 3 (todos con `fts_delete_rowids`) y
`api_admin.rs` con `fts_clear_all`. Un borrado nuevo sin limpieza rompe el test.

---

## 3. Tests

**20 tests en `fts.rs`**, de los cuales 12 recorren caminos reales sobre `SilvaDB::in_memory()`
(`#[tokio::test(flavor = "multi_thread")]`, obligatorio por `block_in_place`):

`delete_node` · `merge_node_into` · `forget_agent` · `prune_dead_nodes` · `apply_cleanup` ·
`purge_deprecated_lessons` · `cleanup_orphan_nodes` · `prune_by_salience` · `prune_cold_nodes` ·
`prune_superseded` · `purge_legacy_nested_graphrag_summaries` · `apply_decay` (poda FSRS).
Cada uno termina en `assert_synced` (`fts_desync() == default`). Los otros 8 cubren el
contador en ambas direcciones, idempotencia, `delete-all`, `rebuild` y el pin anti-deriva.

Notas de calibración que quedan documentadas en los propios tests:
`apply_decay` recalcula el peso desde FSRS antes de podar, así que con `fsrs_last_review = 0`
la retrievabilidad es 1.0 y el nodo nunca baja del umbral: el test envejece la reseña.
`prune_superseded` solo aplica a `agent_summary` (es el tipo que usa `lifecycle_phase.rs`).

### Verificación ejecutada

| Comprobación | Resultado |
|---|---|
| `cargo test -p tylluan-kernel --lib` — dos ejecuciones completas | **1168 passed / 0 failed / 4 ignored** (30.93 s y 30.83 s) |
| `cargo clippy -p tylluan-kernel --tests -- -D warnings` | **EXIT 0** |
| `git apply --check D-23.patch` sobre `59d3eaa` limpio | **EXIT 0** |
| `cargo check -p tylluan-kernel` del patch aislado sobre `57d2e7b` | **EXIT 0** (3m 23s) |

Cifra base en `59d3eaa`: 1152 lib tests; +16 con D-23 → 1168. (En `57d2e7b` eran 1150 → 1166;
ambas cifras se midieron con el mismo código de D-23.)

Aviso de método: el primer intento de correr las dos suites a la vez chocó en el **linker**
(`link.exe` 1104, dos builds escribiendo el mismo `target/`), no en los tests. La segunda
ejecución se repitió en serie y es la que consta arriba.

---

## 4. Límites (lo que NO se hizo)

1. **No se reconstruyó ningún índice.** `fts_rebuild` / `rebuild_fts_index` existen, están
   testeados y **no se llaman desde ningún sitio**; no hay endpoint ni flag. Los 13.497
   huérfanos de la base de producción siguen ahí hasta que José decida ejecutar el rebuild.
   El bloque `silva_index` del health lo hará visible mientras tanto (`ok: false`).
2. **No se tocaron datos de producción** ni se abrió la base real. Todo el trabajo es código
   y tests sobre bases en memoria.
3. **No se añadieron triggers FTS5.** La tarea permitía "punto único o triggers canónicos".
   Se eligió el punto único: los triggers no cubren `DELETE FROM nodes` de
   `api_admin` sin pelear con `delete-all`, y dejan el orden explícito en un único helper
   revisable. Si el TL prefiere triggers, el cambio es sustituible sin tocar los llamadores.
4. **Cambio de comportamiento deliberado:** en `delete_node` y en el purge GraphRAG, un fallo
   de limpieza FTS ahora **aborta** (antes `.ok()` lo silenciaba). Es lo correcto —un docid
   huérfano es corrupción silenciosa— pero es un cambio observable: el borrado puede fallar
   donde antes "funcionaba" dejando basura.
5. El rebuild no está validado contra una base de 24k nodos reales; su coste y su impacto en
   la latencia de búsqueda durante la ejecución no están medidos.

---

## 5. Impact

Sección exigida por ADR-013 (toca `transport/`).

- **Contratos públicos:** ninguno cambia de forma. `/health` gana un bloque aditivo
  `components.silva_index`; `index_freshness` gana tres campos y corrige el valor de
  `fts_rows` (que pasaba de mentir a contar el índice real).
- **Depredación de otros agentes:** no. El parche solo añade `fts.rs` nuevo y edita
  archivos del kernel; no toca dashboard, guilds, integraciones ni configuraciones. Se
  generó contra `57d2e7b` y se probó aislado, sin arrastrar D-15 ni D-22a.
- **Regla de José:** intacta. No se toca `dev_mode`, ni tokens, ni credenciales, ni el
  arranque. El rebuild no se ejecuta por iniciativa propia.
- **Datos:** ninguna migración, ningún borrado, ninguna escritura fuera de tests.
- **Riesgo residual:** el índice actual de producción sigue desincronizado (se ve en health);
  hasta que se reconstruya, el BM25 sigue sesgado por los docids fantasma. Cerrado el código,
  falta la decisión de José sobre el rebuild.
