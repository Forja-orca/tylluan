# D-33 — El juez de la capa 2 deja de morir en silencio

- **Agente:** Deep (OpenCode)
- **Fecha:** 2026-10-23
- **Base:** `origin/main` = `a85f4d7` (el patch **también** aplica sobre `decc315`, base de D-32; `git apply --check` OK en ambos)
- **Patch:** `E:\tylluan-patches\deep\D-33.patch` — 33 902 B, **7 ficheros**, **+461 / −33**
- **Estado:** implementado y verificado en local; aplicado en limpio sobre `a85f4d7` (`git apply` + `cargo check --tests` aislado). Sin commitear.

## Impact

**Qué cambia.** (1) El juez ya puede emitir veredictos contra un backend con varios
modelos cargados: el kernel manda `model` si está configurado (`[security] judge_model`,
o `TYLLUAN_JUDGE_MODEL` como atajo de operador) y el guild `llama_backend` **resuelve
el primero de `/v1/models`** cuando no llega ninguno (cache 60 s). Además, un `model`
explícito que no casa con ningún `[[external_providers]]` ya **no se descarta**: antes
se tiraba en silencio y el backend contestaba 400 igual.
(2) Todo rechazo del backend cuenta en un cubo propio,
`GET /health?verbose=true` → `components.judge.by_kind.backend_error`, y el primer
rechazo de cada arranque emite **un** `warn!` diciendo que la capa 2 y la capa 4 quedan
inertes y cómo arreglarlo. Antes solo quedaba un `debug!` indistinguible de "juez limpio".

**Qué NO cambia.** El modelo por defecto (decisión de José tras M-BUFFY-3: sigue
Qwen2.5-0.5B-Instruct), ningún umbral, ningún valor por defecto de comportamiento
(`judge_model` vacío = no se envía, que es lo de antes), ningún token ni `.tylluan-token`.
`tylluan.example.toml` solo gana un bloque **comentado**.

**Riesgo asumido.** El `warn!` es uno por arranque y por proceso: si el backend se
arregla a mitad de sesión y se vuelve a romper, no vuelve a avisar (los contadores sí
siguen subiendo). Y la resolución por `/v1/models` asume que el backend anuncia IDs
válidos para `chat/completions`; si no los anuncia, se mantiene el comportamiento
anterior (sin `model`) y el fallo se ve como `backend_error`, no se inventa nada.

**No depredación.** Toca `guilds/core/llama_backend.py` y `tylluan.example.toml`, que
`scripts/check_no_predation.sh` marca como rutas sensibles: no cambia valores por
defecto, no escribe ficheros del usuario, no toca `E:\tylluan`, ni tokens, ni
`dev_mode`. Autorización de otro agente: ninguna.

---

## 1. Los dos defectos

| # | Defecto medido (Buffy) | Antes | Ahora |
|---|---|---|---|
| a | Backend OpenAI-compatible con varios modelos (LM Studio) → petición **sin `model`** → **400** | `query_model` construía el cuerpo sin `model`; un `model` explícito que no casaba con un proveedor externo se descartaba | `judge_model` (config/env) viaja en el cuerpo; sin él, el guild consulta `/v1/models` (cache 60 s) y usa el primero; un `model` explícito se reenvía tal cual |
| b | El fallo se registraba con `debug!` y salía: capa 2 muerta sin señal | `resp.status()` no-2xx → `JudgeOutcome::HttpError` (mezclado con fallos de transporte) y `debug!` en `write_gate.rs` | 4xx (salvo 401/403) y 5xx cuyo cuerpo arrastra el rechazo del backend → **`backend_error`** en `/health`, + `warn!` único por arranque |

Lo que **no** toqué y sigue abierto: el segundo hallazgo de Buffy (con el modelo por
defecto el juez marcó 1 de 32 inyecciones que la capa 1 deja pasar). La cola lo dice
explícitamente: *no cambies el modelo por defecto*. Es un problema de calidad del juez,
no de plumbing; queda como decisión de José/TL.

## 2. Diseño

### Kernel (`crates/tylluan-kernel/src/security/`)

- `judge_telemetry.rs`: `JudgeOutcome::BackendError` + contador propio; entra en
  `failures()` (un rechazo del backend **es** un fallo del juez) y en `by_kind` como
  `backend_error`. Sigue sin guardar prompt ni respuesta: solo cifras.
- `coherence_gate.rs`:
  - `configured_judge_model()`: `TYLLUAN_JUDGE_MODEL` y, si no, `[security] judge_model`
    leído con `TylluanConfig::load_cached()`; `None` = no enviar `model`.
  - `call_judge_inner(..., judge_model: Option<&str>)`: el cuerpo solo gana `model`
    cuando hay uno configurado (una cadena vacía no se envía: el guild decide).
  - `classify_judge_failure(status, body)`: 401/403 → `http`; cualquier otro 4xx →
    `backend_error`; 5xx → `backend_error` **solo** si el cuerpo delata el rechazo del
    backend (`bad request`, `http error 400`, `400` + `model`). Así el 400 que el guild
    propaga como 500 también se ve, sin etiquetar de "backend" cualquier 500 del kernel.
  - `warn_judge_cannot_verdict()`: `OnceLock`, un `warn!` por arranque, con la pista
    según el caso (¿ya mandamos `model`? ¿el backend lo pide?).
- `config.rs`: `judge_model: Option<String>` en `SecurityConfig` (`#[serde(default)]`),
  `None` en `Default`. **No** cambia ningún otro valor por defecto.

### Guild (`guilds/core/llama_backend.py`)

- `_fetch_backend_models(base_url)`: `GET {base}/models`, tolerante a fallos.
- `_resolve_legacy_model(base_url, requested)`: `requested` gana (y **no** sondea);
  si no, el primero de `/v1/models`, cacheado 60 s por `base_url`; `""` si no hay pista.
- `query_model`: en la vía legacy (`is_external`) añade `request_body["model"]`. Los
  `[[external_providers]]` no cambian: ya llevan su modelo.

### Configuración

`tylluan.example.toml`: bloque comentado de `[security] judge_model` explicando el
síntoma (`by_kind.backend_error`) y que vacío = primero de `/v1/models`.

## 3. Aceptación

`tests/cola_d33_acceptance.rs` (T-DSH-40) era el contrato: su test "vivo" afirmaba el
defecto y pedía ser invertido al cerrar D-33 ("Deep debe invertir este test").

| Test | Antes | Ahora |
|---|---|---|
| `d33_judge_request_carries_model_and_succeeds` (era `d33_current_..._lacks_model_and_fails`) | pasaba afirmando que **no** había `model` y que la llamada fallaba | pasa afirmando que hay `model`, que sale de `/v1/models` (`fake-model-A`/`B`) y que llega `SAFE` |
| `d33_desired_judge_request_carries_model_and_gets_verdict` | `#[ignore = "D-33"]` | sin ignore, verde |
| `d33_desired_judge_failure_is_visible` | `#[ignore = "D-33"]` | sin ignore, verde (`by_kind.backend_error` sube con un falso que devuelve 400 siempre) |

Los tres corren **de verdad**: `cola_d33_acceptance` levanta un backend falso en
`127.0.0.1:0` y arranca el guild `llama_backend` real por stdio MCP, así que la mitad
(a) está probada end-to-end (guild + backend), no simulada.

## 4. Verificación

```
cargo check -p tylluan-kernel                                             → exit 0
cargo clippy -p tylluan-kernel --lib -- -D warnings                       → exit 0 (40,39 s)
cargo clippy ... --test cola_d33_acceptance --test asi06_judge_hang_writes
             --test asi06_judge_hang_health --test asi06_layer1_ingest_memory_write
             -- -D warnings                                               → exit 0 (25,08 s)
cargo test -p tylluan-kernel --lib                                         → 1191 (42,14 s)
cargo test -p tylluan-kernel --lib                                         → 1191 (65,53 s)  [2ª corrida]
cargo test --test cola_d33_acceptance                                      → 3 passed / 0 ignored
cargo test --test asi06_judge_hang_writes                                  → 1 passed
cargo test --test asi06_judge_hang_health                                  → 1 ignored (lento por diseño)
python -m pytest tests/python/test_llama_backend_model_resolution.py \
                 tests/python/test_guild_security_fixes.py \
                 tests/python/test_injection_guard.py                      → 25 passed
```

Lib: base `a85f4d7` **1187** → **1191** (+3 de `judge_call_tests`, +1 de telemetría).
Aplicación en limpio (`E:\tylluan-patch-wt33` sobre `a85f4d7`):

```
git apply --check → 0 ; git apply → 0
CARGO_TARGET_DIR=… cargo check -p tylluan-kernel --tests → 0 (3m22s)
```

## 5. Pendiente / decisiones

1. **Calidad del juez** (segundo defecto de Buffy, 1/32): fuera de alcance por mandato
   explícito; no se cambia el modelo por defecto.
2. **`judge_model` en config**: la lectura config→cuerpo no tiene test (leer config
   global en un test paralelo es la misma trampa que documenté en D-32). Está cubierta
   la mitad del contrato: el cuerpo lleva `model` cuando se le pasa, y el guild resuelve
   el primero de `/v1/models`. La unión config→cuerpo queda **no verificada**.
3. **`warn!` único por arranque**: no hay captura de logs en la suite; verificado por
   lectura de código. Si se quiere un test, haría falta un `tracing` subscriber de test.
4. **D-33 no revive el juez solo**: arregla el plumbing. Con `coherence_gate_hybrid_enabled`
   en su valor por defecto (`false`) y sin backend de inferencia, la capa 2 seguirá sin
   emitir veredictos — ahora con `backend_error` y un `warn!` en vez de silencio.

## No verificado

- Sin kernel arrancado: ni `:47004` ni la DB real. Todo es `test_state`/in-memory o el
  guild real por stdio contra un backend falso local.
- LM Studio real: el falso reproduce su contrato (`400` sin `model`), no se probó el
  binario. Tampoco se probó un backend que anuncie `id` inválidos para chat.
- `GET /health?verbose=true` servido por HTTP: se verificó el JSON de
  `judge_telemetry().to_json()`, que es exactamente lo que el handler expone
  (`components.judge`), no la ruta HTTP completa.
- `scripts/check_no_predation.sh` no se puede correr (requiere commit); el patch toca
  `guilds/` y `tylluan.example.toml`, así que el commit deberá llevar `## Impact`.
- No se ejecutó `scripts/verify.sh` (10 min, TL).
