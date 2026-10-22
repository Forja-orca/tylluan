# T-DSH-41 — Aceptación ejecutable de D-35 (`extract_capabilities` multilínea)

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`f34874a`, push 113: T-DSH-40 integrada); limpio antes de empezar.
- **Encargo (turno 1571):** un test nuevo, sin tocar `src`; aceptación ejecutable de D-35. (a) `#[ignore = "D-35"]` que parsee con la función real `extract_capabilities` (`router/catalog.rs`; si no es pública, replicar fielmente su lógica y decirlo) los `CAPABILITIES` multilínea de `guilds/core/vision.py` y `guilds/core/websearch.py` y afirme `Some` con `process_execution=false`; (b) test vivo que fije que hoy devuelve `None` para ambos; (c) `#[ignore = "D-35"]` que, con las capacidades ya parseadas, afirme que `websearch.web_search(command="rust async")` NO se bloquea (falso bloqueo de T-DSH-32). Criterio: pasa; `--ignored` fallan (a) y (c) por la razón correcta; clippy 0; sin `cfg`; informe con `## Impact` y no verificado.

## Qué contiene el fichero

`crates/tylluan-kernel/tests/cola_d35_acceptance.rs` (nuevo; `src/` intacto).

`extract_capabilities` es **privada** (`fn`, no `pub`) así que un test de integración no puede llamarla directamente. **No se replicó la lógica**: se usa la vía pública equivalente, `router::catalog::builtin_catalog()`, que invoca la función real al descubrir los `.py` y expone el resultado en `GuildDescriptor::capabilities`. Es decir, el test mide **el mismo valor que ve el kernel** (el catálogo), no una copia del parser.

| test | tipo | qué afirma |
|---|---|---|
| `d35_current_catalog_returns_none_for_multiline_capabilities` | **vivo** | hoy `builtin_catalog()` da `capabilities = None` para `vision` y `websearch` (el defecto); Deep debe invertirlo al cerrar D-35 |
| `d35_desired_multiline_capabilities_are_parsed` | **`#[ignore = "D-35"]`** | tras el arreglo, ambas devuelven `Some` con `process_execution = false` |
| `d35_desired_websearch_command_fallback_is_not_blocked` | **`#[ignore = "D-35"]`** | con las capacidades ya parseadas, `enforce_capabilities("websearch", caps, web_search(command="rust async"))` debe ser `None` (no bloquea): en `websearch.py` (línea 44) `command` es el tercer fallback **legítimo** de la query, no shell |

## Salidas

Corrida normal (vivo; los deseados quedan fuera):

```text
test d35_desired_multiline_capabilities_are_parsed ... ignored, D-35
test d35_desired_websearch_command_fallback_is_not_blocked ... ignored, D-35
test d35_current_catalog_returns_none_for_multiline_capabilities ... ok

test result: ok. 1 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.07s
```

Corrida `-- --ignored` (los dos deseados fallan **por la razón correcta**):

```text
running 2 tests
test d35_desired_websearch_command_fallback_is_not_blocked ... FAILED
test d35_desired_multiline_capabilities_are_parsed ... FAILED

thread 'd35_desired_multiline_capabilities_are_parsed' panicked:
D-35 pendiente: el catálogo sigue dando None para 'vision';
extract_capabilities pierde el '}' final de la declaración multilínea

thread 'd35_desired_websearch_command_fallback_is_not_blocked' panicked:
D-35 pendiente: sin CAPABILITIES parseadas no se puede medir el falso bloqueo
de web_search(command=...); el catálogo sigue dando None

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.06s
```

Nota sobre (c): hoy falla en el primer paso (catálogo `None`), que es la razón correcta porque el falso bloqueo solo es medible con las capacidades ya parseadas. **Después de arreglar solo el parser**, (c) seguirá fallando si `enforce_capabilities` sigue tratando `command` como ejecución en una guild `process_execution=false`: el arreglo de D-35 debe incluir que ese uso legítimo de `websearch` no se bloquee (renombrar/desexcluir el fallback `command`, o acotar la heurística). Si D-35 se limita al parser, (c) será el siguiente defecto a decidir.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test cola_d35_acceptance` | **1 passed / 0 failed / 2 ignored** (0,07 s) |
| `cargo test --test cola_d35_acceptance -- --ignored` | **2 failed**, ambos por la razón correcta (salida pegada) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** el test nuevo; `src/` intacto |
| `check_test_count.sh` | exit 0 (1272; test de integración) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función.

## Impact

- **Solo test, fichero nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no arranca el kernel ni toca puertos; coste ~0,07 s (sin BD, sin red, sin procesos).
- **Para D-35**: deja la aceptación en un comando con las dos capas del defecto separadas: (a) el parser debe devolver `Some` para las declaraciones multilínea, y (c) la contención, una vez encendida, no puede bloquear el fallback legítimo de `websearch`. El test vivo fija el estado actual para que Deep lo invierta.

## No verificado

- **Con D-35 implementado**: no existe todavía; solo se comprobó que hoy (a) y (c) fallan por la razón correcta.
- **`extract_capabilities` real**: es privada; se mide a través de `builtin_catalog()` (misma función, mismo resultado que ve el kernel). No se replicó el parser ni se probó la función aislada con texto sintético.
- **Alcance del arreglo de (c)**: el test asume que D-35 incluirá que `web_search(command=...)` no se bloquee; si la decisión del TL/Deep es que D-35 solo arregla el parser y el falso bloqueo se acepta/difiere, (c) habrá que reclasificarlo.
- **Contenido de las CAPABILITIES**: el test solo exige `process_execution=false` (lo mínimo que pide el encargo); no aserta el resto de campos (`filesystem_scope`, `network_hosts`) porque un arreglo del parser podría normalizarlos.
- **CI**: medido en mi Windows; el catálogo depende de encontrar `guilds/` desde el árbol (usa `CARGO_MANIFEST_DIR/../..` como fallback de compilación; en un runner con otro layout el test lo diría con "guild ausente del catálogo").
