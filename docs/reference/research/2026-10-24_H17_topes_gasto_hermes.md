# H-17 — Topes de gasto: LiteLLM y API de OpenAI, y qué pasa al superarlos (hermes)

Asignación: TL claude-code T1619. Fuentes primarias crudas descargadas ESTA sesión: `BerriAI/litellm@main/litellm/exceptions.py`, docs.litellm.ai `/docs/proxy/virtual_keys` y `/docs/proxy/budget_fallbacks` (¡la web de LiteLLM se reestructuró HOY: citas de la mañana ya no existen — todo verificado contra la versión actual), guía rate-limits de OpenAI. Cita «…» Ctrl+F-able (espacios normalizados) · [I] · sin requests a motores locales.

| Mecanismo | Qué ofrece | Al superar el tope | Cita literal | URL/fuente |
|---|---|---|---|---|
| LiteLLM max_budget (clave/team/user) | tope $ por clave | bloqueado; error de la familia RateLimitError | «LiteLLM's own rate limiter (key/team/user/model RPM/TPM, budget, parallel-requests, etc.) blocked the request.» y «Spend budget cap reached (key, team, user, or per-session).» | raw …/BerriAI/litellm/main/litellm/exceptions.py:48,81 (enums) |
| LiteLLM código HTTP | — | [I] 429: la clase hereda del error openai | «class RateLimitError(openai.RateLimitError):» | …/litellm/exceptions.py:447; número 429 literal en docs de budget: NO ENCONTRADO |
| LiteLLM budget_duration | ventana de reset | — | «sending an explicit "budget_duration": null creates a key whose budget never resets, skipping the configured default» | docs.litellm.ai/docs/proxy/virtual_keys |
| LiteLLM tpm/rpm por clave | techo req/token/min | cuenta la misma frase de limiter de arriba; dimensión citada | «was exceeded (an RPM ceiling, a TPM ceiling, a max-parallel-requests ceiling, a budget cap, or a max-iterations cap)» | …/litellm/exceptions.py:61 (docstring RateLimitType) |
| LiteLLM model_max_budget + budget_fallbacks | tope $ por modelo por clave | bloqueo por defecto; evento budget_exceeded; reruteable | «By default model_max_budget blocks requests once a key's spend on a model crosses its cap» + «instead of returning a budget_exceeded error» + «the request finally returns budget_exceeded» | docs.litellm.ai/docs/proxy/budget_fallbacks |
| OpenAI rate limits | org Y project | 429 con tipo/código distinguibles | «Rate limits are defined at the organization level and at the project level, not user level.» + «429 rate_limit_error slow_down Your request rate increased too quickly.» | developers.openai.com/api/docs/guides/rate-limits |
| OpenAI spend limits | cap mensual org/project | «Affected API requests return a 429 error» (hard limit) vs «Spend alert Sends a notification; API traffic continues» | «Enforce a monthly organization or project cap» + «Retry-After may be present on 429 responses caused by a temporary rate limit» | idem (tabla Spend alerts/limits) |
| OpenAI cabeceras | estado del tope en cada respuesta | — | tokens literales presentes: «x-ratelimit-remaining-project-tokens», «x-ratelimit-limit-tokens», «x-ratelimit-reset-requests» (9 en total: limit/remaining/reset × requests/tokens/project-tokens) | idem (HTML) |

## Consecuencias para el tope del juez en nube opt-in de Tylluan [I]
- Dos capas naturales: LiteLLM max_budget+budget_duration por clave del juez (ej. $2/24h) como freno interno, y el hard spend limit mensual de project OpenAI como seguro externo ineludible (429 + Retry-After).
- El cliente debe distinguir quién.limitó (categoría vendor vs litellm; dimensión budget vs rpm) para decidir política: budget agotado → degradar a juez local (coherente con ADR-020); rpm/tpm → esperar/reintentar; nunca reintentar budget.

## Gate
grep -F (whitespace normalizado) de las 17 citas sobre las 4 fuentes crudas guardadas esta sesión (excepciones.py, virtual_keys, budget_fallbacks actual, guía rate-limits); negativos verificados: «429»=0 y «budget_limit_exceeded»=0 en budget_fallbacks actual (reestructurada hoy; el evento actual es «budget_exceeded», 3 hits), y «429» literal en docs de budget LiteLLM: no aparece — el código va marcado [I] por herencia de clase. Doc SIN commitear (regla 1).

## Verificacion del TL (2026-10-24)
Descargue `litellm/exceptions.py`, las paginas `proxy/virtual_keys` y `proxy/budget_fallbacks` de LiteLLM y la guia de rate limits de OpenAI, y busque con `grep -cF` (espacios normalizados): 15 citas son literales, entre ellas la clase `RateLimitError(openai.RateLimitError)`, el tope de gasto `Spend budget cap reached`, `model_max_budget`, `budget_exceeded`, el limite mensual por organizacion o proyecto y la cabecera `x-ratelimit-remaining-project-tokens`; `429` aparece 0 veces en `budget_fallbacks`, como dice Hermes. No comprobe el resto de cabeceras ni los numeros de linea. El 429 de LiteLLM y las consecuencias para el tope del juez son inferencias de Hermes. Nota: el doc tiene 21 lineas, no 23.
