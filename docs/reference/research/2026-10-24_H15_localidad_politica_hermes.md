# H-15 — Localidad de endpoint como input de política: qué mecanismos existen (hermes)

Asignación: TL claude-code T1613. Fuentes primarias descargadas ESTA sesión (curl, scratch): LiteLLM `docs/proxy/{access_groups, model_access_groups, model_access, tag_routing, fallback_management}` + `docs/routing`; Ollama `docs.ollama.com/{faq, cloud}`; Open WebUI `docs.openwebui.com/features/workspace/models` + `/security`; llama.cpp README (leída en H-13). Cita «…» Ctrl+F-able (normalizando espacios) · [I]=inferencia · sin requests a motores locales.

| # | Fuente | Cita literal | URL | ¿Sirve para "dato sensible solo a endpoints locales"? |
|---|---|---|---|---|
| 1 | LiteLLM | «Access Groups simplify how you define and manage resource access across your organization.» | docs.litellm.ai/docs/proxy/access_groups | SÍ (base): grupos de acceso por clave/equipo |
| 2 | LiteLLM | «Group multiple models under a single name, then grant keys or teams access to the entire group.» | docs.litellm.ai/docs/proxy/model_access_groups | SÍ: deployables etiquetados en grupo → clave X solo grupo "local" |
| 3 | LiteLLM | «The models list both hides a model from GET /v1/models and blocks calls to it.» | docs.litellm.ai/docs/proxy/model_access | SÍ: permitir/denegar real (oculta + bloquea), por clave |
| 4 | LiteLLM | «To exclude multiple providers send separate tags: ["!provider:anthropic", "!provider:openai"]» | docs.litellm.ai/docs/proxy/tag_routing | SÍ: enrutado por etiquetas con exclusión; las tags viven en el deployment (config.yaml), que es donde vive api_base/localidad |
| 5 | LiteLLM | «Cooldowns still apply: a deployment that crosses allowed_fails is cooled down independently of weighted failover.» | docs.litellm.ai/docs/routing | NO: `allowed_fails` es fiabilidad (cooldown), no localidad |
| 6 | LiteLLM | — | routing, fallback_management, tag_routing | NO ENCONTRADO: `disable_fallbacks` no aparece en las páginas leídas (grep -c=0 en los 3 HTML); la localidad NO se detecta sola — las tags son etiquetas arbitrarias del admin |
| 7 | Ollama | «By turning off Ollama’s cloud features, you will lose the ability to use Ollama’s cloud models and web search.» | docs.ollama.com/faq | PARCIAL: apagar cloud = sin remoto; protección por eliminación, no por petición |
| 8 | Ollama | «You can also set the environment variable: OLLAMA_NO_CLOUD = 1» | docs.ollama.com/faq | PARCIAL: knob binario server-wide (config { "disable_ollama_cloud" : true }) |
| 9 | Ollama | «Cloud models do not need to be downloaded.» | docs.ollama.com/cloud | Informativa: distingue local vs cloud por nombre de modelo (sufijo cloud), no por política |
| 10 | Open WebUI | «Controls need a model served through one of the instance's connections, such as an OpenAI-compatible or Ollama connection.» | docs.openwebui.com/features/workspace/models | NO: agrupa endpoints en "connections" pero sin etiqueta de localidad ni allow/deny por ella |
| 11 | Open WebUI | — | idem + /security | NO ENCONTRADO: grep -ciE «locality\|endpoint tag\|local endpoint» = 0 en ambas páginas |
| 12 | llama.cpp | — | tools/server/README.md | NO ENCONTRADO: grep -ciE «access group\|locality\|endpoint tag» = 0; solo auth global `--api-key` (H-13) |

## Veredicto para ADR-020 [I]
- El único stack completo es LiteLLM: tags por deployment (donde se define `api_base`, o sea donde se sabe la localidad) + tag routing con exclusión + access groups + restricción por clave que oculta y bloquea. Pero la LOCALIDAD no es un atributo automático en ninguna de las 4 fuentes: es una etiqueta que el admin asigna al desplegar; el mecanismo sirve si el ADR-020 define una convención (p. ej. tag `locality:local` obligatoria por `api_base` privado).
- Ollama modela la localidad de forma nativa (modelo cloud vs descargado) pero solo con interruptor binario.
- Open WebUI y llama.cpp: no encontrado; en ADR-020 no se puede delegar en ellos la política de localidad.

## Gate
grep -F (whitespace normalizado) de las 9 citas sobre los HTML crudos descargados y normalizados con `sed 's/<[^>]*>//g' | tr -s '[:space:]' ' '`; negativos con grep -c sobre los mismos ficheros (salida en la corrida). Doc SIN commitear (regla 1); único ?? del worktree junto a H-13/H-14.

## Verificacion del TL (2026-10-24)
Descargue yo las paginas de LiteLLM (`access_groups`, `model_access_groups`, `model_access`, `tag_routing`, `routing`) y de Ollama (`faq`, `cloud`), las normalice quitando etiquetas HTML y busque con `grep -cF`: las 8 citas comprobadas dan 1 coincidencia. Negativo confirmado: `disable_fallbacks` aparece 0 veces en `tag_routing` y `routing`. No comprobe las citas de Open WebUI (fila 10) ni los negativos de llama.cpp/Open WebUI. El veredicto es inferencia de Hermes.
