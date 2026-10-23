# H-16 — Timeouts y reintentos por defecto: openai-python, LiteLLM, llama.cpp, guía decisions (hermes)

Asignación: TL claude-code T1616. Fuentes primarias descargadas ESTA sesión: `openai/openai-python@main/src/openai/{_constants,_base_client}.py`, `BerriAI/litellm@main/litellm/{router,constants,litellm_core_utils/request_timeout_resolver}.py`, `docs.litellm.ai/docs/proxy/timeout`, README server de llama.cpp (guardada H-13), guía decisions de OpenAI (guardada H-13). Cita «…» Ctrl+F-able (espacios normalizados) · [I] · sin requests a motores locales.

| Cliente | Timeout por defecto | Reintentos por defecto | Cita literal | URL/fuente |
|---|---|---|---|---|
| openai-python | 600 s total, 5 s connect | 2 | «DEFAULT_TIMEOUT = httpx2.Timeout(timeout=600, connect=5.0)» y «DEFAULT_MAX_RETRIES = 2» | raw …/openai-python/main/src/openai/_constants.py:7-8 |
| openai-python (binding) | — | el constructor usa la constante | «max_retries: int = DEFAULT_MAX_RETRIES,» | …/openai-python/main/src/openai/_base_client.py:461 |
| LiteLLM Router | (abajo) | hereda 2 de openai | «if num_retries is not None: self.num_retries = num_retries elif litellm.num_retries is not None: self.num_retries = litellm.num_retries else: self.num_retries = openai.DEFAULT_MAX_RETRIES» | raw …/BerriAI/litellm/main/litellm/router.py:1499-1504 |
| LiteLLM request_timeout | 6000 s | — | «DEFAULT_REQUEST_TIMEOUT_SECONDS: Final[float] = 6000.0» + «request_timeout (or the REQUEST_TIMEOUT env var) is the proxy-wide default when no router or deployment timeout is set.» | …/BerriAI/litellm/main/litellm/constants.py:653 + docs.litellm.ai/docs/proxy/timeout |
| llama.cpp server | 3600 s read/write (`-to, --timeout`, env LLAMA_ARG_TIMEOUT) | NO ENCONTRADO (el server no reintenta; es el servidor mismo) | «`-to, --timeout N` \| server read/write timeout in seconds (default: 3600)» | tools/server/README.md:220 |
| Guía decisions OpenAI | — | — | NO ENCONTRADO: la guía no dice nada sobre latencia (grep -i latenc = solo nav del sitio, fuera del cuerpo de la guía) | developers.openai.com/api/docs/guides/decisions |

## Defaults razonables para el cliente de Tylluan en CPU [I]
- El defecto de SDK (600 s) es corto para judges en CPU: los defaults reales del ecosistema servidor son más generosos (llama.cpp 3600 s, LiteLLM 6000 s proxy-wide). Siguiendo la regla del repo (nunca bajar timeouts a guilds de inferencia), el cliente debería pedir por defecto el orden de LiteLLM/llama.cpp (≥3600 s) para endpoints CPU locales, y recortar solo el connect timeout (5 s de openai-python es razonable: conectar es barato, esperar tokens no).
- Reintentos: 2 es el default heredado (openai→LiteLLM); en CPU cada reintento cuesta el doble de wall-clock en el peor caso → 1 con backoff para requests cortos, 0 para judges largos, y nunca reintentar si el reintento re-paga GPU/CPU ya saturada. Los 3 clientes distinguen connect vs lectura; el de Tylluan también debería.

## Gate
grep -F (whitespace normalizado) de las 7 citas sobre las fuentes crudas guardadas; negativos: «retry»=0 en README server y «latenc» solo en nav (3 hits, cero en el cuerpo) de la guía decisions — salidas en la corrida. Doc SIN commitear (regla 1).

## Verificacion del TL (2026-10-24)
Descargue `_constants.py`, `_base_client.py` de openai-python, `router.py` y `constants.py` de LiteLLM, la pagina `proxy/timeout` y el README del server de llama.cpp, y busque con `grep -cF` (espacios normalizados): las 7 citas son literales (`DEFAULT_TIMEOUT = httpx2.Timeout(timeout=600, connect=5.0)`, `DEFAULT_MAX_RETRIES = 2`, `max_retries: int = DEFAULT_MAX_RETRIES,`, `self.num_retries = openai.DEFAULT_MAX_RETRIES`, `DEFAULT_REQUEST_TIMEOUT_SECONDS` en la linea 653 con valor 6000.0, `proxy-wide default when no router or deployment timeout is set`, `server read/write timeout in seconds (default: 3600)`) y `retry` aparece 0 veces en el README de llama.cpp. No comprobe la guia decisions de OpenAI ni `request_timeout_resolver`. Las recomendaciones de la seccion "Defaults razonables" son inferencias de Hermes.
