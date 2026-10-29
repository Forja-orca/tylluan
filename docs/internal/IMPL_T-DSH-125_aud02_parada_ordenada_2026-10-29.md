# T-DSH-125 — AUD-02: parada ordenada del HTTP y parada explícita de guilds activos

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = d995340` (push190). Diff sin commitear: 2 ficheros.
- Encargo (T2025/T2048): AUD-02 — `axum::serve` sin `with_graceful_shutdown` (`http/mod.rs` 502) y `reap_idle` solo recoge guilds inactivos (`main.rs` 2223): el HTTP debe dejar de aceptar antes del teardown y hay que parar explícitamente los guilds activos; medir antes/después con el montaje de T-DSH-123.

## Cambios (diff sin commitear)

1. **`crates/tylluan-kernel/src/transport/http/mod.rs`**: el token de apagado se clona antes de `build_router(state)` y `axum::serve(...)` pasa a `.with_graceful_shutdown(async move { shutdown_token.cancelled().await })`. El HTTP deja de aceptar conexiones en cuanto se cancela el token (POST /admin/shutdown o fallo del transporte), en vez de seguir aceptando hasta que el proceso muere.
2. **`crates/tylluan-kernel/src/main.rs`**: antes de `reap_idle()` se recorren `registry.status_all()` y se llama `kill_guild()` a **todos** los guilds que sigan vivos (con warn por guild fallido y log del número parado); `reap_idle()` se mantiene después como red de seguridad. Antes solo se recogían los inactivos.

## Medición antes/después (montaje T-DSH-123)

Binario: build release propio desde el diff (`CARGO_TARGET_DIR=E:/cargo-target/dsh`), puerto 47091, DATA_DIR propio vacío, config de t123 (dev_mode, bge-m3, mesh/p2p/federation off, sentinel off, guilds always_on=[]). 3 corridas. Se mide: (a) `http_stop_s` = tiempo desde POST /admin/shutdown hasta que `/health` deja de responder; (b) `exit_s` = hasta la salida real del proceso.

| corrida | boot | HTTP deja de aceptar | salida real | criterio <5 s |
|---|---|---|---|---|
| t125-1 | 73,0 s | **1,03 s** | 16,44 s | NO |
| t125-2 | 28,8 s | **1,01 s** | 11,37 s | NO |
| t125-3 | 26,1 s | **1,01 s** | 11,66 s | NO |

Antes (T-DSH-123, binario prod-build, mismas condiciones de reposo): HTTP seguía aceptando hasta la muerte del proceso, y la salida real era **10,56-24,60 s** (media 14,1 s, n=8).

## Veredicto

- **El HTTP ya deja de aceptar ~1,0 s después del shutdown** (antes: seguía aceptando hasta el final). El objetivo de esa parte de AUD-02 se cumple.
- **La parada explícita de guilds activos está implementada**, pero en este montaje no había guilds activos (`always_on=[]`), así que el camino `kill_guild` no se ejercitó en vivo: queda como no verificado (ver abajo).
- **La salida del proceso sigue en 11-16 s, no <5 s.** El retardo no está en la parada ordenada (que ahora termina en ~1 s) sino en el desmontaje final del runtime/proceso, como ya midió T-DSH-123 (ONNX/ORT y join de hilos del blocking pool/workers). Para llegar a <5 s haría falta un paso adicional (p. ej. `std::process::exit(0)` tras el teardown ordenado, o abortar/join con deadline las tareas de fondo), que **no** entraba en el alcance de AUD-02.

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1403 passed; 0 failed; 4 ignored
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test contracts_test          → 5 passed
```

## Impact

- El apagado deja de aceptar tráfico HTTP ~1 s después de la petición de shutdown: los clientes reciben conexión rechazada en vez de quedarse colgados mientras el proceso muere (antes podían seguir entrando peticiones durante 10-33 s).
- Los guilds activos se paran explícitamente antes del cierre, en vez de depender de `reap_idle` (que solo mira inactivos): reduce el riesgo de procesos hijo huérfanos al apagar.
- No cambia el comportamiento normal ni la configuración; el token ya existía y se reutiliza.

## No verificado

- **La salida real sigue por encima del criterio <5 s (11-16 s)**: el retardo es el desmontaje del runtime (T-DSH-123), no la parada ordenada; haría falta un paso extra fuera de AUD-02.
- **La parada explícita de guilds no se ejercitó en vivo**: el test-run no tenía guilds activos (`always_on=[]`); el código sigue el patrón ya usado por `admin_emergency_kill_handler` (`status_all` + `kill_guild`), pero no se midió con guilds reales arrancados.
- Medición con 3 corridas, un binario release propio (no el prod-build) y sin carga real; boot variable 26-73 s por carga de ONNX/reranker.
- No se probó el caso de un cliente SSE/keep-alive abierto durante el shutdown (con `with_graceful_shutdown`, las conexiones abiertas pueden alargar la parada del servidor HTTP; el `http_stop_s` mide aceptación nueva, no el cierre de conexiones vivas).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004).
