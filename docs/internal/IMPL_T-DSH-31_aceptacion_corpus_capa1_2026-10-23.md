# T-DSH-31 — Test de aceptación del corpus para la capa 1 (prerrequisito de D-31)

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`d1dec55`); limpio antes de empezar.
- **Encargo (turno 1545):** sin tocar `src/`, crear `tests/layer1_corpus_acceptance.rs` que cargue los CUATRO corpus (`benign_corpus_v1.json`, `benign_corpus_es_v1.json`, `injection_corpus_v1.json`, `injection_corpus_es_v1.json`) y fije:
  (a) **0 falsos positivos** de `matches_injection_pattern` sobre los dos benignos — debe pasar HOY y proteger a D-31;
  (b) un **suelo de detección por categoría** (inglés literal, español, y cada ofuscación: espaciado, ancho cero, homoglifo, ancho completo, puntuación) igual a lo medido HOY, con la línea base impresa en tabla; si una categoría hoy detecta 0, el suelo es 0 (nunca regresión).
  Sin `cfg(unix)/cfg(windows)` en la función entera; clippy exit 0; fichero nuevo; informe con `## Impact` y no verificado; diff sin commitear.

## Qué hace el test

Dos tests independientes en `crates/tylluan-kernel/tests/layer1_corpus_acceptance.rs` (ningún `cfg` de plataforma, solo Rust estándar + `serde_json` + `matches_injection_pattern`):

1. **`layer1_never_flags_benign_corpus`** — recorre los 40 benignos ingleses + 300 españoles (340) y falla si `matches_injection_pattern` marca **una sola** muestra (con id y texto en el mensaje). Se asegura de haber revisado ≥ 340.
2. **`layer1_detection_floors_by_category`** — recorre los 40 de inyección inglesa y los 80 españoles y agrupa por categoría:
   - `en_literal`: corpus inglés, excluyendo las 5 muestras que el propio corpus declara `cat == "espanol"`;
   - `es`: esas 5 declaradas español + las 60 variantes `directa` del corpus español;
   - ofuscaciones por campo `variante`: `es_espaciado` (5), `es_ancho_cero` (5), `es_homoglifo_cirilico` (4), `es_ancho_completo` (3), `es_puntuacion` (3).
   Imprime la tabla de línea base y compara contra `FLOORS` (constantes **medidas hoy**); falla si alguna categoría detecta **menos** que su suelo. Deliberadamente **no** hay techo: D-31 puede (y debe) mejorar la detección; lo que no puede es retroceder ni marcar benignos.

## Línea base medida HOY (2026-10-22, capa 1 = 10 patrones en inglés, sin normalización)

```text
categoria                detectadas    total    suelo  estado
en_literal                        8       35        8      OK
es                                0       65        0      OK
es_espaciado                      0        5        0      OK
es_ancho_cero                     0        5        0      OK
es_homoglifo_cirilico             0        4        0      OK
es_ancho_completo                 0        3        0      OK
es_puntuacion                     0        3        0      OK
capa 1: 0 falsos positivos en 340 muestras benignas (EN+ES)
```

Interpretación: la capa 1 actual detecta **8/35** inyecciones literales en inglés y **0** en todo lo español (base y las cinco ofuscaciones), exactamente lo esperado por sus 10 substrings en inglés sin normalización. Los suelos dejan ese estado fijado: cuando Deep entregue D-31, el delta por categoría será visible en esta misma tabla y cualquier regresión rompe el test.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test layer1_corpus_acceptance -- --nocapture` | **2 passed / 0 failed** (0,01 s); tabla impresa arriba |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** `?? crates/tylluan-kernel/tests/layer1_corpus_acceptance.rs` (fichero nuevo; `src/` intacto) |
| `check_test_count.sh` | exit 0 (1264 sin cambios: es un test de integración en `tests/`, no cuenta en la lib) |
| `check_doc_links.py` | exit 0 |
| `check_docs_reality.sh` | solo el hallazgo **preexistente** de `ROADMAP_O3.md` (artefacto conocido de mi WSL) |

## Impact

- **Solo test, fichero nuevo**: no se modifica `src/`, ni guilds, ni configuración, ni corpus; ningún cambio de comportamiento del kernel. No se arrancó el kernel ni se tocó `E:\tylluan`.
- **Para D-31**: deja un criterio de aceptación ejecutable en un solo comando. El TL ejecuta el test tras el cambio de Deep y ve (1) que ningún benigno se marcó y (2) exactamente qué categorías subieron respecto a la tabla. Los suelos son **conservadores** (mínimo observado hoy): no exigen una mejora concreta, solo prohíben la regresión; cuando D-31 mejore una categoría, se sube su suelo en este mismo fichero (el test seguirá vigilando que no vuelva a bajar).
- **Coste**: 0,01 s; sin red, sin BD, sin embeddings; corre en CI como cualquier test de integración.

## No verificado

- **Cifras de la línea base**: medidas con la capa 1 de `d1dec55` en esta máquina (una ejecución; el test es determinista —solo `matches_injection_pattern` sobre JSON fijo—, no depende de tiempo ni de aleatoriedad). No se comparó con otras plataformas, pero al no haber normalización ni locale-dependent code no debería variar.
- **Suelos por variante**: los puse a 0 porque hoy todas las variantes detectan 0. Si D-31 añade normalización para una variante concreta, el suelo correspondiente habrá que subirlo a lo medido **en ese momento**; el test no lo hace automáticamente (decisión deliberada: el suelo es un contrato revisado a mano).
- **`en_literal` = 8/35**: no he analizado cuáles de los 27 restantes contienen patrones que la capa 1 debería cazar (posible falso negativo del corpus inglés, no del test); tampoco si los 8 son detecciones correctas y no coincidencias accidentales. El test fija el estado, no lo juzga.
- **Categorías de los corpus**: el agrupado usa `cat`/`variante` tal como los declaran los JSON; si alguien añade una variante nueva, el test **falla con "variante desconocida"** a propósito (para que se decida a qué categoría va y su suelo).
- **D-31 en sí**: no se ejecutó ningún patrón nuevo en español (no existe todavía); este test es el prerrequisito, no la implementación.
