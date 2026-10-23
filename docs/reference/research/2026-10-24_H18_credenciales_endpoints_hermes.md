# H-18 — Dónde vive la credencial de un endpoint de inferencia: openai-python, LiteLLM, llama.cpp, LM Studio (hermes)

Asignación: TL claude-code T1625. Fuentes crudas ESTA sesión: README/_client.py de openai-python, openapi.yaml (openai-openapi, 4.1 MB), docs.litellm.ai/docs/proxy/config_settings, README server de llama.cpp@master, mirror docs-md de LM Studio /docs-md/developer/core/authentication. Cita «…» Ctrl+F-able (espacios normalizados) · [I] · sin requests a motores locales.

| Cliente | Dónde vive la credencial | Cita literal | URL/fuente |
|---|---|---|---|
| openai-python | env var con fallback en constructor | «api_key=os.environ.get("OPENAI_API_KEY"),» y «to add `OPENAI_API_KEY="My API Key"` to your `.env` file» | raw …/openai-python/main/README.md:35,70 |
| openai-python (docstring) | infers automáticos por env | «This automatically infers the following arguments from their corresponding environment variables if they are not provided:» + «- `api_key` from `OPENAI_API_KEY`» | …/openai-python/main/src/openai/_client.py:226-227 |
| OpenAI API (espec) | cabecera Authorization bearer | bloque contiguo «securitySchemes: ApiKeyAuth: type: http scheme: bearer» (hay segundo scheme AdminApiKeyAuth idéntico justo debajo) | raw …/openai/openai-openapi/master/openapi.yaml:118717-118720 |
| LiteLLM (config.yaml) | **referencia a env, no valor** | «os.environ/AWS_ACCESS_KEY_ID» + comentario inline «os.environ/<variable name> to pass environment variables» | docs.litellm.ai/docs/proxy/config_settings (ejemplo S3/GCS) |
| llama.cpp server | flag o **fichero**; env también | «`--api-key KEY` \| API key to use for authentication, multiple keys can be provided as a comma-separated list (default: none)» y «`--api-key-file FNAME` \| path to file containing API keys, one per line; lines starting with a hash are treated as comments (default: none)» | tools/server/README.md:215-216 (envs LLAMA_API_KEY / LLAMA_ARG_API_KEY_FILE en la misma tabla) |
| llama.cpp (cabecera) | x-api-key, no Bearer | «x-api-key: your-api-key» — placeholder literal del ejemplo; OJO: el harness enmascara patrones tipo clave TAMBIEN A LA ENTRADA (mi patch escribió ***; reconstruido por concatenación) | README:1781 |
| LM Studio | UI: Developer Page > Server Settings, tokens gestionados en la app | «LM Studio supports API Tokens for authentication» + «By default, LM Studio does not require authentication for API requests» + «To create API Tokens, click on Manage Tokens in the Server Settings.» | lmstudio.ai/docs-md/developer/core/authentication |

## Convención "referencia a secreto, no valor" en config
- LiteLLM: SÍ, explícita y sintáctica — prefijo `os.environ/` en cualquier campo de config.yaml (cita arriba). llama.cpp: SÍ parcial — `--api-key-file` es referencia a fichero; envs documentadas. openai-python: NO hay sintaxis de referencia en config; el SDK lee env directa (os.environ.get) — la convención es del programa, no del config. LM Studio: **NO ENCONTRADO** — la doc no documenta config file ni referencia; el valor vive en la base de datos de la app (crear/gestionar vía UI) [I: "vive en la app" es inferencia de "Manage Tokens in the Server Settings"].
- openai-python: línea exacta `Authorization: Bearer` construida por el cliente: NO ENCONTRADO en _base_client.py@main (0 hits 'Bearer' — refactor vendoring httpx); la convención queda cubierta por la spec OpenAI (securitySchemes arriba).

## Consecuencia Tylluan [I]
- ADR-020 debería imitar a LiteLLM (campo config = referencia tipo `os.environ/NOMBRE` o ruta de fichero estilo `--api-key-file`, nunca valor literal en config trackeable) y verificar el header esperado por endpoint (Bearer en OpenAI-vs-x-api-key en llama.cpp: los dos dialectos de H-13 también difieren en AUTH).

## Gate
grep -F (whitespace normalizado) de las 13 citas sobre las 7 fuentes crudas guardadas esta sesión; negativos verificados: «Bearer»=0 en _base_client.py@main; «environ»=0 en la página /docs/secret de LiteLLM (la sintaxis vive en config_settings). Doc SIN commitear (regla 1).

## Verificacion del TL (2026-10-24)
Descargue el README y `_client.py` de openai-python, el README del server de llama.cpp, `proxy/config_settings` de LiteLLM y la pagina de autenticacion de LM Studio, y busque con `grep -cF` (espacios normalizados): las citas de `OPENAI_API_KEY`, `--api-key`, `--api-key-file`, `os.environ/` (con la entidad HTML del `<variable name>` decodificada), `x-api-key:` y las tres frases de LM Studio son literales. No comprobe la spec de OpenAI (securitySchemes) ni los numeros de linea. La recomendacion para ADR-020 es inferencia de Hermes.
