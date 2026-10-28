# T-DSH-116 PASO 3 — Cambio de punto por operador (REST + CLI)

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = c005979` + pasos 1 y 2 (diff sin commitear).
- **Alcance T1953 paso 3:** "POST de cambio de punto = clase operador, solo REST/CLI" (nunca MCP).

## REST

- `POST /api/v1/inference/points/{point}` con `{provider?, endpoint?, model?, api_key_env?, cloud_enabled?}` (actualización **parcial**; los campos omitidos se conservan).
- **Clase operador**: `auth.rs::operator_class_required` marca `POST /api/v1/inference/points/*` (misma clase que config/centinela); por MCP no existe cambio (solo lectura, paso 1).
- Validación antes de aplicar: se clona el punto, se aplica el update y se valida con las mismas reglas del paso 1 (proveedor conocido, nube opt-in, endpoint/clave). Si falla ⇒ **400** con `errors` y el estado actual; no se muta nada.
- Si es válido ⇒ se actualiza **en memoria** (`HttpState.config`) y se devuelve `{ok, point, updated, state}`; se registra `tracing::warn` del cambio (E-3: registro del cambio). No se persiste en `tylluan.toml` (ver No verificado).
- `apply_point_update` es una función **pura** testeada (update parcial, nube sin opt-in rechazada, systemone loopback aceptado).

## CLI

- `tylluan inference [--port <p>] points` — lectura del registro (mismo JSON que REST, sin claves).
- `tylluan inference [--port <p>] set-point <point> [--provider ...] [--endpoint ...] [--model ...] [--api-key-env ...] [--cloud-enabled <bool>]` — cambio por operador; sin cambios ⇒ error de uso; respuesta 400 del kernel ⇒ exit 1 con el mensaje del kernel.
- Token: `resolve_cli_token` (flag `--token` > `TYLLUAN_TOKEN` > `.tylluan-token`).

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo clippy -p tylluan-cli --all-targets -- -D warnings    → 0
cargo test -p tylluan-kernel --lib                          → 1367 passed; 0 failed; 4 ignored (51,02 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Tests nuevos (3): `point_update_rejects_cloud_without_opt_in`, `point_update_accepts_loopback_systemone_without_opt_in`, `point_update_is_partial_and_keeps_other_fields`.

### Verificación en vivo (kernel test-run, puerto/DATA_DIR propios, BM25-only)

```
GET  /api/v1/inference/points                     → 200, valid=true (defaults locales)
POST /api/v1/inference/points/chat {openai_compatible, cloud_enabled=false}
                                                  → 400 con el error de opt-in de nube
POST .../chat {openai_compatible, cloud_enabled=true}
                                                  → 200 {ok:true}, locality=cloud
POST .../sentinel {systemone_http, endpoint 127.0.0.1}
                                                  → 200 {ok:true}, locality=local (sin opt-in)
tylluan inference --port <p> points               → exit 0, JSON del registro
tylluan inference --port <p> set-point chat --cloud-enabled false
                                                  → exit 1, mensaje 400 del kernel
tylluan inference --port <p> set-point chat --provider local_ephemeral
                                                  → exit 0 {ok:true}
```

## Impact

- El operador puede cambiar el punto de inferencia en caliente por REST/CLI (clase operador), con la misma validación que el registro (nube opt-in, endpoint/clave) y registro del cambio; MCP sigue solo-lectura.
- `systemone_http` loopback se acepta como local sin exigir `cloud_enabled` (corrección T1958); fuera de loopback exige opt-in explícito.
- El registro queda operativo de punta a punta: config (paso 1) → runtime + juez/decisión (paso 2) → cambio por operador (paso 3).

## No verificado

- **Persistencia**: el cambio de punto es **en memoria**; no se escribe en `tylluan.toml`, así que un reinicio vuelve a la config. No se pidió explícitamente, pero conviene decidirlo (persistir requeriría escritura segura del TOML).
- **Auth en vivo**: el test-run usa `dev_mode=true`; el gate de clase operador está cubierto por test unitario de `operator_class_required` y el mismo middleware que config, no se probó con token real.
- **Sin recarga de proveedores en caliente**: cambiar un punto con el kernel en marcha actualiza la config; los proveedores ya construidos (p. ej. decisión) no se reconstruyen hasta el siguiente arranque/construcción del servicio. Los puntos de chat/juez leen config por llamada, esos sí aplican al momento.
- **CLI**: no probado contra producción (`:47004`), solo contra test-run con `--port`.
- **Adaptadores de decisión OpenAI/Anthropic/Ollama** siguen pendientes (paso 2, documentado).
- **No se tocó** `:47004`, `E:\tylluan` ni el 9000 ajeno.
