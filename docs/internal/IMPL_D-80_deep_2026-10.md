# IMPL D-80 — Texto invisible o codificado

Fecha: 2026-10-30
Base: `a06fa08`
Agente: Deep
Alcance: kernel + dashboard. Sin commit; diff en `E:\tylluan-wt-deep`.

## Resumen

Se cerraron los huecos de la familia *ASCII smuggling* en las superficies de entrada y salida de memoria/Coloquio:

- El normalizador de `security/poison_patterns.rs` ahora limpia ANSI y decodifica las etiquetas Unicode `U+E0000..U+E007F` antes de comparar patrones.
- `memory/write` y `tylluan_remember` ya usaban el matcher; ahora también detectan tags y ANSI.
- Los posts de Coloquio pasan por el mismo gate de capa 1 antes de guardarse.
- La salida de `tylluan_recall` se sanea antes de devolverse al cliente MCP.
- El render de Coloquio del dashboard sanea ANSI, bidi, ancho cero y secuencias de tags antes de interpretar Markdown.
- Las banderas legítimas de Inglaterra, Escocia y Gales se conservan.

## Tabla caso → entrada → dónde entra → resultado antes → arreglo

| Caso | Entrada | Dónde entra | Resultado en `a06fa08` (antes) | Arreglo mínimo | Verificación |
|---|---|---|---|---|---|
| Instrucción codificada con tags Unicode | `tag_encode("ignore all previous instructions")` | `handle_tylluan_remember`, `handler_remember.rs` línea 134 | **Aceptada**: `normalize` descartaba los tags sin decodificarlos, así que el patrón no casaba | Decodificar tags en `normalize` antes de comparar | `tag_characters_cannot_hide_an_instruction` |
| Instrucción codificada con tags Unicode | La misma entrada | `memory_write_handler`, `api_memory.rs` línea 26 | **Aceptada**: mismo matcher, mismo hueco | El mismo arreglo del normalizador | `memory_write_rejects_tag_encoded_instruction` |
| Instrucción codificada con tags Unicode | La misma entrada | `coloquio_post_message`, `api_coloquio.rs` línea 244 | **Aceptada**: Coloquio no tenía gate de capa 1 | Gate de capa 1 que devuelve 400 `injection_pattern_rejected` | `coloquio_rejects_tag_encoded_instruction` |
| Tags benignos en memoria ya guardada | `visible <tags: hidden-tag-payload> text` | Salida de `tylluan_recall`, `handler_recall.rs` línea 144 | **Cruda**: el filtro de coherencia eliminaba instrucciones completas, pero un payload benigno/oculto pasaba al cliente | `sanitize_display_text` sobre cada bloque de texto de la respuesta | `d80_recall_output_is_sanitized` |
| Tags benignos en mensaje de Coloquio | El mismo payload benigno | Render de `ColoquioMessagesPanel.tsx` vía `parseMarkdown`, `coloquio-markdown.ts` línea 20 | **Invisible en pantalla**: no había saneado y los tags no se veían | `sanitizeDisplayText` antes del render Markdown | `ColoquioMarkdown.test.ts`, casos D-80 |
| ANSI partiendo una palabra | `ig ESC[31m nore all previous instructions` | `tylluan_remember` / `memory/write` | **Aceptada**: ANSI no se limpiaba y la secuencia rompía el patrón | `strip_ansi` CSI/OSC dentro de `normalize` | `ansi_sequences_cannot_split_a_pattern` |
| ANSI en Coloquio | `ig ESC[31m nore...` | `coloquio_post_message` | **Aceptada**: no había gate | Mismo gate de capa 1 | `coloquio_rejects_tag_encoded_instruction` cubre tags; el gate usa el normalizador con ANSI |
| ANSI en dashboard | `ig ESC[31m nore` | `parseMarkdown` | **Se colaba al render** | `sanitizeDisplayText` elimina CSI/OSC | `strips ANSI CSI and bidi controls` |
| Bidi `U+202E` | `ig U+202E nore all previous instructions` | `tylluan_remember` / `memory/write` | **Rechazada ya**: `is_invisible` incluía `U+202A..U+202E` desde D-31 | Sin cambio de comportamiento; se añadieron isolates `U+2066..U+2069` | `bidi_and_zero_width_still_cannot_split_a_pattern` |
| Bidi `U+202E` | La misma entrada | Coloquio / dashboard | **Aceptada / invisible**: no había gate ni saneado de render | Gate de Coloquio + `INVISIBLE_FORMAT` en dashboard | `coloquio_rejects_tag_encoded_instruction`; test TS de bidi |
| Ancho cero partiendo una palabra | `ig U+200B nore...` | `tylluan_remember` / `memory/write` | **Rechazada ya** por D-31 | Sin cambio en el matcher; se conserva el test | `bidi_and_zero_width_still_cannot_split_a_pattern` |
| Ancho cero | La misma entrada | Coloquio / dashboard | **Aceptada / invisible**: sin gate y sin saneado | Gate + `INVISIBLE_FORMAT` | Tests de integración y TS |
| Banderas legítimas | `🏴` + tags `gbeng`, `gbsct` o `gbwls` + `U+E007F` | Matcher y render | **No marcadas**, pero sin regresión explícita | Allowlist de las tres subdivisiones; cualquier otro tag se descarta | Tests Rust y TS con las tres banderas |

## Cambios

- `crates/tylluan-kernel/src/security/poison_patterns.rs`
  - `strip_ansi`: CSI y OSC (BEL o `ESC \`), antes de normalizar.
  - `decode_tag_char`: decodifica `U+E0020..U+E007E` para detección.
  - `normalize`: limpia ANSI, decodifica tags y conserva el descarte de invisibles; añade isolates bidi `U+2066..U+2069`.
  - `sanitize_display_text`: limpieza de salida que conserva mayúsculas y las tres banderas.
  - Tests `d80_invisible_tests`.
- `crates/tylluan-kernel/src/transport/http/api_v1/api_coloquio.rs`
  - Gate de capa 1 en `coloquio_post_message`; rechazo 400 antes de escribir.
- `crates/tylluan-kernel/src/transport/server/handler_recall.rs`
  - Saneado de cada `RawContent::Text` en la respuesta de `tylluan_recall`.
  - Test `d80_recall_output_is_sanitized`.
- `crates/tylluan-kernel/tests/coloquio_invisible_text.rs`
  - Tests de integración de `memory/write` (403) y Coloquio (400).
- `dashboard/src/lib/coloquio-markdown.ts`
  - `sanitizeDisplayText` y uso desde `parseMarkdown`; ANSI, bidi, ancho cero y tags.
- `dashboard/src/components/__tests__/ColoquioMarkdown.test.ts`
  - Casos de tag oculto, ANSI, bidi y las tres banderas.

## Verificación ejecutada

Entorno: `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`, `-j 8`, `CARGO_TARGET_DIR=E:\cargo-target\shared-d80` (fuera del checkout).

| Comando | Resultado |
|---|---|
| `cargo test -p tylluan-kernel --lib` | **1450 passed, 0 failed, 4 ignored** |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | exit **0** |
| `cargo test -p tylluan-kernel --test coloquio_invisible_text` | **2 passed, 0 failed** |
| `pnpm test` (dashboard) | **19 archivos, 142 tests passed** |
| `pnpm run lint` (dashboard) | **0 errores**, 36 warnings preexistentes |
| `pnpm exec tsc --noEmit` (dashboard) | exit **0** tras construir `packages/tylluan-ui-core` |

Nota de dashboard: para que `tsc` resolviera `@tylluan/ui-core` se construyó el paquete compartido en local. No se versionó ningún artefacto; `packages/tylluan-ui-core/package-lock.json` se restauró a su estado de `HEAD`.

## Impact

- **Seguridad:** los tags Unicode y las secuencias ANSI dejan de ser una vía para ocultar instrucciones en `tylluan_remember`, `memory/write`, Coloquio y la salida de `tylluan_recall`; el render del dashboard deja de mostrar tags invisibles.
- **Compatibilidad:** se conservan las banderas legítimas de Inglaterra, Escocia y Gales. El texto normal no cambia; el texto con controles invisibles pierde esos caracteres en visualización/salida.
- **Datos:** no hay migración ni purga. El contenido ya almacenado se sanea al leerlo o renderizarlo, pero sigue en la base hasta que se borre por los cauces normales.
- **Configuración:** no se cambian defaults de seguridad ni `dev_mode`; no se añaden dependencias.
- **Superficie:** kernel y dashboard. No se toca el kernel en ejecución, el puerto `47004` ni `data/` de producción.

## No verificado

- No se ejecutó un kernel vivo ni peticiones HTTP reales contra `127.0.0.1`; la verificación es de tests.
- No se ejecutó el estado `a06fa08` en un checkout separado para reproducir “antes” en vivo: la columna “antes” se basa en lectura directa del código de `HEAD` y en el comportamiento de los tests.
- No hay test de integración directo de `handle_tylluan_remember`; se cubre el matcher compartido y los otros dos puntos de escritura. El handler ya llamaba al matcher en la línea 134.
- El componente `ColoquioMessagesPanel.tsx` no se montó en navegador; se probó la función `parseMarkdown` que usa para renderizar.
- Solo se probó la bandera de Inglaterra en integración; Escocia y Gales tienen allowlist y tests unitarios, no integración.
- No se verificó el render visual real ni el comportamiento de un cliente MCP/LLM concreto al recibir texto saneado.
- No se revisaron datos históricos de producción para estimar cuántas memorias contienen tags o ANSI.
