# T-DSH-119 — Adaptadores de decisión multiproveedor (OpenAI / Anthropic / Ollama)

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = 3e76953` (push182). Diff sin commitear.
- **Encargo (T1979):** implementar tal cual el diseño aprobado (`docs/internal/DISENO_decision_multiproveedor.md`): `adapters/` + `resolve_model_slot` + tests con servidor falso por dialecto; clippy 0; informe con `## Impact`. Decisiones del TL aplicadas: (1) chat/completions con `json_schema` (sin API Decisions nativa); (2) `confidence` opcional (1.0 por defecto); (3) responder TODAS las preguntas o abstención completa; (4) `instructions` igual a los tres dialectos, saneado.

## Adaptadores (`router/decision/adapters.rs`)

- **Común**: prompt determinista (estado ≤16 KiB, preguntas con sus opciones, `instructions` saneadas ≤1024 y tratadas como datos no confiables), contrato de salida estricto `{"answers":[{"question_id","option","confidence"}]}`, `temperature 0`, `max_tokens 512`.
- **Validación en el núcleo** (`parse_answers`): todas las preguntas exactamente una vez; `question_id` conocido; `option` dentro de `allowed_values`; sin campos extra; `confidence` en `[0,1]` (ausente ⇒ 1.0). Cualquier incumplimiento ⇒ `DecisionError::Abstained("provider_invalid: …")` (abstención, sin reintento ciego). Distribución: opción elegida = `confidence`, resto reparte `1-confidence` uniformemente (suma 1).
- **OpenAI compatible**: `POST {endpoint}/chat/completions` con `response_format: json_schema` strict; si el servidor responde **HTTP 400**, **un** reintento sin `response_format` y parseo JSON estricto. `provider_id = "openai_chat_json"`.
- **Anthropic compatible**: `POST {endpoint}/messages` con tool-use forzado (`tool_choice: {type:"tool", name:"answer"}`, `input_schema` = contrato); parseo del bloque `tool_use.input`. `provider_id = "anthropic_tool_json"`.
- **Ollama compatible**: `POST {endpoint}/api/chat` con `format` = JSON Schema; parseo `message.content`. `provider_id = "ollama_json"`.
- Clave solo por `api_key_env` (Bearer en OpenAI, `x-api-key` en Anthropic), leída del entorno en la llamada; nunca se registra.

## `resolve_model_slot`

- Si `[inference.points.decision].provider` es `openai_compatible`/`anthropic_compatible`/`ollama_compatible`, construye el adaptador con endpoint/model/key_env del punto (timeout de cliente 30 s; `request_timeout` 5 s / `shadow_timeout` 30 s en el slot).
- **Nube opt-in por punto**: si el punto resuelve a nube y `cloud_enabled=false`, no se construye adaptador (aviso + reglas). Se ha **unificado la localidad**: cualquier proveedor con endpoint **loopback** es local (no exige `cloud_enabled`) — coherente con E-3 y con la corrección de T1958; fuera de loopback sí exige opt-in.
- `systemone_http` mantiene su camino anterior (loopback local / remoto con `cloud_enabled`). `local_ephemeral` sigue usando `[decision.provider]` como antes.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo test -p tylluan-kernel --lib                          → 1383 passed; 0 failed; 4 ignored (40,91 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Tests nuevos (9):
- `decision::adapters` (5, servidor HTTP falso en loopback): OpenAI respuesta válida (confianza 0.8 ⇒ distribución 0.8/0.2, 1 petición); OpenAI opción fuera del conjunto ⇒ abstención; respuesta parcial ⇒ abstención; Anthropic tool_use ⇒ parseo correcto; Ollama JSON ⇒ parseo correcto.
- `decision::service::t119` (4): OpenAI sin opt-in ⇒ sin proveedor; OpenAI con opt-in ⇒ adaptador (`openai_chat_json:modelo-decision`); Anthropic con opt-in ⇒ adaptador; Ollama en loopback ⇒ adaptador sin opt-in.
- `inference_points_runtime` (7, actualizados): loopback es local para cualquier dialecto (conecta sin opt-in); nube no loopback sin opt-in ⇒ error de política antes de red; resto de dialectos.

## Impact

- El punto `decision` puede usar **OpenAI/Anthropic/Ollama** además de `systemone_http`, respetando el wire tipado: el proveedor solo transporta; las opciones se validan en el núcleo y cualquier desviación se convierte en **abstención** (nunca en una respuesta inventada).
- `confidence` opcional y reparto determinista; `instructions` se reenvían saneadas a los tres dialectos.
- La nube sigue siendo **opt-in por punto** y cualquier endpoint loopback es local (sin exigir `cloud_enabled`), coherente con E-3.

## No verificado

- **Sin prueba en vivo contra proveedores reales**: todo con servidores HTTP falsos en loopback; no se probó OpenAI/Anthropic/Ollama reales ni se midió latencia.
- **Ruta de reintento OpenAI por HTTP 400** (sin `json_schema`) no ejercitada: el servidor falso siempre responde 200; el código existe pero no tiene test que simule el 400.
- **HITL clínico**: los adaptadores no comprueban `DataSensitivity::Clinical`; confían en la puerta del servicio de decisión (que hoy no recibe sensibilidad en el wire). Si se quiere garantía explícita para nube+clínico, hay que añadirla en el servicio antes de usar adaptadores cloud (no estaba en el alcance del diseño aprobado).
- **`instructions`**: se incluyen en el prompt y se sanean, pero no hay test dedicado de que el texto saneado llegue idéntico a los tres dialectos (solo se cubre indirectamente).
- **Distribución con `confidence`**: la semántica elegida (elegida=confidence, resto uniforme) es una decisión de implementación; conviene que el TL confirme que encaja con el contrato de `DecisionAnswer`.
- **`systemone_http` remoto** ya estaba cubierto por T-DSH-116; aquí no se re-probó en vivo.
- **No se tocó** `:47004`, `E:\tylluan` ni el 9000 ajeno.
