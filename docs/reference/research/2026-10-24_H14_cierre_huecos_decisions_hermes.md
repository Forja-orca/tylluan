# H-14 — Cierre de huecos decisions: campo refusal + error /v1/systemone + LM Studio no-decision (hermes)

Asignación: TL claude-code T1612. Fuentes primarias abiertas ESTA sesión: `openai/openai-openapi@master/openapi.yaml`, código fuente `ggml-org/llama.cpp@master/tools/server/{server.cpp,server-common.cpp,server-decision.cpp}`, páginas oficiales LM Studio (URLs abajo). Cita «…» Ctrl+F-able en la URL · [I]=inferencia · sin requests a motores locales.

## (1a) Campo `refusal` en respuestas /v1/decisions (OpenAI) — CERRADO, está en la spec
| # | Cita literal (crudo YAML) | Fuente (línea) | Estado |
|---|---|---|---|
| 1 | «Any question can return a refusal» | raw.githubusercontent.com/openai/openai-openapi/master/openapi.yaml (líneas 91883) (DecisionResponse.answers) | CITADA |
| 2 | «instead of an answer; other questions can still receive answers. A» | openapi.yaml (líneas 91884) | CITADA |
| 3 | «refusal has `type: \"refusal\"` and the question's `name`, or `null`» | openapi.yaml (líneas 91885) (escape `\"` literal en la spec) | CITADA |
| 4 | «A text string or an ordered list of output text and refusal parts.» | openapi.yaml (DecisionInputAssistantContentParts/DecisionInput: refusal también como parte de ENTRADA assistant) | CITADA |
| 5 | «DecisionInputRefusal:» (schema con campos `type` enum [refusal] y string `refusal`, required ambos) | openapi.yaml (líneas 91050) | CITADA |

## (1b) Cuerpo de error exacto de /v1/systemone en llama.cpp — CERRADO desde código fuente
| # | Cita literal | Fuente (línea) | Estado |
|---|---|---|---|
| 6 | «// treat other exceptions as server error (500)» | raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/server.cpp:71 (ex_wrapper) | CITADA |
| 7 | «json error_data = format_error_response(message, error);» | server.cpp:82 | CITADA |
| 8 | «res->data = safe_json_to_str({{ "error", error_data }});» | server.cpp:84 | CITADA |
| 9 | «type_str = "invalid_request_error";» y «type_str = "server_error";» | tools/server/server-common.cpp:96,108 (format_error_response; code 400 y 500) | CITADAS |
| 10 | «the model could not evaluate the decision» | tools/server/server-decision.cpp (líneas 1518) (throw std::runtime_error) | CITADA |
| 11 | «decision model has no \"systemone\" template» | server-decision.cpp:51 (comillas internas escapadas en el literal C++) | CITADA |
- [I] Forma exacta del cuerpo (ensamblada de 7+8+9, no es una cadena única en la fuente): `{"error": {"code": 500, "message": "<what() del throw>", "type": "server_error"}}`; los `std::runtime_error` de server-decision.cpp caen en la rama 500 del ex_wrapper (comentado en 6); `std::invalid_argument`/errores JSON → 400 invalid_request_error. La ruta /v1/systemone está envuelta por ex_wrapper: «ctx_http.post("/v1/systemone",» (server.cpp:308, CITADA como substring contiguo hasta la coma; los espacios de alineación posteriores se omiten).

## (2) LM Studio ante un modelo no-decision en /v1/decisions — texto literal de error: NO ENCONTRADO
- [no encontrado] El texto literal del error NO aparece en ninguna fuente primaria oficial: `grep -i error` sobre https://lmstudio.ai/docs-md/developer/openai-compat/decisions (25 líneas) y sobre /docs-md/developer/openai-compat = 0 coincidencias. Probar el runtime está prohibido esta noche por el TL (regla: ningún request a motores locales), así que no se capturó la respuesta real.
| # | Cita literal (página oficial correcta) | Fuente | Estado |
|---|---|---|---|
| 12 | «Requires a loaded decision model, not a regular chat model.» | lmstudio.ai/docs/developer/openai-compat/decisions (fuente markdown: lmstudio.ai/docs-md/…/decisions) | CITADA |
| 13 | «Use the identifier of a loaded decision model, not a regular chat model.» | lmstudio.ai/docs/developer/jev-compat/systemone | CITADA |
- [I] Las dos páginas oficiales fijan el REQUISITO pero documentan el error como cuerpo; para el ADR-020: tratar el fallo como error de backend no estandarizado y no depender de su mensaje.

## Verificación (re-ejecutable, corrida esta sesión: 16/16 OK)
`grep -F` de cada «cita» sobre las fuentes crudas guardadas (HTML normalizado con `sed 's/<[^>]*>//g' | tr -s '[:space:]' ' '`; YAML/C++ crudos; rutas: openapi.yaml (líneas 91883-91885),91050; server.cpp:71,82,84,308; server-common.cpp:96,111; server-decision.cpp:51,1518). Negative: `grep -ci error lm_dec.md lm_ocmd.md` = 0. Doc SIN commitear en tylluan-wt-hermes (regla 1).

## Verificacion del TL (2026-10-24)
Descargue `server.cpp`, `server-common.cpp` y `server-decision.cpp` de llama.cpp (master) y `openapi.yaml`, y busque con `grep -cF`: coinciden `treat other exceptions as server error (500)`, `format_error_response(message, error)`, `ctx_http.post("/v1/systemone",`, `invalid_request_error`, `the model could not evaluate the decision` y `has no \"systemone\" template` (en el fuente C++ la linea 51 es `decision model has no \"systemone\" template`); `Any question can return a refusal` y `DecisionInputRefusal:` estan en las lineas 91883 y 91050 del YAML. La pagina de LM Studio (713 bytes) contiene `Requires a ` y `loaded decision model` y `, not a regular chat model.` (con un enlace markdown en medio) y 0 apariciones de `error`, como dice Hermes. La forma del cuerpo de error es ensamblada por Hermes (inferencia).
