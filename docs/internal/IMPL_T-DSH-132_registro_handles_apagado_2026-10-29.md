# T-DSH-132 — Registro de `JoinHandle` de tareas de fondo para el apagado (implementación de T-DSH-131)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = be6ea74` (push192). Diff sin commitear: `main.rs` + `runtime/shutdown.rs`.
- Encargo (T2067): implementar T-DSH-131 — `BackgroundTask` + `WaitPolicy` + `wait_for(Vec<BackgroundTask>) -> (pending, skipped)`, registro en `main.rs` de las tareas de fondo **sin cambiar la lógica de ninguna tarea**, `Skip` para los bucles ONNX/SQLite de riesgo alto, `Wait`/`Drain` solo para one-shot; tests propuestos; con `shutdown_force_exit=false` la salida como hoy (2 corridas) y con `true` ~1 s. Clippy 0, lib 1409+tests, informe con `## Impact` y `## No verificado`.

## Implementación

### `runtime/shutdown.rs`

- `WaitPolicy { Wait, Drain, Skip }`.
- `BackgroundTask { name: &'static str, subruntime: &'static str, handle: JoinHandle<()>, policy }` con constructores `wait`/`drain`/`skip`.
- `ShutdownDeadline::wait_for(Vec<BackgroundTask>) -> (pending, skipped)`:
  - `Skip`: **no se espera ni se aborta** (R6); el handle se dropea y la tarea muere con el proceso.
  - `Wait`/`Drain`: se espera hasta el deadline; si no termina, va a `pending` (sin abortar).
- `force_exit_after_shutdown(pending, skipped, exit_code, force_exit)` loguea `pending`/`skipped` y corta con `process::exit` solo si `force_exit`.
- Tests: 6 en total (terminada no pendiente, lenta pendiente sin abort, `Skip` no espera, políticas mixtas, `timeout_ms=0`, rama `force_exit=false`).

### `main.rs`

- Macro `bg_spawn!(background_tasks, "nombre", "subruntime", WaitPolicy::X, fut)` que registra el `tokio::spawn` sin tocar su lógica.
- **27 tareas registradas** (líneas de la tabla de T-DSH-131): `nat-discovery`, `mailbox-listener`, `registry-actor`, `metrics-interval`, `doc-token-guard`, `ivf-consolidate-once`, `ivf-interval`, `version-check`, `always-on-guilds`, `warm-pool`, `model-download`, `external-mcps`, `coloquio-loop`, `blackboard-loop`, `seed-corpus`, `retro-link-5s`, `retro-link-20s`, `hormones`, `memory-decay`, `flywheel`, `reindexer`, `hnsw-rebuild`, `embedding-migrate`, `consensus`, `retro-link-10s`, `night-consolidation`, `maintenance`. No se registran HTTP server ni stdio (ya unidos por `wait_for_shutdown`) ni spawns por evento.
- Al apagar: se cancela también `doc_token` (token separado del maestro) para que `doc-token-guard` termine dentro del deadline; después `wait_for(background_tasks)` y `force_exit_after_shutdown(&pending, &skipped, …)`.
- Política final (tras la medición, ver abajo): `Skip` para los bucles y para todos los one-shot que pueden bloquear >~1 s; **`Wait` solo para `doc-token-guard`** (la única tarea cooperativa, ya con su token cancelado).

## Medición (kernel de PRUEBA 47091, DATA_DIR propio vacío, release propio)

`http_stop_s` = POST /admin/shutdown → `/health` deja de responder; `exit_s` = salida real.

### Primer intento (con la política del diseño: Wait/Drain en one-shot) — 3 corridas `force_exit=true`

| corrida | HTTP deja de aceptar | salida real |
|---|---|---|
| t132-force-1 | 1,02 s | 5,05 s |
| t132-force-2 | 1,01 s | 5,06 s |
| t132-force-3 | 1,01 s | 5,06 s |

Los one-shot `Wait`/`Drain` (siempre-on guilds, external-mcps, model-download, retro-link 20 s, seed-corpus, nat/version-check…) **consumen el deadline entero** (5 s): no terminan antes. Se ajustó la política: `Skip` para todos ellos y `Wait` solo para `doc-token-guard` (cancelando su token). No se aborta ninguna tarea (R6): simplemente no se esperan.

### Política final — `shutdown_force_exit=false` (por defecto, 2 corridas)

| corrida | HTTP deja de aceptar | salida real |
|---|---|---|
| t132b-default-1 | 1,02 s | 17,52 s |
| t132b-default-2 | 1,01 s | 14,81 s |

### Política final — `shutdown_force_exit=true` (3 corridas)

| corrida | HTTP deja de aceptar | salida real |
|---|---|---|
| t132b-force-1 | 1,02 s | **1,67 s** |
| t132b-force-2 | 1,01 s | **1,22 s** |
| t132b-force-3 | 1,01 s | **2,19 s** |

Comparación: T-DSH-123 (10,56-24,60 s), T-DSH-125 (11,37-16,44 s), T-DSH-130 sin handles (1,01-1,02 s con `force_exit=true`). Con el registro real y la política final, `force_exit=true` sale en **1,2-2,2 s** (≈1 s), y por defecto sigue como hoy (14,8-17,5 s).

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1411 passed; 0 failed; 4 ignored
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test contracts_test          → 5 passed
```

Lib: 1411 = 1409 de main + **2 tests nuevos** en `runtime::shutdown` (Skip no espera; políticas mixtas); los 4 restantes ya existían de T-DSH-130.

## Impact

- `wait_for` deja de recibir lista vacía: **27 tareas de fondo reales** con nombre estable y política, registradas sin cambiar la lógica de ninguna.
- Con `shutdown_force_exit=true` el proceso sale en **~1,2-2,2 s** (criterio de T-DSH-132 ≈1 s), frente a 14,8-17,5 s por defecto; el `pending`/`skipped` queda en el log por nombre.
- La cancelación sigue siendo cooperativa (R6): los `Skip` no se abortan, mueren con el proceso; el deadline solo acota la espera.
- Se descubrió midiendo que esperar one-shot aparentemente cortos consume el deadline entero; la política final prioriza el criterio de salida ~1 s y lo deja documentado.

## No verificado

- **Los one-shot pasan a `Skip`**: cualquier trabajo en vuelo de esas tareas (semillas, consolidación IVF, descarga de modelo, siempre-on de guilds, retro-link) **se abandona sin abortar** al apagar; no se ha medido si queda estado parcial (SQLite transaccional, pero el trabajo no se reanuda).
- Solo se registran tareas de `main.rs`; HTTP/stdio (unidos por `wait_for_shutdown`) y spawns por evento quedan fuera por diseño.
- `doc-token-guard` es la única `Wait`: se cancela `doc_token` en el apagado; no se ha probado qué pasa si esa tarea está a mitad de una unidad de trabajo.
- 2 corridas con `false` y 3 con `true`, release propio, sin carga real, sin guilds activos (`always_on=[]`); no se probó con embeddings en vuelo ni con clientes SSE abiertos.
- `process::exit` no ejecuta `Drop` (WAL/ONNX sin cierre ordenado; recuperación al siguiente arranque no probada).
- El registro cubre las 27 tareas de la tabla de T-DSH-131, pero no se verificó una a una que todas existan en todos los modos de arranque (p. ej. ramas condicionales de stdio/P2P).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004).
