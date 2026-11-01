# ADR-026: Intermediario de espacio de trabajo para varios agentes (F9)

- **Estado:** ✅ **ACEPTADO PARCIALMENTE (José, 2026-11-01)** — solo las fases F9-0 y F9-1 (esquema, reclamos y escritor único de git por rutas). Los worktrees por agente NO se retiran hasta que pase la prueba de aceptación (§6).
- **Frente:** F9 del roadmap («un espacio de trabajo para todo el equipo»).
- **Decide el dueño:** si, pasada la prueba, se retiran los worktrees por agente.

## 1. Contexto

Hoy cada agente trabaja en su propio `git worktree` porque un checkout compartido produjo colisiones repetidas (`git add -A` que arrastró trabajo ajeno, cambios de rama, `index.lock`). El precio: una copia y una compilación por agente, una integración manual de cada entrega y bases desfasadas que casi deshacen trabajo ya integrado (copiar un fichero entero de una base antigua revierte cambios nuevos).

Objetivo: varios agentes de código sobre **un solo repositorio y un solo árbol**, sin pisarse, sin bloquearse, sin perder escrituras, con asignación automática al agente ocioso y con trazabilidad completa.

## 2. Qué dice la investigación (fuentes abiertas en esta sesión)

| Patrón | Fuente | Qué aporta |
|---|---|---|
| Worktree por agente | Documentación de Claude Code (worktrees) y `git-worktree` | Aísla ficheros pero no resuelve la integración. |
| Reclamo de tareas con bloqueo de fichero y reparto por ruta | Documentación de Claude Code (agent teams) | Reclamo atómico + partición por ruta. |
| Estados explícitos de tarea | Especificación A2A | `submitted`, `working`, `input-required`, `completed`, `failed`, `canceled`. |
| Estado solo como eventos | Google ADK (sesiones) | Reconstruible tras una caída. |
| Checkpoint por hilo | LangGraph (persistencia) | Reanudar desde el último paso. |
| Commit previo de cambios sucios, un solo escritor | Aider | Punto de recuperación antes de editar. |
| Gestor que valida antes de cerrar | CrewAI jerárquico | Estado «en revisión». |
| Arrendamiento de ruta (consultivo) | Proyecto `pact` | Sin ejecución obligatoria: solo sirve si el intermediario valida al escribir. |

No encajan: elegir hablante por LLM (AG2), handoffs como reparto de tareas (OpenAI Agents SDK), auto-commit con varios escritores.

## 3. Decisión propuesta

**Un intermediario dentro de Tylluan** que es el único que escribe en git y el único que concede derechos de escritura.

### 3.1 Datos (base propia `workspace.db`, solo-añadir donde se indica)
- `events(seq, ts, agent_id, task_id, kind, payload)` — **solo añadir**, fuente de verdad. `kind` ∈ `created, claimed, heartbeat, released, expired, review, merged, failed, requeued`.
- `tasks(id, title, paths_glob[], priority, base_commit, acceptance_cmd, status)` — derivada de `events`. `status` ∈ `queued, claimed, running, review, merged, failed`.
- `claims(task_id, agent_id, path_glob, token_hash, epoch, expires_at)` — **índice único** que impide dos reclamos de escritura con rutas solapadas.
- `agents(id, kind, capabilities[], state, last_heartbeat)` — `state` ∈ `idle, busy, offline`.

### 3.2 Reglas
1. Solo el intermediario ejecuta `git add/commit/merge/checkout`, con **commit por rutas** (`git commit -- <rutas>`), nunca `add -A`.
2. Un agente escribe solo en rutas de su reclamo; cada escritura lleva el token y la época del reclamo, y el servidor la rechaza si no coinciden.
3. Arrendamiento de 120 s renovado con latido cada 15 s. Si caduca: con cambios sucios pasa a `review` con un **parche de recuperación** guardado; sin cambios, a `queued`. Nunca se borra ni se hace `reset`.
4. Antes de conceder un reclamo se registra el `base_commit` y se hace una foto de los cambios sin confirmar.
5. Un agente ocioso con latido fresco recibe la siguiente tarea `queued` cuya ruta no solape reclamos activos y cuyas capacidades encajen; si no hay, espera y se le avisa al llegar una.
6. Una tarea no pasa a `merged` sin pasar su `acceptance_cmd` y revisión.
7. Los agentes no cambian de rama, ni hacen `reset` ni `stash`.

### 3.3 Interfaz
Sin herramientas MCP nuevas: se expone por REST bajo `/api/v1/workspace/*` y, si hace falta MCP, por `tylluan_do` con intención. Cualquier herramienta nueva exige la enmienda formal de CONTRACT-01 (AUD-07).

## 4. Fases
| Fase | Contenido |
|---|---|
| F9-0 | Esquema, tabla de eventos, reclamo atómico y caducidad, **sin tocar git**; pruebas unitarias. |
| F9-1 | Escritor de git único con commit por rutas y parche de recuperación. |
| F9-2 | Asignación automática al ocioso y estado `review`. |
| F9-3 | Prueba de aceptación (§6) en Windows y Linux. Solo si pasa se propone retirar los worktrees. |

## 5. Riesgos
- Un arrendamiento caducado con ficheros a medio escribir en el árbol compartido → por eso el parche de recuperación previo.
- Dependencia de que los agentes obedezcan: la validación debe estar en el servidor, no en la cortesía del agente.
- Windows: usar transacciones SQLite y no semánticas POSIX de bloqueo.
- Coste de reescribir los flujos actuales de los agentes: se hace por fases, conviviendo con los worktrees.

## 6. Prueba de aceptación (criterio de salida)
Arnés con 4 agentes simulados (scripts, sin LLM) sobre un repositorio de prueba, 200 tareas con rutas solapadas a propósito, un 10 % de agentes muertos a mitad de edición, 50 repeticiones, en Windows y Linux. Cada escritura deja un marcador único que debe aparecer en el árbol o en un parche de recuperación.

Pasa solo si: **0** escrituras perdidas, **0** solapes de reclamos, **0** tareas reclamadas dos veces, **0** commits con ficheros fuera de la ruta reclamada, **0** errores `index.lock`, ningún agente ocioso más de 30 s con tarea abierta, y p95 del reclamo < 200 ms. Línea base: checkout compartido ingenuo y worktree por agente, comparando pérdidas y tiempo total.

## 7. No verificado
OpenHands, SWE-agent y Codex: aislamiento y delegación no confirmados en sus páginas. Las cifras del paper `grite` son autoevaluadas. El coste real de disco y compilación de los worktrees actuales no se ha medido. Las citas proceden de extractos de lectura, no de copias literales.

## Impact
Documento de diseño: no cambia código. Deja fijados el esquema, las reglas y la prueba de aceptación antes de implementar F9.
