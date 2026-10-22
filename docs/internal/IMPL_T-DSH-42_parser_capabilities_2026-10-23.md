# T-DSH-42 — D-35(a): el parser de CAPABILITIES cierra los dicts multilínea

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`d56f8b8`, push 114: T-DSH-41 integrada); limpio antes de empezar.
- **Encargo (turno 1573):** implementar D-35 parte (a) tocando `src` solo en `crates/tylluan-kernel/src/router/catalog.rs`; arreglar `extract_capabilities` para que cierre bien los dicts `CAPABILITIES` multilínea; no cambiar `capabilities_enforce`, ni valores por defecto, ni `guild_process.rs`, ni `websearch.py`.

## Cambio en el parser (`router/catalog.rs`)

La causa raíz: al llegar a la línea con la llave de cierre (`}` o `},`), el parser calculaba `brace_depth == 0` y hacía `break` **antes** de añadir esa línea al buffer. El buffer quedaba truncado (`{ ..., ` sin cerrar), `serde_json` fallaba y `extract_capabilities` devolvía `None` — que es lo que veía el catálogo para `vision.py` y `websearch.py`.

Reescritura (misma firma y mismo rol; solo se añaden helpers privados):

- `push_capabilities_block(code, out, depth) -> bool`: acumula **caracteres** del bloque contando `{`/`}` **fuera de cadenas** (comillas simples y dobles, con escapes) e **incluye la llave de cierre**; devuelve `true` cuando el bloque se cierra.
- `strip_python_comment(line)`: quita un comentario `#` solo fuera de cadenas (antes, un `#` con llaves dentro desincronizaba el conteo).
- `starts_with_triple_quote(line)`: sigue saltando líneas de docstring triple (comparación en bytes, sin incrustar secuencias de comillas en el fuente).
- Normalización a JSON: `True`/`False`/`None` → `true`/`false`/`null` (igual que antes) **más** eliminación de la coma final de Python antes de `}` (`.replace(",}", "}")` / `", }"`), que si no deja el JSON inválido en cualquier dict multilínea.

Se conserva el comportamiento previo: dicts de una línea funcionan igual; `CAPABILITIES = {}` sigue devolviendo `None`; sin declaración, `None`. **No se tocó** `capabilities_enforce`, `guild_process.rs`, `websearch.py`, ni ningún valor por defecto.

## Test de regresión unitario (dentro de `catalog.rs`)

`extract_capabilities_handles_multiline_singleline_and_comments` (módulo `tests` del propio fichero, sin `cfg(unix)`/`cfg(windows)`):

- dict de **una línea** con `True` → parsea y `process_execution = true`;
- dict **multilínea** con lista y array vacío → parsea los cuatro campos;
- dict multilínea con **comentario dentro** que contiene `{ }` y comentario al final de línea → los ignora y parsea;
- `OTRA_COSA = 1` y `CAPABILITIES = {}` → `None`.

## Cambios en los tests existentes (y por qué)

1. `tests/cola_d35_acceptance.rs`
   - `d35_desired_multiline_capabilities_are_parsed`: **se le quitó `#[ignore = "D-35"]`** y **pasa** (criterio 1). Sus aserciones no cambiaron.
   - `d35_current_catalog_returns_none_for_multiline_capabilities` → renombrado **`d35_catalog_now_parses_multiline_capabilities`**: se **invirtió** (ya no devuelve `None`). Ahora fija el comportamiento nuevo como pin de regresión: si el catálogo vuelve a `None` para vision/websearch, el test falla con "regresión de D-35".
   - `d35_desired_websearch_command_fallback_is_not_blocked`: **se queda ignorado** (criterio 3). Con el parser arreglado ahora falla por la razón concreta: `enforce_capabilities("websearch", caps, web_search(command=...))` → `CAPABILITY_BLOCKED ... process_execution=false` (el falso bloqueo de T-DSH-32, ya medible). Evidencia abajo.
2. `tests/capabilities_enforce_matrix.rs`
   - `catalog_capability_parser_gap` → renombrado **`catalog_capabilities_are_parsed_after_d35`**: ya no espera `["bash"]` sino el conjunto `{bash, vision, websearch}` (comparado ordenado) y, para las tres, exige que las capacidades parseadas **sean iguales a las declaradas en su `.py`** (antes solo se comprobaba bash). Se eliminó el helper `expected_caps` (quedó sin uso) y el import `json`.
   - **Nuevo** `strict_profile_does_not_block_legit_vision_and_documents_websearch_fallback` (criterio 6): con las capacidades ya parseadas, recorre los casos legítimos de la matriz para `vision` y `websearch`, aplica el override de Strict (`process_execution=false`, emulado porque `check_capabilities` es privada) y comprueba que **solo** queda bloqueado `websearch.web_search`. Es decir: Strict no bloquea nada legítimo de `vision`; en `websearch` el único bloqueo legítimo es el fallback `command`, que es el falso bloqueo conocido y se documenta aquí (no se arregla en D-35a).
   - `capabilities_enforce_matrix_fixes_current_behaviour` (la matriz de T-DSH-32) **no se tocó** y sigue verde: sigue midiendo con las CAPABILITIES declaradas, independientes del catálogo.

## Salidas

```text
$ cargo test -p tylluan-kernel --lib
test result: ok. 1186 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 31.60s

$ cargo test -p tylluan-kernel --test capabilities_enforce_matrix
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
catalogo: guilds con capabilities declaradas y PARSEADAS = ["bash", "vision", "websearch"]
Strict: llamadas legítimas bloqueadas = ["websearch.web_search"]

$ cargo test -p tylluan-kernel --test cola_d35_acceptance
test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
test d35_catalog_now_parses_multiline_capabilities ... ok
test d35_desired_multiline_capabilities_are_parsed ... ok
test d35_desired_websearch_command_fallback_is_not_blocked ... ignored, D-35

$ cargo test -p tylluan-kernel --test cola_d35_acceptance -- --ignored
test d35_desired_websearch_command_fallback_is_not_blocked ... FAILED
D-35 pendiente (falso bloqueo de T-DSH-32): el uso legítimo web_search(command="rust async")
se bloquea porque process_execution=false + argumento 'command'; en websearch.py 'command'
es fallback de query, no shell. motivo: Some("CAPABILITY_BLOCKED: guild 'websearch'
declares process_execution=false. This guild may not execute system commands.")

$ cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings
EXIT 0
```

Cifras y puertas: `check_test_count.sh` **exit 0** (1273 = 1186 kernel lib + 75 link + 12 fsrs; el test unitario nuevo sube kernel lib de 1185 a 1186, así que el gate mecánico exigió sincronizar la línea canónica — ver abajo), `check_doc_links.py` **exit 0**, `check_docs_reality.sh` solo el hallazgo preexistente `ROADMAP_O3.md:42` (artefacto conocido de mi checkout WSL, no de este cambio).

**Sincronización de cifras (mecánica, no funcional):** al añadir un test unitario, `scripts/check_test_count.sh --fix` actualizó `AGENTS.md` y `README.md` a 1273; `STATUS.md` (línea 50, "1272 total", y línea 10) se actualizó a mano a 1273/1186. El diff incluye esos 3 ficheros solo por esa cifra; ninguna otra línea de docs cambia.

## Dos opciones para el falso bloqueo de `websearch.web_search(command=...)` (D-35b, no implementadas)

1. **Quitar el heurístico "argumento `command` = ejecución" y quedarse solo con el nombre de herramienta exec-like.** `enforce_capabilities` ya bloquea `run_shell`/equivalentes por nombre; el heurístico del argumento `command` es el que produce el falso bloqueo en `web_search`. Coste: un `run_shell(command=...)` seguiría bloqueado por nombre; un ejecutor futuro con nombre inocente y arg `command` dejaría de bloquearse (riesgo que se aceptaría explícitamente). Ventaja: no toca `websearch.py` ni la firma pública; una línea menos de heurística.
2. **Mantener el heurístico pero con allowlist de herramientas no-ejecutoras (o renombrar el fallback).** Añadir a `websearch` (y solo a herramientas declaradas sin ejecución) una excepción para el arg `command`, o renombrar el parámetro del fallback de `websearch.py` a `query` y aceptar `command` como alias deprecado sin pasarlo a `enforce_capabilities`. Coste: permite ser explícito y conserva el bloqueo para el resto; requiere tocar el `.py` (hoy prohibido en este encargo) o una lista mantenida a mano.

Mi recomendación técnica (sin implementar): opción 1 si el objetivo es cerrar el falso bloqueo con mínima superficie; opción 2 si se prefiere conservar el heurístico para ejecutores nuevos y se acepta tocar `websearch.py` más adelante.

## Verificación

| criterio | resultado |
|---|---|
| (1) `d35_desired_multiline_capabilities_are_parsed` sin `#[ignore]` y pasa | **ok** |
| (2) invertido `d35_current_...` y actualizado `catalog_capability_parser_gap` | **ok** (explicado arriba) |
| (3) `d35_desired_websearch_command_fallback_is_not_blocked` sigue ignorado; 2 opciones en el informe | **ok** |
| (4) `cargo test -p tylluan-kernel --lib` entero + clippy `-D warnings` | **1186 passed / 4 ignored**; clippy **exit 0** |
| (5) test de regresión unitario del parser (multilínea, una línea, comentario) sin `cfg` | **ok** |
| (6) comprobación con perfil Strict y documentación | **ok** (test nuevo; solo `websearch.web_search` bloqueado) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función.

## Impact

- **Ruta afectada:** descubrimiento de guilds en el catálogo (`router/catalog.rs::extract_capabilities`, ahora también multilínea). Efecto funcional: `vision` y `websearch` pasan de "sin CAPABILITIES" a tenerlas parseadas; con `capabilities_enforce` (Balanced) activo, sus capacidades **por fin se aplican** — y con perfil **Strict** se aplica además el override `process_execution=false`. Esto es justo lo que D-35 quería, pero cambia el comportamiento observable de esas dos guilds cuando la contención está encendida: la matriz de T-DSH-32 lo deja medido y el test nuevo lo documenta (un falso bloqueo legítimo en `websearch`, pendiente de decisión D-35b).
- **No cambia** valores por defecto, ni `capabilities_enforce`, ni `guild_process.rs`, ni `websearch.py`, ni la lógica de ejecución; el parser solo devuelve `Some` donde antes devolvía `None`.
- **Riesgo declarado:** cualquier guild con declaración multilínea ahora expone capacidades que antes no se aplicaban; la matriz dice que solo vision/websearch declaran hoy. Si alguna declaración tuviera un error de sintaxis, el catálogo la expondría como `None` igual que antes (no rompe el arranque).
- **Docs:** 3 ficheros tocados solo por la cifra de tests (1273), requerido por `check_test_count.sh`.

## No verificado

- **En vivo** (kernel real, `capabilities_enforce` encendido, perfil Balanced/Strict): no arranco el kernel; la comprobación es por `builtin_catalog()` + `enforce_capabilities` en tests, no contra el endpoint `/health` ni MCP real.
- **`check_capabilities` real:** es privada; el test de Strict **emula** su override (`process_execution=false`), no invoca la función. El camino completo (config → perfil → check) no se ejercitó.
- **Declaraciones con comillas simples:** el parser sigue asumiendo JSON-compatible (comillas dobles) para los valores; Python aceptaría `'texto'`, que produciría JSON inválido → `None`. `vision.py`/`websearch.py`/`bash.py` usan dobles; no se probó con otros `.py` que usen simples.
- **Docstrings con `CAPABILITIES = {` dentro:** se siguen saltando solo las líneas que empiezan con comilla triple; una declaración de ejemplo en un docstring multilínea podría confundir al parser (comportamiento preexistente, no cubierto por test). No encontré ese caso en `guilds/`.
- **D-35(b)** (visibilidad del falso bloqueo / contador) no se implementa aquí; el falso bloqueo queda medido y documentado, no arreglado.
- **CI**: medido en mi Windows; el recuento de docs (1273) y la sincronización AGENTS/README/STATUS quedan en el diff para que el TL los valide al integrar (si prefiere otra cifra canónica, es un cambio de una línea).
