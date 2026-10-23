# T-DSH-55 — Sesiones ONNX de arranque en paralelo (sparse + dense + reranker)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `00293fe` (push 129; T-DSH-54 commit `96fd758` ya integrado, como pedía el encargo: «cuando vea el push en origin/main, base = ese hash»).
- **Encargo (T1622):** crear las sesiones ONNX de arranque (SparseEngine BGE-M3, mxbai, reranker) **en paralelo** con `spawn_blocking`+join antes del bind, sin cambiar timeouts ni el orden de los resultados. Criterios: `cargo test -p tylluan-kernel --lib` verde (recuento = 1293 + tests nuevos), clippy `-D warnings` exit 0, test de que el fallo de una sesión sigue el mismo camino de degradación. Medición opcional solo con kernel `test-run-*` con puerto/DATA_DIR propios.
- **Entregable (diff SIN commitear):** `crates/tylluan-kernel/src/router/embeddings.rs`, `crates/tylluan-kernel/src/main.rs`, `crates/tylluan-kernel/src/router/matcher.rs` + conteo de tests en AGENTS.md/README.md/STATUS.md + este informe.

## 1. Por qué toca `matcher.rs` (además de los dos ficheros indicados)

El encargo citaba `router/embeddings.rs` y `main.rs` (zona `SparseEngine::try_new`). Para cargar el motor dense en paralelo hace falta **instalar un `EmbeddingEngine` ya cargado** sin volver a cargarlo: antes, `GuildMatcher::load_model_with_device` hacía carga + instalación + pre-embedding de descripciones en una sola llamada `&mut self` (no se puede lanzar a `spawn_blocking`). Se añadió `GuildMatcher::install_engine(engine, allowed_guilds)` (público) y `load_model_with_device` ahora delega en él tras cargar: **mismo comportamiento para todos sus llamantes**, y el arranque puede cargar en paralelo y instalar después. Es el cambio mínimo para no recargar el modelo; se documenta aquí porque el encargo nombraba dos ficheros y este es el tercero.

## 2. Diseño

En `main.rs`, tras `mailbox.init()` y **antes del bind**:

1. `apply_onnx_thread_cap(None)` se ejecuta **una sola vez en el hilo principal** (prepara `ORT_NUM_THREADS`/`OMP_NUM_THREADS`/`RAYON_NUM_THREADS` y el `ort::init()` global). En `embeddings.rs` se añadió un `AtomicBool` `ONNX_CAP_ALREADY_APPLIED`: las llamadas internas de cada `load_*` con `cap=None` pasan a ser **no-op**, de modo que las tres cargas en paralelo **no hacen `std::env::set_var` concurrente** (la API de entorno no es thread-safe). Los tests que llaman con `Some(n)` siguen aplicándolo siempre.
2. Se lanzan, según la config y sin cambiar condiciones:
   - `spawn_blocking(|| SparseEngine::try_new(&device))` si `silva.hybrid_sparse_enabled`;
   - `spawn_blocking(|| EmbeddingEngine::load_with_device(&path, &device))` si hay modelo configurado **y cacheado** (si no está cacheado se mantiene el arranque rápido BM25 + descarga en background, igual que antes);
   - `spawn_blocking(|| catch_unwind(RerankEngine::load_with_device(&device)))` si `onnx_runtime_ort_compatible() && silva.reranker_enabled` (el `catch_unwind` viaja dentro de la tarea: misma defensa que había).
3. Cada sesión se **espera e instala en su punto original**, así que el **orden de resultados es el mismo de antes**: sparse → dense → reranker. Sin cambios de timeouts ni de condiciones.
4. Fallos: `embeddings.rs` expone `EngineLoadOutcome<T> { Loaded(T), Failed(String) }` y `await_engine_load(handle)` (`#[doc(hidden)]`, público porque `main.rs` es otro crate). `Ok(Err(..))` y `JoinError` (panic de la tarea) ⇒ `Failed(msg)`; `main` conserva **exactamente** los caminos de degradación previos:
   - sparse: `warn!("⚠️ SparseEngine failed to load (non-fatal, sparse source disabled): {e}")` y sigue sin sparse;
   - dense: `error!("❌ Embedding model '{}' failed to load: {e} — continuing BM25-only (semantic search disabled)")`;
   - reranker: `warn!("⚠️ Reranker no disponible (fallback a RRF puro): {e}")` ⇒ `None`.

## 3. Test de degradación (en la lib, para el gate `--lib`)

`engine_load_outcome_maps_failures_to_the_same_degradation_path` (`mod tests` de `embeddings.rs`):

- éxito ⇒ `Loaded(valor)` (el motor se propaga tal cual);
- `Err` de carga ⇒ `Failed(mensaje)` con el texto del error (el `main` aplica su warn/log/BM25);
- **panic dentro de la tarea** ⇒ `Failed("tarea de carga de motor abortada: …")`: nunca se propaga como éxito ni aborta el arranque por esta vía;
- **el fallo de una sesión no contamina a las otras dos**: con tres cargas (ok, error, ok) en `tokio::join!`, las otras dos terminan `Loaded`.

## 4. Verificación

| comprobación | resultado |
|---|---|
| `cargo test -p tylluan-kernel --lib` | **1207 passed, 0 failed, 4 ignored** (1206 previos + 1 nuevo; 38,93 s) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** (incluye bin `main.rs` y matcher) |
| `cargo check -p tylluan-kernel --bins` | exit 0 |
| `bash scripts/check_test_count.sh` | ✅ **1294** (1207 kernel + 75 link + 12 fsrs); `--fix` actualizó AGENTS.md/README.md y STATUS.md líneas 10 y 50 a mano |
| `python scripts/check_doc_links.py` | exit 0 |
| `bash scripts/check_docs_reality.sh` | solo el artefacto preexistente `ROADMAP_O3.md` (línea 42, WSL) |
| orden de resultados | sin cambios: sparse → dense (instala y pre-embebe guilds) → reranker |
| timeouts / condiciones | sin cambios (mismas condiciones y mismos valores) |
| diff | `embeddings.rs`, `main.rs`, `matcher.rs` + AGENTS/README/STATUS (conteo) + este informe; sin commitear |

## Impact

- **Arranque más corto**: las tres lecturas de modelo dejan de sumar. En la medición de T-DSH-51/53, sparse ≈ 8,2 s + dense (mxbai) ≈ 5,7 s + reranker ≈ 1,2 s (≈ 15 s en serie); en paralelo el bloque pasa a ≈ max(8,2 s) + instalación ⇒ **~6-7 s menos** antes del bind (estimación aritmética, no medida: ver «No verificado»). El fingerprint de T-DSH-54 (12-15 s) queda cubierto por su caché y no se toca.
- **Cero cambio de semántica**: mismos logs/condiciones/orden de instalación, mismos caminos de degradación; una sesión que falla (Err o panic) no tumba el arranque.
- **Protección añadida**: el thread-pool/entorno de ONNX se configura una vez antes de paralelizar y las cargas ya no escriben variables de entorno concurrentemente.

## No verificado

- **NO medí el ahorro real en un arranque del kernel**: la medición opcional con `test-run-*` (puerto y DATA_DIR propios) no se ejecutó; no arranqué ningún kernel. Los ~6-7 s son aritmética de las fases medidas en T-DSH-51/53.
- **Pico de RAM**: tres sesiones cargando a la vez (BGE-M3 ~1-2 GB + mxbai 1,34 GB + reranker 151 MB) pueden subir el pico de memoria respecto a la carga en serie; **no medido**. En hardware modesto (RPi4 4 GB) convendría limitar la concurrencia a 2 o cargar el reranker después; queda como riesgo declarado.
- **Concurrencia interna de `ort`/fastembed**: el pre-`ort::init().commit()` en el hilo principal evita el caso peor, pero si `TextEmbedding::try_new`/`SparseTextEmbedding::try_new` intentan `ort::init` a la vez, la garantía de hilo-seguridad no la verifiqué en vivo (no ejecuté el arranque). En debug (panics unwind) el `catch_unwind` del helper convierte un panic de tarea en degradación; en release `panic="abort"` sigue siendo abort (igual que antes).
- **`apply_onnx_thread_cap` con guard**: los `hot_swap_engine` posteriores llaman con `None` y ahora son no-op (el cap ya está aplicado con el mismo valor). No probé un cambio de config en caliente (el cap se fija una vez por proceso, que es el comportamiento deseado, pero es un cambio observable de la función).
- **Medición de guild embeddings**: siguen en serie al instalar el dense (≈24 s en frío según T-DSH-51); el encargo era solo las sesiones ONNX. No lo toqué.
- **Comportamiento real con GPU (DirectML/CUDA)**: las tres sesiones en paralelo sobre el mismo device no está probado; aquí es CPU.

## Verificacion del TL (2026-10-24)
Aplique tu diff sobre `fa39592` y repeti: `cargo test -p tylluan-kernel --lib` = 1207 passed, 0 failed, 4 ignored (igual que el informe); `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` = exit 0. Revise `main.rs`: los tres `spawn_blocking` se esperan en su punto original, los mensajes de degradacion se conservan y `apply_onnx_thread_cap` se llama una vez antes de lanzarlos. No medido: el ahorro real de arranque (requiere reiniciar el kernel) ni el pico de RAM con las tres cargas a la vez.
