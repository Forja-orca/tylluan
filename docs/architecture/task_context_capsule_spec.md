# Task Context Capsule — Especificación de Implementación

> **Autor:** Claude Code (Tech Lead), a partir de la idea original de José
> **Target:** Deep, Buffy, Antigravity, José
> **Status:** Spec cerrada para implementación — turno 582+, Coloquio
> **Fecha:** 2026-09-19
> **Precede a:** ningún documento previo — esta idea nace en esta sesión,
> a partir de una reformulación explícita confirmada por José: *"memoria
> de trabajo rápida por tarea (no caché de inferencia LLM), con punteros
> a documentos/decisiones y ciclo de vida cerrar→sintetizar-o-borrar"*.

Este documento formaliza una idea de José surgida durante la investigación
de KV-cache de inferencia (contrato `bwc-8bfc9aca`, cerrado) pero que es
**un problema distinto**: no toca `llama-server` ni el motor de
inferencia. Es una capa de coordinación — memoria de trabajo compartida,
rápida, por tarea, con un ciclo de vida explícito de cierre. Nadie
implementa nada de esto sin que primero salga como contrato BWC reclamado
en Coloquio (regla de trabajo a la vista, `WORK_PROTOCOL.md`).

---

## 1. El problema real, en palabras de José

> "la kv cache y runtimes debe ser para compartir contexto entre vosotros
> de los proyectos y tareas de forma rápida, eliminando el problema de
> quien a elegido que, añadiendo acceso rápido, cuando un plan se a
> decidido [...] debe tener toda la información importante para todo el
> equipo, y la ruta a todos los documentos y partes de ese trabajo [...]
> cuando la tarea sea acabada y esté resuelta o se decida NO-GO, esa
> kvcache o se sintetiza en nodo como memoria o se borra directamente y
> se libera para otros task"

Traducido a un problema de ingeniería concreto: **hoy, "quién decidió
qué" en una tarea multi-agente vive disperso** — en Coloquio (log de
turnos, hay que releer todo), en commits (hay que hacer `git log`), en
la cabeza del agente que lo decidió (si su sesión se compactó o cerró,
se pierde). Un agente que se une a una tarea a mitad de camino, o que
retoma una tarea horas después, paga el coste completo de reconstrucción
cada vez — exactamente el mismo coste que ya pagamos nosotros mismos en
esta sesión larga con las compactaciones de contexto.

---

## 2. Esto NO es KV-cache de inferencia — deslinde explícito

El contrato `bwc-8bfc9aca` (cerrado, veredicto GO configuración / NO-GO
motor) resolvió una pregunta distinta: si el *prefijo de tokens* que un
LLM procesa es reutilizable entre llamadas. Esta spec resuelve: si el
*contexto de una tarea* (decisiones, punteros a documentos) es accesible
rápido entre agentes sin releer todo el historial. Ambos usan la palabra
"caché" por la misma intuición (evitar recomputar/releer algo caro), pero
operan en capas completamente distintas del stack — una en tokens dentro
de un modelo, otra en metadatos de coordinación entre agentes. No se
fusionan en el código ni en el diseño.

---

## 3. Precedentes reales verificados (no asumidos)

### 3.1 Patrón Blackboard (clásico, IA simbólica, Hearsay-II)

El patrón blackboard es una arquitectura clásica donde un único objeto de
estado compartido es leído y escrito por agentes independientes, cada uno
contribuyendo sin comunicación directa entre sí — el estado compartido es
el único canal. Fuente: revisión de frameworks modernos que replican este
patrón explícitamente frente a la alternativa de paso de mensajes puro
(LangChain/CrewAI), citando el desacoplo entre fases como su ventaja
central. Relevante aquí porque el Task Context Capsule es exactamente
esto: un blackboard con alcance por tarea, no global.

### 3.2 Anthropic — Claude Managed Agents (2026)

Anthropic diseñó explícitamente "Managed Agents" para **compartir
contexto, estado y trazabilidad en un solo lugar** cuando un agente líder
delega piezas de trabajo a sub-agentes especializados que trabajan en
paralelo sobre un sistema de archivos compartido, retroalimentando al
contexto del agente líder. Confirma que un laboratorio de frontera llegó
a la misma necesidad — coordinación multi-agente real requiere un lugar
común de estado, no solo mensajes.

### 3.3 OpenAI Agents SDK — Sessions y `RunContextWrapper`

El SDK oficial de OpenAI separa explícitamente dos conceptos que nuestra
propuesta también debe separar: **Sessions** (memoria persistente de la
conversación, se envía al modelo) y **`RunContextWrapper.context`**
(estado local suministrado por quien llama, compartido con
agentes/tools/handoffs durante una ejecución, **nunca enviado al
modelo**). El Task Context Capsule es análogo a este segundo concepto,
pero con alcance de tarea multi-agente/multi-sesión en vez de una sola
ejecución — punteros y decisiones, no prompt.

### 3.4 Mesh Memory Protocol (arXiv:2604.19540, 2026)

Aborda directamente "colaboración cognitiva agente-a-agente entre
sesiones" — equipos de agentes LLM colaborando en tareas de días/semanas,
necesitando compartir, evaluar y combinar el estado cognitivo del otro en
tiempo real *entre reinicios de sesión*. Tres problemas que identifica,
aplicables tal cual a Tylluan:
- **P1**: cada agente decide campo por campo qué aceptar de sus pares, en
  vez de aceptar/rechazar el mensaje completo — nuestra cápsula debe ser
  aditiva estructurada, no texto libre a reinterpretar.
- **P2**: cada afirmación debe ser trazable a su fuente, para que una
  afirmación que vuelve no se confunda con el propio pensamiento previo
  del receptor — de ahí que cada entrada de `decisions[]` lleve
  `by: agent_id` obligatorio.
- **P3**: la memoria que sobrevive a un reinicio de sesión es relevante
  por *cómo* se guardó, no por cómo se recupera — justifica que la
  síntesis final sea un nodo de memoria estructurado, no un volcado.

MMP en producción usa primitivas más pesadas (CAT7, SVAF, linaje de
hashes) diseñadas para colaboración cognitiva rica entre muchos agentes
autónomos de larga duración — deliberadamente NO las adoptamos completas
aquí (ver §7, alcance mínimo viable); las citamos porque validan la
dirección, no porque las copiemos.

### 3.5 Decentralized Multi-Agent Systems with Shared Context (arXiv:2606.10662, 2026)

Confirma la tendencia general 2026 de sistemas multi-agente que dependen
de contexto compartido explícito en vez de paso de mensajes puro como
mecanismo de coordinación — mismo sentido de dirección que el resto de
la literatura citada.

### 3.6 Consolidación y olvido de memoria de agentes (SSGM, AgeMem — 2026)

- **SSGM** (arXiv:2603.11768, *Stability and Safety Governed Memory*)
  formaliza un framework de cuatro palancas para consolidación:
  **importancia, fusión (merge), decaimiento (decay), evicción**. Nuestra
  regla "sintetiza o borra al cerrar" es una versión mínima y explícita
  de evicción gobernada por evento (cierre de tarea), no por tiempo.
- **AgeMem** trata las operaciones de memoria (`store`, `retrieve`,
  `update`, `summarize`, `discard`) como herramientas invocables
  explícitas del propio agente, en vez de mecanismos implícitos del
  runtime. Confirma que `summarize` (sintetizar) y `discard` (borrar)
  como las dos únicas rutas de salida de una cápsula, disparadas por
  evento y nunca implícitas, es un patrón ya validado en la literatura
  2026, no una invención ad-hoc.
- El "problema de consolidación" (Hindsight, 2026) señala que la
  consolidación episodio→semántico casi nunca es automática en sistemas
  actuales — requiere disparo explícito. Nuestro diseño la dispara
  explícitamente en `/close`, evitando ese punto ciego documentado.

---

## 4. Arquitectura: qué existe ya, qué falta

### 4.1 Lo que ya existe (verificado, no asumido)

- **Work Contracts**: `POST /api/v1/work-contracts` (`crates/tylluan-kernel/src/transport/http/api_v1/api_contracts.rs`)
  ya crea `contract_id` (`bwc-<uuid>`), `task`, `team`, `budget`,
  `consolidator`, `channel_id`, con `/deliver`, `/vote`, `/close`
  reales, persistidos en `data/contracts.db` tabla `work_contracts`.
  **El Task Context Capsule reutiliza este mismo `contract_id` como
  clave — no crea un espacio de nombres nuevo.**
- **`tylluan_remember`**: mecanismo ya existente y con gate de coherencia
  (ASI06, dos capas) para escribir memoria persistente en SilvaDB — el
  destino final de la síntesis.
- **Coloquio**: canal de eventos ya broadcast (`coloquio:new_turn`),
  reutilizado por el dispatcher (BWC-1..4, cerrado) — el mismo mecanismo
  de "trabajar a la vista" que ya usa el proyecto.

### 4.2 Lo que falta construir

1. Tabla `task_context` — la cápsula en sí, keyed por `contract_id`.
2. Endpoints HTTP para leer/añadir (aditivo, nunca sobreescribe).
3. El gancho de cierre: `/close` de un work-contract dispara síntesis o
   descarte de su cápsula asociada, si existe.

---

## 5. Modelo de datos

```rust
/// Una entrada de decisión — aditiva, nunca se sobreescribe ni se borra
/// individualmente. El historial completo de decisiones de la tarea es
/// en sí mismo la trazabilidad de "quién decidió qué" que José pidió.
struct CapsuleDecision {
    by: String,           // agent_id, obligatorio (MMP P2: trazabilidad a la fuente)
    what: String,         // texto de la decisión
    at: i64,               // unix timestamp
}

/// Un puntero a algo relevante para la tarea — no se copia el contenido,
/// solo la referencia (la cápsula debe ser barata de leer/escribir).
enum PointerKind { Doc, Commit, File, ColoquioTurn, ContractId }
struct CapsulePointer {
    kind: PointerKind,
    reference: String,     // ruta, hash, número de turno, u otro contract_id
    added_by: String,
    at: i64,
}

/// La cápsula completa — una fila por contract_id, mutación aditiva.
struct TaskContextCapsule {
    contract_id: String,           // FK a work_contracts.contract_id (BWC ya existente)
    decisions: Vec<CapsuleDecision>,
    pointers: Vec<CapsulePointer>,
    updated_at: i64,
    updated_by: String,
}
```

Almacenamiento: tabla SQLite nueva `task_context` (mismo patrón que
`work_contracts` y `pending_dispatches`) — `decisions`/`pointers`
serializados como JSON en columnas TEXT (mismo patrón ya usado en
`command_json` de `dispatch_queue.rs`), evitando tablas relacionales
nuevas para algo de escritura poco frecuente y lectura por clave única.

```sql
CREATE TABLE IF NOT EXISTS task_context (
    contract_id  TEXT PRIMARY KEY,
    decisions    TEXT NOT NULL DEFAULT '[]',  -- JSON array de CapsuleDecision
    pointers     TEXT NOT NULL DEFAULT '[]',  -- JSON array de CapsulePointer
    updated_at   INTEGER NOT NULL,
    updated_by   TEXT NOT NULL
);
```

Vía `config::open_db()` (choke-point de cifrado en reposo, mismo patrón
que todo el resto del kernel — ver `965e8ba`, encontrado por el propio
gate de CI cuando `dispatch_queue.rs` no lo siguió a la primera).

---

## 6. API — aditiva, sin sobreescritura

```
GET  /api/v1/work-contracts/{id}/context
     -> TaskContextCapsule completa (o 404 si nunca se escribió nada)

POST /api/v1/work-contracts/{id}/context/decision
     body: { agent_id, what }
     -> añade una CapsuleDecision, nunca reemplaza las existentes

POST /api/v1/work-contracts/{id}/context/pointer
     body: { agent_id, kind, reference }
     -> añade un CapsulePointer, nunca reemplaza los existentes
```

Ninguna ruta de "editar" o "borrar" una entrada individual — la cápsula
es un log aditivo mientras la tarea vive (MMP P1: aceptar/rechazar el
mensaje completo, no reescribir campo por campo lo que otro agente ya
escribió). El único borrado es total, disparado por `/close` (§7).

---

## 7. Ciclo de vida — el gancho de cierre

`POST /api/v1/work-contracts/{id}/close` **ya existe**. Se extiende (sin
cambiar su firma actual — compatibilidad hacia atrás real) para que,
después de marcar el contrato como cerrado, mire si existe una cápsula
para ese `contract_id`:

```
[close ya ejecutado, contrato = done]
  capsule = task_context.get(contract_id)
  if capsule is None: return   // nunca hubo cápsula, nada que hacer

  if contract.status == "done":
      // Síntesis: un nodo de memoria estructurado, no un volcado de JSON crudo.
      tylluan_remember(
          content = format_synthesis(capsule, contract),  // decisiones + resumen + punteros como metadata
          node_type = "task_synthesis",
          metadata = { contract_id, pointers: capsule.pointers, team: contract.team }
      )
  else:  // NO-GO, o cualquier estado terminal no exitoso
      // También se sintetiza — el motivo del NO-GO ya es información
      // valiosa hoy (a mano, en el Desván de la biblia). Aquí se
      // automatiza esa misma disciplina, no se inventa una nueva.
      tylluan_remember(
          content = format_synthesis(capsule, contract, verdict="NO-GO"),
          node_type = "task_synthesis",
          metadata = { contract_id, pointers: capsule.pointers, team: contract.team }
      )

  task_context.delete(contract_id)   // la cápsula rápida siempre se libera
```

**Invariante central**: cerrar un contrato SIEMPRE dispara una de las dos
rutas — nunca queda una cápsula huérfana. Esto responde directamente al
principio SSGM de evicción gobernada (§3.6): el evento de gobierno es el
cierre del contrato, no un timer ni un LRU.

---

## 8. Qué NO hace esta spec (alcance mínimo viable, deliberado)

- **No** implementa CAT7/SVAF/linaje completo de Mesh Memory Protocol —
  esas primitivas resuelven colaboración cognitiva rica entre muchos
  agentes autónomos de larga duración; nuestro caso de uso (equipo
  pequeño, tareas acotadas por contrato) no lo necesita todavía. Revisar
  si la cápsula demuestra valor real primero.
- **No** reemplaza Coloquio — Coloquio sigue siendo el registro completo
  del debate; la cápsula es un resumen estructurado y de acceso rápido,
  no una copia.
- **No** toca `tylluan_remember`'s gate de coherencia (ASI06) — la
  síntesis pasa por el mismo camino que cualquier otra escritura de
  memoria, sin atajos.
- **No** introduce ninguna tool MCP nueva (CONTRACT-01 intacto) — son
  únicamente endpoints HTTP nuevos bajo `/api/v1/work-contracts/{id}/...`,
  igual que el patrón ya usado por `/deliver`, `/vote`, `/close`.

---

## 9. Plan de contratos (orden sugerido)

1. **TCC-1**: tabla `task_context` + tipos Rust (`CapsuleDecision`,
   `CapsulePointer`, `TaskContextCapsule`) + persistencia vía
   `config::open_db()`. Sin endpoints HTTP todavía. Tests: escritura
   aditiva nunca pierde una entrada previa, lectura de cápsula
   inexistente da `None` limpio, no error.
2. **TCC-2**: los 3 endpoints HTTP (`GET .../context`,
   `POST .../context/decision`, `POST .../context/pointer`).
3. **TCC-3**: el gancho de cierre en el handler de `/close` — síntesis vía
   `tylluan_remember` + borrado de la fila, para ambas rutas (done/NO-GO).
   Tests: cerrar un contrato sin cápsula no falla; cerrar uno con cápsula
   deja exactamente un nodo `task_synthesis` nuevo y cero filas en
   `task_context` para ese id.
4. **(Opcional, tras medir uso real)**: panel de dashboard mostrando la
   cápsula activa de cada contrato en curso — mismo patrón visual que
   `DispatchesPanel.tsx` (BWC-2).

Cada contrato se reclama en Coloquio antes de escribir una línea, se
entrega vía `/deliver`, se verifica de forma independiente por alguien
que no lo escribió, y se cierra con voto — el mismo ciclo ya probado con
el dispatcher.

---

## 10. Citas y referencias verificadas

1. **Mesh Memory Protocol**: Xu, H. (2026). *Mesh Memory Protocol:
   Semantic Infrastructure for Multi-Agent LLM Systems*.
   [arXiv:2604.19540](https://arxiv.org/abs/2604.19540).
2. **Decentralized Multi-Agent Systems with Shared Context** (2026).
   [arXiv:2606.10662](https://arxiv.org/pdf/2606.10662).
3. **SSGM**: *Governing Evolving Memory in LLM Agents: Risks, Mechanisms,
   and the Stability and Safety Governed Memory Framework* (2026).
   [arXiv:2603.11768](https://arxiv.org/pdf/2603.11768).
4. **AgeMem** y consolidación de memoria de agentes — resumen verificado
   vía búsqueda cruzada de múltiples fuentes 2026 (Hindsight blog,
   ACM HAI 2026 proceedings sobre memoria inspirada en ACT-R). Cifras y
   nombres de mecanismos confirmados, no una única fuente aislada.
5. **OpenAI Agents SDK** — Sessions y `RunContextWrapper`:
   [openai.github.io/openai-agents-python/context/](https://openai.github.io/openai-agents-python/context/),
   [openai.github.io/openai-agents-python/sessions/](https://openai.github.io/openai-agents-python/sessions/).
6. **Anthropic Claude Managed Agents** (Code with Claude, mayo 2026) —
   resumen verificado vía cobertura de prensa técnica (VentureBeat,
   MindStudio) citando el diseño oficial de Anthropic, no una fuente
   primaria de Anthropic con acceso directo — marcar como corroborado
   por prensa especializada, no como cita primaria de laboratorio.
7. **Patrón Blackboard** — arquitectura clásica de IA simbólica
   (Hearsay-II, desde los 70), descrita en comparativas modernas de
   frameworks multi-agente (LangGraph/CrewAI/AutoGen) frente al paso de
   mensajes puro.
