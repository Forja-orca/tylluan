# IMPL — T1277-D: filtro de huella de embeddings en la búsqueda, **modo observación**

**Agente:** Deep (OpenCode · Rust/kernel)
**Fecha:** 2026-10-19
**Base:** `a220fb8` (worktree `E:\tylluan-wt-deep`, detached) — **diff sin commitear**, lo commitea el tech lead
**Estado:** implementado y verificado; el filtro real queda **apagado por defecto**

---

## Qué se hizo

Una fila de `node_embeddings` escrita por un motor de embeddings distinto al que
vectoriza la consulta **no es comparable** con el vector de consulta. Hasta ahora:

- el path lineal **descartaba en silencio** las filas con dimensión distinta
  (`blob.len() != expected_bytes`, `search.rs`) — invisible en cualquier métrica;
- las filas con `model_name`/`model_hash` distintos pero misma dimensión **pasaban**
  al ranking y lo contaminaban.

T1277-D añade una **puerta de huella de motor** con dos modos:

| Modo | Comportamiento |
|------|----------------|
| **Observación (defecto, flag OFF)** | cuenta cuántos candidatos *habría* excluido (por modelo y por dimensión) y **los deja pasar** — cero cambio de ranking |
| **Filtro (flag ON)** | el path lineal **descarta** los candidatos de motor ajeno |

### Semántica de hash (espejo exacto de `get_stale_embeddings`)

Una fila es compatible con este proceso si:

1. `model_name == expected.model_name`, **y**
2. `model_hash` es `NULL`, **o** el centinela `unknown-pre-hash`, **o** igual al
   fingerprint esperado; un `expected.model_hash = None` (motor sin identidad
   verificable en disco) acepta cualquier hash conocido.

Las filas pre-trazabilidad (`unknown-pre-hash`) **nunca** cuentan como mismatch:
mismo criterio que ya usa el Agnostico Reindexer (TL 2026-10-05).

### Superficie tocada

| Archivo | Cambio |
|---------|--------|
| `crates/tylluan-kernel/src/config.rs` | `[silva] embedding_fingerprint_filter: bool` (default `false`) + `default_embedding_fingerprint_filter()` + entrada en `impl Default for SilvaConfig` |
| `crates/tylluan-kernel/src/memory/silva/mod.rs` | `pub struct ExpectedEmbedding { model_name, model_hash }`; 5 campos en `SilvaDB` (`expected_embedding`, `fingerprint_filter_enabled`, 3 contadores atómicos); métodos `set_expected_embedding`, `set_expected_embedding_from_dense_engine`, `set_fingerprint_filter_enabled`, `fingerprint_filter_enabled`, `fingerprint_observation`, `expected_embedding_snapshot`; campos inicializados en los **dos** literales (`open()` e `in_memory()`) |
| `crates/tylluan-kernel/src/memory/silva/search.rs` | predicado libre `embedding_row_matches_engine()`; path lineal: `SELECT node_id, embedding, model_name, model_hash`, contador de dimensión (antes mudo), puerta de modelo; 3 tests |
| `crates/tylluan-kernel/src/main.rs` | tras `install_dense_engine` en el boot: `set_expected_embedding_from_dense_engine()` + flag desde config; tras el **hot-swap** en background: refresco de la huella esperada |
| `README.md`, `AGENTS.md`, `STATUS.md` | recuento de tests (ver «Recuentos» abajo) |

**Invariante de inercia:** sin huella esperada declarada (`expected = None`), la
puerta es **no-op total** — ni cuenta ni excluye (asertado en test).

## Impact (ADR-013 — pregunta de no-depredación)

- **¿Rompe algo existente?** No. Con el flag en su default (`false`) y/o sin huella
  declarada, el comportamiento es **byte-idéntico**: los mismatches de dimensión
  siguen descartándose (ahora contados), los de modelo siguen pasando. Activarlo
  (decisión del tech lead/José) sí cambia resultados de recall: deja de devolver
  memorias vectorizadas por otro motor (por diseño, son vectores incompatibles).
- **¿Toca `transport/`, `guilds/`, `integrations/`, `tylluan*.toml`?** No.
- **¿Depreda trabajo de otro agente?** No: no toca `identity.rs` ni los scripts de
  claims (entrega de Deep en integración por el tech lead); sin solape de archivos
  con esa entrega.
- **¿Migración/schema?** Ninguna. No se escribe ni reescribe ninguna fila.

## Verificación real (ejecutada, no inferida)

| Comando | Resultado |
|---------|-----------|
| `cargo check -p tylluan-kernel` | OK |
| `cargo test -p tylluan-kernel --lib embedding_fingerprint_tests` | **3 passed** (0 failed) |
| `cargo test -p tylluan-kernel --lib` | **1060 passed**, 0 failed, 3 ignored |
| `cargo clippy -p tylluan-kernel -- -D warnings` | **exit 0** |
| `scripts/check_test_count.sh` | **exit 0** — kernel=1060 link=74 fsrs=12 total=1146 |
| `scripts/check_docs_reality.sh` | único ❌ preexistente `docs/roadmap/ROADMAP_O3.md:42`; sin ❌ nuevos (AGENTS/STATUS consistentes) |
| `scripts/check_no_predation.sh` | sin señal útil en este worktree (HEAD detached → "empty repo"); el tech lead lo corre tras commitear |

### Los 3 tests

1. `predicate_mirrors_get_stale_embeddings_semantics` (tabla): mismo modelo+hash ✓,
   `NULL` ✓, centinela ✓, otro modelo ✗, otro hash ✗, `expected_hash=None` acepta
   cualquier hash conocido.
2. `observation_mode_counts_without_excluding`: sin huella → `(0,0,0)`; con huella,
   el candidato de modelo ajeno **se cuenta y se devuelve**, el de dimensión ajena se
   descarta (conducta previa) pero **se cuenta**; `excluded == 0`.
3. `enabled_filter_excludes_foreign_model_only`: flag ON → el modelo ajeno
   desaparece de los resultados, `excluded == 1`; propio modelo y centinela siguen.

## Recuentos de tests (y aviso de merge)

Recuentos escritos contra **mi base** (`a220fb8`, link=74): AGENTS/README/STATUS →
`1060 kernel + 74 link + 12 fsrs = 1146`.

⚠️ **La entrega de `identity.key` del tech lead sube link 74→75 (total 1144) y toca
las mismas 4 líneas de recuento.** Al mergear ambas, la cifra correcta es
**`1060 kernel + 75 link + 12 fsrs = 1147`**, y la línea `**HEAD:**` de `STATUS.md`
debe re-sincronizarse (hash + cifra) al commit resultante — no lo hago yo porque el
commit no es mío.

## NO cubierto en este incremento (límites declarados)

1. **HNSW e IVF no están cubiertos.** `search_vector` prueba HNSW primero y IVF
   después; con un índice en memoria cargado, el path lineal **no se ejecuta** y los
   contadores quedan en 0. Es decir: los contadores son significativos cuando el
   corpus va por el path lineal (< `HNSW_THRESHOLD` = 12k nodos) y **ciegos** con
   índice HNSW/IVF cargado. Extender la observación a esas rutas (filtro por id con
   un `SELECT ... WHERE node_id IN (...)`, y/o `WHERE` en el rebuild) es el
   siguiente incremento — no lo he hecho a medias a propósito.
2. **Sin exposición por API/dashboard.** `fingerprint_observation()` es `pub` en
   `SilvaDB`, pero ningún endpoint lo publica todavía (decisión de superficie).
3. **Sin conteo DB-wide** de filas ajenas al margen de una consulta (solo se cuenta
   lo que entra en la ventana `LIMIT 5000` del path lineal).
4. **No verificado en vivo** contra el kernel de José (puerto 47004 intocable);
   verificación por test/crate, no por boot.

## Para el tech lead

- El flag `[silva] embedding_fingerprint_filter` es la palanca para pasar de
  observación a filtro real; por defecto queda en observación.
- Dado el límite (1), antes de decidir activarlo conviene saber si el corpus real
  pasa por HNSW (≥12k nodos): con índice cargado, activar el flag solo afectaría al
  path lineal. Verificable con `fingerprint_observation()` + tamaño de la DB.
