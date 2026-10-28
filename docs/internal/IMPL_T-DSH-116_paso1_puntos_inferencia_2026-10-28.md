# T-DSH-116 — Registro de puntos de inferencia (PASO 1: modelo de config + validación + lectura REST/MCP)

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = 399db1f` + T-DSH-115 (diff sin commitear, pendiente de verificación del TL). La enmienda ADR-020 "Enmienda 2026-10-28" E-1..E-6 **no está en el worktree** (`docs/reference/adr/ADR020_inference_gateway.md` de 399db1f no contiene esa sección); este paso se ha implementado desde el alcance literal de T1953 y la sección 3.1 existente del ADR ("perfil por función"). Se revisará contra E-1..E-6 cuando el TL la publique.
- **Entrega en pasos (T1953):** este diff es **solo el paso 1**. Pasos 2 (conectar `tylluan_decide` y el juez al registro) y 3 (POST de cambio de punto, clase operador) **no** están en este diff.

## Paso 1 — Modelo de config + validación + lectura

### Config (`config.rs`, `[inference.points]`)

```toml
[inference.points.decision]
provider = "local_ephemeral"      # local_ephemeral | user_llama_server |
                                  # openai_compatible | anthropic_compatible |
                                  # ollama_compatible | systemone_http
endpoint = ""                     # URL http(s) obligatoria para nube
model = ""
api_key_env = ""                  # NOMBRE de la variable de entorno, nunca la clave
cloud_enabled = false             # la nube SOLO se usa si el punto la activa

[inference.points.grammar_judge]  # idem
[inference.points.sentinel]       # idem
[inference.points.chat]           # idem
```

- Cuatro puntos: `decision`, `grammar_judge`, `sentinel`, `chat` (chat/razonamiento).
- Tipos: `InferencePointConfig` + `InferencePointsConfig` con `#[serde(default)]`; `provider` vacío ⇒ `local_ephemeral` (default de los cuatro puntos, sin romper configs existentes).
- **Validación** (`InferencePointsConfig::validate`), por punto:
  - proveedor desconocido ⇒ error;
  - proveedor de nube (`openai_compatible`, `anthropic_compatible`, `ollama_compatible`, `systemone_http`) con `cloud_enabled = false` ⇒ **error** (la nube solo se usa si el punto la activa);
  - endpoint http(s) obligatorio para nube;
  - `api_key_env` obligatorio para `openai_compatible`/`anthropic_compatible`/`systemone_http` (clave por variable de entorno, nunca en claro); `ollama_compatible` puede no necesitarla.
- `public_json()`: vista **sin valores de clave** — solo `provider`, `locality` (`local|cloud`), `cloud_enabled`, `endpoint`, `model`, `api_key_env` (nombre), más `valid` y `errors`.

### REST (lectura)

- `GET /api/v1/inference/points` (`api_inference::inference_points_handler`): devuelve `{points, valid, errors, note}`. Es lectura (no clase operador); **no arranca nada** y no expone claves.
- Ruta añadida a `routes.rs`; sin cambios de auth (lectura).

### MCP (lectura)

- Intents deterministas en `tylluan_do` (`handler_do/llama_cmd.rs::handle_points_intent`), antes del routing:
  `inference points`, `puntos de inferencia`, `puntos inferencia` (+ `status`/`estado`). Devuelven el mismo JSON que REST. Sin arrancar/parar/cambiar (eso es paso 3, clase operador).

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo test -p tylluan-kernel --lib                          → 1350 passed; 0 failed; 4 ignored (53,71 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Tests nuevos (6, `config::inference_points_tests`):
- `default_points_are_local_and_valid` — los 4 puntos default son `local_ephemeral`, `valid=true`.
- `cloud_provider_without_opt_in_is_invalid` — nube sin `cloud_enabled` ⇒ error exacto.
- `cloud_provider_with_opt_in_endpoint_and_env_is_valid` — nube con opt-in+endpoint+env ⇒ válido; la vista pública solo contiene el **nombre** de la env, nunca un valor (`sk-` no aparece).
- `unknown_provider_is_invalid`.
- `points_parse_from_toml_section` — `[points.decision]` con `ollama_compatible` + endpoint parsea y valida.
- Prefijos MCP: `inference_points_prefixes_recognized` y `unrelated_intents_not_llama` (los intents ajenos caen al routing normal).

### Verificación en vivo (kernel test-run, puerto/DATA_DIR propios, BM25-only)

Config con `[inference.points.chat] provider="openai_compatible" cloud_enabled=false` (caso inválido a propósito):

```
GET  /api/v1/inference/points → 200
  valid=false, errors=["inference.points.chat: proveedor 'openai_compatible' es de nube y
  cloud_enabled=false (la nube solo se usa si el punto la activa)"]
  points.chat = {provider:"openai_compatible", locality:"cloud", cloud_enabled:false,
                 endpoint:"https://api.example.invalid/v1", model:"modelo-prueba",
                 api_key_env:"T116_FAKE_KEY"}   ← nombre de env, nunca el valor
POST /api/v1/do {"intent":"puntos de inferencia"} → 200, is_error=false, mismo JSON
  (el JSON no contiene ningún valor de clave; "sk-" ausente)
```

## Impact

- El kernel gana un **registro declarativo de puntos de inferencia** por función (`decision`, `grammar_judge`, `sentinel`, `chat`) con proveedor/endpoint/modelo/clave-por-env y **nube opt-in por punto** (`cloud_enabled=false` por defecto), validado en lectura: una config cloud sin opt-in se reporta como inválida, no se usa silenciosamente.
- Expuesto por REST y MCP **solo lectura**, sin claves (solo el nombre de la variable de entorno), sin arrancar procesos.
- Defaults compatibles: sin `[inference.points]`, los cuatro puntos son `local_ephemeral` y el comportamiento actual no cambia.

## No verificado

- **Enmienda ADR-020 E-1..E-6 no leída**: no está en `399db1f` (ni en `E:\tylluan-patches`); el paso 1 sigue el alcance literal de T1953 y la sección 3.1 del ADR. Si E-1..E-6 cambia nombres de proveedores/campos, habrá que ajustar.
- **Pasos 2 y 3 no hechos** (conectar `tylluan_decide`/juez al registro; POST de cambio de punto clase operador + CLI): no están en este diff, como permite el encargo ("puedes entregar el paso 1 solo").
- **Dialectos HTTP** (`/v1/chat/completions`, `/v1/messages`, Ollama, SystemOne) no implementados en este paso; solo se declaran/validan (los adaptadores son paso 2).
- **"cloud_enabled=false jamás abre conexión externa"**: no hay código de conexión todavía en el paso 1, así que no hay test de conexión; se cubrirá en el paso 2 con servidor HTTP falso en loopback (criterio del encargo).
- **Validación al arrancar**: `validate()` se llama en la lectura (REST/MCP); no se ha integrado en el boot del kernel ni se ha probado con un `tylluan.toml` real de producción (solo test-run).
- **No se tocó** `:47004`, `E:\tylluan` ni el 9000 ajeno.
