# T-DSH-54 — Caché persistente de la huella del modelo (evita re-hashear 1,34 GB por arranque)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `124b9d0` (push 128). Encargo en Coloquio T1609 (lo leí tarde: había saltado del turno 1608 al 1610; disculpa registrada en la entrega).
- **Entregable (diff SIN commitear):** `crates/tylluan-kernel/src/router/embeddings.rs` (src + unit tests en el mismo módulo) + este informe. Sin tocar el algoritmo del hash ni el formato de la huella; sin arrancar el kernel ni tocar puertos.

## 1. Problema que resuelve

`compute_engine_fingerprint` (`router/embeddings.rs`) hashea el `model.onnx` del snapshot de HF y el `tokenizer.json` para sellar la identidad del motor. En el arranque medido (T-DSH-53) eso es **15,5 s / 12,3 s antes del bind**, porque el `model.onnx` de mxbai pesa **1,34 GB** (lleva los pesos dentro, no en `model.onnx_data`). El mismo fichero lo acaba de leer `ort` para crear la sesión: se lee dos veces.

El contenido de un snapshot de HF es inmutable por revisión, y la huella solo cambia si cambia el contenido. Con la clave **(ruta, tamaño, mtime)** se puede reutilizar el sha256 ya calculado mientras el fichero no se toque.

## 2. Implementación

Todo en `crates/tylluan-kernel/src/router/embeddings.rs` (un solo fichero de `src`; los tests son unitarios en su `mod tests`, como pide el gate `cargo test -p tylluan-kernel --lib embeddings`):

- **Fichero de caché**: `<DATA_DIR>/fingerprint_cache.json`, donde `DATA_DIR` = `TYLLUAN_DATA_DIR` si está definido y no vacío, si no `data/`. `data/` está en `.gitignore` (no ensucia el árbol).
- **Estructura JSON** (`version: 1`):
  ```json
  { "version": 1,
    "entries": { "<ruta absoluta>": { "size": 1336854282, "mtime_ns": 1759..., "sha256": "31086..." } } }
  ```
- **Lectura tolerante**: fichero ausente, JSON corrupto o versión distinta ⇒ caché vacía; nunca panic.
- **Escritura atómica**: JSON a `*.tmp-<pid>` en el mismo directorio y `rename`; en Windows, si el rename falla porque el destino existe, se borra el destino y se reintenta. Cualquier error de I/O se ignora (la caché es una optimización, la huella debe salir igual).
- **`sha256_file_cached(path, cache_path)`**: si `cache_path` es `None`, comportamiento anterior exacto (hash directo). Si hay caché: calcula la clave (ruta, tamaño, mtime_ns); si la entrada coincide en tamaño **y** mtime, devuelve el sha guardado sin leer el fichero; si no, llama al `sha256_file` original, guarda/actualiza la entrada y devuelve el hash real.
- **`compute_engine_fingerprint`** ahora localiza el snapshot y delega en `compute_engine_fingerprint_from_snapshot(snapshot, model_type, dimension, cache_path)`; el núcleo usa `sha256_file_cached` para el grafo y el tokenizer. **Las `parts` y su orden no cambian**, así que las huellas guardadas en las embeddings siguen siendo comparables byte a byte.
- **Sin cambios prohibidos**: `sha256_file` intacto (streaming 64 KiB, mismo algoritmo), `fingerprint_from_parts` intacto (formato `sha256:<64 hex>`).

## 3. Tests (ficheros temporales; sin modelos reales ni red)

5 nuevos tests en el `mod tests` de `embeddings.rs`:

| test | qué comprueba |
|---|---|
| `fingerprint_cache_hit_returns_stored_hash_without_rehashing` | primera llamada hashea y escribe; se sabotea la entrada con el mismo (size, mtime) y un sha centinela → la segunda devuelve el centinela (no re-hashea) |
| `fingerprint_cache_invalidates_when_size_changes` | cambiar el contenido (y tamaño) invalida: recalcula y devuelve el sha real |
| `fingerprint_cache_invalidates_when_mtime_changes` | `File::set_times` a +120 s con **mismo contenido y tamaño** → invalida y recalcula (el sha real es el mismo, pero sale del disco) |
| `fingerprint_cache_corrupt_json_recomputes_and_rewrites` | JSON corrupto → recalcula, no panic, y reescribe JSON válido v1 con la entrada |
| `engine_fingerprint_is_identical_with_and_without_cache` | snapshot falso con layout `onnx/model.onnx`: la huella con caché (1.ª y 2.ª llamada) es **idéntica** a la de sin caché, empieza por `sha256:` y mide 71 caracteres (formato intacto) |

## 4. Verificación

| comprobación | resultado |
|---|---|
| `cargo test -p tylluan-kernel --lib embeddings` | **29 passed, 0 failed, 2 ignored** (incluye los 5 nuevos; 3,36 s) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `bash scripts/check_test_count.sh` | ✅ 1293 (1206 kernel + 75 link + 12 fsrs); `--fix` actualizó AGENTS.md/README.md y **STATUS.md** línea 10 y línea 50 a mano (el flag no cubre STATUS) |
| `python scripts/check_doc_links.py` | exit 0 |
| `bash scripts/check_docs_reality.sh` | solo el artefacto preexistente `ROADMAP_O3.md` (línea 42, WSL) |
| huella idéntica con/sin caché | test `engine_fingerprint_is_identical_with_and_without_cache` |
| diff | `router/embeddings.rs` modificado + AGENTS.md/README.md/STATUS.md (solo conteo de tests) + este informe; sin commitear |

## Impact

- **Ahorro esperado en el arranque**: la primera vez se paga el sha256 (12-15 s); en los arranques siguientes, con el snapshot intacto, se reutiliza el hash y desaparece ese bloque del camino crítico antes del bind (la medición exacta del ahorro requiere reiniciar el kernel, prohibido para mí: **no medido**).
- **Cero cambio semántico de la huella**: `parts` y formato idénticos; las comparaciones contra `engine_hash` guardados siguen funcionando. Si el modelo se re-descarga con el mismo contenido pero otro mtime, se recalcula (seguro, solo se pierde el ahorro).
- **La caché nunca puede romper el arranque**: cualquier error de lectura/escritura del JSON cae al hash directo.
- **Coste**: un fichero JSON pequeño bajo `data/` (una entrada por fichero de identidad: grafo + tokenizer).

## No verificado

- **Ahorro real en un arranque del kernel**: no lo medí (no puedo reiniciar el kernel). La lógica garantiza que con (size, mtime) coincidentes no se lee el fichero, pero el tiempo de boot no está medido end-to-end.
- **Huella real de mxbai con caché**: los tests usan ficheros temporales; no ejecuté `compute_engine_fingerprint` contra el `.fastembed_cache` real (el test existente `engine_fingerprint_matches_local_cache_when_present` sigue verde, pero no mide tiempos).
- **Granularidad de mtime**: NTFS da 100 ns; en un FS con mtime grueso (FAT, algunos montajes) un cambio de contenido del mismo tamaño dentro del mismo tick podría pasar desapercibido. El contenido de un snapshot de HF no se modifica in-place, así que el riesgo práctico es bajo, pero no lo probé en FAT.
- **Integridad de la caché como fuente de confianza**: el sha cacheado no se re-verifica contra el contenido (ésa es la gracia); un proceso con permiso de escritura sobre `data/fingerprint_cache.json` podría envenenarla. Es el mismo nivel de confianza que el resto de `data/` (operador local); no lo traté como superficie de ataque.
- **Concurrencia multi-proceso**: la escritura es atómica (tmp+rename) y la última gana; dos kernels simultáneos podrían pisarse entradas, sin corrupción (JSON completo). No probado con dos kernels.
- **`TYLLUAN_DATA_DIR` y cwd**: si el kernel arranca con cwd distinto de la raíz del workspace y sin `TYLLUAN_DATA_DIR`, la caché irá a otro `data/` relativo y simplemente no acertará; no verificado en ese escenario.
- **Número de líneas de STATUS**: actualicé a mano 1293/1206 porque el gate lo exige; el commit del conteo lo verifica el TL.

## Verificacion del TL (2026-10-24)
Copie `router/embeddings.rs` y el recuento de tests de AGENTS.md, README.md y STATUS.md sobre `124b9d0` y repeti: `cargo test -p tylluan-kernel --lib embeddings` = 29 passed, 0 failed, 2 ignored (como el informe); `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` = exit 0. Revise el diff: el hash sigue siendo el sha256 real del contenido, la clave es (ruta, tamano, mtime) y la escritura es atomica con fallo silencioso hacia el hash directo. Limite conocido: un fichero modificado conservando tamano y mtime no invalida la entrada. El ahorro de 12-15 s en el arranque no esta medido (requiere reiniciar el kernel).
