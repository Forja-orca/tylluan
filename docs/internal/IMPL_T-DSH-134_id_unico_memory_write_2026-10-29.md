# T-DSH-134 — `node_id` único (uuid) en `memory_write_handler` + regresión de concurrencia

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = efc2f40` (push196), `git rev-parse --short HEAD` = `efc2f40` al empezar.
- Diff sin commitear: `crates/tylluan-kernel/src/transport/http/api_v1/api_memory.rs` (+42/−1) y nuevo `crates/tylluan-kernel/tests/memory_write_concurrency.rs`.
- Encargo (T2088): id único (uuid) en `memory_write_handler` + test de regresión con N escrituras concurrentes que falle antes y pase después; clippy `-D warnings` y lib test; informe con `## Impact` y `## No verificado`.

## Cambio

En `api_memory.rs`:

- Antes: `let node_id = format!("memory_write:{}", chrono::Utc::now().timestamp_millis());`
  Dos escrituras concurrentes en el mismo milisegundo compartían id; la segunda **sobrescribía** a la primera y ambas devolvían 200 (hallazgo de T-DSH-133, verificado por el TL en código).
- Ahora: helper `new_memory_write_node_id()` → `format!("memory_write:{}", uuid::Uuid::new_v4())` (uuid v4; `uuid` ya era dependencia del kernel con feature `v4`).
- Sin otros cambios de comportamiento: el handler sigue igual (capa 1 ASI06, `upsert_node_with_validity`, juez capa 2, respuesta 200 con `node_id`).

## Tests

### Unitarios (`api_memory.rs`, módulo `t134_memory_write_id_tests`)

1. `ids_are_unique_in_a_tight_loop`: 1000 llamadas seguidas al generador → 1000 ids únicos.
2. `ids_are_unique_under_concurrent_calls`: 8 hilos × 200 llamadas → 1600 ids únicos.

### Integración (`tests/memory_write_concurrency.rs`)

3. `concurrent_memory_writes_get_unique_node_ids`: 8 llamadas **concurrentes** a `memory_write_handler` con `HttpState` real (harness `support::test_state`); cada una debe devolver 200 y un `node_id` distinto (8 únicos).
4. `memory_write_returns_ok_and_node_id`: escritura simple devuelve 200 y un `node_id` con prefijo `memory_write:`.

### Evidencia rojo → verde

Con el generador anterior (`timestamp_millis`) los dos tests unitarios **fallan** (ejecutado temporalmente para verificarlo):

```
thread '...ids_are_unique_under_concurrent_calls' panicked:
assertion `left == right` failed: 8x200 node_id concurrentes deben ser unicos
test result: FAILED. 0 passed; 2 failed; ...
```

Restaurado uuid: ambos pasan (`2 passed`). El test unitario es determinista (1000/1600 llamadas en el mismo instante colisionan con millis); el de integración es la comprobación a nivel handler.

## Verificación

```
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib                          → 1420 passed; 0 failed; 4 ignored
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --lib t134                     → 2 passed
CARGO_TARGET_DIR=E:/cargo-target/dsh cargo test -p tylluan-kernel --test memory_write_concurrency → 2 passed
```

Lib 1420 = 1418 de main + **2 tests unitarios nuevos**.

## Impact

- Dos (o más) escrituras concurrentes de `POST /api/v1/memory/write` ya **no comparten `node_id`**: antes una pisaba a la otra y ambas respondían 200 (pérdida silenciosa de contenido); ahora cada una crea su nodo con uuid v4.
- El formato del id sigue siendo `memory_write:<id>`; no se encontró ningún consumidor en `crates/` que dependa de que el sufijo sea un timestamp (grep de `memory_write:` sin usos aparte del generador), así que el cambio es compatible.
- Regresión cubierta: unitaria determinista + integración a nivel handler con 8 escrituras concurrentes.

## No verificado

- No se probó con carga real ni con más de 8 escrituras concurrentes; el test de integración verifica ids únicos en la respuesta, **no** cuenta los nodos finales en `silva.db` (el unitario cubre la unicidad del generador, que es la causa raíz).
- El fallo del test unitario con la implementación anterior se comprobó en esta máquina con 8×200 hilos; con otro planificador el número exacto de colisiones puede variar (la unicidad con uuid no depende de eso).
- No se ha probado el comportamiento con `node_id` duplicado si el cliente reintenta la misma petición (idempotencia no existe: cada llamada crea un nodo nuevo, como antes).
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); no se compiló en release para esta tarea (solo clippy + lib + tests de integración en dev).
