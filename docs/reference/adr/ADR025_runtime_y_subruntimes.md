# ADR-025: Runtime de Tylluan y subruntimes — contrato único para los flujos de fondo

- **Estado:** ✅ **APROBADO por José (2026-10-29)** — borrador del Tech Lead (Claude), aprobado en la revisión de decisiones del 2026-10-29. Se implementa fase por fase (§9); T-DSH-130 y T-DSH-132 ya están en `main`.
- **Fecha:** 2026-10-28
- **Origen:** pregunta de José (2026-10-28): ¿debe Tylluan ser un runtime completo, con sus flujos organizados en runtime y subruntimes? Conecta con su motivo fundacional (`docs/architecture/tylluan_as_living_organism.md`): "todo Tylluan debe ser un pipeline, un mega workflow donde sus circuitos se conectan y crean flujos".
- **Insumos (todos en el repo):** `crates/tylluan-kernel/src/main.rs`, `guard.rs`, `memory/background_budget.rs`, `registry/supervisor.rs`, `registry/lifecycle.rs`, `router/scheduler/mod.rs`, `docs/architecture/DESIGN_cognitive_scheduler.md`, ADR-013, ADR-014, ADR-024, `docs/reference/research/2026-10-28_H42_diseno_reembed_b3_onnx_cpu_hermes.md`, `..._H43_...`, `..._H45_...`.
- **Ámbito:** cómo se declaran, se acotan, se detienen y se observan las tareas de fondo del kernel. **Fuera de ámbito:** el runtime de inferencia (ONNX frente a llama.cpp, ya decidido: no cambiar), el KV-cache (`unified_runtime_engine_addendum.md`) y el aislamiento de procesos de guilds (ADR-022/024).

---

## 1. Resumen

Hoy Tylluan ya tiene piezas de runtime, pero cada una resuelve su caso y no hay un contrato común. Este ADR propone **formalizar lo que existe**, no reescribirlo: un contrato declarativo por tarea de fondo y seis subruntimes como fronteras de módulo. Se introduce por fases, cada una con un criterio medible y todas apagadas por defecto hasta su aprobación (ADR-013).

## 2. Estado real verificado (árbol `1a24e82`)

| Medida | Valor | Comando |
|---|---|---|
| Ocurrencias de `tokio::spawn`, `spawn_blocking`, `thread::spawn` | 191 en 61 ficheros: 121 spawns reales de producción (43 + 78), 21 menciones en comentarios o cadenas y 49 en módulos de test | `grep -rn "tokio::spawn\|spawn_blocking\|thread::spawn" crates/tylluan-kernel/src --include=*.rs`; reparto en la Fase 0 (`2026-10-28_FASE0_inventario_spawns_hermes.md` y `..._dsh.md`) |
| Ficheros con `CancellationToken` | 3 (8 usos) | `grep -rln "CancellationToken" crates/tylluan-kernel/src` |
| Sitios con `GuardedTask` | 6, todos en `main.rs` | `grep -rn "GuardedTask::new" crates/tylluan-kernel/src` |
| Usuarios de `BackgroundBudget` | 9 | `grep -rn "BackgroundBudget" crates/tylluan-kernel/src` |
| Tamaño de `main.rs` | 2.673 líneas | `wc -l crates/tylluan-kernel/src/main.rs` |
| Cadencia del reindexer | 100 nodos cada 600 s, fijo en el código, sin clave de config | `main.rs`, línea 1919 y siguientes (H-42) |
| Avisos del guard del reindexer | 363 en 4 días (ronda de 100 nodos supera 120 s) | `kernel.log`, medido por Hermes en H-45 |
| Convergencia del reindexer | no converge: ~6.400 nodos pendientes, 4.493 nunca embebidos | H-42 y H-43 |
| Parada del kernel (2026-10-28) | tras el log "stopped" el proceso sigue vivo 10-33 s (8 corridas en reposo, media 14,1 s; T-DSH-123 en un kernel de prueba) y hasta 2 min en producción (dos observaciones del TL). La orden de parada tarda 0,01 s; el retardo es el desmontaje final del runtime. La hipótesis "espera a los lotes ONNX del reindexer" quedó **refutada** (con reindex en vuelo, 10-33 s, sin proporción) | `docs/reference/research/2026-10-28_T-DSH-123_apagado_kernel_47091.md`; causa concreta **no verificada** (¿sesión ONNX, hilos del pool bloqueante?) |
| Bucles de `main.rs` sin token de apagado | 27 de 28 | Fase 0 (Hermes) |

Piezas que ya existen y este ADR reutiliza: `GuardedTask` (timeout adaptativo), `BackgroundBudget` (semáforo de CPU de fondo), `supervisor`/`lifecycle` (guilds), Cognitive Scheduler (hoy solo observa, ADR-014) y el registro de puntos de inferencia (T-DSH-116).

## 3. Decisiones de José que este ADR respeta

- Pulir antes que reconstruir; el trabajo en curso no se detiene (`tylluan_as_living_organism.md`).
- Todo lo nuevo es opt-in y apagado por defecto, con declaración de impacto (ADR-013).
- ONNX se queda como runtime de embeddings; llama.cpp opcional y apagado por defecto.
- Toda capacidad nueva se expone en REST y en MCP.

## 4. Decisiones propuestas (ninguna aprobada)

### R1. Contrato declarativo de tarea de fondo
Cada tarea declara: `id`, subruntime dueño, disparador (intervalo o evento) con clave de configuración, clase de presupuesto, timeout con extensiones, cancelación cooperativa al apagar y contadores de telemetría (ejecuciones, duración, avisos del guard, saltos por presupuesto).

### R2. `TaskRegistry` mínimo
Un registro en el kernel que arranca, supervisa y detiene las tareas declaradas, reutilizando `GuardedTask` y `BackgroundBudget`. No sustituye al supervisor de guilds. Migración tarea a tarea, empezando por el reindexer.

### R3. Seis subruntimes como fronteras de módulo
Memoria (decay, reindex, HNSW, consenso, ciclo nocturno), Inferencia (puntos, pool ONNX, servidores efímeros, batcher), Procesos y guilds (supervisor, ciclo de vida, contención), Decisión (servicio, adaptadores, gate clínico), Coordinación (Coloquio, dispatch, flota) y Red (federación, link). Son agrupaciones de módulos con un trait común, no procesos separados.

### R4. Apagado ordenado, con drenaje y con tiempo máximo medible
Orden fijo al apagar: (1) el servidor HTTP deja de aceptar peticiones (`with_graceful_shutdown`); (2) el token maestro cancela todas las tareas registradas y se espera a cada una hasta un máximo configurable, registrando por nombre las que no respondan; (3) se **drenan la auditoría y las escrituras en vuelo** antes de cerrar los almacenes; (4) se paran los guilds activos (hoy `reap_idle` solo recoge los inactivos); (5) se borra el PID. Criterio: el proceso sale en menos de 10 s en un arranque normal (hoy 10-33 s, T-DSH-123); solo se baja de ahí si T-DSH-125 lo demuestra.

### R5. Observabilidad con superficie mínima (revisión de seguridad de Deep, D-67)
`GET /api/v1/runtime/tasks` solo REST, **solo lectura**, clase operador, con campos saneados (id, subruntime, estado, última ejecución, contadores; nunca rutas, agentes ni contenido). **No hay herramienta MCP nueva**: CONTRACT-01 fija seis tools; si hiciera falta verlo por MCP, va por `tylluan_do` o `tylluan_think`, o con una enmienda formal aprobada por José. El control de tareas (parar, arrancar) no se expone por HTTP en esta fase.

### R6. Cancelación cooperativa; nunca `abort()` sobre escrituras
Las tareas comprueban el token entre unidades de trabajo. `GuardedTask::abort` no cancela un `spawn_blocking` de SQLite o de ONNX; por tanto no se usa para interrumpir escrituras: se espera a que termine la unidad en curso.

### R7. Frontera con el supervisor existente
El `TaskRegistry` gobierna las tareas de fondo del kernel. `registry/supervisor.rs` y `registry/lifecycle.rs` siguen gobernando los procesos de guilds; el registro no los sustituye ni los duplica y solo comparte la señal de apagado.

## 5. Orden recomendado (propuesta, no decisión)

| Fase | Contenido | Criterio de paso (comando y salida esperada) |
|---|---|---|
| 0 | Inventario de solo lectura de los 123 spawns | tabla completa; la suma de filas coincide con el `grep` del §2 |
| 1 | `TaskRegistry` + reindexer migrado, con cadencia y tope configurables | avisos del guard del reindexer: 0 en 24 h; parada del proceso en menos de 10 s |
| 2 | Endpoint y herramienta MCP de solo lectura; vista en el dashboard | el endpoint lista las 6 tareas de `GuardedTask` |
| 3 | Resto de tareas y fronteras de subruntime, solo donde la evidencia lo pida | una tarea por PR, con su `## Impact` |

## 6. Alternativas descartadas

- **Reescribir `main.rs` de golpe:** riesgo alto, sin dato que lo exija.
- **Adoptar un marco externo de actores:** añade dependencia y no resuelve el contrato propio; se revisaría solo si la fase 1 no basta.
- **No hacer nada:** el reindexer seguiría sin converger y la parada sin tiempo acotado.

## 7. Consecuencias

- Positivas: cadencias y topes configurables sin recompilar, apagado acotado, un único sitio donde mirar qué corre.
- Costes: migración gradual, disciplina de declarar cada tarea nueva, y mantener un registro más.
- Riesgos: duplicar `GuardedTask` o `BackgroundBudget` en vez de reutilizarlos; convertirlo en framework antes de necesitarlo (YAGNI).

## 8. Plan de implementación y DoD por fase (si se aprueba)

Cada fase: diff sin commitear, `clippy -D warnings` en 0, tests propios, `## Impact` y apartado "No verificado". Ninguna fase activa nada por defecto.

## 9. Qué debe decidir José (punto por punto)

1. ¿Aprobar R1 y R2 (contrato y registro mínimo)? Recomendación: sí.
2. ¿Aprobar la fase 1 con el reindexer como primera tarea migrada? Recomendación: sí.
3. ¿Aprobar R3 solo como lenguaje y fronteras de módulo, sin procesos separados? Recomendación: sí.
4. ¿Prioridad frente al resto (seguridad, release v0.17.2, dashboard)? Decide José.

## 9b. Revisión independiente

Deep revisó este ADR (`docs/internal/REVIEW_ADR025_deep_2026-10.md`): recomendación **aprobar con cambios**. Los cinco cambios están incorporados en R4 a R7 y en las cifras del §2.

## 10. No verificado

- La causa exacta del retardo de 10-33 s al salir (T-DSH-123 descartó el reindexer; falta un volcado de hilos durante la espera). El criterio de la fase 1 pasa de 10 s a 5 s solo si T-DSH-125 lo demuestra.
- La clasificación fina por subruntime (la Fase 0 la hizo por lectura estática, sin instrumentar el kernel vivo).
- El coste de la fase 1 y si 10 s de parada es alcanzable sin tocar las guilds.
- Que `max_threads = 8` (config de producción desde 2026-10-28) reduzca los avisos del guard.
