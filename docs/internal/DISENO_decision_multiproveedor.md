# DISEÑO — Contrato de decisión multiproveedor (OpenAI / Anthropic / Ollama sobre el wire tipado)

Estado: **propuesta para aprobación del TL** (T-DSH-118). Sin código hasta el OK.

## 1. Invariante

El wire de decisión no cambia (`DecisionRequest`/`DecisionAnswer`, §1217): preguntas con opciones cerradas y respuesta por distribución. Los adaptadores solo traducen **transporte**; la validación de opciones sigue siendo la del núcleo (`validated_argmax`), nunca la del proveedor.

## 2. Respuesta exigida al modelo (contrato de salida)

JSON estricto, sin texto libre:

```json
{"answers": [{"question_id": "q1", "option": "A", "confidence": 0.82}]}
```

- `question_id` debe existir en la petición; `option` debe pertenecer a `allowed_values` de esa pregunta.
- `confidence` opcional en `[0,1]`; sin ella se usa 1.0 y el reparto es determinista.
- Cualquier campo extra, opción fuera del conjunto, pregunta desconocida o JSON inválido ⇒ **abstención** (`DecisionError::ProviderInvalid`, sin reintento ciego) y contador de rechazos, igual que hoy.

## 3. Mapeo por dialecto

| Proveedor | Endpoint | Salida estructurada | Parseo |
|---|---|---|---|
| OpenAI compatible | `POST {endpoint}/chat/completions` | `response_format: {type:"json_schema", json_schema:{name:"decision", strict:true, schema}}`; si el servidor lo rechaza (400), reintento **una vez** sin `response_format` y parseo JSON estricto | `choices[0].message.content` → JSON |
| Anthropic compatible | `POST {endpoint}/messages` | tool-use forzado (`tool_choice:{type:"tool",name:"answer"}`) con `input_schema` = contrato de §2 | `content[]` tipo `tool_use` → `input` |
| Ollama compatible | `POST {endpoint}/api/chat` | `format` = JSON Schema (soportado por Ollama reciente); fallback `format:"json"` | `message.content` → JSON |

Prompt (común, determinista, sin instrucciones libres del usuario salvo el canal `instructions` ya existente y saneado): estado serializado (≤16 KiB, ya impuesto), preguntas con `allowed_values`, y la orden de responder SOLO el JSON del §2. `temperature = 0`, `max_tokens` acotado por pregunta (≤64).

## 4. Configuración y seguridad

- Se usa `[inference.points.decision]` (paso 1–3): `provider`, `endpoint`, `model`, `api_key_env`, `cloud_enabled`. Clave solo desde entorno; nunca se registra.
- **Nube opt-in por punto**: `cloud_enabled=false` ⇒ error antes de abrir conexión (ya implementado en el runtime). `systemone_http` loopback sigue siendo local sin opt-in.
- `DataSensitivity::Clinical` hacia nube sigue exigiendo HITL por petición (ADR-020 E-4); los adaptadores **no** relajan esa puerta.
- Timeout por punto (el del runtime, 60 s) y sin reintentos en bucle: 1 reintento solo por rechazo de `response_format`.
- Fallback (E-5): cualquier error ⇒ proveedor local de decisión (reglas o systemone loopback si está configurado). Nunca fallback a nube.

## 5. Cambios de código previstos (para el paso de implementación)

1. `router/decision/adapters/` con tres structs que implementan `DecisionProvider` (`chat_json`, `anthropic_tool`, `ollama_json`), reutilizando `reqwest` del runtime de puntos.
2. `resolve_model_slot`: además de `systemone_http`, construir el adaptador según el punto (hoy avisa y cae a reglas).
3. Tests con servidor HTTP falso en loopback por dialecto: respuesta válida, opción fuera de conjunto ⇒ abstención, JSON inválido ⇒ abstención, `cloud_enabled=false` ⇒ 0 conexiones, y no-regresión del wire rules/systemone.

## 6. Preguntas abiertas para el TL

1. ¿OpenAI por `chat/completions` con `json_schema` (propuesto) o por la API Decisions nativa si el endpoint la expone?
2. ¿`confidence` se acepta como opcional (propuesto) o se prohíbe para que el reparto sea siempre determinista?
3. ¿Se permite respuesta parcial (solo algunas preguntas) o se exige responder todas para no abstenerse?
4. ¿El canal `instructions` se reenvía igual a los tres dialectos (propuesto sí, ya saneado)?
