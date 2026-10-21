# D-18 — `untrusted_content` deja de ser una bandera decorativa

- **Fecha:** 2026-10-21
- **Agente:** Deep (OpenCode)
- **Cola:** `docs/internal/COLA_deep_2026-10-21.md`, ítem **D-18**
- **Base:** `origin/main` @ `b326d73` (worktree `E:\tylluan-wt-deep`, diff sin commitear)
- **Patch:** `E:\tylluan-patches\deep\D-18.patch` (14 archivos, +595/−17)
- **Hallazgos de origen:** B-8 (`IMPL_b8_mapa_untrusted_content_buffy_2026-10.md`) y D-11 (`DISENO_D-11_capa1_y_acciones_deep_2026-10.md`)

## Qué estaba roto

B-8 lo midió con archivo:línea: el flag `untrusted_content` **solo** penalizaba ×0.1 el score en
`tylluan_recall` (`coherence_gate.rs`), y **no existía ni un solo escritor del booleano en todo el
repo Rust**. El único "marcado" real era un banner de texto en Python (`guilds/core/utils.py`) que
el gate no lee (no es metadata). Consecuencias:

- Un nodo marcado que se consolidaba en un `agent_summary` **perdía el flag** (nodo nuevo, metadata
  nueva): el texto viajaba por bootstrap y contexto de sesión sin ninguna degradación.
- El bootstrap automático (se inyecta en **cada** tool call con `agent_id`) y el modo `@inbox` del
  recall devuelven contenido **sin pasar por el gate** y sin ninguna señal de desconfianza.

## Qué cambia

### 1. Un contrato único: `security::trust` (nuevo, 186 líneas con tests)

- `is_untrusted_metadata` / `node_is_untrusted`: lectura tolerante (JSON ilegible, clave ausente o
  valor no booleano ⇒ no marca; la ruta caliente del recall no puede fallar).
- `mark_untrusted(&mut Value, origin)`: escritura idempotente y **no destructiva** — si el productor
  remoto ya traía su propio `untrusted_reason`, se respeta.
- `any_source_untrusted` / `banner` / `mark_text` / `origin_of`: herencia y marca textual.
- El gate (`coherence_gate::metadata_untrusted`) pasa a delegar aquí; una sola implementación.

### 2. Escritores reales del flag

| Escritor | Dónde | Qué hace |
|---|---|---|
| `POST /api/v1/ingest` | `api_ingest.rs` (D-17) | marca + juez, ya en `main` |
| Federación (peer) | `api_federation.rs::ingest_from_peer` | **nuevo**: `mark_untrusted("federation_peer:<nombre>")` en la única ruta de ingesta de peers (ADR-019 step 2) |
| A2A (agente externo) | `a2a.rs` (nodo `a2a_result`) | **nuevo**: `untrusted_content` + `untrusted_reason: "a2a external agent: <id>"` |
| Correo de agentes externos | `api_v1/mod.rs::mailbox_send_handler` | **nuevo**: si `IdentityManager::get_identity(from)` es `None` (remitente sin identidad en ESTE kernel), el `BlackboardMessage` viaja con `untrusted: true` |

`BlackboardMessage` gana `untrusted: bool` (`#[serde(default)]`, omitido cuando es `false`):
los payloads antiguos siguen parseando y los nuevos solo cambian de tamaño cuando la marca existe.

### 3. Herencia (antes se perdía)

`AgentMemoryManager::consolidate_if_needed` y `create_session_digest` marcan el nodo resultante si
**cualquiera** de sus fuentes estaba marcada (`"inherited: …"` como motivo). Se hizo en los dos
porque `get_summary` devuelve `agent_summary` **y** `session_digest`: arreglar solo uno reabría el
mismo agujero por la puerta de al lado.

### 4. Marca visible (sin cambiar la experiencia del humano ni bloquear nada)

- **Bootstrap** (`build_agent_bootstrap`): el `content` del resumen y de las memorias recientes se
  devuelve con banner `[CONTENIDO NO CONFIABLE - origen: …]` delante. **La forma del JSON no
  cambia** (mismos campos; solo el texto de `content`).
- **Contexto de sesión del recall**: si el resumen está marcado, la cabecera lo dice (antes solo
  distinguía `federation_peer`).
- **`@inbox`**: el mensaje externo se muestra con el mismo banner; el local no.

El banner es ASCII a propósito: el código del repo ya arrastra mojibake en mensajes con emoji y el
log queda greppable (`[CONTENIDO NO CONFIABLE`).

## Las ocho rutas del mapa de B-8 y su estado tras D-18

| # | Ruta | Gate | Estado tras D-18 |
|---|---|---|---|
| 1 | `tylluan_recall` (búsqueda híbrida) | sí (penaliza) | sin cambios (criterio de la cola) |
| 2 | `tylluan_recall` `@inbox` | no | **marcado visiblemente** |
| 3 | Contexto de sesión prepuesto por recall | no | **marcado visiblemente** |
| 4 | `agent_bootstrap` / `resume_context` (y bootstrap automático por tool call) | no | **marcado visiblemente** |
| 5 | `read_channel` / `search_channel` (Coloquio) | no | **no cubierto** (fuera de esta tajada; ver "no verificado") |
| 6 | `agent_mail` / blackboard por REST | no | **escritor marcado + `@inbox`**; el listado REST devuelve el payload con el campo, pero no lo formatea |
| 7 | `tylluan_think` (broadcasts recientes) | no | **no cubierto** |
| 8 | Creación de `agent_summary` / `session_digest` | — | **herencia implementada** (ya no pierde el flag) |

## Verificación real

- **Lib:** `cargo test -p tylluan-kernel --lib` → `1141 passed; 0 failed; 4 ignored` (31.74 s) y
  segunda pasada `1141 passed` (29.65 s). Incluye 6 tests nuevos de `security::trust` y 2 de
  herencia en `memory::agent_memory::tests` (resumen y digest, con control negativo limpio).
- **Integración nueva:** `cargo test -p tylluan-kernel --test d18_untrusted_visibility` →
  `2 passed`. Cubre (a) bootstrap: resumen marcado y memoria marcada salen con banner, memoria
  limpia no, y la forma del JSON se conserva; (b) escritor del buzón: remitente sin identidad →
  `untrusted: true`, remitente registrado → `false`; `tylluan_recall @inbox` real (vía
  `handle_kernel_tool`) muestra el banner en el externo y no en el local.
- **Clippy:** `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → `Finished`, EXIT 0.

## Impact

- **Comportamiento:** sin cambios de permisos, roles, umbrales ni bloqueos. El recall conserva su
  penalización ×0.1 y su aviso. Lo nuevo es (i) metadata con el flag en tres escritores, (ii) texto
  con banner en tres rutas de contexto, (iii) herencia en resúmenes.
- **Clientes REST:** `POST /api/v1/mailbox/send` no cambia su contrato de entrada; el payload
  guardado puede llevar `untrusted: true`. `GET /api/v1/mailbox` devuelve el payload tal cual (el
  campo aparece solo cuando aplica). El bootstrap mantiene sus claves; solo cambia el texto de
  `content` cuando el nodo está marcado.
- **Datos:** nodos históricos no se reescriben (el marcado es de escritura nueva). Los nodos de
  federación ya ingeridos antes de este parche siguen sin flag en metadata, pero el gate los sigue
  penalizando por `provenance`.
- **Coste:** una consulta a `get_identity` por envío de correo (lectura local de SilvaDB). Sin
  llamadas de red, sin juez nuevo (el juez ya lo lanzaba D-17 en las rutas de memoria).
- **No-depredación:** sin dependencias nuevas, sin procesos, sin tocar `dev_mode`, tokens, puertos
  ni `README`/`AGENTS`/`STATUS`/`docs-site`.

## No verificado / límites

- **Rutas 5 y 7 siguen sin marca** (Coloquio y `tylluan_think`): la cola pedía "primera tajada, la
  que no cambia la experiencia del humano" y no las nombra; quedan como hueco declarado.
- **Escritores de federación y A2A no tienen test de integración propio** (montar un peer firmado /
  una tarea A2A completa excede la tajada): ambos son una llamada a `trust::mark_untrusted`, cuyos
  tests unitarios cubren idempotencia, respeto del motivo del productor y no-objeto. Verificado por
  lectura del punto exacto, no por ejecución.
- **No se ha comprobado en vivo** que un cliente externo real (REA/A2A) escriba hoy el booleano; el
  contrato existe, pero el productor externo no está en este repo.
- **`agent_summary` preexistentes** (creados antes del parche) no se remarcan retroactivamente.

## Siguiente

`D-19` (causa C3 del juez, con el log de Buffy).
