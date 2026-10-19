# T1277-E — Huella de embedding en las rutas HNSW e IVF (Deep, 2026-10-19)

Diff **sin commitear** en el worktree `E:\tylluan-wt-deep`, base `c00b902`
(commit local del tech lead que integra T1277-D; **no** está en `origin/main`).

## Qué se entregó

La observación de huella de embedding de T1277-D cubría solo la ruta lineal
(`search_vector_linear`, la que lee `node_embeddings` fila a fila). T1277-E la
extiende a las dos rutas indexadas de `search_vector`:

| Ruta | Antes | Ahora |
|------|-------|-------|
| HNSW (`search_vector_hnsw`) | candidatos del índice, sin juicio de procedencia | mismos candidatos + contador de modelo/dimensión ajenos |
| IVF (`search_vector_ivf`) | idem | idem |

`crates/tylluan-kernel/src/memory/silva/search.rs`:

- **`fingerprint_dropped_ids(conn, ids, expected_bytes)`** (nuevo método privado de
  `SilvaDB`): devuelve los ids que la puerta **descarta**. Ni `HnswIndex`
  (`node_ids + points`) ni `MmapEmbeddingStore` (ids + vectores cuantizados)
  guardan `model_name`/`model_hash` por nodo — verificado leyendo ambos tipos—,
  así que la procedencia sale de **una sola consulta acotada**:
  `SELECT node_id, model_name, model_hash, length(embedding) FROM node_embeddings WHERE node_id IN (...)`.
  El `IN` va **trocceado a 400 parámetros** (tope de bind de SQLite) y cubre solo
  los ids candidatos (la ventana top-k, ≤ `limit*3`), nunca un escaneo de tabla.
- **Inerte y gratis cuando no hay huella declarada:** si
  `expected_embedding_snapshot()` es `None`, el llamante ni siquiera construye la
  lista de ids (`if self.expected_embedding_snapshot().is_some()`); no hay consulta.
  Con la huella declarada y el flag `false`, los contadores suben y **el
  ranking/orden no cambia** (se devuelve exactamente lo que devolvía antes).
- **Precedencia de dimensión** igual que la ruta lineal: `length(embedding)`
  primero, modelo después. Una fila sin `node_embeddings` **no se juzga** (se
  conserva): la ausencia no es evidencia de motor ajeno.
- **Flag `true`:** descarta y cuenta en `fp_excluded`. Con el flag en `true` el
  HNSW/IVF pasa a tener la misma semántica de dimensión que ya tenía la ruta
  lineal (dropea la fila de 8 floats con query de 4).

## Verificación (ejecutada por mí, salidas reales)

```
cargo test -p tylluan-kernel --lib embedding_fingerprint -- --nocapture
  8 passed; 0 failed; 0 ignored; 0 measured; 1062 filtered out   (3 T1277-D + 5 T1277-E)

cargo clippy -p tylluan-kernel --all-targets -- -D warnings
  EXIT=0  (Finished `dev` profile in 2m 33s; único mensaje: "bundled-dashboard
  feature not enabled — skipping UI build")

scripts/check_test_count.sh
  Real counts: kernel=1067 link=75 fsrs=12 total=1154
  ✅ README.md 1154 · ✅ AGENTS.md kernel=1067 link=75 fsrs=12 total=1154   EXIT=0

scripts/check_docs_reality.sh
  EXIT=1 con UN solo ❌: docs/roadmap/ROADMAP_O3.md:42 (`onnx/int8/decima.json`)
  — preexistente y ajeno a este cambio (artefacto de entorno ya conocido).
  Sin hallazgos nuevos; los checks 4/5 (aritmética STATUS ↔ AGENTS) pasan.
```

Tests nuevos (`mod embedding_fingerprint_index_tests`):

- `hnsw_observation_counts_without_changing_results` — `HnswIndex` real construido
  directamente (el guard de 12 000 vive en `build_hnsw()`, no en el tipo);
  inerte ⇒ 4/4 candidatos y contadores `(0,0,0)`; con huella ⇒ mismos 4 y `(1,1,0)`.
- `hnsw_enabled_filter_drops_foreign_candidates` — flag `true` ⇒ `(1,1,2)`.
- `ivf_observation_counts_without_changing_results` — `MmapEmbeddingStore::create`
  real + `IVFSearcher`; inerte ⇒ 4/4 y `(0,0,0)`; con huella ⇒ lista idéntica
  (mismo orden) y `(1,1,0)`.
- `ivf_enabled_filter_drops_foreign_candidates` — flag `true` ⇒ `(1,1,2)`.
- `fingerprint_gate_cost` — sonda de coste.

## Coste medido (la cifra que pedía el tech lead)

```
cargo test -p tylluan-kernel --lib fingerprint_gate_cost -- --nocapture
  T1277-E fingerprint_gate_cost: rows=1000 candidates=300 iters=100
    gate_p50=2082us gate_p95=2329us (min=1769us max=2898us)
    — reference get_node_sync loop p50=13272us
```

- 300 candidatos = ventana HNSW con `limit=100` (`limit * 3`), tabla de 1000 filas,
  100 iteraciones. Una corrida previa sin el bloque de referencia dio
  `p50=1850us p95=1952us` (mismo entorno): el rango real ronda 1.8–2.3 ms.
- **Punto de comparación honesto:** el bucle `get_node_sync` por candidato que ya
  existía en esa misma ruta tarda `p50=13272us` para los mismos 300 ids ⇒ la
  consulta acotada añadida es **~6.4× más barata que el trabajo al que acompaña**.
- Esta cifra es de un SQLite en memoria con 1000 filas; no la he medido contra una
  base real de producción (1024d, ~10^5 filas). Ver "no verificado".

## Documentación

Cifras actualizadas (todas verificadas por `check_test_count.sh`, no por aritmética
mía): 1062→1067 kernel, 1149→1154 total en `README.md:198`, `AGENTS.md:64`,
`STATUS.md:10` y `:50`, `docs-site/src/app/page.tsx:43`,
`docs-site/.../architecture/{architecture-map,fsrs-model,roadmap}.tsx`,
`scripts/generate_architecture_svg.py:116` y `docs/assets/architecture.svg:106`
(el badge mide igual: "1,149" y "1,154" tienen la misma longitud).
`STATUS.md:50` cita `HEAD c00b902` (base, diff sin commitear) — el tech lead debe
sustituirlo por el hash real del commit cuando integre esto.

## Impact

ADR-013 / no-depredación: **añadido y con cero cambio de comportamiento por
defecto** (flag `embedding_fingerprint_filter = false`, sin huella declarada el
camino nuevo no ejecuta ninguna consulta). No se toca `transport/`, `guilds/`,
`integrations/` ni `tylluan*.toml`; solo el buscador vectorial del kernel y
ficheros de documentación. No hay WIP ajeno en el diff. No se arrancó ningún
kernel ni se tocó el puerto 47004 (exclusiones del briefing).

## No verificado / huecos honestos

1. **Ningún kernel arrancado** (exclusión explícita del briefing): todo son tests
   y `cargo`, ninguna prueba en vivo contra `:47004`.
2. **`check_head_sync.sh` no es ejecutable en este worktree** con este Bash:
   Git resuelve el fichero `.git` del worktree como
   `/mnt/e/tylluan-wt-deep/E:/tylluan/.git/worktrees/...` → `fatal: not a git
   repository` (artefacto de entorno, misma clase que `check_no_predation.sh`,
   que el tech lead corre post-commit). Verificado a mano con git nativo:
   `git rev-parse --short=7 HEAD` = `c00b902`, que es lo que cita `STATUS.md`.
3. **Sin índice HNSW de 12 000 puntos real**: `build_hnsw()` devuelve `None` por
   debajo de `HNSW_THRESHOLD`, así que el test construye un `HnswMap` de 4 puntos
   directamente. Cubre la lógica de la puerta y del orden, **no** la integración
   con el índice a escala real ni su serialización.
4. **Coste medido solo en SQLite en memoria/1000 filas.** No extrapolado a la base
   real ni con embeddings de 1024d.
5. **T1277-D no está en `origin/main`** (`99199d0` no contiene
   `embedding_fingerprint_filter`); el trabajo va sobre `c00b902`, commit local del
   tech lead. Si el push de T1277-D cambia al integrar, este diff se rebasa.
6. No he tocado los `❗️/❌` preexistentes de `ROADMAP_O3.md:42` (artefacto de
   entorno, fuera de alcance).
