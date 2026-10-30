# T-DSH-140 — `TaskRegistry` mínimo + `GET /api/v1/runtime/tasks` (ADR-025 R2)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = 8060a5b` (push202), HEAD confirmado `8060a5b` al empezar.
- Diff sin commitear (combinado con el de T-DSH-138 re-aplicado): 10 ficheros + 3 nuevos (`runtime/tasks.rs`, `api_v1/api_runtime.rs`, `tests/runtime_tasks.rs`).
- Encargo (T2120): código, ADR-025 aprobado: `TaskRegistry` mínimo + `GET /api/v1/runtime/tasks` solo lectura, clase operador, campos saneados, **sin tool MCP nueva**; reindexer como primera tarea migrada; defaults sin cambio.

## Implementación

### `runtime/tasks.rs` (nuevo)

- `TaskRegistry` con `RwLock<HashMap<&'static str, TaskEntry>>`:
  - `register(name, subruntime)` idempotente (estado `idle`).
  - `mark_running` (estado `running`, `runs += 1`, `last_run_unix = now`).
  - `mark_idle`, `mark_error` (`errors += 1`).
  - `snapshot() -> Vec<TaskSnapshot>` ordenado por nombre.
- `TaskSnapshot` serializable con **solo** `name`, `subruntime`, `state`, `last_run_unix`, `runs`, `errors` (sin rutas, agentes ni contenido).
- Registro global de proceso: `tasks::global()` con `OnceLock` (no toca `HttpState`; evita romper las construcciones de estado de los tests).

### `api_v1/api_runtime.rs` (nuevo) + ruta

- `runtime_tasks_handler`: `200 {"tasks": [...], "count": N}`.
- Ruta añadida al final del bloque de `routes.rs`: `GET /api/v1/runtime/tasks`.
- `auth::operator_class_required` incluye exactamente `/api/v1/runtime/tasks` (clase operador, como `/api/v1/decide/shadow` y `/api/v1/silva/conflicted`).
- **No** se añade ninguna herramienta MCP (CONTRACT-01: seis tools).

### `main.rs` — reindexer como primera tarea migrada

- `tasks::global().register("reindexer", "Memoria")` antes del `bg_spawn`.
- En cada tick, tras adquirir el `BackgroundBudget`: `mark_running("reindexer")`; al terminar el `GuardedTask`: `mark_idle("reindexer")`.
- Sin cambios de comportamiento: mismos 100 nodos/600 s (defaults), mismo guard, mismo presupuesto.

## Tests

- Unitarios (`runtime/tasks.rs`): registro idempotente + snapshot ordenado; contadores `running`/`idle`/`error`; el snapshot serializado **no** contiene `path`/`content`/`agent_id`.
- Integración (`tests/runtime_tasks.rs`): handler devuelve 200, `count >= 1`, el reindexer aparece con `subruntime=Memoria` y campos saneados; `operator_class_required("GET", "/api/v1/runtime/tasks") == true` y `/health` sigue siendo público.

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test runtime_tasks           → 2 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1431 passed; 0 failed; 4 ignored
```

Lib 1431 = 1425 de main + 3 tests de `runtime::tasks` + 3 tests de T-DSH-138 (2 config + 1 silva, diff re-aplicado tras el rebase; ver nota T2134).

## Impact

- Existe ya el `TaskRegistry` mínimo de ADR-025 R2 y su vista REST de solo lectura, con **clase operador** y campos saneados; el **reindexer es la primera tarea migrada** (registro + estado + contadores + última ejecución).
- Sin tool MCP nueva y **sin cambio de comportamiento por defecto** (mismos defaults de reindexer, mismo guard y presupuesto).
- La vista permite ver qué tarea está corriendo y cuántas veces/errores, sin exponer rutas ni contenido.

## No verificado

- El endpoint no se ha probado por **HTTP real** con autenticación (solo el handler directo y la función de clase); no se verificó la respuesta 401/403 de un cliente sin master token.
- Las transiciones del reindexer (`running`/`idle`) no se observaron en un kernel vivo; el test-run de T-DSH-139 está en marcha con otro binario y no se ha arrancado un kernel con este cambio.
- No hay consumo desde dashboard ni desde MCP (por diseño); no se probó la vista con más tareas (solo el reindexer está migrado).
- El registro es **en proceso**: se pierde al reiniciar (contadores no persistentes), por diseño de esta fase.
- No se ha medido el coste de `mark_*` (RwLock en cada tick, despreciable pero no medido).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); solo clippy + lib + test de integración en dev.
