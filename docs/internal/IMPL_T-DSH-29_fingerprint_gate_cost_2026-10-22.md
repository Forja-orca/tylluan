# T-DSH-29 — `fingerprint_gate_cost` inestable bajo carga: reproducción y arreglo del test

- **Agente:** dsh
- **Fecha:** 2026-10-22
- **Base:** worktree `E:\tylluan-wt-dsh` en `--detach origin/main` (`27e3a57`); limpio antes de empezar.
- **Encargo (turno 1538):** `fingerprint_gate_cost` falló otra vez bajo carga (p95 179 ms vs presupuesto 53 ms) aunque D-20 lo hizo relativo. Reproducirlo (lib completa en paralelo, 10 pasadas, anotar fallos) y proponer el arreglo MÁS SIMPLE y estable: medir el mínimo de N rondas, un umbral derivado de la línea base de la misma pasada, o moverlo al carril pesado con `#[ignore]` y motivo. Diff sin commitear solo del test (`src/memory/silva/search.rs`), clippy exit 0, **5 pasadas completas de la lib sin fallos pegadas**. Informe con `## Impact` y no verificado.

## Reproducción (10 pasadas de la lib completa, `-j 8`)

| pasada | resultado | `fingerprint_gate_cost` |
|---|---|---|
| 1 | ok 1177 | ok |
| **2** | **FAILED (1 failed)** | **FAILED** |
| 3-10 | ok 1177 | ok |

Cifras de la pasada 2 (stdout del test):

```text
T1277-E fingerprint_gate_cost: rows=1000 candidates=300 iters=100
gate_p50=4493us gate_p95=129447us (min=2937us max=227654us)
— reference get_node_sync loop p50=23501us, budget=94004us (4x, floor 2000us)
```

**Diagnóstico:** la puerta estaba **5× más barata en mediana** que la referencia (4,5 ms vs 23,5 ms), pero su **p95** (129 ms, min 2,9 / max 227 ms) se comparaba con el **p50** de la referencia. El p95 no medía la puerta: medía la cola de preempción del scheduler de una pasada completa de la lib en paralelo. D-20 ya había hecho relativo el umbral, pero mezclaba percentiles (p95 contra p50), así que la cola de la máquina seguía decidiendo el veredicto.

## Arreglo aplicado (solo test, en el módulo `embedding_fingerprint_index_tests` de `src/memory/silva/search.rs`)

1. **Mismo percentil a los dos lados:** se compara `p95` de la puerta contra `p95` de la referencia **de la misma ronda** (misma cola de carga, mismo tipo de estadístico).
2. **Mínimo de 3 rondas:** 3 rondas × 60 iteraciones, intercalando puerta y referencia en cada iteración; se elige la ronda con **menor p95 de la puerta** (la menos interferida por el host) y se exige que su p95 ≤ `max(4 × ref_p95_esa_ronda, 2000 µs)`.
3. **Margen y suelo siguen explícitos** (4× el p95 de la referencia, suelo 2 ms), y el mensaje de fallo imprime las tres rondas y la elegida. Si la mejor de 3 rondas supera el presupuesto, **el test falla**: la propiedad ("la puerta acotada no domina el coste") se sigue midiendo; lo que se descarta es el peor p95 de una pasada como si fuera regresión.

Por qué **no** se movió a carril pesado (`#[ignore]`): la propiedad **sí** se puede medir de forma estable con mismo-percentil + min-de-rondas; ignorarlo habría quitado la vigilancia de una optimización de memoria real del camino caliente.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test --lib fingerprint_gate_cost -- --nocapture` (aislado) | **1 passed**; gate p50 2057 µs / p95 2211 µs, referencia p50 13205 / p95 13645 µs, presupuesto 54580 µs |
| **5 pasadas completas de la lib pegadas** (`-j 8`) | **5/5 verdes**: 1177 passed / 0 failed / 4 ignored en cada una (33,6 / 34,0 / 40,6 / 35,3 / 35,3 s), `fingerprint_gate_cost ... ok` en las 5 |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `git diff` | **solo** `crates/tylluan-kernel/src/memory/silva/search.rs` (+75/−41, todo dentro del `mod` de tests) |
| `check_test_count.sh` | exit 0 (1264; el recuento lib no cambia: el test ya existía) |
| `check_doc_links.py` | exit 0 |
| `check_docs_reality.sh` | solo el hallazgo **preexistente** de `ROADMAP_O3.md` (artefacto conocido de mi WSL, no tocado) |

## Impact

- **Test-only**: un único fichero modificado, y dentro de él solo el módulo `#[cfg(test)]`; **cero cambios en código de producción**, API, configuración o comportamiento del kernel. No se arrancó el kernel ni se tocó `E:\tylluan`.
- **CI**: elimina la causa del fallo intermitente de `fingerprint_gate_cost` en pasadas completas de la lib (1 de 10 reproducido hoy; el propio D-20 documentaba "una de cada tres" y el turno 1538 cita otra vez 179 ms vs 53 ms). Coste: el test pasa de 100 a 180 iteraciones (~3,3 s aislado), despreciable frente a los ~35-45 s de la lib.
- No cambia el recuento de tests ni el contenido de la suite; la puerta de huella de embeddings sigue teniendo un probe de coste en el camino de búsqueda.

## No verificado

- **Entorno de CI real**: las 5 pasadas son locales (Windows, misma máquina); no se ejecutó en el runner de CI ni en ARM64/macOS. En un host aún más cargado el p95 de las 3 rondas podría quedar por encima de 4× el p95 de la referencia; el test seguiría siendo más robusto que antes, pero no se puede garantizar que no falle nunca bajo contención extrema (sería una regresión real o un host saturado, y el mensaje imprime las 3 rondas para distinguirlo).
- **Enmascaramiento por contención compartida**: si la contención del `Mutex`/conexión SQLite domina **por igual** a puerta y referencia, el cociente p95-p95 podría tapar una regresión de la puerta que no se manifieste por encima de la referencia. Es el precio de medir en relativo; se mitiga con el margen 4× y el suelo de 2 ms, pero no se ha hecho un estudio de sensibilidad.
- **Percentil con 60 muestras/ronda**: el índice `ITERS*95/100` = 57 de 60 (aprox. p95); con más muestras el p95 sería más fino. Se eligió 3×60 en vez de 3×100 para no alargar el test; no se comparó estadísticamente la estabilidad de ambos tamaños.
- **No se reproduce bajo carga artificial**: la repro fue "lib completa en paralelo" real; no se usó un generador de carga externo ni se midió la distribución de preempciones.
- **No se ejecutó `verify.sh` completo** (Rust: test objetivo + 5 pasadas de lib + clippy).
