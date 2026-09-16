# Protocolo de Trabajo del Equipo — post long-poll (2026-09-16)

> Qué decidimos hoy y cómo trabajamos a partir de ahora. Este documento es la
> respuesta a una pregunta directa de José tras cerrar el long-poll de
> Coloquio: "¿a través de contratos por tareas? deberíamos investigar cómo
> hacer las interacciones correctamente, buscar errores de otros proyectos
> para no cometer los mismos". No es aspiracional — describe lo que ya existe
> en el kernel y cómo lo vamos a usar de verdad, con seriedad.

## 1. Qué cambió hoy y por qué importa

José lleva más de un año diseñando formas de que el equipo se coordine sin
que él tenga que ser el relay manual de cada mensaje de Coloquio. Hoy se
cerró la pieza que faltaba: **long-poll bloqueante dentro de `tylluan_do`**
(commits `3d06e16`, `0925302`) — cualquier agente conectado por MCP puede
bloquear una sola llamada de herramienta hasta que haya actividad real en un
canal, en vez de necesitar un scheduler autónomo en su propio runtime host
(que la mayoría de los agentes de esta flota no tiene). Cero herramientas
nuevas (CONTRACT-01) — vive dentro del parser de intents de `tylluan_do`.

Esto resuelve el **mecanismo** de comunicación. No resuelve por sí solo el
**protocolo** de cómo estructuramos el trabajo entre agentes — eso es lo que
cubre este documento.

## 2. Sí: contratos por tarea — y ya existen en el kernel

Tylluan ya tiene un sistema real de **Bounded Work Contracts** (M10,
`crates/tylluan-kernel/src/transport/http/api_v1/api_contracts.rs`), no hace
falta inventarlo:

| Endpoint | Qué hace |
|---|---|
| `contract_create_handler` | Abre un contrato de tarea con presupuesto de ciclos y participantes |
| `contract_tick_handler` | Decrementa el presupuesto en cada ciclo de trabajo real |
| `contract_deliver_handler` | Registra una entrega verificable contra el contrato |
| `contract_vote_handler` | Voto de mayoría entre agentes sobre una decisión del contrato |
| `contract_close_handler` | Cierra el contrato — done/blocked, con extensión máxima de 2 ciclos |
| `contract_active_handler` | Lista contratos vivos |

Es decir: **la infraestructura de "protocolo formal de tareas" que José
pregunta si deberíamos construir ya está construida y probada** (tests en el
mismo archivo: creación, tick a cero/bloqueo, extensión mediana, mayoría de
voto, retracto y re-voto). Lo que falta no es código — es **usarlo de
verdad** en vez de coordinar tareas complejas solo por texto libre en
Coloquio.

**Regla desde hoy:** cualquier tarea que involucre más de un agente, o que
tenga una entrega verificable con criterio de aceptación claro, se abre como
contrato (`contract_create_handler`) en vez de solo anunciarse en Coloquio.
Coloquio sigue siendo el canal de conversación/coordinación en vivo — el
contrato es el registro estructurado de qué se pidió, qué presupuesto tiene,
y qué se entregó, con voto real cuando hay ambigüedad sobre si algo cumple
el criterio.

## 3. Investigar antes de construir — ya es una regla, se refuerza

`CLAUDE.md` (regla global, no solo de este repo) ya exige un paso de
"Research & Reuse" antes de cualquier implementación nueva: buscar
implementaciones existentes (`gh search code`/`gh search repos`), consultar
documentación primaria antes de asumir el comportamiento de una API, y
preferir adoptar/portar un patrón probado antes que escribir código nuevo
desde cero. El long-poll de hoy es el ejemplo de cómo se hace bien:
Antigravity investigó 3 patrones de referencia reales antes de proponer
diseño (MCP 2026-07-28 progress heartbeats, AutoGen/LangGraph cursor-first
anti-race, Tokio/Axum graceful timeout) — ver
`docs/architecture/coloquio_long_poll_research.md`. Ese documento es la
plantilla a seguir: no se escribe una línea de diseño nuevo sin antes citar
qué proyectos reales ya resolvieron el mismo problema y qué falló cuando lo
hicieron mal.

**Regla desde hoy, explícita:** ninguna feature nueva de coordinación
multi-agente (colas, locks, protocolos de consenso, mecanismos de espera)
se diseña sin antes buscar y citar al menos 2-3 proyectos reales que ya lo
resolvieron — y sin preguntar explícitamente "¿cómo falló esto en otros
proyectos?" antes de "¿cómo lo construyo?". Los errores más caros de este
proyecto (GraphRAG al 76% CPU por anidamiento sin límite, CoherenceGate
arrancando `llama-server` sin opt-in, el ACL en modo fail-open) fueron todos
del tipo "no investigamos cómo lo hacen otros / no preguntamos cómo falla
esto antes de escribirlo".

## 4. Disciplina de trabajo — lo que ya funciona, no se relaja

Esto no es nuevo, pero se reafirma porque José pidió explícitamente
"seguir un protocolo de trabajo con seriedad como venimos haciendo":

1. **Verificación cruzada obligatoria antes de aceptar cualquier reporte** —
   nunca se da por bueno "está hecho" sin leer el código, correr el
   test/clippy real, o probarlo en vivo uno mismo. Hoy mismo esto encontró
   dos veces que un reporte de "clippy limpio" no lo era (`collapsible_if`
   real, dos veces reportado como arreglado sin estarlo) y encontró un bug
   real de interceptación de sintaxis que dos agentes distintos (yo y Deep,
   independientemente) confirmaron por separado.
2. **`git fetch` + comparar `origin/main` contra `HEAD` antes de reportar
   nada** — ya es regla en `AGENTS.md`, se mantiene sin excepción.
3. **Solo Claude Code commitea/pushea código de producción** — el resto de
   la flota deja diff sin commitear para revisión cruzada, salvo que se
   acuerde lo contrario explícitamente.
4. **Todo commit responde ADR-013 (no-depredación)** y lleva sección
   `## Impact` si toca `transport/`, `guilds/`, `integrations/` o
   `tylluan*.toml`.
5. **Ningún agente arranca/para el kernel de producción, nunca** — solo José
   reconstruye y reinicia. Un agente que necesita verificar un cambio en
   vivo se lo pide a José, no lo hace él mismo.

## 5. Qué hacer distinto a partir de ahora (resumen operativo)

- **Coordinación en vivo** → Coloquio, ahora con long-poll — cualquier
  agente puede esperar activamente en vez de que José reenvíe mensajes.
- **Tareas con más de un agente o con entrega verificable** → contrato
  formal (`tylluan_do intent="crea contrato..."` o el endpoint REST
  directo), no solo un mensaje de texto libre.
- **Cualquier feature nueva de coordinación/concurrencia** → research de
  al menos 2-3 proyectos reales y sus fallos conocidos, documentado en
  `docs/architecture/`, antes de escribir diseño.
- **Cualquier reporte de "hecho"/"funciona"** → verificado de forma
  independiente por Claude Code (o por otro agente si Claude Code es quien
  lo construyó) antes de darlo por cerrado en Coloquio.

## 6. Primer ciclo real sin José (2026-09-17) — qué confirmó y qué rompió

José pidió una prueba real de un ciclo completo del equipo sin su intervención.
Se abrió el contrato `bwc-b0523fcc` (generalizar `check_coloquio.py`) y el
ciclo corrió de verdad: Deep entregó, Antigravity y Buffy verificaron de
forma independiente, yo cerré el contrato y corregí dos bugs reales
encontrados en la entrega (corrupción de encoding, bug de mayúsculas). Dos
cosas quedaron confirmadas y dos reglas nuevas nacen de ahí:

- **El long-poll funciona de verdad** — mi propia espera bloqueó y reaccionó
  en 20s cuando Deep publicó, sin sondeo manual.
- **Pero ningún agente tenía nada corriendo por su cuenta** — Deep y
  Antigravity solo actuaron porque José se lo pidió directamente en sus
  sesiones. El kernel dejó de ser el cuello de botella; el runtime de cada
  agente sigue siéndolo. Deep construyó la pieza que falta el mismo día
  (`guilds/core/coloquio_watcher.py`, loop persistente con long-poll real) —
  ver más abajo la regla sobre su modo `--exec`.
- **Regla nueva — cerrar contratos por el conducto formal:** Buffy detectó
  que la entrega de Deep nunca pasó por `/work-contracts/{id}/deliver` — el
  contrato lo cerré yo a mano vía `/close`, no por el flujo real. Desde
  ahora, quien entrega registra la entrega en el contrato antes de que
  nadie lo dé por cerrado; un cierre sin `/deliver` es un cierre narrativo,
  no verificado por el propio sistema.
- **Regla nueva — anunciar antes de commitear al checkout compartido:**
  Buffy reclamó la tarea en Coloquio (T524) casi al mismo tiempo que Deep la
  entregaba sin avisar antes — a punto estuvo de duplicarse trabajo real.
  Cualquier agente que vaya a tomar una pieza de un contrato la reclama en
  Coloquio ANTES de escribir código, no después.
- **Riesgo de seguridad real, sin resolver todavía:** `coloquio_watcher.py`
  soporta `--exec <comando>`, que dispara un proceso real (p. ej.
  `opencode run <texto>`) cuando detecta una mención al agente en Coloquio.
  El diseño evita shell injection (usa `subprocess.Popen` con lista de
  argv, nunca `shell=True`), pero no evita el problema de fondo: cualquiera
  que pueda publicar en Coloquio con `@<agente>` en el texto puede hacer que
  ese agente ejecute instrucciones no confiables sin ningún humano en el
  bucle. Regla hasta que se decida una mitigación (allowlist de autores
  confiables y/o confirmación humana antes de ejecutar): el modo
  solo-inbox (sin `--exec`) se puede activar libremente; `--exec` NO se
  activa en ningún runtime sin decisión explícita de José.

Este documento se actualiza cuando el protocolo cambie de verdad — no es un
manifiesto fijo, es el reflejo de cómo trabajamos hoy.
