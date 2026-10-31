# T-DSH-7 — Uso real de Tylluan: auditoría, mailbox y silva en copias de solo lectura

- **Agente:** dsh
- **Fecha:** 2026-10-20 (copia de datos tomada 2026-10-19, 23:25-23:30 con kernel 2458b4a en marcha)
- **Base del repo:** `01b6aa4` == origin/main al entregar (ff limpio; los entrantes desde `17465b9` — base cuando arranqué el análisis — no tocan `data/` ni los scripts de análisis; kernel en vivo al copiar: 2458b4a)
- **Tarea:** briefing TL turno 1366 — cinco preguntas sobre uso real con cifra y comando, tabla + 5 conclusiones + 3 recomendaciones.

## Método (reproducible)

1. `Copy-Item data\{audit,mailbox,silva}.db{,-wal,-shm}` a un directorio temporal propio **fuera del repo** (única operación sobre `data/`: lectura; cero escrituras ahí).
2. Python del entorno de trabajo con `sqlite3`; apertura de las **copias** (la recuperación del WAL requiere que el fichero copiado sea escribible — el original nunca se abrió en modo escritura).
3. `PRAGMA quick_check` en las tres copias: **ok** (audit 5,8 MB, mailbox 4,2 MB, silva 164,1 MB).
4. Ventana de datos: `guild_audit_log` cubre 2026-07-11 → 2026-10-19 (el `audit_log` antiguo, 194 filas, muere el 2026-07-07 con otro esquema). Notas por pregunta bajo «no verificado».
5. Exclusión «benchmarks» aplicada a: `bench_worker_0..7`, `antigravity:teb_pilot`, `latency_bench`, `ci-latency-bench`, `warmup` (3570 de 5945 filas de auditoría = 60 %). Criterio propio ampliado (el briefing citaba `bench_worker_*` y `teb_pilot`): `latency_bench`/`ci-latency-bench`/`warmup` son harness, no agentes.

SQL base usado en todo el informe (adaptando SELECT por pregunta):

```sql
SELECT tool_name, COUNT(*) FROM guild_audit_log
WHERE agent_id NOT IN (<benchmarks>) GROUP BY 1 ORDER BY 2 DESC;
```

## Q1 — Qué herramientas se usan y por quién (excluidos benchmarks)

**2375 llamadas reales en 14 semanas** (5945 totales, 3570 de benchmarks).

| Herramienta | Llamadas reales |
|---|---|
| post_to_channel (coloquio) | 1116 |
| read_channel (coloquio) | 333 |
| tylluan_recall | 200 |
| bash_execute | 166 |
| tylluan_remember | 121 |
| whoami | 38 |
| system_cpu | 38 |
| tylluan_graph | 25 |
| list_available_guilds | 24 |
| system_memory / system_disk / system_metrics | 22 / 20 / 15 |
| agent_bootstrap / list_pending_actions | 17 / 17 |
| web_search / doctor_diagnose / tylluan_think / state_restore | 16 / 15 / 14 / 15 |

**Por quién:** claude-code 925, (sin atribuir) 426, anonymous 274, deep 221 (`Deep` añade 15 en otra caja: identidad partida por mayúsculas), antigravity 167, coordinator-worker 103, tylluan-cli 97, **dsh 27**, claude-planificacion 25, buffy 21, opencode 18, mimo 14, qwen 6, codex 3, resto ≤ 6.

Ayunos: el guild coloquio concentra **1462 de 2375 llamadas reales (61,6 %)**; memoria son 321 (13,5 %). El segundo consumidor de `tylluan_recall` es `anonymous` (126 de 200 = 63 %): clientes MCP que nunca hicieron `register_identity`. Y ojo: las 27 de dsh **infravaloran** mi actividad real — los posts que hago por REST no pasan por el audit de guilds (ver recomendación 1).

## Q2 — Ratio recall/remember por agente y semana

**Global real: 200 recalls / 121 remembers = 1,65:1.** 12 identidades distintas hacen recall; 11, remember (de ~28 vistas en el log).

| Agente | recall | remember |
|---|---|---|
| anonymous | 126 | 55 |
| deep (+Deep) | 31 | 17 |
| claude-code | 10 | 21 |
| antigravity | 17 | 5 |
| tylluan-cli | 6 | 0 |
| claude-planificacion | 0 | 10 |
| claude-code-predecesor | 0 | 6 |
| buffy / mimo / opencode | 1 / 1 / 2 | 2 / 2 / 1 |

Por semana ISO (todos los agentes reales): pico en **W30 (36 recalls)**; W33 19; W35 9+25mem; **descolgada en octubre: W40 1, W41 4, W42 3 recalls en total** mientras la flota sigue publicando ~100 mensajes/semana. El único agente con más remember que recall es claude-code (21/10) — el TL escribe; casi nadie lee.

## Q3 — Recalls con 0 resultados y latencia

**No calculable con el log actual — y eso es el hallazgo.** Desglose:

- Recalls reales: 196 `status=ok`, 4 `error` (2,0 % de fallo).
- **Preview: 0 de 200 recalls reales tienen `result_preview`**; los 960 previews existentes los produjo íntegra y únicamente la ruta de bench/teb_pilot.
- **Latencia real: 71 filas con `latency_ms` y las 71 valen 0; 129 NULL.** La latencia solo se registra en bench: p50 = 0 ms (1274 de 2234 también ceros), p95 = 94 ms, max 25516 ms — ni ahí es fiable; parece medir primer byte, no resultado.
- `recall_misses`, la tabla diseñada para esto: **0 filas escritas jamás**.

Es decir: el camino de producción de `tylluan_recall` no ancla ni «cuántos resultados» ni latencia; hoy «¿funciona el recall?» es indistinguible de «¿se ejecuta?» (comando probado: `SELECT result_preview, latency_ms FROM guild_audit_log WHERE tool_name='tylluan_recall' AND agent_id NOT IN (<bench>)` → todo vacío/0). Los 321 `recall_feedback` reales (claude-code 55, deep 51+, antigravity 25, menos lo de teb) prueban que **cuando hay resultados se guardan ranks**, pero no hay contrapartida de «0 resultados».

## Q4 — Nodos silva por semana/provenance y nunca recuperados

**10 471 nodos vivos; 6636 creados en las últimas 14 semanas ISO (media ~474/semana; pico W41 867).**

| Provenance | Nodos | % |
|---|---|---|
| unverified | 8958 | 85,5 |
| agent_generated | 1512 | 14,4 |
| guild_stub | 1 | 0,01 |

Tipos top: code_entity 2717, episode 1818, coloquio_memory 1804, routing_anchor 1688, agent_summary 928, session_digest 689, summary 257, lesson 213.

**Nunca recuperados:**

- Con traza `node_traces.trace_type='recall'`: **8589 de 10471 (82,0 %) jamás fueron recuperados**. Solo 1882 nodos vivos tienen ≥1 recall, sobre los que caen 47628 recalls (media **25 toques por nodo recuperado**: rico-gets-richer). (5919 node_ids con traza recall incluyen 4037 ya borrados de `nodes`.)
- Con `access_count=0`: 1732 (16,5 %) — concentrados en octubre (1240: 1007 agent_generated + 233 unverified): lo más reciente escrito no se releído casi nada (el contador es de alta reciente, no histórico).
- Extras de calidad: `nodes_fts_docsize` tiene 23787 filas frente a 10471 nodos (desincronía FTS, x2,3).

## Q5 — Coloquio: agente-agente vs humano

**2169 mensajes en 9 canales** (general 1242, equipo 489, mision-activa 404, resto <15):

| Tipo | Mensajes | % |
|---|---|---|
| agente | 1880 | 86,7 |
| system (avisos kernel) | 250 | 11,5 |
| humano (jose 24 + 'user' 14) | 39 | **1,8** |

El **61,2 %** de los mensajes de agente (1149/1878) llevan @mención a otro agente o `reply_to_turn` — bucle agente↔agente sostenido sin intervención. Actividad humana por mes: 2026-06 15, **2026-07 1**, 2026-09 6, 2026-10 17 — el humano aparece en ráfagas de decisión, no habita el canal. El mail agente↔agente (`agent_mail`) es marginal: 74 mensajes totales (claude-code 37, antigravity 15, dsh 12, deep 7).

## Conclusiones (5)

1. **Tylluan se usa hoy como tablón de mensajes entre agentes, no como memoria.** El 61,6 % de las llamadas reales son coloquio (post 1116 + read 333); memoria, 13,5 %. Las cifras crudas sin excluir benchmarks (2504 recalls) multiplican x12,5 el uso real de memoria (200).
2. **El bucle de recuperación está muriendo:** 36 recalls/semana en W30 → 3 en W41/W42, con la flota publicando igual. Se escribe memoria (474 nodos/semana) y no se lee: 82 % de nodos sin una sola recuperación; los recalls que sí ocurren se concentran en 1882 nodos (25 toques de media).
3. **La observabilidad de producción está rota justo donde más se necesita:** recalls reales sin preview, sin latencia y sin filas en `recall_misses`; los únicos datos de resultados/latencia los genera el harness de bench. Cualquier decisión sobre «¿se usa la memoria?» tomada con estos logs es alucinación con forma de cifra.
4. **Las identidades son ficción a un tercio de la casa:** 63 % de los recalls los firma `anonymous` y 426 llamadas no tienen agente; hay identidades partidas (`deep`/`Deep`). El ratio «por agente» de Q2 es la mejor aproximación posible, no un dato limpio.
5. **Coloquio es un sistema autónomo con humano periférico (1,8 %).** 61 % de los mensajes de agentes van dirigidos a otros agentes; el humano entra en ráfagas (07: 1 mensaje; 10: 17) y el kernel emite 250 avisos propios. Esto es lo que José dice vigilar: el bucle ya es cerrado — le falta el consumidor de memoria.

## Recomendaciones (3, concretas)

1. **Arreglar la observabilidad del recall ANTES de ninguna decisión de uso** (tarea de kernel pequeña, sin cambiar defaults): grabar `result_count` + `result_preview` + latencia real de `tylluan_recall` en `guild_audit_log` para todas las rutas (hoy solo la de bench), poblar `recall_misses` con el camino vacío ya existente, y auditar también los posts REST a coloquio (mi propia actividad real es invisible: 27 llamadas auditadas vs ~150 posts reales). Sin esto, cualquier dashboard de uso miente.
2. **Cerrar el bucle read_channel → recall → post (opt-in, default OFF):** inyectar como contexto los 2-3 nodos `node_communities` relevantes del canal cuando un agente lee o le mencionan — aprovechando que ya existen 1804 nodos `coloquio_memory` — y penalizar en decaimiento los nodos sin trazas (82 % del corpus). Es la palanca directa para que los 474 nodos/semana dejen de ser escritura ciega; nace apagado por la regla de opt-in y pide decisión de José.
3. **Identidad obligatoria al conectar:** MCP `initialize` debería exigir `agent_id` (hay `.tylluan/agents.toml` y ACL ya hechas), con periodo de gracia y log de anónimos. Mientras el 63 % del recall sea anónimo, ni las políticas de decay, ni los permisos, ni «quién usa qué» son gobernables — y `mcp_sessions` (104 sesiones, many `client_name` genéricos) demuestra que es barato de aplicar.

## ## Impact

- **Cero cambios de código y cero escrituras en producción**: este informe es el único fichero nuevo del repo; los análisis corrieron sobre copias fuera del repo (quick_check ok); `data/` solo se leyó.
- Los hallazgos afectan a decisiones de priorización (observabilidad del recall, opt-in de contexto automático, identidad MCP) — todas requieren decisión TL/José; aquí no se activa nada.
- Sensibilidad: el informe contiene cifras agregadas de uso interno de la flota (volumen, canales, agentes). Sin secretos: no se extrajo ninguna columna de tokens (`a2a_agents.auth_token`, 0 filas) ni contenido de mensajes; solo conteos.
- Riesgo de reuso: las cifras dependen de la ventana de copia (2026-10-19 23:25) y del kernel `2458b4a`; repetible con los comandos SQL citados.

## No verificado

- Recalls con 0 resultados y latencia reales: **indelegable del log actual** (vacío en ruta de producción) — lo reportado es el hueco, no una estimación.
- Que `anonymous` = clientes MCP sin `register_identity` (hipótesis plausible por el patrón de tools; no verificada contra código del handshake en esta tarea).
- Qué purgó los 4037 node_ids con traza recall y sin nodo vivo (decay u otro; no investigado).
- Que 'user'/14 mensajes humanos adicional a jose sean humanos reales (autor `user` puede ser harness).
- Semántica exacta de `access_count` (parece alta reciente: solo marca octubre) frente a las 47628 trazas históricas — ambas métricas se dan juntas precisamente por esto.
- Los tipos mixtos de `coloquio_messages.created_at` (int/str) se normalizaron con `CASE` — el desglose mensual humano puede desviarse en ±1 mensaje.
- Cobertura histórica: `guild_audit_log` empieza 2026-07-11; el `audit_log` previo (otra forma, 194 filas) quedó fuera de las ratios.
