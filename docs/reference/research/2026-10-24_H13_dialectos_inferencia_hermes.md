# H-13 — Dialectos de inferencia para un cliente único configurable por URL (hermes)

Asignación: TL claude-code T1604. Fuentes primarias descargadas ESTA sesión a scratch: spec oficial `openai/openai-openapi@master/openapi.yaml` (4,1 MB), guía `developers.openai.com/api/docs/guides/decisions`, docs LM Studio `/docs-md/developer/*` (markdown crudo), `ggml-org/llama.cpp@master/tools/server/README.md`, `docs.litellm.ai/docs/providers/openai`. Cita literal Ctrl+F-able · [I]=inferencia.

| # | Pregunta | Cita literal | Fuente (local guardada) | Estado |
|---|---|---|---|---|
| 1 | chat/completions (OpenAI) | **Starting a new project?** We recommend trying [Responses] | openapi.yaml (líneas 2494) (desc. createChatCompletion) | CITADA, grep -F OK |
| 2 | embeddings (OpenAI) | Creates an embedding vector representing the input text. | openapi.yaml (líneas 5186) | CITADA, grep -F OK |
| 3 | models (OpenAI) | Lists the currently available models, and provides basic information about each one such as the owner and availability. | openapi.yaml (líneas 10473-10474) | CITADA, grep -F OK |
| 4 | errors (OpenAI) | "type": "invalid_request_error" | openapi.yaml (6 ejemplos del esquema Error) | CITADA, grep -F OK |
| 5 | decisions: propósito | Use this endpoint to quickly answer classification or scoring questions about some input. You can ask multiple questions in one request. | openapi.yaml (líneas 32580-32582) (POST /decisions) | CITADA, grep -F OK |
| 6 | decisions: modelo | The model to use. Only `gpt-6-luna` is supported. | openapi.yaml (líneas 91576) (DecisionRequest.model) | CITADA, grep -F OK |
| 7 | decisions: tipos nativos | Ask 1–400 questions about the input. Each question can be a predicate, choice, or score. Answers come back in the same order. | openapi.yaml (líneas 91583-91585) | CITADA, grep -F OK |
| 8 | decisions: estado | Currently, only gpt-6-luna is supported. | oa_decisions_guide (guía oficial) | CITADA, grep -F OK |
| 9 | LM Studio: auth | By default, LM Studio does not require authentication for API requests. | lm_authentication (0.4.0+, tokens opcionales) | CITADA, grep -F OK |
| 10 | LM Studio: chat | Prompt template is applied automatically for chat‑tuned models | lm_chat-completions | CITADA, grep -F OK |
| 11 | LM Studio: models | Returns the models visible to the server. The list may include all downloaded models when Just‑In‑Time loading is enabled. | lm_models | CITADA, grep -F OK |
| 12 | LM Studio: decisions | `Requires a ` + `loaded decision model` + `, not a regular chat model.` (en la fuente `loaded decision model` es un enlace markdown a la pagina de carga) | lm_decisions | CITADA, grep -F OK |
| 13 | llama.cpp: chat | ### POST `/v1/chat/completions`: OpenAI-compatible Chat Completions API | llama_server_readme (líneas 1434) | CITADA, grep -F OK |
| 14 | llama.cpp: embeddings | This endpoint is **not** OAI-compatible. For OAI-compatible client, use `/v1/embeddings` instead. | llama_server_readme:753 (sobre /embedding) | CITADA, grep -F OK |
| 15 | llama.cpp: embeddings norm. | The embeddings are normalized using the Eucledian norm. | llama_server_readme (líneas 1653) (sic "Eucledian" en la fuente) | CITADA, grep -F OK |
| 16 | llama.cpp: models | The returned list always has one single element. | llama_server_readme (líneas 1346) | CITADA, grep -F OK |
| 17 | llama.cpp: auth | \| `--api-key KEY` \| API key to use for authentication, multiple keys can be provided as a comma-separated list (default: none) | llama_server_readme:215 | CITADA, grep -F OK |
| 18 | llama.cpp: cabecera auth | -H "Authorization: Bearer no-key" \\ (ejemplo OAI literal cuando no hay `--api-key`) | llama_server_readme (líneas 1640) | CITADA, grep -F OK |
| 19 | llama.cpp: dialecto decisions | ### POST `/v1/systemone`: TypeSafe-compatible System One API — `type`: One of `choice`, `score`, `noul`. | llama_server_readme (líneas 1820),1841 | CITADA, grep -F OK |
| 20 | LiteLLM | LiteLLM supports OpenAI Chat + Embedding calls. + a custom api_base pointing at another OpenAI-compatible server keeps using the key. | litellm_openai (txt normalizado) | CITADA, grep -F OK |

## Veredicto para la enmienda ADR-020 [I]
- **chat/embeddings/models son el eje estable** (los 3 lo implementan); las trampas están en los quirks: llama.cpp lista 1 solo modelo, normaliza embeddings L2, y tiene un /embedding obsoleto NO compatible; LM Studio puede listar todos los descargados (JIT).
- **Hay DOS dialectos decisions en el mercado**: nativo OpenAI (`input`, array de questions con `name`, tipo **predicate**, gpt-6-luna) y pasarela TypeSafe/systemone (`state`, objeto keyed, tipo **noul**). Un cliente por URL no puede asumir uno solo; el `noul` del spec del TL es el dialecto de pasarela, NO el nativo.
- Ruta local relevante: llama.cpp sirve el dialecto systemone y la README lista modelos decision **lfm2-d1/lfm2-d1-omni** (la familia del candidato d1 de H-11) — juez local sin cloud vía el mismo cliente.
- Auth: enviar `Authorization: Bearer` SIEMPRE pero servidor puede ignorarlo (LM Studio off por defecto, llama.cpp default none); OpenAI lo exige.
- Timeouts: **no encontrado** en las 5 fuentes primarias leídas; manejo de timeout es decisión del cliente [I]. El campo `refusal` en respuestas decisions aparece solo en fuentes secundarias (no lo vi en el spec ni la guía) [needs-verify].

## Gate de citas
grep -F (whitespace normalizado) de las 20 citas sobre las fuentes guardadas en scratch: ver salida de la corrida. Sin commitear (regla 1).

## Verificacion del TL (2026-10-24)
Descargue yo mismo `llama.cpp@master/tools/server/README.md` y `openai-openapi@master/openapi.yaml` y busque con `grep -cF`: coinciden las citas de `one single element`, `--api-key ... comma-separated` (fragmento), `TypeSafe-compatible System One API`, `Eucledian norm`, `gpt-6-luna is supported`, `Ask 1–400 questions` y `Creates an embedding vector`; la cita 5 esta partida en dos lineas del YAML (32581-32582) y coincide al normalizar espacios. No pude contrastar la cita 12 de LM Studio (la URL que probe devolvio 404), queda sin verificar por mi. El resto de la tabla y el veredicto son inferencias de Hermes.
