# T-DSH-130 — Implementación de T-DSH-129: deadline de join y corte duro opcional (ADR-025 R4/R6)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = 0441cda` (push191). Diff sin commitear: 4 ficheros + `runtime/shutdown.rs` nuevo.
- Encargo (T2060): implementar T-DSH-129 con `[runtime] shutdown_timeout_ms=5000` y `shutdown_force_exit=false` **por defecto** (activarlo lo decide el TL tras la medición); **primero medir la hipótesis** (el retardo ocurre tras el retorno de `main`) con `force_exit=true` en kernel de prueba, 5 corridas, y reportar antes/después; tests propuestos del diseño. Clippy 0, lib 1403 + tests, informe con `## Impact` y `## No verificado`.

## Implementación (diff sin commitear)

### Config (`config.rs`)

- Nuevo `[runtime]` → `RuntimeConfig { shutdown_timeout_ms: u64 = 5000, shutdown_force_exit: bool = false }`; campo `runtime` en `TylluanConfig` (con `#[serde(default)]`, por lo que un `tylluan.toml` sin la sección sigue funcionando).
- `0` en `shutdown_timeout_ms` = sin deadline (control A/B / comportamiento histórico).
- `tylluan.example.toml`: documentadas ambas claves.

### Módulo `runtime/shutdown.rs` (nuevo)

- `ShutdownDeadline::start(timeout_ms)` — reloj del apagado.
- `async fn wait_for(Vec<(String, JoinHandle<()>)>) -> Vec<String>` — espera hasta el deadline y devuelve los **nombres de las tareas que no respondieron sin abortarlas** (R6). Las tareas ya terminadas se consumen sin coste.
- `force_exit_after_shutdown(pending, exit_code, force_exit)`:
  - `force_exit=false`: no hace nada (solo `warn` si hubiera pendientes) → comportamiento actual.
  - `force_exit=true`: log `shutdown_force_exit` con pendientes + `std::process::exit(exit_code)` (0 normal, 1 si falló el HTTP).

### `main.rs`

Tras la secuencia ordenada (HTTP graceful, cancelación, parada explícita de guilds, cierre de servicios y borrado del PID), se crea el `ShutdownDeadline` con `config.runtime.shutdown_timeout_ms`, se espera a las tareas registradas y se llama a `force_exit_after_shutdown`. **Hoy `main` no conserva los `JoinHandle`** de las tareas de fondo (los `tokio::spawn` no se guardan; eso llega con el `TaskRegistry`, ADR-025 R2), así que la lista va vacía: el deadline acota la espera y el corte duro es lo que elimina el retardo del desmontaje del runtime.

## Medición de la hipótesis (kernel de PRUEBA, montaje T-DSH-123/125)

Binario release propio del diff (`CARGO_TARGET_DIR=E:/cargo-target/dsh`), puerto 47091, DATA_DIR propio vacío, config de t123 + `[runtime]`. `http_stop_s` = POST /admin/shutdown → `/health` deja de responder; `exit_s` = salida real.

### `shutdown_force_exit = true` (5 corridas)

| corrida | boot | HTTP deja de aceptar | salida real |
|---|---|---|---|
| t130-1 | 67,6 s | 1,02 s | **1,02 s** |
| t130-2 | 31,9 s | 1,01 s | **1,01 s** |
| t130-3 | 23,3 s | 1,01 s | **1,01 s** |
| t130-4 | 24,3 s | 1,01 s | **1,01 s** |
| t130-5 | 21,5 s | 1,01 s | **1,01 s** |

### `shutdown_force_exit = false` (por defecto, 2 corridas)

| corrida | HTTP deja de aceptar | salida real |
|---|---|---|
| t130-default-1 | 1,02 s | 14,50 s |
| t130-default-2 | 1,01 s | 11,37 s |

### Antes

- T-DSH-123 (prod-build, reposo, n=8): salida **10,56-24,60 s**.
- T-DSH-125 (HTTP graceful + guilds, release propio, n=3): **11,37-16,44 s**.

### Lectura

**Hipótesis CONFIRMADA**: el retardo no está en la secuencia ordenada (que termina en ~1 s) sino **después del retorno de `main`**, en el desmontaje del runtime (hilos de ONNX/SQLite que Tokio no puede cancelar). Con `force_exit=true` la salida cae a **~1,0 s**; con el valor por defecto (`false`) se mantiene el comportamiento actual (11-14 s). El criterio de R4 (<10 s) y el objetivo de T-DSH-125 (<5 s) se cumplen **solo con la clave activada**; el default no cambia nada.

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1409 passed; 0 failed; 4 ignored
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test contracts_test          → 5 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test contracts_audit         → 7 passed
```

Lib: 1409 = 1403 de main + **6 tests nuevos** (4 de `runtime::shutdown`: tarea terminada no pendiente, tarea lenta se reporta sin abortar, `timeout_ms=0` no expira, rama `force_exit=false` no sale; 2 de `RuntimeConfig`: defaults 5000/false y parseo de `[runtime]` desde TOML).

## Impact

- El apagado tiene ya la pieza que faltaba de ADR-025 R4: **deadline de join configurable** y **corte duro opcional**. Con `shutdown_force_exit=true` el proceso sale en **~1 s** (antes 10-33 s; >2 min en prod), cumpliendo el criterio de R4 y el objetivo de T-DSH-125.
- Por defecto **no cambia nada** (`false`): el operador/TL decide cuándo activarlo, tal como pediste; y `shutdown_timeout_ms=0` reproduce exactamente el comportamiento histórico para A/B.
- La cancelación sigue siendo cooperativa (R6): el deadline no aborta tareas; solo deja de esperar y, si el operador lo activó, corta el proceso tras la secuencia ordenada.

## No verificado

- **La lista de tareas del deadline va vacía hoy**: `main` no conserva los `JoinHandle` (los ~39 `tokio::spawn` no se guardan). El `wait_for` está implementado y testeado, pero en producción no espera a nada hasta que exista el `TaskRegistry` (ADR-025 R2). El corte duro es lo que se midió.
- **Drenaje de auditoría/escrituras no implementado**: no hay cola ni API de flush (las escrituras de auditoría son `spawn_blocking` fire-and-forget); el `process::exit` puede abandonar alguna escrita en vuelo, igual que hoy. Queda como deuda explícita del diseño (R4 paso 3).
- `process::exit` no ejecuta `Drop`: WAL de SQLite y sesión ONNX no se cierran ordenadamente; **no se probó la recuperación al siguiente arranque** ni el comportamiento en Windows con hilos de ONNX vivos.
- 5 corridas con `force_exit=true` y 2 con `false`, un binario release propio, sin carga real; boot variable 21-68 s. No se repitió con embeddings en vuelo (T-DSH-123 ya mostró que el patrón es el mismo).
- No se ejercitó la parada de guilds activos (test-run sin guilds, `always_on=[]`) ni conexiones SSE vivas durante el corte.
- No se tocó `E:\tylluan` ni el kernel de producción (:47004).
