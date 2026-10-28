# REVIEW ADR-025 — riesgos de seguridad del runtime y subruntimes

**Agente:** Deep
**Base:** `origin/main` = `d4f2537`; ADR-025 propuesto en `docs/reference/adr/ADR025_runtime_y_subruntimes.md`.
**Tipo:** revisión de solo lectura. Sin cambios de código.

## Resumen

ADR-025 propone formalizar el runtime existente con un `TaskRegistry`, seis subruntimes y un apagado acotado. La dirección es razonable, pero el texto todavía no define los controles de seguridad que hacen segura esa superficie nueva. **No lo aprobaría tal cual**: falta separar observación de control, drenar escrituras en vuelo antes de salir y resolver la compatibilidad de una tool MCP nueva con CONTRACT-01.

## Hallazgos

| Hallazgo | Fichero y línea | Reproducción | Arreglo propuesto |
|---|---|---|---|
| **A. El apagado actual no drena la auditoría ni las escrituras en vuelo.** `wait_for_shutdown` espera al HTTP, luego se anuncian mensajes, se hace `reap_idle` de guilds, se apaga `ServiceManager` y se sale. Ninguna fase espera los `spawn_blocking` de auditoría, que son fire-and-forget. | `main.rs`, `main.rs` líneas 2194-2248; `handler_do/outcome.rs` líneas 193-195; `handler_do/audit.rs` líneas 33-88 | Enviar un `tylluan_do` que escribe auditoría y apagar inmediatamente. La fila puede perderse o quedar a medias; si el proceso sale antes de que termine el INSERT, la cadena de hashes queda sin esa entrada. | Orden explícito: (1) dejar de aceptar tool calls, (2) drenar la cola de auditoría/escrituras, (3) reapear guilds, (4) apagar servicios, (5) quitar PID y salir. Añadir un test con un writer lento y un shutdown durante la escritura. |
| **B. `TaskRegistry` puede convertirse en una superficie de control sin autenticación definida.** El ADR dice que arranca, supervisa y detiene tareas; no dice si el estado y el control salen por REST/MCP, ni con qué clase. | `ADR025_runtime_y_subruntimes.md`, líneas 44-45 y 53-54; `transport/http/auth.rs`, línea 259 (`operator_class_required`) | Si el registro se expone y una ruta de control queda en clase agente, un agente podría detener tareas de seguridad (write gate, centinela, auditoría) o reiniciar el reindexer. | Separar **lectura** (estado saneado) de **control** (start/stop/cambiar cadencia). Control solo operador, por cabecera, auditado; lectura por operador o loopback. Las tools MCP de control no deben existir sin enmienda. |
| **C. El endpoint de estado de tareas es una fuga de información si se publica sin redacción.** El ADR propone `GET /api/v1/runtime/tasks` y una tool MCP de solo lectura, sin definir qué campos salen. | `ADR025_runtime_y_subruntimes.md`, línea 54; `transport/http/auth.rs`, líneas 259 y 413-414; `tools.rs`, líneas 49-54 | Un endpoint con `task_id`, rutas de ficheros, nombres de guilds, `agent_id`, mensajes de error y tiempos permite enumerar la topología y actividad del kernel. Si se añade sin clasificarlo, el test de inventario de rutas de `auth.rs` lo detecta como no clasificado. | Devolver solo `id` lógico, estado, subruntime, `last_run`, duración y contadores agregados. Sin rutas absolutas, prompts, contenidos de memoria ni `agent_id` salvo para operador. Ruta de detalle solo operador. |
| **D. Cancelar con `GuardedTask` no cancela un `spawn_blocking` de SQLite.** El guard hace `handle.abort()` tras agotar extensiones; tokio cancela la tarea async, no el hilo blocking que sigue dentro de SQLite/ONNX. | `guard.rs`, líneas 35 y 62; `security/write_gate.rs`, líneas 126, 170 y 217; `handler_do/outcome.rs`, línea 193; `main.rs`, línea 1941 | Un reindexado o write gate lento supera el guard; el guard marca timeout y devuelve error, pero el hilo blocking sigue escribiendo. Un `TaskRegistry` que marque la tarea como cancelada mientras la escritura continúa produce estado falso y posibles carreras con el siguiente ciclo. | Cancelación cooperativa en límites de lote, no `abort` de una escritura SQLite. Esperar el `JoinHandle` del blocking antes de marcar terminada; transacciones idempotentes y `busy_timeout`; el registro debe distinguir `cancel_requested` de `finished`. |
| **E. No hay un token maestro único de apagado.** El ADR reconoce 27 de 28 bucles de `main.rs` sin token; `doc_token` se crea y se retiene, pero nunca se cancela. | `ADR025_runtime_y_subruntimes.md`, línea 28; `main.rs`, líneas 1272 y 1290; `main.rs`, líneas 2194-2248 | Parar el kernel deja tareas periódicas sin señal; el runtime puede quedar vivo tras el “stopped” (el ADR ya documenta 85 s). Un `TaskRegistry` que no herede el token maestro reproduce el problema. | Un `CancellationToken` maestro, hijos por subruntime, cancelación en orden y espera con timeout; registrar por nombre las tareas que no responden. Test de parada con un bucle que ignora el primer aviso. |
| **F. La tool MCP de estado de tareas choca con CONTRACT-01.** El ADR pide “una herramienta MCP de solo lectura”; el contrato exige exactamente seis tools soberanas y `REVIEW.md` lo declara invariante. | `ADR025_runtime_y_subruntimes.md`, línea 54; `tools.rs`, líneas 49-54; `REVIEW.md`, línea 25 | Añadir `tylluan_runtime_tasks` como séptima tool rompe `all_tools()` y los tests de contrato. | Exponer el estado por `/api/v1/runtime/tasks` y, si debe llegar por MCP, mediante una intención de `tylluan_do` o `tylluan_think`, no como tool nueva. Si se quiere una tool propia, exige enmienda formal de CONTRACT-01. |
| **G. El ADR y la Fase 0 no usan la misma cifra ni los mismos anclajes.** El §2 dice 121 spawns reales; la fase 0 dice 123; el anclaje del `process::exit` apunta a la línea 2239, que en `d4f2537` ya no es la salida. | `ADR025_runtime_y_subruntimes.md`, líneas 19, 27 y 60; `main.rs`, línea 2248 | Un revisor que siga el ADR cuenta 123 o busca el exit en la línea equivocada y concluye que el documento no describe el árbol. | Corregir a 191 ocurrencias / 121 spawns reales (o el criterio que se adopte) y usar “línea N” con el árbol actual, o anclas semánticas. No aprobar con cifras contradictorias. |
| **H. `TaskRegistry` y el supervisor existente pueden duplicar la supervisión.** El ADR dice que el registro reutiliza `GuardedTask` y `BackgroundBudget`, pero no define la frontera con `registry/supervisor.rs` y `registry/lifecycle.rs`. | `ADR025_runtime_y_subruntimes.md`, líneas 45 y 61; Fase 0 DSH, filas de `registry/supervisor.rs` y `registry/lifecycle.rs` | Dos mecanismos intentan reiniciar o matar la misma tarea: uno por timeout y otro por watchdog; un reinicio puede competir con una cancelación. | Declarar que el registro es el único dueño de las tareas de fondo y que el supervisor de guilds sigue siendo el único dueño de los procesos de guild. Prohibir que una tarea se registre en ambos. |

## Recomendación

**Aprobar con cambios.**

Antes de aprobar R1/R2/R4/R5:

1. Añadir al ADR el contrato de apagado con **drenaje de auditoría y escrituras en vuelo**, no solo cancelación de futuros.
2. Separar estado (lectura saneada) de control (operador, auditado) y definir la autorización exacta de `runtime/tasks`.
3. Resolver la compatibilidad con CONTRACT-01: la observación MCP va por `tylluan_do`/`tylluan_think`, o hay enmienda formal; no una séptima tool.
4. Prohibir `abort()` sobre escrituras SQLite/ONNX; usar cancelación cooperativa y esperar el blocking.
5. Corregir las cifras y anclajes del ADR, y declarar la frontera con `supervisor`/`lifecycle`.

La dirección de fondo (runtime declarativo, presupuesto, apagado acotado) es correcta; el riesgo está en convertir el registro en un plano de control sin autenticación y en prometer un apagado ordenado sin drenar las escrituras que hoy son fire-and-forget.

## Impact

- **Diseño:** revisión de seguridad del ADR; no se implementó ni se cambió comportamiento.
- **Riesgo principal:** un `TaskRegistry` con control expuesto y sin drenaje de auditoría puede perder evidencia o permitir que un agente pare tareas de seguridad.
- **No depredación:** solo lectura; no se tocó el checkout compartido, el kernel vivo ni `target/`.

## No verificado

- No ejecuté un apagado instrumentado ni medí los 85 s; la reproducción es lectura de código y de la Fase 0.
- No existe todavía `TaskRegistry` ni `/api/v1/runtime/tasks`; los hallazgos B, C y H son de diseño, no de código implementado.
- No probé la semántica de `abort()` sobre `spawn_blocking` en este kernel; se apoya en la semántica documentada de tokio y en el informe H-45, no en una ejecución propia.
- No revisé el diseño completo de `BackgroundBudget`; solo su relación con el apagado.
