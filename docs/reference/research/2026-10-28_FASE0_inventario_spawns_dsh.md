# FASE 0 (v2) — Inventario de spawns del kernel (ADR-025, solo lectura)

- Agente: dsh
- Base: origin/main 3d95e14 (push186; ADR-025 ya presente). Sin tocar codigo.
- Alcance dsh: transport/, registry/, security/, router/, federation/, tunnel/, consensus.rs, process_isolation.rs, repo_map.rs y lo que quede fuera de la lista de @hermes.
- Rehecho tras T2017: propositos reales (una frase) en vez de trozos de codigo; menciones (comentarios/cadenas) separadas de la tabla.

## Comando y cifra

```
grep -rn "tokio::spawn\|spawn_blocking\|thread::spawn" crates/tylluan-kernel/src --include=*.rs
```

Salida real: **191 ocurrencias** en 61 ficheros.

Reparto (suma exacta): **hermes 63** + **dsh 128** = 191.
- dsh: **78 spawns reales** (tabla) + **12 menciones** (11 en comentarios + 1 en cadena de warn; seccion aparte) + **38 ocurrencias en tests** (apartadas) = 128.
- hermes: 63 (main.rs 39, memory/ 19, metrics_ring.rs 1, guard.rs 2, version_check.rs 2).
- Spawns reales dsh por tipo: tokio::spawn 60, spawn_blocking 16, thread::spawn 2.
- Nota sobre ADR-025 §2 (123 `tokio::spawn` fuera de tests): mi grep real de `tokio::spawn` fuera de tests (ambos ambitos) da 93; la diferencia es de criterio de filtrado (el ADR cuenta solo tokio::spawn y probablemente excluye mas casos); el numero que cuadra con el comando pedido es 191.

## Tabla por spawn — produccion (dsh, 78 reales)

| archivo | linea | proposito | subruntime | disparador | cadencia | guards | apagado | riesgo |
|---|---|---|---|---|---|---|---|---|
| registry/actor.rs | linea 133 | Ejecuta en segundo plano una llamada a tool de guild (con reintentos y timeout) para no bloquear el bucle del actor de registro. | Procesos y guilds | por peticion | n/a | no | no | medio |
| registry/actor.rs | linea 366 | Indexa el registro de la llamada en el ledger de outputs (hash + escritura) fuera del camino de respuesta; detached a proposito. | Procesos y guilds | evento (fin de llamada) | n/a | no | no | bajo |
| registry/lifecycle.rs | linea 57 | Reaper periodico que hace WAL checkpoint de SilvaDB y monitoriza el estado de los guilds. | Procesos y guilds | intervalo | configurable (check_interval_secs) | no | no | alto |
| registry/proxy.rs | linea 725 | Listener SSE en segundo plano que recibe eventos del servidor MCP clasico y los enruta al proxy. | Procesos y guilds | evento | n/a | no | si (shutdown_rx) | bajo |
| registry/service_manager.rs | linea 90 | Arranca un servicio always_on en un hilo bloqueante (para no bloquear el reactor) con timeout de 25 s. | Procesos y guilds | por peticion (arranque) | n/a | no | no | medio |
| registry/service_manager.rs | linea 168 | Watchdog que cada 30 s comprueba servicios always_on caidos y los reinicia. | Procesos y guilds | intervalo | hardcodeada (30 s) | no | no | alto |
| registry/supervisor.rs | linea 19 | Supervisor que comprueba periodicamente el estado de los guilds y los reinicia con backoff exponencial. | Procesos y guilds | intervalo | configurable (check_interval_secs) | no | no | alto |
| router/decision/service.rs | linea 788 | Consulta shadow asincrona al proveedor de decision para registrar su respuesta junto a la de reglas, sin alterar la respuesta emitida. | Decision | por peticion | n/a (shadow_timeout) | no | no | medio |
| router/embeddings.rs | linea 1233 | Ejecuta la inferencia ONNX sincrona (embed_batch) en el pool blocking para no bloquear workers async. | Inferencia | por peticion | n/a | no | no | medio |
| security/coherence_gate.rs | linea 680 | Aplica en blocking pool la cuarentena ASI06 decidida por Layer 4 sobre el nodo. | Decision | por peticion | n/a | no | no | medio |
| security/dispatch_executor.rs | linea 132 | Hilo OS que espera al proceso hijo de un dispatch para evitar zombis (reaper). | Coordinacion | evento (fin de dispatch) | n/a | no | no | alto |
| security/dispatch_executor.rs | linea 156 | Bucle que sondea la cola y ejecuta dispatches aprobados por humano, con rate limit por agente. | Coordinacion | intervalo | hardcodeada (POLL_INTERVAL) | no | no | alto |
| security/dispatch_subscriber.rs | linea 145 | Suscriptor del broadcast que encola PendingDispatch para eventos que cumplen contrato; nunca ejecuta. | Coordinacion | evento | n/a | no | no | medio |
| security/grants.rs | linea 338 | Reaper que cada 60 s purga grants pendientes caducados. | Decision | intervalo | hardcodeada (60 s) | no | no | alto |
| security/write_gate.rs | linea 126 | Marca el nodo en SQLite (cuarentena o revision) desde el gate de escritura, en blocking pool. | Decision | por peticion | n/a | no | no | medio |
| security/write_gate.rs | linea 170 | Juez asincrono fire-and-forget del gate de escritura (centinela/gramatica) que marca o pone en cuarentena el nodo. | Decision | por peticion (escritura) | n/a | no | no | medio |
| security/write_gate.rs | linea 217 | Pone el nodo en cuarentena cuando el juez LLM lo marca como SOSPECHOSO, en blocking pool. | Decision | por peticion | n/a | no | no | medio |
| transport/http/a2a.rs | linea 477 | Ejecuta en segundo plano un intent A2A message/send y actualiza el estado de la tarea. | Red | por peticion | n/a | no | no | medio |
| transport/http/a2a.rs | linea 588 | Emite por SSE el resultado de un intent A2A message/stream en segundo plano. | Red | por peticion | n/a | no | no | medio |
| transport/http/api_v1/api_admin.rs | linea 1665 | Purga manual (maintenance/purge) de FTS y nodos en blocking pool. | Memoria | por peticion | n/a | no | no | medio |
| transport/http/api_v1/api_audit.rs | linea 158 | Calcula el bloque de errores de golden-signals leyendo guild_audit_log en blocking pool. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/api_audit.rs | linea 239 | Agrega latencias de auditoria (conexion readonly) en blocking pool. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/api_canvas.rs | linea 115 | Tarea por cliente WebSocket que reenvia mensajes del broadcast global al cliente canvas. | Red | por conexion | n/a | no | parcial (aborta al cerrar socket) | medio |
| transport/http/api_v1/api_canvas.rs | linea 131 | Tarea por cliente que recibe del WebSocket, publica al broadcast y persiste el canvas. | Red | por conexion | n/a | no | parcial (aborta al cerrar socket) | medio |
| transport/http/api_v1/api_canvas.rs | linea 146 | Persiste el estado del canvas de forma asincrona cuando llega un mensaje de sync. | Red | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/api_federation.rs | linea 1129 | Bucle de auto-sync de federacion con guardia de solapamiento (un ciclo a la vez). | Red | intervalo | configurable (sync_interval_ms) | no | no | alto |
| transport/http/api_v1/api_guilds.rs | linea 90 | Registra traza de trabajo (stigmergia) fuera del camino de respuesta de la tool de guild. | Procesos y guilds | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/api_ingest.rs | linea 155 | Procesa la subida (vision/OCR) en segundo plano tras validar los guilds. | Procesos y guilds | por peticion | n/a | no | no | medio |
| transport/http/api_v1/api_memory.rs | linea 126 | Lanza el reindexado manual en background y emite evento de inicio. | Memoria | por peticion | n/a | no | no | medio |
| transport/http/api_v1/api_memory.rs | linea 143 | Guarda en segundo plano los embeddings recien calculados de cada nodo durante el reindexado. | Memoria | por peticion (por nodo) | n/a | no | no | medio |
| transport/http/api_v1/api_memory.rs | linea 311 | Escribe la auditoria de liberacion de cuarentena en blocking pool (fire-and-forget). | Memoria | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/api_monitor.rs | linea 632 | Sincroniza puntuaciones de reputacion de perfiles de agente a SilvaDB en background. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/api_outputs.rs | linea 28 | Lista runs del ledger de outputs en blocking pool. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/api_outputs.rs | linea 37 | Lee el manifest de un run del ledger en blocking pool. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/mod.rs | linea 317 | Drena una tarea de blackboard completada a SilvaDB como episodio (R12-1). | Memoria | evento | n/a | no | no | bajo |
| transport/http/api_v1/mod.rs | linea 779 | Registra la auditoria de una arista Silva (excepcion D-34 de guild) en blocking pool. | Memoria | por peticion | n/a | no | no | bajo |
| transport/http/api_v1/mod.rs | linea 1550 | Genera el session_digest del agente en background (R12-1). | Memoria | por peticion | n/a | no | no | bajo |
| transport/http/mod.rs | linea 732 | Bootstrap DHT/mesh (mainline/mDNS) en background tras arrancar el servidor HTTP. | Red | una vez/evento | n/a | no | no | medio |
| transport/http/mod.rs | linea 764 | Bucle de gossip del mesh en background (M14-B). | Red | intervalo | configurable (gossip.interval_secs) | no | no | alto |
| transport/http/mod.rs | linea 1025 | Pruner TTL opt-in del ledger de outputs cada 600 s, solo si TYLLUAN_OUTPUTS_TTL_SECS. | Coordinacion | intervalo | hardcodeada (600 s), opt-in por env | no | no | medio |
| transport/http/mod.rs | linea 1062 | Auto-link de peers al arrancar: espera health_ready y registra peers (deadline 300 s). | Red | una vez/evento | n/a | no | no | medio |
| transport/http/mod.rs | linea 1135 | Daemon de autoresearch/idle lab en background. | Memoria | intervalo (interno) | configurable (autoresearch) | no | no | medio |
| transport/http/mod.rs | linea 1142 | Listener P2P de dispatch (M14-F) si p2p.enabled. | Red | evento | configurable (p2p.enabled) | no | no | medio |
| transport/http/mod.rs | linea 1187 | Construye el repo map en background tras el arranque (espera bound_rx). | Coordinacion | una vez | n/a | no | no | bajo |
| transport/http/mod.rs | linea 1192 | Construccion bloqueante del repo map (spawn_blocking) tras el arranque. | Coordinacion | una vez | n/a | no | no | bajo |
| transport/http/sse.rs | linea 312 | Heartbeat SSE cada 15 s: emite latido y dispara tareas periodicas (health, decay, digests). | Red | intervalo | hardcodeada (15 s) | no | no | alto |
| transport/http/sse.rs | linea 329 | Actualiza los health scores de guilds cada 30 min desde el heartbeat. | Procesos y guilds | intervalo (120 ticks) | hardcodeada | no | no | medio |
| transport/http/sse.rs | linea 353 | Ejecuta el decay biologico periodico desde el heartbeat. | Memoria | intervalo | configurable (decay_interval_secs) | no | no | medio |
| transport/http/sse.rs | linea 390 | Genera el session_digest de sesiones SSE expiradas. | Memoria | evento | n/a | no | no | bajo |
| transport/http/sse.rs | linea 407 | Broadcaster de metricas en tiempo real cada 5 s. | Red | intervalo | hardcodeada (5 s) | no | no | alto |
| transport/mdns.rs | linea 75 | Hilo OS que escucha eventos mDNS y registra peers TylluanNexus descubiertos. | Red | evento | n/a | no | no | alto |
| transport/server/background_jobs.rs | linea 223 | Worker de trabajos de guild en background: sondea cada 2 s y procesa uno a uno. | Procesos y guilds | intervalo | hardcodeada (2 s) | no | no | alto |
| transport/server/handler_do/correct.rs | linea 114 | Marca valid_until del nodo antiguo en blocking pool al aplicar @correct. | Memoria | por peticion | n/a | no | no | medio |
| transport/server/handler_do/external_mcp.rs | linea 187 | Registra la auditoria de una llamada MCP externa en blocking pool (fire-and-forget). | Procesos y guilds | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/outcome.rs | linea 39 | Penaliza la leccion de intencion tras un rechazo (touch_node rejected). | Coordinacion | evento | n/a | no | no | bajo |
| transport/server/handler_do/outcome.rs | linea 193 | Registra la auditoria de la llamada tylluan_do y captura friccion en blocking pool. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/outcome.rs | linea 203 | Registra la traza de trabajo del fichero tocado por la tool (stigmergia). | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/outcome.rs | linea 212 | Aprende un routing anchor (embedding de la intencion) tras una llamada exitosa. | Coordinacion | por peticion | n/a | no | no | medio |
| transport/server/handler_do/outcome.rs | linea 228 | Actualiza el perfil/reputacion del agente tras la llamada. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/outcome.rs | linea 276 | Persiste una leccion de resultado en SilvaDB (lesson_drain). | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/outcome.rs | linea 310 | Guarda la leccion de routing tras la llamada. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/outcome.rs | linea 360 | Auto-enlaza nodos similares tras persistir un remember (auto_link_similar). | Memoria | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/prepare.rs | linea 363 | Ticker de progreso del guild call con heartbeat y timeout efectivo. | Procesos y guilds | por peticion | n/a | no | no | medio |
| transport/server/handler_do/routing.rs | linea 127 | Aplica decay a una leccion de routing expirada (mas de 30 dias). | Coordinacion | evento | n/a | no | no | bajo |
| transport/server/handler_do/routing.rs | linea 140 | Depreca y decae una leccion con tasa de rechazo alta. | Coordinacion | evento | n/a | no | no | bajo |
| transport/server/handler_do/routing.rs | linea 263 | Registra la identidad del agente en el handshake (nodo agent_identity). | Coordinacion | evento | n/a | no | no | bajo |
| transport/server/handler_do/routing.rs | linea 285 | Registra la traza de actividad de la tool en la zona del guild. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/routing.rs | linea 304 | Refuerza o crea la arista agente->guild ejecutada (stigmergia). | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handler_do/routing.rs | linea 327 | Extrae tripletas automaticamente del resultado de la tool (auto_extract_triples). | Coordinacion | por peticion | n/a | no | no | medio |
| transport/server/handler_graph.rs | linea 194 | Extrae tripletas retroactivas del grafo en background. | Memoria | por peticion | n/a | no | no | medio |
| transport/server/handler_remember.rs | linea 416 | Difunde conocimiento a los agentes conocidos por mailbox tras un remember. | Memoria | por peticion | n/a | no | no | bajo |
| transport/server/handler_remember.rs | linea 438 | Consolida la memoria del agente si procede tras un remember. | Memoria | por peticion | n/a | no | no | bajo |
| transport/server/handler_remember.rs | linea 541 | Extrae tripletas automaticamente del contenido recordado. | Memoria | por peticion | n/a | no | no | medio |
| transport/server/handler_remember.rs | linea 557 | Auto-enlaza nodos similares tras un remember de contenido corto. | Memoria | por peticion | n/a | no | no | bajo |
| transport/server/handlers.rs | linea 58 | Auto-bootstrap de la identidad minima del agente en su primera llamada. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handlers.rs | linea 302 | Registra la auditoria de una tool con aprobacion (human_intervention segun operador) en blocking pool. | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/handlers.rs | linea 608 | Audita cada llamada a tool soberana fire-and-forget (excepto tylluan_do). | Coordinacion | por peticion | n/a | no | no | bajo |
| transport/server/mod.rs | linea 397 | Guarda el Peer MCP del cliente cuando rmcp lo entrega (set_peer). | Red | evento | n/a | no | no | bajo |

## Menciones que el grep cuenta pero NO son spawns (dsh)

- router/embeddings.rs linea 903 — mencion en comentario
- router/embeddings.rs linea 919 — mencion en comentario
- router/embeddings.rs linea 1227 — mencion en comentario
- router/matcher.rs linea 234 — mencion en comentario
- router/scheduler/confusion.rs linea 27 — mencion en comentario
- security/write_gate.rs linea 9 — mencion en comentario
- security/write_gate.rs linea 19 — mencion en comentario
- transport/http/api_v1/api_audit.rs linea 152 — mencion en comentario
- transport/server/handler_do/correct.rs linea 109 — mencion en comentario
- transport/server/handler_do/external_mcp.rs linea 180 — mencion en comentario
- transport/server/handler_do/tests.rs linea 1272 — mencion en comentario
- transport/server/handler_do/correct.rs linea 123 — mencion dentro de una cadena (mensaje de warn), no es un spawn

## Ocurrencias en modulos de test (dsh, apartadas)

- config.rs: lineas 3533
- router/decision/adapters.rs: lineas 455, 462, 610, 618
- router/embeddings.rs: lineas 2071, 2078, 2085, 2094, 2095, 2096
- router/inference_points_runtime.rs: lineas 201, 208
- security/coherence_gate.rs: lineas 996, 1917, 2237
- security/dispatch_queue.rs: lineas 641, 642, 751, 752
- security/friction_log.rs: lineas 91, 874, 915, 945, 997, 1011
- security/sentinel.rs: lineas 836, 841
- transport/http/a2a_client.rs: lineas 594
- transport/http/api_v1/api_admin.rs: lineas 1855, 1912, 1997
- transport/http/api_v1/api_coloquio.rs: lineas 274
- transport/http/mod.rs: lineas 1981
- transport/http/oauth_consent.rs: lineas 924
- transport/server/handler_do/audit.rs: lineas 97
- transport/server/handler_do/coloquio_wait.rs: lineas 252
- transport/server/handlers.rs: lineas 802

## Top 5 de mayor riesgo (spawns reales dsh)

1. **transport/http/sse.rs 312 y 407 — bucles periodicos sin token**: heartbeat cada 15 s y broadcaster de metricas cada 5 s; el heartbeat ademas lanza spawns anidados (health, decay, digests) cada cierto numero de ticks, asi que el numero de tareas crece con las conexiones SSE. Sin CancellationToken ni presupuesto.
2. **transport/http/mod.rs 764 — gossip del mesh**: bucle con intervalo configurable que corre hasta que muere el proceso; sin token de apagado ni guardia de supervision.
3. **registry/lifecycle.rs 57, supervisor.rs 19, service_manager.rs 168 — loops de guilds**: reaper WAL, supervisor con backoff y watchdog de 30 s; ninguno usa GuardedTask ni token, y no participan en un apagado ordenado por tarea.
4. **security/dispatch_executor.rs 156 (loop) + 132 (thread::spawn) + transport/mdns.rs 75 (thread::spawn)**: dos hilos OS sin guardia (reaper de hijos y listener mDNS) y un bucle de ejecucion de dispatches sin cancelacion cooperativa.
5. **transport/server/background_jobs.rs 223 + handler_do (14 spawns por tylluan_do)**: worker que sondea cada 2 s sin token y 14 tareas fire-and-forget por llamada de tool (outcome/routing); el rate limiter acota agentes, no tareas vivas.

## No verificado

- Inventario estatico (grep + lectura de contexto); no se ejecuto el kernel ni se midieron tareas vivas por subruntime.
- Las columnas disparador/cadencia/guards/apagado/riesgo son lectura del codigo y contexto (+-6 lineas), no instrumentacion.
- Los 78 propositos se redactaron a mano a partir de la funcion/comentarios; en dos o tres filas el proposito es el del bloque contenedor, no una descripcion linea a linea.
- `apagado`: solo se marco `si` donde hay `shutdown_rx`/cancelacion explicita; el resto no participa en un apagado ordenado por tarea (el proceso termina por `process::exit`, como reporta Hermes).
- La nota sobre los 123 `tokio::spawn` de ADR-025 no se pudo reconciliar al 100% (no conozco el filtro exacto usado); el cuadre con el comando pedido es 191 = 63 + 128.
- No se toco codigo, ni E:\tylluan, ni el kernel vivo (:47004).
