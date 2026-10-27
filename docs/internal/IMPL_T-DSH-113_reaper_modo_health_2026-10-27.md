# T-DSH-113 — Reaper con peticiones en vuelo, modo de 3 estados y bloque `/health` del centinela

- **Agente:** dsh
- **Fecha:** 2026-10-27
- **Base:** `origin/main = 4da459a` (push175). Diff sin commitear.
- **Encargo (T1923):** (1) **reaper**: no descargar con peticiones en vuelo; contador in-flight y `last_used` al **empezar** y al terminar cada petición, con test que reproduzca (cola larga + idle corto: antes falla, después pasa); (2) **modo de 3 estados** en `[security.sentinel]`: `mode = "off" | "observe" | "enforce"` (`enabled=true` equivale a `enforce`, compatibilidad); en `observe` el centinela puntúa y **registra** (tracing + contador) pero NO pone cuarentena ni marca; fail-open igual; test por estado; (3) **`/health`**: bloque `sentinel {mode, loaded, verdicts{safe,controversial,unsafe,unavailable}, fallbacks}`, sin tokens ni rutas. Criterios: clippy `-D warnings` 0; lib sin fallos (total); `tylluan.example.toml` documenta `mode`; NO tocar timeouts a la baja; no cambiar el comportamiento por defecto (off); diff sin commitear + informe.

## 1) Reaper: no descargar con peticiones en vuelo

- **Bug real** (detectado en T-DSH-112): el reaper de T-DSH-110 miraba solo `last_used`; con una cola larga y `idle_unload_secs` corto podía descargar el `llama-server` **en mitad de la cola** (visto en vivo: `descargo llama-server tras 173s inactivo` con peticiones pendientes), provocando rearranques en frío y fail-open en cadena.
- **Fix** (`security/sentinel.rs`):
  - `in_flight: AtomicUsize` en `SentinelJudge`, con `InFlightGuard` que incrementa al entrar en `judge()` y decrementa al salir (incluso si el future se cancela).
  - `last_used` se actualiza **al empezar** la petición (además de al terminar), así el tiempo de una cola larga no cuenta como inactividad.
  - Función pura `should_unload_idle(alive, in_flight, idle_secs, idle_elapsed_secs)` = `alive && in_flight == 0 && idle_elapsed > idle_secs`; la usan tanto el reaper como el chequeo dentro de `judge()`.
  - El reaper ahora **no** descarga si hay peticiones en vuelo.
- **Test de reproducción**: `should_unload_idle_respects_in_flight_requests` — con `in_flight = 1/5` e inactividad vencida devuelve **false** (antes el criterio solo miraba `alive + idle`, así que habría descargado); sin in-flight y con idle vencido devuelve true; apagado o idle no vencido, false.

## 2) Modo de 3 estados

- **Config** (`config.rs`): `SentinelConfig.mode: Option<String>` (nuevo, `serde(default)`), documentado; `Default` = `None`.
  - `resolve_mode(enabled, mode)`: `mode` explícito manda (`off`/`observe`/`enforce`, case-insensitive); `None` → `enabled=true` ⇒ **Enforce**, `enabled=false` ⇒ **Off** (compatibilidad total con T-DSH-109); valor inválido ⇒ `Off` + `warn` (nunca activa por accidente).
  - `SentinelMode::judges()` (`observe`/`enforce` puntúan) y `mode_applies_action()` (`solo enforce` aplica).
- **Wiring** (`write_gate.rs`): en `spawn_write_gate_judge`:
  - `Off` → **no** llama al centinela; comportamiento de siempre (juez por gramática vía `llama_backend`).
  - `Observe` → puntúa (con la capa de decodificación de T-DSH-111), **registra** `tracing::info!` con el veredicto y **no aplica** cuarentena ni marca; si el veredicto es `Unavailable` → fail-open igual que en enforce (cuenta `fallbacks` y cae a las reglas L1/L2).
  - `Enforce` → comportamiento de T-DSH-109/110/111 (Unsafe → cuarentena; Controversial → marca; Unavailable → fallback).
- **Contadores** en `SentinelJudge` (`verdicts[safe|controversial|unsafe|unavailable]`, `fallbacks`, `in_flight`) para `/health`.
- **Tests por estado**: `resolve_mode_three_states_and_compat` (incluye `enabled=true`→Enforce, `mode` explícito gana, inválido→Off), `observe_scores_but_does_not_enforce`, y `observe_mode_counts_verdicts_but_does_not_apply_actions` (servidor falso: `judge` devuelve Unsafe, `stats.unsafe_verdicts == 1`, `mode_applies_action == false`, `in_flight == 0`).
- **`tylluan.example.toml`**: documenta `mode = "off" | "observe" | "enforce"` y que `enabled=true` sin `mode` equivale a `enforce`.

## 3) Bloque `sentinel` en `/health`

- `HealthResponse` (`transport/http/mod.rs`) gana `sentinel: Option<serde_json::Value>` con `skip_serializing_if` (si es `None` se omite, para no romper el contrato JSON); el handler por defecto lo rellena con `sentinel::health_json()`.
- `health_json()`: `{ mode, loaded, verdicts {safe, controversial, unsafe, unavailable}, fallbacks, in_flight }` — **sin tokens ni rutas** (test `health_json_has_required_keys_and_no_paths_or_tokens`).
- **Contrato**: `schemas/health_response.json` añade la propiedad `sentinel` (objeto con esos campos, `additionalProperties: false`); `contracts_test` actualizado (`sentinel: None`) y **5/5 tests del contrato en verde**.
- **Verificación en vivo** (kernel test-run con `mode="observe"`, puerto/DATA_DIR propios, BM25-only para arrancar rápido; solo PID exacto):
  - `/health` inicial: `{"fallbacks":0,"in_flight":0,"loaded":false,"mode":"observe","verdicts":{"controversial":0,"safe":0,"unavailable":0,"unsafe":0}}`
  - Tras escribir una nota maliciosa conocida: `loaded=true`, `verdicts.controversial=1`, `fallbacks=0`, `in_flight=0`; log `[T-DSH-113] centinela observe: veredicto Controversial para nodo … (no se aplica)`; nodo en BD `quarantined=0`, `reason=NULL` ⇒ **observe puntuó y registró sin marcar**.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → exit 0
cargo test -p tylluan-kernel --lib                          → 1330 passed; 0 failed; 4 ignored (53,91 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed; 0 failed
python scripts/check_async_guild_io.py                      → exit 0 (73 ficheros, limpio)
git diff --stat → config.rs +4, sentinel.rs +250, write_gate.rs +29, transport/http/mod.rs +5,
                  contracts_test.rs +1, schemas/health_response.json +21, tylluan.example.toml +3
```

Lib: 1330 = 1325 previos + **5 tests nuevos** (los 8 de sentinel pasan a 13). Timeouts intactos (no se bajó ninguno); comportamiento por defecto sin `mode` = **Off** (idéntico a T-DSH-109).

## Impact

- **Cierra el bug del reaper** que T-DSH-112 encontró en vivo: ya no puede descargar el servidor con peticiones en vuelo, y `last_used` refleja el inicio de la petición (no solo el final), con test que reproduce el fallo.
- **Modo de 3 estados**: permite desplegar el centinela en `observe` para medir FP/recall en producción sin marcar ni cuarentenar nada (el TL podrá decidir con datos), manteniendo `off` por defecto y `enforce` como antes.
- **`/health`** expone el estado del centinela (modo, cargado, contadores de veredictos, fallbacks) sin filtrar tokens ni rutas, con contrato JSON actualizado y verificado en vivo.
- Sin tocar timeouts a la baja; sin cambios de comportamiento por defecto; sin CLI/MCP/REST/dashboard ni toggle en caliente (fuera de alcance, T-DSH-114).

## No verificado

- **El reaper no se re-probó en vivo con una cola larga + idle corto**: el fix está cubierto por la función pura y su test, pero no se ha reproducido el escenario completo en un kernel (haría falta forzar idle bajo y cola; en T-DSH-112 se subió el idle para medir).
- **`in_flight` cuenta llamadas a `judge()`**, no peticiones HTTP: es la semántica que necesita el reaper (una nota puede hacer 1–4 llamadas), pero no es "peticiones de usuario".
- **Verificación en vivo solo del modo `observe`** (y con BM25-only para arrancar rápido); `enforce` en vivo está verificado en T-DSH-110/112 y `off` es el comportamiento por defecto anterior, pero no se repitió un run live de `enforce`/`off` en este ciclo.
- **`/health` incluye `in_flight`** además de los campos pedidos (útil para diagnosticar el reaper); si el TL prefiere el contrato exacto, se quita.
- **`schemas/health_response.json` cambia**: el contrato ahora permite el bloque `sentinel`; CI/consumidores del schema deben aceptarlo (los tests del contrato pasan, pero no se ha probado contra consumidores externos).
- **No hay toggle en caliente** del modo (T-DSH-114) ni exposición por CLI/MCP/REST/dashboard.
- No se ha medido el coste de `observe` en producción (sí el de `enforce` en T-DSH-112); `observe` paga las mismas llamadas al modelo que `enforce`.
- El test-run de `/health` usó `[memory] embedding_model = ""` (BM25-only) para arrancar en ~18 s; no se repitió con embeddings ONNX cargados.
