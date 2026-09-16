# Push vs Pull: Por Qué el Watcher de Hoy Es el Diseño Equivocado

> **Autor:** Claude Code (Tech Lead)
> **Target:** Deep, Buffy, Antigravity, José
> **Status:** Investigación / Propuesta de Rediseño
> **Fecha:** 2026-09-17
> **Contexto:** Segundo ciclo de prueba sin José (turno 540+) — cero actividad
> del equipo pese a tener el long-poll y un watcher persistente corriendo.
> José preguntó directamente: "¿estamos razonando correctamente? ¿hemos
> investigado la realidad de otros proyectos?" — la respuesta honesta es no,
> no lo suficiente. Este documento corrige eso antes de escribir más código.

---

## 1. Resumen Ejecutivo

El long-poll de `tylluan_do` (turno 498-508, commits `3d06e16`/`0925302`/`b4ad148`)
es un mecanismo de **espera bloqueante** correcto y bien fundamentado — resuelve
el problema de "¿cómo bloqueo una llamada hasta que haya actividad?". Pero hoy
construimos algo distinto encima: `guilds/core/coloquio_watcher.py`, un script
que cada agente arrancaría en su propia terminal para "escuchar" Coloquio de
forma continua. Ese script es **arquitectónicamente el patrón equivocado**:

- Muere si se cierra la sesión/terminal que lo lanzó.
- Su stdout queda bufferizado sin TTY — ni siquiera se puede verificar en vivo
  si sigue corriendo sin matarlo o instrumentarlo aparte.
- Requiere que un humano lo arranque para cada agente, en cada máquina —
  exactamente el trabajo manual que se supone que íbamos a eliminar.
- Resultado medido hoy: watcher corriendo, contrato abierto, **cero
  reacciones reales del equipo** sin que José interviniera de nuevo.

La industria ya resolvió este problema, y no lo resolvió así.

## 2. Cómo lo resuelve la industria real (investigado, no asumido)

### Slack Events API — el caso de referencia para "menciones en un chat"
Slack migró explícitamente de polling a un modelo push: en vez de que un bot
pregunte "¿hay mensajes nuevos?" en un bucle, **Slack empuja un HTTP POST al
endpoint del bot** cada vez que ocurre un evento suscrito (`app_mention`,
`message.channels`). La recomendación oficial es responder con HTTP 200
inmediatamente y encolar el procesamiento aparte — el bot nunca mantiene un
proceso "escuchando" en el sentido de sondeo; su servidor simplemente recibe
la llamada cuando hay algo. Migrar de polling a este modelo bajó la latencia
de eventos de decenas de segundos a **1-2 segundos**.
— [Slack Events API docs](https://api.slack.com/events-api), [The Events API — Slack Developer Docs](https://docs.slack.dev/apis/events-api/)

### Cursor Background Agents — el caso de referencia para "agentes de código autónomos"
Los agentes en background de Cursor se disparan **por webhook o evento**
(GitHub, GitLab, Slack, Linear, Sentry, PagerDuty, o un webhook HTTP privado
propio) o por cron — nunca por un bucle que el agente mismo mantiene vivo.
El webhook crea un endpoint HTTP privado; cuando algo hace POST a ese
endpoint, **eso** arranca la ejecución del agente. El agente no pregunta, lo
despiertan.
— [Cursor Docs: Webhooks](https://cursor.com/docs/background-agent/api/webhooks), [Cursor Docs: Automations](https://cursor.com/docs/cloud-agent/automations)

### AutoGen v0.4 (AG2) — el caso de referencia para frameworks multi-agente
La reescritura v0.4 de AutoGen movió deliberadamente el núcleo de
orquestación a un **modelo event-driven async-first**, alejándose de un
bucle de conversación síncrono. Es el mismo movimiento arquitectónico:
de "pregunta en bucle" a "reacciona a evento".
— [LangGraph vs CrewAI vs AutoGen 2026](https://dev.to/pockit_tools/langgraph-vs-crewai-vs-autogen-the-complete-multi-agent-ai-orchestration-guide-for-2026-2d63)

### Principio general (Confluent, Atlan — arquitecturas event-driven para agentes)
"En vez de que un agente pregunte '¿hay trabajo nuevo?' cada X segundos,
duerme hasta que el sistema lo despierta con una tarea específica."
Un agente en polling que revisa cada 5 minutos desperdicia recursos el 99%+
del tiempo si los cambios llegan con poca frecuencia; el modelo event-driven
reduce la latencia 70-90% frente a polling.
— [Event-Driven Architecture for AI Agents (Atlan)](https://atlan.com/know/event-driven-architecture-for-ai-agents/), [Confluent: Autonomous Agentic Event-Driven Systems](https://www.confluent.io/blog/autonomous-agentic-event-driven-systems-architecture/)

## 3. Matriz comparativa

| | Pull (watcher de hoy) | Push (propuesto) |
|---|---|---|
| ¿Quién mantiene el proceso vivo? | Cada agente, en su propia terminal | El kernel Tylluan (siempre encendido) |
| ¿Qué pasa si se cierra la sesión del agente? | El watcher muere, silencio total | No aplica — no hay proceso del agente que mantener vivo |
| Latencia | Depende de que el watcher siga vivo y del ciclo de long-poll | Inmediata — el kernel invoca al detectar el evento |
| Observabilidad | Mala (stdout bufferizado, sin proceso persistente que auditar) | Buena — el propio kernel loggea el despacho, es un endpoint más |
| Consistente con la industria (Slack, Cursor, AutoGen v0.4) | No | Sí |
| Requiere trabajo manual de José para arrancar cada agente | Sí (exactamente lo que probamos hoy) | No |

## 4. Diseño propuesto para Tylluan

El kernel ya tiene todas las piezas: `server.notifier` (el broadcast de
`coloquio:new_turn`, `api_coloquio.rs:173`) y la lógica de menciones que hoy
vive duplicada en `coloquio_wait.rs` y en `coloquio_watcher.py`. La pieza que
falta es un **registro de despacho** dentro del kernel:

```
[agents.toml o tabla nueva en SilvaDB]
  agent_id = "deep"
  wake_command = ["opencode", "run"]   # o un webhook_url si el runtime lo soporta
  trusted = true                        # mismo concepto que --trusted-authors de hoy

[server.notifier subscriber interno, no un endpoint HTTP más]
  on coloquio:new_turn:
    for each agent in registry:
      if mentions(event.content, agent.agent_id) and event.author_id != agent.agent_id:
        queue_for_approval(agent, event)   # MISMA cola HITL que ya construimos hoy
```

Puntos clave:

- **El HITL y el allowlist de hoy no se tiran — se mueven de sitio.** La
  decisión de José (ambas mitigaciones obligatorias) sigue aplicando
  exactamente igual: el kernel encola, un humano aprueba con
  `--approve <id>` o vía dashboard. Lo único que cambia es QUIÉN detecta el
  evento (el kernel, no un script del agente) — no relaja ninguna
  salvaguarda de seguridad ya decidida.
- **Cero procesos nuevos que mantener vivos.** El kernel ya está siempre
  encendido (es el requisito fundacional del proyecto). Añadir un
  suscriptor interno al broadcast existente no es un componente nuevo con
  su propio ciclo de vida — vive dentro del proceso que ya no se apaga.
- **`coloquio_watcher.py` no se descarta entero** — el modo `--exec` (con
  sus dos mitigaciones) se retira a favor del despacho del kernel, pero el
  modo **solo-inbox** sigue siendo útil como fallback local/manual para un
  agente que sí quiera correr algo en su propia máquina sin depender del
  despacho central (p. ej. debugging, o un agente sin `wake_command`
  configurado todavía).

## 5. Qué NO hacer todavía

- No implementar esto sin que José apruebe el diseño primero — es un cambio
  de arquitectura real (nuevo suscriptor interno, nueva tabla de registro),
  no un fix de una línea.
- No reactivar `--exec` del watcher pull mientras se decide esto — sigue
  aplicando la decisión de hoy (ambas mitigaciones obligatorias, apagado
  por defecto).
- No asumir que "webhook" significa exponer un puerto nuevo a internet —
  para los runtimes locales de este equipo (OpenCode/Deep, terminal local),
  el "webhook" es simplemente invocar el comando del agente como proceso
  hijo desde el kernel, igual que ya hace `subprocess.Popen` en el watcher
  de hoy — el cambio es de ubicación (dentro del kernel) no de mecanismo.

## 6. Siguiente paso

Traer este diseño a Coloquio para discusión del equipo antes de que nadie
escriba código de implementación. Preguntas abiertas para la discusión:
¿el registro de despacho vive en `tylluan.toml`, en `.tylluan/agents.toml`
(ya existe y ya mapea agent_id → rol), o en una tabla nueva de SilvaDB?
¿Antigravity, con su `schedule` nativo, necesita esto del kernel o ya lo
resuelve por su cuenta del lado del runtime?
