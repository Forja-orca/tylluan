# T-DSH-109 — Backend de centinela OPT-IN para write_gate L2 (Qwen3Guard local)

- **Agente:** dsh
- **Fecha:** 2026-10-27
- **Base:** `origin/main = 2142e57` (push173). Diff sin commitear.
- **Encargo (T1912):** backend de centinela OPT-IN y **apagado por defecto** en el juez (write_gate L2): llama-server propio con el modelo Qwen3Guard Q4 indicado por config (ruta explícita, nunca descarga automática), `--threads <= 4`, sin `--mlock`, BELOW_NORMAL si se puede, carga bajo demanda y descarga tras inactividad, cola serie; política `Unsafe → cuarentena`, `Controversial → marca para revisión`, `Safe → pasa`; fallo/timeout del centinela ⇒ **no bloquea** (cae a las reglas L1/L2 existentes). Criterios: clippy `-D warnings` 0; lib sin regresión (cifra); tests con servidor falso (sin descargar nada) para las 3 etiquetas, timeout y apagado por defecto; `check_async_guild_io.py` limpio; informe con `## Impact` y no verificado; puertos como `<puerto>`.

## Implementación

- **Nuevo módulo** `crates/tylluan-kernel/src/security/sentinel.rs` (~470 líneas con tests):
  - `SentinelVerdict { Safe, Controversial, Unsafe, Unavailable }` y `classify_verdict()` (comprueba `unsafe` antes que `safe`, porque "unsafe" contiene "safe").
  - `SentinelAction { Quarantine, MarkReview, Pass, FallbackToExistingRules }` + `action_for()` — política pura y testeable.
  - `SentinelJudge`: `tokio::sync::Mutex` = **cola serie** (un veredicto a la vez); `ensure_server()` arranca `llama-server` **bajo demanda** con `-m <model_path>` explícito, `--host 127.0.0.1`, puerto propio (efímero si `port = 0`, **nunca 47004**), `-t min(threads,4)`, `--parallel 1`, `-c ctx_size`, `--no-webui`, **sin `--mlock`**, stdout/stderr a null, prioridad **BELOW_NORMAL** best-effort en Windows (`SetPriorityClass` vía winapi).
  - Espera real a `/health` (no solo puerto abierto) con deadline `timeout_ms`; si el proceso muere o no llega, `Unavailable` y fail-open.
  - Petición `POST /v1/chat/completions` con timeout `timeout_ms`; parsea `choices[0].message.content`; `last_used` actualizado. **Descarga**: si al entrar una llamada el servidor lleva más de `idle_unload_secs` inactivo, se mata y se rearranca solo si vuelve a usarse. `Drop` mata el hijo.
  - `global()` perezoso: lee `[security.sentinel]` una vez; si está apagado no arranca nada.
  - Binario: `llama_server_path` de config > `TYLLUAN_LLAMA_SERVER` > `llama-server(.exe)` del PATH.
- **Config** (`crates/tylluan-kernel/src/config.rs`): `SecurityConfig.sentinel: SentinelConfig` con `#[serde(default)]` y `Default` **apagado**:
  `enabled = false`, `model_path = None`, `llama_server_path = None`, `port = 0`, `threads = 4`, `ctx_size = 2048`, `timeout_ms = 20000`, `idle_unload_secs = 300`.
- **Wiring en write_gate L2** (`security/write_gate.rs`): `spawn_write_gate_judge` llama primero a `sentinel::global().judge(content)`:
  - `Unsafe` → `quarantined = 1` + `quarantine_reason` (cuarentena, como hoy);
  - `Controversial` → **solo** `quarantine_reason = 'T-DSH-109 centinela: Controversial (pendiente de revisión)'`, `quarantined` intacto (marca de revisión sin borrar ni bloquear);
  - `Safe` → no toca el nodo (pasa);
  - `Unavailable` (apagado/sin modelo/timeout/error) → **fail-open**: se ejecuta el camino anterior (juez por gramática vía `llama_backend`) exactamente como antes.
- **`tylluan.example.toml`**: bloque `[security.sentinel]` comentado con los 8 campos y la política.

## Verificación (final, tras normalizar EOL a LF)

```
cargo test -p tylluan-kernel --lib                    → 1315 passed; 0 failed; 4 ignored (41,70 s)
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → exit 0
python scripts/check_async_guild_io.py                → exit 0 (73 ficheros, limpio)
git diff --stat → config.rs +41, mod.rs +1, write_gate.rs +79, tylluan.example.toml +18 (+ sentinel.rs nuevo)
```

- **8 tests nuevos, todos en verde** (`security::sentinel`): `classify_three_labels_and_unknown`, `policy_maps_verdicts`, `default_config_is_disabled_and_never_spawns`, `fake_server_unsafe_is_quarantine`, `fake_server_controversial_is_review`, `fake_server_safe_passes`, `timeout_is_unavailable_never_blocks`, `serial_queue_handles_two_calls_in_order`.
- Los tests de servidor falso levantan un `tokio::net::TcpListener` efímero que responde JSON estilo OpenAI (3 etiquetas), uno que duerme para probar el timeout, y comprueban la cola serie con dos llamadas concurrentes. **No descargan nada** ni arrancan llama-server.
- Lib: 1315 = 1307 previos + 8 nuevos, sin regresión.

## Impact

- Añade la palanca que el TL pidió para el juez de escritura: **opt-in, apagada por defecto**, con modelo local explícito (sin descargas), cola serie, carga/descarga por inactividad y política de 2 niveles (Unsafe cuarentena / Controversial revisión) coherente con la evaluación T-DSH-108.
- **Fail-open** por diseño: si el centinela no está, falla o expira, el comportamiento es el actual (L1/L2); no puede bloquear escrituras por un fallo del juez.
- No toca timeouts existentes ni prod; puerto propio efímero; `--threads` acotado a 4; sin `--mlock`; BELOW_NORMAL best-effort.
- `Controversial` queda como marca en `quarantine_reason` (consultable por `LIKE 'T-DSH-109 centinela:%'`), sin migración de esquema.

## No verificado

- **No se ha arrancado un llama-server real con el GGUF de Qwen3Guard desde el kernel**: los tests usan servidor falso (a propósito, para no descargar nada). La ruta `ensure_server` (spawn, `/health`, BELOW_NORMAL, descarga) está compilada y testeada solo indirectamente; falta una prueba manual con `enabled = true` y `model_path` real (requiere que José/TL decidan activar el juez).
- **BELOW_NORMAL**: solo se compila en Windows y se aplica best-effort; no se ha comprobado con `GetPriorityClass` que el hijo quede efectivamente en esa clase.
- **Descarga por inactividad**: se evalúa al entrar la siguiente llamada (no hay reaper en background); si el kernel queda ocioso para siempre, el servidor sigue vivo hasta el siguiente `judge` o hasta el `Drop` del juez global (que no ocurre durante la vida del proceso). No medido el RSS real del modelo cargado en el kernel.
- **`Controversial` no tiene cola/UI de revisión**: solo marca `quarantine_reason`; no hay endpoint ni panel que liste esos nodos.
- **No hay validación al arranque** de `model_path`/binario: si faltan, el primer veredicto loguea y devuelve `Unavailable` (fail-open), no falla el boot.
- **No probado con el kernel de producción ni con el guild `llama_backend` real**: el centinela es un proceso propio del kernel, independiente del guild; no se ha medido convivencia con `llama_backend`/`llama-server` ya corriendo (puerto efímero evita colisión, pero no se ha ejercitado en vivo).
- No se ha medido latencia/RSS del centinela dentro del kernel (solo la evaluación offline de T-DSH-108: p50 ≈ 2,3 s/nota con Q4 en este host).
