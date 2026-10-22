# T-DSH-37 — Colisiones de los patrones APLASTADOS de la capa 1 (post D-31)

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`be7d30b`, push 109: incluye D-31 con `imstart`/`imend` ya retirados por el TL y el fix del d28).
- **Encargo (turno 1560):** buscar la MISMA clase de falso positivo en el resto de patrones aplastados (`youarenow`, `ignoreallpreviousinstructions`, `sinrestricciones`, `sinfiltros`, `sincensura`…); nuevo `tests/layer1_squash_collisions.rs` que, para cada patrón de `SQUASHED_PATTERNS`, busque colisiones sobre un diccionario propio de ≥2000 entradas generadas de forma reproducible a partir de los `.rs`, `.py` y `.md` del repo (identificadores + bigramas) y liste cualquier colisión. Debe pasar hoy y fijar el máximo; informe con `## Impact` y no verificado; sin `cfg(unix)/cfg(windows)`; no tocar `src`; clippy exit 0.

## Qué hace el test

`crates/tylluan-kernel/tests/layer1_squash_collisions.rs` (nuevo; `src/` intacto):

1. Lee `SQUASHED_PATTERNS` e `INJECTION_PATTERNS` **del propio fuente** (`poison_patterns.rs`), sin duplicarlos.
2. Construye un diccionario reproducible con los 796 ficheros `.rs`/`.py`/`.md` rastreados (`git ls-files`; fallback a `crates/`, `guilds/`, `docs/` sin git):
   - **palabras** de 4–40 caracteres (identificadores y prosa, partidos en cualquier no-alfanumérico);
   - **bigramas** de palabras consecutivas de cada línea (frases técnicas/documentación).
   Resultado: **417.911 entradas** (mínimo exigido: 2.000).
3. Para cada patrón aplastado y cada entrada comprueba si `squash(entrada)` lo contiene.
4. Clasifica cada coincidencia:
   - **intencionada**: el patrón es exactamente la concatenación de **fragmentos enteros** de la entrada (p. ej. `you are now` → `youarenow`, o `ignore_previous_instructions` → `ignorepreviousinstructions`, que es justo la ofuscación por separadores que D-31 quiere cazar);
   - **colisión de aplastado**: el patrón se forma **dentro de una palabra o cruzando fragmentos** (la clase `trim_end` → `trimend` que producía el FP de `imend`).
5. Fija el máximo de colisiones en `MAX_SQUASH_ONLY_COLLISIONS = 0` (medido hoy) y falla si sube; también hace de sanity-check: toda colisión debe disparar `matches_injection_pattern` (si no, el modelo de atribución está roto).

## Medición (árbol `be7d30b`)

```text
ficheros: 796
diccionario: 417911 entradas
patrones aplastados: 19
coincidencias totales: 36 (intencionadas: 36, colisiones de aplastado: 0)
test squashed_patterns_do_not_collide_with_benign_vocabulary ... ok
```

**0 colisiones de la clase `imstart`/`imend`** en los 19 patrones aplastados restantes sobre el vocabulario técnico del repo. Las 36 coincidencias son todas intencionadas (frases del propio patrón o su forma con separadores).

Durante el desarrollo, la primera versión de la clasificación marcó dos candidatas (`def test_flags_ignore_previous_instructions` y su bigrama, en `tests/python/test_injection_guard.py`): el patrón `ignorepreviousinstructions` aparecía en un identificador con guiones bajos. Al refinar la alineación a **fragmentos alfanuméricos** (cualquier separador, incluido `_`), se clasifican como **intencionadas**: es la frase con separadores, exactamente la ofuscación por espaciado que D-31 decidió cazar, no la clase `trim_end`. (Si en el futuro se decide que un identificador Python que contiene la frase no debe marcar, es una decisión de producto sobre el corpus, no una colisión de vista aplastada.)

## Lectura

- Los patrones aplastados que quedan son **seguros frente a la clase de FP de D-31**: ninguna palabra técnica del repo los forma por fragmentos accidentales.
- **Fricción que el test NO marca como colisión y conviene tener presente**: los patrones-cláusula (`sin restricciones`, `sin filtros`, `sin censura`, `autoridad maxima`, `no sigue las politicas`, `ignora las reglas`…) sí marcan frases benignas que los contengan (p. ej. «un buscador sin filtros»). Hoy eso no aparece en prosa benigna del repo (las 3+1 apariciones son documentación que cita los patrones) ni en los 300 benignos del corpus ES (0 FPs medidos por Deep), pero es fricción de producto si ese texto entra en memoria; es una decisión del corpus/D-31, no una colisión de aplastado.
- Esta prueba es **complementaria** a la de T-DSH-36: allí se escanean párrafos reales del repo (detecta `trim_end` en contexto); aquí se escanea vocabulario (detecta cualquier palabra/frase técnica que, sola, colisione con un patrón). Con `imstart`/`imend` fuera, ambas dan 0 de esa clase.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test layer1_squash_collisions -- --nocapture` | **1 passed / 0 failed** (7,2 s); tablas impresas |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** el test nuevo + este informe; `src/` intacto |
| `check_test_count.sh` | exit 0 (1272; test de integración, no cuenta en lib) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función.

## Impact

- **Solo lectura sobre `src` + test nuevo**: cero cambios en código de producción, guilds, configuración o corpus; no se arrancó el kernel; no se tocó `E:\tylluan`. Coste del test: ~7 s, sin red.
- **Para D-31/patrones futuros**: deja ejecutable la lección que pidió el TL ("un patrón aplastado debe pasar por un barrido del repo antes de entrar"): cualquier patrón nuevo que colisione por fragmentos con vocabulario técnico hace fallar el test con el patrón y la entrada. Máximo fijado en **0**; subirlo exige motivo escrito.
- Cierra la vigilancia de la clase `imstart`/`imend` en el nivel de vocabulario, complementando el barrido de párrafos de T-DSH-36.

## No verificado

- **Diccionario limitado al vocabulario del repo**: no incluye diccionarios generales de español/inglés; una colisión con una palabra benigna que no aparezca en estos 796 ficheros no se detectaría. El barrido de párrafos de T-DSH-36 cubre el texto real, no el vocabulario ausente.
- **No se probaron patrones futuros**: el test los leerá del fuente, pero la clasificación (intencionada vs colisión) es una heurística de alineación por fragmentos; un patrón con estructura distinta (p. ej. con puntuación) podría necesitar ajuste.
- **Fricción de las cláusulas** (`sin filtros` en prosa benigna): no la medí con un corpus nuevo; me apoyo en el 0/300 de Deep y en que en el repo solo aparece en documentación que cita los patrones.
- **El caso `ignore_previous_instructions` como identificador**: lo clasifiqué como intencionado por diseño (ofuscación por separadores); no hay decisión del TL registrada sobre si un identificador así debería marcar en memoria.
- **CI**: medido en mi Windows con el árbol de `be7d30b`; el runner puede tener otro conjunto de ficheros (el test tolera git ausente con fallback).
