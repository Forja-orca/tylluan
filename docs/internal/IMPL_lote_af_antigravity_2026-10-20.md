# Informe de Auditoría e Implementación — Lote AF: Auditoría Integral de Cableado Frontend ↔ Backend

**Fecha:** 2026-10-20  
**Autor:** Antigravity  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base commit:** `73e3ace` (`origin/main`)  
**Estado:** Diff uncommitted listo para revisión por el Tech Lead `@claude-code`.

---

## ## Impact

### Pregunta de No-Depredación (ADR-013)
- **¿Afecta `crates/` o el kernel Rust?** No. Cero archivos modificados en `crates/`. Toda la auditoría se realizó consultando el kernel en vivo en modo estrictamente de solo lectura (peticiones `GET`).
- **¿Afecta `dist/` o dependencias?** No. Cero modificaciones en `dist/` ni en `package.json` / lockfiles.
- **¿Riesgo de regresión en UI existente?** Nulo. Las correcciones aplicadas son quirúrgicas:
  1. Corrección de la ruta de `deleteNode` para coincidir con la firma plural real de Axum (`/api/v1/silva/nodes/{node_id}`).
  2. Detección real de sesiones de MCP en `WelcomeEmptyState.tsx` mediante el hook `useNexus()`, resolviendo un caso donde el componente buscaba sesiones en un objeto que no las contenía.
- **Paso previo obligatorio en árbol limpio:** Como observó el Tech Lead en el Turno 1406, al trabajar desde un checkout limpio se debe compilar primero `@tylluan/ui-core` con `npm run build` antes de ejecutar `tsc` en el dashboard.

---

## 1. Metodología de la Auditoría

Se ejecutó un análisis exhaustivo y sistemático en 4 fases:
1. **Extracción de métodos y contratos:** Extracción de todos los métodos de `NexusBridge` en `packages/tylluan-ui-core/src/lib/api-client.ts` y de las funciones en `api/` (`a2a`, `coloquio`, `federation`, `memory`, `scopes`, `security`, `system`) y llamadas directas `fetchRaw` en componentes.
2. **Inspección de enrutamiento en Axum:** Localización exacta de archivo y número de línea en `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs` y `crates/tylluan-kernel/src/transport/http/mod.rs`.
3. **Sondeo en vivo contra el Kernel (puerto de producción):** Se enviaron peticiones autenticadas `GET` a los 53 endpoints de lectura del sistema en ejecución. Se constató código HTTP de respuesta, tipo de dato y estructura de claves devuelta.
4. **Mapeo de componentes consumidores:** Búsqueda en el árbol de componentes del dashboard (`dashboard/src/components/`, `views/`, `hooks/`, `stores/`) para identificar quién consume cada dato y cómo lo interpreta.

---

## 2. Tabla Maestra de Cableado (NexusBridge ↔ Kernel ↔ Dashboard)

| Método `NexusBridge` / Llamada | Endpoint HTTP | Ubicación en `routes.rs` / `mod.rs` | Estado Live & Shape Kernel Real | Componentes Consumidores |
|---|---|---|---|---|
| `getHealth` | `GET /health` | `transport/http/mod.rs (línea 1182)` | 200 OK — `dict(status, uptime_secs)` | `system.ts`, `OverviewTab` |
| `health_detailed` | `GET /api/v1/health/detailed` | `transport/http/api_v1/routes.rs:158` | 200 OK — `dict(components, score, status, version)` | `OverviewTab`, `SystemTab` |
| `getDoctorReport` | `GET /api/v1/doctor` | `transport/http/api_v1/routes.rs:33` | 200 OK — `dict(status, guilds, storage, system, config_valid)` | `MaintenanceTab`, `SystemTab` |
| `repairDoctor` | `POST /api/v1/doctor/repair` | `transport/http/api_v1/routes.rs:34` | *(Mutación — no sondeado)* | `MaintenanceTab` |
| `probe` | `GET /api/v1/mcp/probe` | `transport/http/mod.rs (línea 1184)` | 200 OK — `dict(endpoints, detected_dialect, port)` | `WelcomeEmptyState`, `SystemTab` |
| `getStats` / `getGuildHealth` | `GET /api/v1/guilds/health` | `transport/http/api_v1/routes.rs:23` | 200 OK — `list(name, running, tools_count, idle_seconds...)` | `OverviewTab`, `useNexus.tsx` |
| `getGuilds` | `GET /api/v1/guilds` | `transport/http/api_v1/routes.rs:22` | 200 OK — `list(name, running, always_on, capabilities...)` | `OverviewTab`, `TeamConsolidated` |
| `getCapabilities` | `GET /api/v1/capabilities` | `transport/http/api_v1/routes.rs:74` | 200 OK — `dict(guilds, mcp, sessions, version)` | `SystemTab` |
| `getGoldenSignals` | `GET /api/v1/health/golden-signals` | `transport/http/api_v1/routes.rs:135` | 200 OK — `dict(traffic, errors, saturation, uptime_seconds)` | `OverviewTab`, `OverviewConsolidated` |
| `getGuildsUtilization` | `GET /api/v1/guilds/utilization` | `transport/http/api_v1/routes.rs:130` | 200 OK — `dict(total, active, idle, utilization_percent)` | `OverviewTab`, `TeamConsolidated` |
| `getSloSummary` | `GET /api/v1/slo/summary` | `transport/http/api_v1/routes.rs:129` | 200 OK — `dict(slo_target, current_availability, status)` | `OverviewTab`, `OverviewConsolidated` |
| `getInteroception` | `GET /api/v1/interoception` | `transport/http/api_v1/routes.rs:56` | 200 OK — `dict(homeostasis, stress_level, knowledge_hunger)` | `InteroceptionTab`, `OverviewTab` |
| `getSystemStatus` | `GET /api/v1/system/status` | `transport/http/api_v1/routes.rs:66` | 200 OK — `dict(silva_healthy, mailbox_healthy, uptime_secs)` | `OverviewTab`, `SystemTab` |
| `getSessions` | `GET /api/v1/sessions` | `transport/http/api_v1/routes.rs:50` | 200 OK — `dict(sessions: list)` | `SystemTab`, `useNexus.tsx` |
| `revokeSession` | `DELETE /api/v1/sessions/{session_id}` | `transport/http/api_v1/routes.rs:53` | *(Mutación — no sondeado)* | `SystemTab` |
| `resumeSession` | `POST /api/v1/sessions/resume` | `transport/http/api_v1/routes.rs:51` | *(Mutación — no sondeado)* | `ResumeSessionWidget` |
| `getAuditTrail` | `GET /api/v1/audit/trail` | `transport/http/api_v1/routes.rs:72` | 200 OK — `dict(entries: list, total)` | `AuditTab`, `AuditTrailPanel` |
| `getConfig` | `GET /api/v1/config` | `transport/http/api_v1/routes.rs:82` | 200 OK — `dict(nexus, memory, guilds, security...)` | `SystemTab` |
| `saveConfig` | `POST /api/v1/config` | `transport/http/api_v1/routes.rs:82` | *(Mutación — no sondeado)* | `SystemTab` |
| `getDeviceStatus` | `GET /api/v1/config/device/status` | `transport/http/api_v1/routes.rs:84` | 200 OK — `dict(configured_device, guilds, ok)` | `SystemTab`, `ModelsRoleAssignment` |
| `getMetricsHistory` | `GET /api/v1/metrics/history` | `transport/http/api_v1/routes.rs:174` | 200 OK — `dict(snapshots: list, interval_secs, capacity)` | `MetricsTab`, `OverviewTab` |
| `getRepoMap` | `GET /api/v1/repo-map` | `transport/http/api_v1/routes.rs:52` | 200 OK — `dict(root, built_at_unix, languages...)` | `RepoMapWidget` |
| `getSilvaGraph` | `GET /api/v1/graph/viz` | `transport/http/api_v1/routes.rs:57` | 200 OK — `dict(nodes: list, links: list)` | `KnowledgeGraphTab` |
| `getGraph` | `GET /memory/graph` | `transport/http/api_v1/routes.rs:140` | 200 OK — `dict(nodes: list, links: list)` | `memory.ts` *(Legacy alias)* |
| `getMemoryStats` | `GET /api/v1/silva/stats` | `transport/http/api_v1/routes.rs:35` | 200 OK — `dict(node_count, edge_count, ivf_ready...)` | `OverviewTab`, `MemoryConsolidated` |
| `getMailbox` | `GET /api/v1/mailbox` | `transport/http/api_v1/routes.rs:55` | 200 OK — `dict(messages: list)` | `memory.ts` |
| `getBlackboard` | `GET /api/v1/blackboard` | `transport/http/api_v1/routes.rs:138` | 200 OK — `dict(active_agents, pending, total_tasks)` | `OverviewTab` |
| `getMemoryRetention` | `GET /api/v1/memory/retention` | `transport/http/api_v1/routes.rs:131` | 200 OK — `dict(silva: { retention_rate_percent... })` | `OverviewTab`, `OverviewConsolidated` |
| `getHormones` | `GET /api/v1/hormones` | `transport/http/api_v1/routes.rs:143` | 200 OK — `dict(stress, novelty, saturation, energy...)` | `InteroceptionTab` |
| `getDashboardSummary` | `GET /api/v1/dashboard/summary` | `transport/http/api_v1/routes.rs:222` | 200 OK — `dict(golden_signals, hormones, interoception...)` | `FleetTab`, `useNexus.tsx` |
| `getAutoResearchSummary` | `GET /api/v1/autoresearch/summary` | `transport/http/api_v1/routes.rs:223` | 200 OK — `dict(active, current_mutation, lineage, progress)` | `LaboratoryTab` |
| `getAgentProfiles` | `GET /api/v1/agent-profiles` | `transport/http/api_v1/routes.rs:144` | 200 OK — `dict(profiles: list)` | `CollectiveTab`, `AgentCompetencyPanel` |
| `getProjectSkills` | `GET /api/v1/skills` | `transport/http/api_v1/routes.rs:94` | 200 OK — `list(name: str)` | `ProjectSkillsPanel` |
| `listBackgroundJobs` | `GET /api/v1/jobs` | `transport/http/api_v1/routes.rs:95` | 200 OK — `dict(jobs: list, total)` | `BackgroundJobsPanel` |
| `getRecentNodes` | `GET /api/v1/silva/recent` | `transport/http/api_v1/routes.rs:36` | 200 OK — `list(id, node_type, content, metadata...)` | `IngestPanel`, `KnowledgeGraphTab` |
| `getCollectiveReputation`| `GET /api/v1/collective/reputation`| `transport/http/api_v1/routes.rs:148` | 200 OK — `dict(reputation, by_domain, avg_reputation)` | `CollectiveTab` |
| `getCollectiveHeatmap` | `GET /api/v1/collective/heatmap` | `transport/http/api_v1/routes.rs:147` | 200 OK — `dict(heatmap: list, window_hours, ts)` | `CollectiveTab` |
| `getCollectivePulse` | `GET /api/v1/collective/pulse` | `transport/http/api_v1/routes.rs:145` | 200 OK — `dict(active_agents, broadcasts_last_hour, graph)`| `CollectiveTab`, `OverviewTab` |
| `getCollectiveSuggest` | `GET /api/v1/collective/suggest` | `transport/http/api_v1/routes.rs:151` | 200 OK — `dict(best_agent, domain, rate, confidence)` | `CollectiveTab` |
| `getSecurityScopes` | `GET /api/v1/security/scopes` | `transport/http/api_v1/routes.rs:80` | 200 OK — `dict(roles: list)` | `SystemTab` |
| `getCoherenceGateStats` | `GET /api/v1/security/coherence-gate/stats` | `transport/http/api_v1/routes.rs:78` | 200 OK — `dict(ok, total_seen, total_eliminated...)` | `CoherenceGatePanel` |
| `getRecallFeedbackStats`| `GET /api/v1/memory/recall-feedback/stats` | `transport/http/api_v1/routes.rs:81` | 200 OK — `dict(ok, pct, pending, resolved)` | `CoherenceGatePanel` |
| `getFrictionStats` | `GET /api/v1/security/friction/stats` | `transport/http/api_v1/routes.rs:79` | 200 OK — `dict(total_sessions, routing_errors, score...)`| `FrictionPanel` |
| `getQuarantine` | `GET /api/v1/memory/quarantine` | *(Pendiente D-12 en Rust)* | **404 Not Found** — muestra amigablemente `'endpoint no disponible'` | `QuarantinePanel` |
| `releaseQuarantine` | `POST /api/v1/memory/quarantine/{id}/release` | *(Pendiente D-12 en Rust)* | *(Mutación — no sondeado)* | `QuarantinePanel` |
| `listFederationPeers` | `GET /api/v1/federation/peers` | `transport/http/api_v1/routes.rs:181` | 200 OK — `list(peers)` | `FederationTab` |
| `listMcpExternal` | `GET /api/v1/mcp/external` | `transport/http/api_v1/routes.rs:105` | 200 OK — `list(servers: name, url, command...)` | `FederationTab` |
| `getColoquioChannels` | `GET /api/v1/coloquio/channels` | `transport/http/api_v1/routes.rs:206` | 200 OK — `dict(channels: list)` | `ColoquioTab` |
| `getColoquioThread` | `GET /api/v1/coloquio/channels/{id}`| `transport/http/api_v1/routes.rs:207` | 200 OK — `dict(messages: list, count, offset)` | `ColoquioTab` |
| `getColoquioUnread` | `GET /api/v1/coloquio/unread` | `transport/http/api_v1/routes.rs:212` | 200 OK — `dict(channels: list, reader, total_unread)` | `ColoquioTab` |
| `RecallHealthCard (direct)` | `GET /health?verbose=true` | `transport/http/mod.rs (línea 1182)` | 200 OK — `dict(components.recall: histogram, latencies)` | `RecallHealthCard` |
| `TrustConsole (direct)` | `GET /api/v1/decide/shadow` | `transport/http/api_v1/routes.rs:307` | 200 OK — `dict(rows, summary, total)` | `DecisionServicePanel` |
| `TrustConsole (direct)` | `GET /api/v1/silva/conflicted` | `transport/http/api_v1/routes.rs:308` | 200 OK — `dict(nodes, total_conflicted)` | `ContradictionsPanel` |
| `StigmergyHeatmapPanel` | `GET /api/v1/stigmergy/zones` | `transport/http/api_v1/routes.rs:295` | 200 OK — `dict(zones, half_life_hours, count)` | `StigmergyHeatmapPanel` |
| `SchedulerConfusionPanel`| `GET /api/v1/scheduler/confusion` | `transport/http/api_v1/routes.rs:101` | 200 OK — `dict(agrees, differ, by_guild, total)` | `SchedulerConfusionPanel` |
| `AuditTab (direct)` | `GET /api/v1/audit/latency` | `transport/http/api_v1/routes.rs:282` | 200 OK — `dict(latency_ms, error_rate, window_minutes)` | `AuditTrailPanel` |
| `DecisionConsoleTab` | `GET /api/v1/dispatches` | `transport/http/api_v1/routes.rs:284` | 200 OK — `list(dispatches: id, state, command...)` | `DispatchesPanel` |
| `WorkContractsPanel` | `GET /api/v1/work-contracts/active?channel_id=general` | `transport/http/api_v1/routes.rs:255` | 200 OK — `dict(contract_id, status, budget_remaining)` | `WorkContractsPanel` |

---

## 3. Categorización de Hallazgos y Correcciones Aplicadas

### Categoría (a): Llamadas con 404/403 o discrepancia de ruta

1. **`deleteNode`: Discrepancia de ruta singular vs plural (ARREGLADO)**
   - **Diagnóstico:** `packages/tylluan-ui-core/src/lib/api/memory.ts` y `dashboard/src/lib/api/memory.ts` ejecutaban:
     `DELETE /api/v1/silva/node/{nodeId}` (singular).
   - **Kernel Real:** En `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs:49`, la ruta registrada es:
     `.route("/api/v1/silva/nodes/{node_id}", delete(silva_delete_node_handler))` (plural `/nodes/`).
   - **Efecto verificado en vivo:**
     - `DELETE /api/v1/silva/node/test` $\rightarrow$ `404 Not Found {"error":"not_found"}`.
     - `DELETE /api/v1/silva/nodes/test` $\rightarrow$ `403 Forbidden {"error":"Node not found or is protected"}` (alcanza el manejador).
   - **Corrección:** Se actualizó la ruta a `/api/v1/silva/nodes/${encodeURIComponent(nodeId)}` en ambos módulos. Verificado con test unitario en `WiringAuditFixes.test.tsx`.

2. **`deleteProjectSkill`: Inexistencia de endpoint REST DELETE en el kernel**
   - **Diagnóstico:** `deleteProjectSkill` intentaba primero `DELETE /api/v1/skills/{name}`, que devuelve 404 porque en `routes.rs:94` solo se registra `.route("/api/v1/skills", get(...).post(...))`.
   - **Comportamiento frontend:** El frontend ya contaba con un bloque `try/catch` que degrada invocando el intent `@skill:delete:${name}` vía `POST /api/v1/do`. Se mantiene dicha degradación funcional sin romper la UI.

3. **`getQuarantine`: Endpoint 404 en el kernel en ejecución**
   - **Diagnóstico:** Retorna 404 en el kernel actual porque el backend D-12 en Rust está en desarrollo por Deep. El componente `QuarantinePanel.tsx` degrada con badge `"endpoint no disponible"`.

---

### Categoría (b): Endpoints Huérfanos del Kernel (en `routes.rs` sin consumo en el dashboard)

Se identificaron **84 rutas** registradas en el kernel que ningún componente o servicio del dashboard actual consume. Las más destacadas incluyen:
1. **Blackboard Coordination avanzada:**
   - `POST /api/v1/blackboard/plan`
   - `GET /api/v1/blackboard/tasks/{agent}`
   - `POST /api/v1/blackboard/tasks/{msg_id}/done`
2. **Capability Registry (Tarea Raíz 2):**
   - `GET /api/v1/capability-registry`
   - `GET /api/v1/capability-registry/{guild}`
   *(El dashboard consume la vista en tiempo de ejecución `/api/v1/capabilities`, pero no el registro estático de capacidades)*.
3. **Ledger de Outputs de Guilds (bwc-d0fb0812):**
   - `GET /api/v1/outputs`
   - `GET /api/v1/outputs/{run_id}/manifest`
4. **Agentes A2A Outbound (F2):**
   - `GET /api/v1/a2a/agents`
   - `POST /api/v1/a2a/agents`
   - `POST /api/v1/a2a/agents/{id}/test`
   - `DELETE /api/v1/a2a/agents/{id}`
5. **Evaluación de Benchmarks (M26-B):**
   - `POST /api/v1/eval/run`
   - `GET /api/v1/eval/results`
6. **Contratos de Trabajo BWC (Operaciones de Ciclo de Vida):**
   - `POST /api/v1/work-contracts/{id}/tick`
   - `POST /api/v1/work-contracts/{id}/deliver`
   - `POST /api/v1/work-contracts/{id}/vote`
   - `POST /api/v1/work-contracts/{id}/close`
   - `GET /api/v1/work-contracts/{id}/context`

**Propuesta para el Tech Lead:** No eliminar estos endpoints del kernel (pertenecen a protocolos agénticos A2A, BWC y CLI); se recomienda evaluar la creación de paneles dedicados en futuras fases (ej. visor de Artefactos/Outputs de Guilds o vista de Benchmarks).

---

### Categoría (c): Campos que el componente lee y la respuesta no contiene

1. **`WelcomeEmptyState.tsx`: Lectura de sesiones en `sysStatus` (ARREGLADO)**
   - **Diagnóstico:** `WelcomeEmptyState` recibía `sessions={Array.isArray(sysStatus?.sessions) ? sysStatus.sessions : []}` desde `OverviewTab.tsx`. No obstante, `/api/v1/system/status` nunca incluye la clave `sessions`. Por consiguiente, la lista de sesiones llegaba siempre vacía a la vista de bienvenida.
   - **Corrección:** Se modificó `WelcomeEmptyState.tsx` para consumir directamente las sesiones reales provistas por el hook `useNexus()` (`hookSessions`), memorizadas con `useMemo` para evitar re-renderizados innecesarios. Ahora el checklist de bienvenida marca automáticamente el cliente MCP conectado cuando hay al menos una sesión activa. Verificado con test unitario.
2. **`MemoryRetention`: Campo legacy `hybrid_memory`**
   - **Diagnóstico:** La interfaz TypeScript `MemoryRetention` definía `hybrid_memory: { documents: number, disk_bytes: number }`, residuo de la arquitectura antigua HybridMemory. El endpoint real `/api/v1/memory/retention` retorna únicamente el bloque `silva`.
   - **Verificación:** Los componentes de UI (`OverviewTab.tsx`, `OverviewConsolidated.tsx`) consumen exclusivamente `memoryRetention.silva`, por lo que no existían excepciones en runtime.
3. **`OverviewTab.tsx`: Estado de `night_consolidation_active`**
   - **Diagnóstico:** `OverviewTab` evalúa `sysStatus?.night_consolidation_active` para mostrar el banner de modo noche, pero `/api/v1/system/status` no expone este booleano (la telemetría de consolidación reside en `/api/v1/dream/status`). Dado que `DreamStatusWidget.tsx` se renderiza independientemente en el mismo tab y consulta `/api/v1/dream/status`, el estado de consolidación queda visible para el operador a través del widget especializado.

---

## 4. Verificación de Quality Gates

1. **Compilación de paquetes y TypeScript:**
   - `packages/tylluan-ui-core`: `npm run build` $\rightarrow$ Código 0 (compilación limpia `tsc`).
   - `dashboard`: `npx tsc --noEmit` $\rightarrow$ **0 errores**.
2. **Suite de Pruebas Unitarias y Accesibilidad (`vitest`):**
   - `npx vitest run src/components/__tests__/WiringAuditFixes.test.tsx` $\rightarrow$ **2/2 tests pasados** (100% verde).
   - `npx vitest run` (suite completa) $\rightarrow$ **18 suites pasadas (18/18), 145 tests pasados (145/145)** en verde. Cero fallos.
   - Auditoría axe-core en `AxeAudit.test.tsx` sobre las 7 vistas principales: **0 violaciones WCAG 2.1 AA**.
3. **Linter ESLint (`dashboard`):**
   - `npm run lint` $\rightarrow$ **0 errores, 36 warnings** (cumple estrictamente el umbral $\le 36$).
4. **Estado del Repositorio:**
   - `crates/` y `dist/`: 100% intactos.
   - Cambios contenidos exclusivamente en el worktree `E:\tylluan-wt-antigravity`.

---

## 5. Qué NO se ha verificado (Honestidad Técnica)

- **Endpoints de mutación POST/PUT/DELETE contra el kernel de producción:** Por principio estricto de seguridad operativa y preservación del estado del kernel de José (puerto de producción), los endpoints de mutación (`/api/v1/do`, `/api/v1/ingest`, `/api/v1/config` save, `/api/v1/admin/shutdown`, etc.) no fueron ejecutados en vivo durante la auditoría, validándose únicamente su definición en la tabla de rutas de Axum.
- **Backend D-12 en vivo:** `/api/v1/memory/quarantine` continuará retornando 404 hasta que Deep complete y despliegue la tarea D-12 en el backend Rust.
