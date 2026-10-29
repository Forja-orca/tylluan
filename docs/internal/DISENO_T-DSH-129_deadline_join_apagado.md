# T-DSH-129 — Diseño: deadline de join para tareas de fondo en el apagado

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = d995340` (push190) + diff sin commitear de T-DSH-125/124.
- Encargo (T2056): **solo diseño, sin código**; deadline de join para tareas de fondo, citando ADR-025 R4/R6. No compilar (silencio de carga).

## 1. Problema medido (T-DSH-123 + T-DSH-125)

Con el apagado actual, el orden de parada termina muy pronto pero el **proceso sigue vivo**:

| fase | antes (T-DSH-123, prod-build) | después (T-DSH-125, release propio) |
|---|---|---|
| POST /admin/shutdown → log "TylluanNexus stopped" | 0,01 s | 0,01 s |
| POST /admin/shutdown → HTTP deja de aceptar | hasta morir el proceso | **~1,0 s** |
| POST /admin/shutdown → salida real del proceso | 10,56-24,60 s (n=8, reposo) | **11,37-16,44 s** (n=3) |
| reproducción del TL en prod | log "stopped" 21:25:30, PID vivo 2 min después, 17 hilos, 19.388 s CPU | — |

El retardo restante está en el **desmontaje final del runtime** (join del blocking pool / workers con ONNX en curso), no en la parada ordenada: el `main` ya retornó y el runtime de Tokio espera a hilos que no puede cancelar. T-DSH-125 cumple su parte (HTTP deja de aceptar + guilds activos parados), pero no acota ese join.

## 2. Lo que ya fija ADR-025 (citado)

- **R4 (líneas 50-51)**: orden fijo — (1) HTTP deja de aceptar (`with_graceful_shutdown`); (2) el token maestro cancela las tareas registradas y **se espera a cada una hasta un máximo configurable**, registrando por nombre las que no respondan; (3) se drenan auditoría y escrituras en vuelo antes de cerrar almacenes; (4) se paran los guilds activos; (5) se borra el PID. **Criterio: salida <10 s en arranque normal** (hoy 10-33 s).
- **R6 (líneas 56-57)**: cancelación **cooperativa**; `GuardedTask::abort` **no** cancela un `spawn_blocking` de SQLite/ONNX; no se usa para interrumpir escrituras: se espera a que termine la unidad en curso.

La pieza que falta es el **"máximo configurable"** de R4 y el **corte duro** cuando se agota: hoy no hay deadline ni salida forzada, así que R6 se aplica de forma ilimitada.

## 3. Diseño propuesto

### 3.1 Pieza nueva: `ShutdownDeadline`

Módulo nuevo `crates/tylluan-kernel/src/runtime/shutdown.rs` (o extensión de `guard.rs`, decisión del TL):

```
ShutdownDeadline {
    token: CancellationToken,        // el maestro que ya existe
    deadline: Instant,               // now + [runtime].shutdown_timeout_ms
}
```

API mínima (sin cambios de comportamiento fuera del apagado):
- `fn start(timeout_ms) -> Self` — arranca el reloj al entrar en la secuencia de apagado.
- `async fn join_tasks(&self, tasks: Vec<(String, JoinHandle<()>)>) -> Vec<String>` — `timeout_at(deadline, join_all(tasks))`; devuelve los **nombres de las que no respondieron** (no las aborta, R6).
- `fn force_exit_if_pending(pending: &[String], exit_code: i32)` — si quedan pendientes: log estructurado `shutdown_timeout` con la lista, flush de tracing, y `std::process::exit(exit_code)`.

### 3.2 Registro de handles (fase 1 de ADR-025 R2)

Sin esperar al `TaskRegistry` completo, en el arranque se recogen los `JoinHandle` de las tareas de fondo que ya existen (`main.rs` los crea con `tokio::spawn` y hoy descarta el handle). Se guardan en un `Vec<(String, JoinHandle<()>)>` con nombre estable por tarea (`reindexer`, `memory-decay`, `night-consolidation`, `http-heartbeat`, `autoresearch`, `metrics-broadcaster`, `gossip`, `repo-map`, ...). Cuando llegue el `TaskRegistry` (R2), este vector pasa a ser su implementación; el deadline no cambia.

### 3.3 Secuencia de apagado (sustituye a la actual de `main.rs`)

1. **HTTP**: `with_graceful_shutdown(cancel_token)` (ya en T-DSH-125) — deja de aceptar en ~1 s.
2. **Cancelar** el token maestro (ya existe).
3. **Join con deadline**: `join_tasks` hasta `[runtime].shutdown_timeout_ms`; se registran por nombre las pendientes.
4. **Drenar auditoría/escrituras** (R4 paso 3): llamada síncrona con su propio presupuesto corto (p. ej. 500 ms) a los buffers de auditoría; si no drena, se registra y se sigue (nunca bloquear indefinidamente).
5. **Parar guilds activos** (ya en T-DSH-125, dentro del mismo deadline: si `kill_guild` se cuelga, cuenta para el deadline).
6. **Borrar PID** (ya existe).
7. **Si quedan tareas pendientes**: log `shutdown_timeout` con nombres + `std::process::exit(0)` (o `1` si `http_failure`, como hoy). Si no quedan: retorno normal (sin `process::exit`).

### 3.4 Configuración propuesta (`tylluan.toml`)

```
[runtime]
shutdown_timeout_ms = 5000   # máximo de espera de join antes del corte duro (R4)
shutdown_force_exit = true   # corte duro al agotar el deadline
```

- `shutdown_timeout_ms`: 0 = sin corte (comportamiento actual), para poder medir A/B.
- `shutdown_force_exit`: separado para que el TL/José pueda activar el deadline sin el `process::exit` (solo medir), o ambos.
- Nota ADR-013: es teardown, no una capacidad nueva de runtime; aun así el valor por defecto debe decidirlo el TL. Propuesta: `5000`/`true` para cumplir R4 (<10 s) sin tocar el comportamiento en caliente.

### 3.5 Riesgos y mitigaciones

| riesgo | mitigación |
|---|---|
| `process::exit` no ejecuta `Drop`: sesión ONNX no se libera, WAL no se checkpointea | SQLite WAL es recuperable al siguiente arranque; ONNX se libera al morir el proceso. Aceptable en apagado. |
| Abandonar un `spawn_blocking` a mitad de escritura | R6: no se aborta; se abandona **solo al agotar el deadline**, tras el drenaje de auditoría. Las escrituras SQLite son transaccionales (o commit o rollback). Embeddings son idempotentes: el reindexer los rehará. |
| Auditoría perdida si el deadline vence antes de drenar | drenaje síncrono con presupuesto propio **antes** del corte; si falla, se loguea (nunca se espera indefinidamente). |
| Guilds hijo huérfanos si el deadline vence durante `kill_guild` | T-DSH-125 ya los para explícitamente; el deadline los incluye y el log nombra el guild pendiente. En Windows el proceso hijo muere con el padre solo si no se desasocia; verificar en implementación. |
| Matar el proceso con peticiones HTTP en vuelo | HTTP deja de aceptar en ~1 s; las conexiones vivas (SSE) pueden alargar el cierre: el deadline las cubre y se loguea. |
| `process::exit` desde otro hilo mientras main retorna | el corte duro se hace solo desde el hilo de `main` tras el join fallido; no hay doble salida. |

## 4. Tests propuestos (para la implementación, no ahora)

1. **Unitario**: `join_tasks` con una tarea que duerme 30 s y deadline 200 ms devuelve su nombre como pendiente y **no** la aborta.
2. **E2E con test-run** (montaje T-DSH-123/125): tarea de fondo que duerme 30 s → el proceso sale en `shutdown_timeout_ms + margen` (≤ 7 s con 5 s) y el log contiene `shutdown_timeout` con el nombre.
3. **E2E normal**: sin tareas colgadas, salida ≤ 5 s (objetivo de T-DSH-125) y sin `process::exit` (se puede comprobar por el log de cierre).
4. **Auditoría**: contar líneas de auditoría antes/después de un shutdown con deadline; no debe perderse ninguna escrita ya confirmada.
5. **Guilds**: arrancar un guild real (test-run), apagar, comprobar que no queda proceso hijo.
6. **Regresión**: `[runtime] shutdown_timeout_ms = 0` reproduce el comportamiento actual (10-33 s) — sirve de control A/B.

## 5. No verificado

- **Diseño únicamente**: no hay código ni tests; los tiempos citados son de T-DSH-123/125 y de la reproducción del TL, no de este diseño.
- No se ha comprobado en vivo si `std::process::exit` tras el deadline reduce la salida a <10 s: es la hipótesis de diseño (el retardo observado está después de que `main` retorne, así que el corte duro debería eliminarlo; falta medirlo).
- No se ha medido el coste del drenaje de auditoría (presupuesto 500 ms es una propuesta, no un dato).
- No se ha verificado el comportamiento de `process::exit` con WAL de SQLite ni con hilos de ONNX en Windows (recuperación al siguiente arranque).
- La lista de tareas con nombre estable depende de qué handles se recojan en el arranque; el inventario de FASE 0 (T-DSH-128/191 filas) es la base, pero no se ha mapeado 1:1 todavía.
