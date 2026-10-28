# T-DSH-116 PASO 2 — Runtime de puntos + juez de gramática conectado + decisión (systemone) conectada

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = c005979` (push178: T-DSH-115 + enmienda ADR-020 + diseño UI). Diff sin commitear, encima del paso 1 (aceptado por el TL, aún no en `origin/main`).
- **Corrección pedida en T1958 aplicada:** `systemone_http` **ya no se trata como nube** cuando el endpoint es loopback (ADR-020 E-2/E-3). Solo exige `cloud_enabled` si el endpoint **no** es loopback. Tests para ambos casos.

## E-1..E-6 (ya en `c005979`) — contraste

- **E-2 Dialectos**: soportados OpenAI (`/v1/chat/completions`), Anthropic (`/v1/messages`) y Ollama (`/api/chat`) en el runtime nuevo; SystemOne sigue por su wire tipado de decisión (`/v1/systemone`).
- **E-3 nube por punto**: guard duro antes de abrir conexión; `cloud_enabled=false` ⇒ error, nunca conexión.
- **E-5 último recurso local**: el juez de gramática cae al servidor efímero propio si el proveedor del punto falla o no está; la decisión cae a reglas deterministas.
- **E-6 llama-server opcional**: `user_llama_server` es una opción del punto (nunca arranque automático; si está apagado, el punto falla y cae al local).
- Posible divergencia a revisar: E-2 dice que el tipo se declara en `[[external_providers]]`; T1953 pide el proveedor directamente en el bloque del punto. Se ha implementado lo segundo (literal de T1953); si E-2 exige referencia por nombre a `[[external_providers]]`, es un cambio de esquema pequeño.

## Runtime nuevo — `router/inference_points_runtime.rs`

- `complete_for_point(point_name, prompt, max_tokens)` lee `[inference.points.<punto>]` y ejecuta la llamada HTTP según proveedor:
  - `user_llama_server` y `openai_compatible` → `POST {endpoint}/chat/completions` (Bearer si `api_key_env` está definida en el entorno).
  - `anthropic_compatible` → `POST {endpoint}/messages` (`x-api-key`, `anthropic-version: 2023-06-01`).
  - `ollama_compatible` → `POST {endpoint}/api/chat` (`stream:false`).
  - `systemone_http` → **no** es dialecto de chat: error explícito (se resuelve en `router::decision`).
  - `local_ephemeral` → error "usar el camino local" (el llamante cae a su local, E-5).
- **Guard de nube**: `effective_is_cloud(point)` (systemone_http es local si el endpoint es loopback) y si es nube con `cloud_enabled=false` ⇒ error **antes** de cualquier conexión.
- Clave: solo se lee del entorno en el momento de la llamada (`std::env::var(api_key_env)`); nunca se registra ni se devuelve.

## Juez de gramática conectado (`security/grammar_judge.rs`)

- `judge_grammar` consulta `[inference.points.grammar_judge]`: si el proveedor **no** es `local_ephemeral`, intenta `complete_for_point("grammar_judge", …)`; si responde, se usa su veredicto (mayúsculas).
- Si falla (sin red, servidor apagado, nube no activada), **cae al servidor efímero local** existente (patrón centinela) — E-5, sin subir a la nube automáticamente.
- Default (`local_ephemeral`) = comportamiento anterior intacto.

## Decisión conectada (`router/decision`)

- `resolve_model_slot` lee `[inference.points.decision]` **antes** de `[decision.provider]`:
  - `systemone_http` ⇒ construye el proveedor tipado existente con endpoint/model del punto y `cloud_enabled` del punto. Loopback ⇒ local sin opt-in; no-loopback ⇒ exige `cloud_enabled=true` (E-3), si no cae a reglas.
  - Otro proveedor no-local ⇒ aviso explícito "no implementado en el dialecto de decisión (solo systemone_http)" y baseline de reglas (ver No verificado).
  - `local_ephemeral` (default) ⇒ comportamiento anterior de `[decision.provider]` intacto.
- `SystemoneHttpConfig` gana `cloud_enabled: bool` (default false); `SystemOneHttpProvider::from_config` rechaza no-loopback sin opt-in.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo test -p tylluan-kernel --lib                          → 1364 passed; 0 failed; 4 ignored (52,30 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Tests nuevos (14):
- `inference_points_runtime` (6): dialecto OpenAI, Anthropic y Ollama con **servidor HTTP falso en loopback** (parseo correcto y 1 petición); `cloud_enabled=false` ⇒ **0 peticiones** (guard antes de red); `local_ephemeral` delega; `systemone_http` no es chat.
- `systemone_http` (2): no-loopback sin opt-in ⇒ error; con `cloud_enabled=true` ⇒ OK; loopback sin opt-in ⇒ OK.
- `decision::service` (4): punto systemone loopback construye proveedor; remoto sin opt-in ⇒ reglas; remoto con opt-in ⇒ proveedor; defaults intactos.
- Paso 1 (8) siguen verdes, incluidos los dos casos systemone loopback/no-loopback pedidos en T1958.

## Impact

- El registro de puntos ya no es solo declarativo: **juez de gramática** y **decisión** consumen su punto, con adaptadores de chat OpenAI/Anthropic/Ollama y SystemOne tipado para decisión.
- La nube sigue siendo opt-in **por punto** con guard previo a la red (test de 0 conexiones); `systemone_http` loopback ya no exige activación de nube (corrección T1958).
- Último recurso local garantizado: juez → servidor efímero; decisión → reglas; nunca subida automática a la nube.

## No verificado

- **Paso 3 (POST de cambio de punto, clase operador, REST/CLI) NO hecho todavía**: este diff entrega el paso 2.
- **Adaptadores de decisión OpenAI/Anthropic/Ollama no implementados**: el punto `decision` solo soporta `systemone_http` (wire tipado); con otros proveedores avisa y usa reglas. E-2 los admite; falta el mapeo del wire de decisión a esos dialectos (OpenAI Decisions / chat JSON).
- **Sin prueba en vivo** de los proveedores de paso 2: todo se ha verificado con servidores HTTP falsos en loopback (tests); no se probó contra un servidor real de usuario/OpenAI/Ollama ni se midió latencia.
- **`cloud_enabled=false` nunca abre conexión**: test con servidor falso en loopback (0 peticiones); no se probó con un endpoint externo real (no se quiere salir a Internet en verificación).
- **Posible divergencia E-2**: `[[external_providers]]` vs proveedor directo en el punto (ver arriba).
- **No se probó la decisión en vivo** con un SystemOne real (loopback/remoto); solo construcción del proveedor y rechazo por política.
- **No se tocó** `:47004`, `E:\tylluan`, ni el 9000 ajeno.
