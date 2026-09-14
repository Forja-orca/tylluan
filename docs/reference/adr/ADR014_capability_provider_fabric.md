# ADR-014 — Capability Provider Fabric: de "guild router" a tejido de capacidades

**Status:** Draft (pendiente de validación del Tech Lead — Claude Code). Documento de diseño only — **cero cambios de código**; este ADR no adopta nada por sí mismo. Igual que el gate de cutover del WS3, la adopción no es de quien escribe el documento.
**Date:** 2026-09-14
**Authors:** Buffy (documento; dirección heredada del audit externo 2026-09-13, adoptado como ciclo WS8 del plan aprobado)
**Depends on:** ADR-009 (contrato declarativo de agentes), ADR-013 (non-predation — el fabric DEBE respetar las reglas de rollout opt-in/default-off y declaración de impacto), G6 (identity gate — `61fb702`), WS2/WS3/WS5 (infraestructura de decisión y observación de esta campaña, commits `9fc8bfb`/`8ac01f4`/`73c3f12`), CONTRACT-01 (5 tools soberanos, inviolable)
**Implements:** la dirección `Guild → Capability → Provider → Scheduler` del audit externo 2026-09-13 (§22-23 "Capability Provider Fabric", §26 deuda semántica) — convertida en contrato de diseño verificable, no en prosa aspiracional.

---

## Context

Tylluan hoy decide *dónde* se ejecuta una intención con una cadena de piezas reales y verificadas, pero cada capa usa su propio vocabulario para lo mismo:

- El **router** (`handler_do/routing.rs`, `router/matcher/`) resuelve guild por prefijos, triggers y matching semántico.
- El **Complextimator** (`router/complexity.rs`) puntúa complejidad y — desde WS2 (`9fc8bfb`) — distingue *querer delegar* (`wants_delegation`, línea 232) de *ser elegible* para coordinación (`coordinator_eligible`, línea 249): complejidad ≠ permiso de delegación, cerrado el hijack de J-13 con test de regresión.
- El **Cognitive Scheduler** (`router/scheduler/`) emite veredictos completos (`decide(ctx, score) → SchedulingDecision`, `decision.rs:61`, 6 `ExecutionClass` en `types.rs:50-63`) pero **solo en modo observación** — el veredicto se registra (`observe_scheduling` en `handler_do/mod.rs:486`) y se descarta.
- El **riesgo** ya es por-herramienta, no por-guild: `TOOL_METADATA` (`registry/tools.rs:40`) tiene **151 entradas verificadas por conteo el 2026-09-14** (una por función real; el comentario en `types.rs:81` ya lo decía — esta vez contado, no citado), consumidas por `check_tool_risk()` (`logic.rs:10`) con su cadena de 4 niveles.
- La **federación** ya tiene su propio registro de capacidades: `CapabilityRegistry` en `tylluan-link/src/capability.rs:24` (peers → `CapabilityRecord` con TTL), alimentando `DispatchRouter`.

El audit externo de 2026-09-13 (§26) nombró el riesgo de fondo: **deuda semántica**. `guild`, `tool`, `capability`, `provider`, `subtool`, `agent` y `service` aparecen representados en formas distintas sin jerarquía formal única. Ese mismo día el proyecto ya había detectado el problema en primera persona: la identidad de guilds necesitó G6 (`tools/guild_catalog.py` como fuente única, con `scripts/check_guild_consistency.py` como gate de divergencia, commit `61fb702`) y el J-13 rerun demostró que "accuracy del matcher" mezclaba routing con validación de argumentos (MD-3 del registro de deuda de medición).

Este ADR propone la jerarquía formal que falta — **sin implementarla**. Es el documento de diseño para que la Fase 3 del Scheduler (cutover), el TaskContext v2 y la gobernanza de recursos (WS4 de Deep, lifecycle) tengan un contrato común en vez de tres vocabularios en convivencia tensa.

## Non-goals

- **No implementar nada.** Cada fase de migración requiere su propio ciclo con validación; este ADR solo define el contrato y el orden.
- **No toca los 5 tools soberanos.** CONTRACT-01 es inviolable: el fabric es *infraestructura de decisión*, no una sexta herramienta.
- **No reemplaza al router heurístico de la noche a la mañana.** El patrón del proyecto es observación-antes-de-cutover (CoherenceGate Layer 4, WS3). El fabric nace observando, igual.
- **No unifica el risk model.** `TOOL_METADATA` ya es por-herramienta; el fabric lo *hereda* como `ProviderRisk`, no lo rediseña.
- **No es una reescritura del matcher.** `router/matcher/`, `VERB_TRIGGERS`, trigger phrases y el dataset I-7/J-13 siguen siendo la fuente de `StandardGuild` mientras el fabric madura.

## Design

### 1. La jerarquía formal (el contrato semántico)

```
CapabilityId     — QUÉ se necesita (ej: "fs.write", "code.analysis",
                   "web.scrape", "coloquio.post")
ProviderId       — QUIÉN puede satisfacerla
ProviderKind     — { LocalGuild, InternalModel, MeshPeer, HumanHitl }
ProviderHealth   — estado del ciclo de vida (Deep/WS4: FAILURE vs
                   INTENTIONAL_STOP vs REMOTE_DISCONNECT vs PROTOCOL_FAILURE)
ProviderRisk     — HEREDADO de ToolMetadata.risk_level (sin rediseño)
ProviderCost     — presupuesto: latencia p50/p95 (fuente real: /api/v1/audit/latency, 900816a)
ProviderTrust    — multidimensional, fase 2 (fase 1: trusted/untrusted)
```

Reglas del contrato:

1. **Toda capacidad ejecutable tiene ≥1 provider y ≤N providers.** Un guild *es* un `ProviderKind::LocalGuild`; no es la unidad de decisión, sino una implementación de ella.
2. **El riesgo viaja con la capability, no con el guild.** `docker` mezcla tools High/Medium/Low — el audit lo señaló y `check_tool_risk()` ya lo resuelve por-tool; el fabric lo hace first-class (`ProviderRisk` por capability×provider, no por guild).
3. **Todo provider declara su capa de origen:** local (guild catalog G6), modelo interno, peer federado (`CapabilityRegistry` de tylluan-link) o humano (HITL). La selección cruza capas — un `CapabilityId` puede resolver a un peer si el local está degradado y la política lo permite.
4. **El snapshot WS5 identifica cada decisión:** `SystemSnapshot` (`router/system_snapshot.rs:29`, en `/health` verbose y sellado en cada fila de `guild_audit_log` desde `73c3f12`) es la identidad del fabric que decidió — sin ella, cualquier benchmark del fabric repite la ambigüedad de MD-2.

### 2. Qué cambia y qué explícitamente se queda

**Se queda (verificado en disco 2026-09-14):**

| Pieza | Por qué se queda |
|-------|------------------|
| `router/matcher/` + dataset I-7/J-13 | sigue produciendo el guild candidato para `StandardGuild` |
| `check_tool_risk()` (`logic.rs:10`) | ya es la cadena de riesgo correcta; el fabric la reutiliza |
| `tools/guild_catalog.py` + `check_guild_consistency.py` | G6 ya es la fuente de identidad de guilds; el fabric la consume como `ProviderCatalog` local, no la duplica |
| WS2 `wants_delegation`/`coordinator_eligible` (`complexity.rs:232,249`) | ya separan intención de elegibilidad; el fabric los usa como *policy input*, no los sustituye |
| `CapabilityRegistry` (tylluan-link) | ya es el registro de capacidades federado; el fabric lo integra como capa `MeshPeer`, no crea otro |
| CONTRACT-01, puerto 47004, observación-antes-de-cutover | invariantes del proyecto |

**Cambia (fases de migración, cada una con su ciclo):**

| Fase | Cambio | Análogo de observación |
|------|--------|------------------------|
| F1 | `TaskContext v2`: añadir `privacy`, `trust_hint`, `required_capability: Option<CapabilityId>`, `data_locality` como first-class (hoy faltan: `TaskContext` en `types.rs:74-93` solo tiene intent/agent_id/latency/allow_remote/rollback/tool_risk_hint/background_budget — brecha documentada por el audit §3) | extender el struct, tests de tipos; cero comportamiento |
| F2 | `CapabilityCatalog` unificado: G6 + registry de tools + `CapabilityRegistry` de tylluan-link bajo una sola interfaz read-only | construir y exponer en `/health` + snapshot WS5; cero decisión |
| F3 | `ProviderSelector`: dado `TaskContext v2` + `SchedulingDecision`, emitir `ProviderSelection { provider, policy_trace }` en **modo observación** | patrón WS3: tabla confusion de selección vs lo que el router hizo, endpoint `/provider/selections/confusion` |
| F4 | Cutover de selección **solo si** la confusion matrix acumulada lo justifica, con sign-off del Tech Lead (igual que WS3) | la decisión no es de quien construye |

### 3. Observabilidad: el fabric nace midiendo

Consistente con el patrón Layer 4/WS3 — ninguna fase cambia comportamiento antes de tener su contraste:

- **F1:** campos nuevos en `TaskContext` aparecen en los logs de observación del Scheduler ya cableados (`observe_scheduling`, `handler_do/mod.rs:486`).
- **F2:** el catálogo unificado entra al snapshot WS5 (hash de capacidades) — cada benchmark puede citar el catálogo exacto que tenía.
- **F3:** la confusion matrix de selección usa la infraestructura WS3 (`router/scheduler/confusion.rs`, endpoint `/api/v1/scheduler/confusion` en `routes.rs:100`) — misma base de datos de patrón audit-log, misma convención `TYLLUAN_*_DB` seam.
- **Latencia por-provider:** la fuente ya existe (`/api/v1/audit/latency`, `900816a`) — el fabric solo añade la dimensión provider a la agregación, no inventa una nueva.

### 4. Riesgos honestos

- **Riesgo de capa extra:** cada indirección añade latencia en el hot path. Mitigación: el catálogo es read-only y cacheado; el selector en F3 es puro y medible; F4 nunca ocurre sin números de la confusion matrix.
- **Riesgo de segunda fuente de verdad para guilds:** G6 existe exactamente para evitarlo — el fabric *consume* `tools/guild_catalog.py` (verificado: 46 `GuildEntry`, 61fb702); si el fabric necesita algo que G6 no tiene, se extiende G6, no se duplica.
- **Riesgo de re-crear el hijack de coordinator a nivel fabric:** la lección WS2 (complejidad ≠ permiso de delegación) se generaliza: *necesitar una capability ≠ poder delegarla a un provider remoto*. La política de delegación cruza `ProviderTrust` y `data_locality`, nunca solo complejidad.

## Consequences

### Positivas

- Una jerarquía formal mata la deuda semántica nombrada por el audit (§26): `guild`/`tool`/`capability`/`provider` dejan de ser intercambiables.
- El Scheduler deja de decidir con la mitad del espacio de decisión (F1 cierra la brecha TaskContext que el audit documentó).
- La federación y el kernel comparten vocabulario de capacidades — `CapabilityRegistry` de tylluan-link deja de ser una isla.
- La selección de providers con Health/Cost/Trust hace ejecutable la visión de "OpenClaw, Hermes, Claude Code y agente propio como providers equivalentes" (audit §23).

### Negativas

- Es el cambio de mayor radio del plan: F1-F4 atraviesan router, scheduler, registry y federación. Por eso este ADR no adopta nada: cada fase necesita su ciclo con validación completa.
- Ventana de deuda transitiva: mientras F1-F3 conviven con el router heurístico, hay dos vocabularios en el código (el nuevo en structs, el viejo en call sites). El registro de deuda de medición ya documenta el patrón de conviviencia (MD-2: cerrado hacia adelante, retroactivo pendiente).

### Neutras

- No cambia el modelo de despliegue, el puerto, ni la lista de tools soberanos.
- No añade dependencias nuevas (todos los crates involucrados ya existen).

## Verification

1. Este ADR cita solo superficies verificadas en disco el 2026-09-14: `TOOL_METADATA` 151 entradas (contadas, no citadas), G6 con 46 `GuildEntry` (`61fb702`), WS2 en `complexity.rs:232/249` (`9fc8bfb`), WS3 en `handler_do/mod.rs:486` + `routes.rs:100` (`8ac01f4`), WS5 en `router/system_snapshot.rs:29/108/141` (`73c3f12`), MD-7 en `900816a`, `TaskContext` en `types.rs:74-93`, `CapabilityRegistry` en `tylluan-link/src/capability.rs:24`.
2. Cero cambios de código en este commit — `git show --stat` debe listar solo este ADR.
3. Cada afirmación de "hoy X funciona así" es replicable con el comando/linea citada.
4. La batería WS7 (`64e1457`) sigue verde: este ADR no toca código que ella ejercita.

## Decision

**Proponer** la Capability Provider Fabric como dirección arquitectónica (jerarquía §1, fases §2-F1..F4). **Pendiente de aceptación del Tech Lead (Claude Code) — y de José para cualquier fase que cambie comportamiento.** Si se acepta, F1 (TaskContext v2) es la primera fase construible; F4 (cutover) requiere la confusion matrix de F3 con datos reales acumulados y sign-off explícito, igual que el gate WS3. Si se rechaza, el documento queda como registro del análisis — igual filosofía que ADR-013: la ética/dirección no depende del mecanismo, el mecanismo solo la hace verificable.

## Open questions (honestos)

1. **Granularidad de `CapabilityId`:** ¿"fs.write" cubre `write_file` y también las tools de guilds filesystem? La taxonomía correcta entre capability y tool es la decisión de diseño más delicada del fabric — propuesta inicial: 1 capability = grupo de tools con el mismo risk y el mismo contract de IO, a validar contra los 151 entries reales.
2. **¿Dónde vive el `CapabilityCatalog` unificado?** Kernel (`router/`) vs crate nuevo vs `tylluan-link` (que ya tiene la parte federada). F2 necesita esta decisión antes de escribir código.
3. **`ProviderTrust` fase 1:** ¿binario (trusted/untrusted) o heredado del ACL role del caller (ADR-009)? La segunda es más barata y reusa el ACL fail-closed (`09b9668`).
4. **Coste de F3 en el hot path:** la confusion de selección añade una escritura SQLite por dispatch (patrón audit/confusion ya probado, pero es la tercera escritura por request). ¿Muestreo (1/N) o budget de escritura vía `background_budget`?
5. **HITL como provider:** `ExecutionClass::HumanAuthorizationRequired` ya existe (`types.rs:58`) — ¿el fabric lo modela como `ProviderKind::HumanHitl` de primera clase o lo deja fuera del selector v1?
