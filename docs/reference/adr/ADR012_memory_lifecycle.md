# ADR-012: Memory Lifecycle State Machine

- **Estado:** Propuesto — pendiente de aprobacion en Coloquio
- **Fecha:** 2026-08-21
- **Autores:** Buffy (auditoria documental), equipo (Coloquio T124-T135)
- **Ambito:** Kernel Rust (crates/tylluan-kernel/src/memory/silva/)
- **Dependencias:** ADR-011 (Signal Loop + Coherence Gate), fix 242a6db (Fase 0)

---

## 1. Contexto y Problema

SilvaDB gestiona nodos de memoria con multiples mecanismos ad-hoc que no forman una maquina de estados coherente:

| Mecanismo | Ubicacion actual | Que hace hoy |
|-----------|-----------------|--------------|
| memory_status() | mod.rs:66 | Devuelve confirmed/provisional/superseded/contradicted — basado en confidence + valid_until, responde "¿es confiable?" |
| quarantined column | schema.rs:237 (v21) | Campo booleano orthogonal — responde "¿es seguro?" (ASI06 write-gate) |
| meta_cognitive_prune() | maintenance.rs:141 | Archiva nodos low-weight → type = archived — irreversible, sin camino de reversion |
| apply_decay() + FSRS-5 | decay.rs | Decrementa weight via retrievability 2^(-t/S), luego prune eliminan bajo threshold |
| 3 DELETE paths | decay.rs:78, maintenance.rs:403,453 | Excluyen agent_summary/session_digest/consolidated_summary — fix 242a6db |
| No hay quiet state | — | No existe transicion activo→inactivo antes de archivado |
| No hay undo de archivado | — | Una vez type = archived, no hay camino de reversion |

**El gap que 242a6db cerro (Fase 0):** apply_decay() ya excluia agent_summary de su DELETE, pero prune_by_salience() y prune_cold_nodes() no excluian agent_summary, session_digest, ni consolidated_summary — solo excluian identity/protected. Los summaries duraderos sobrevivian por suerte de salience/peso, no por diseno. Live audit contra data/silva.db encontro 0/377 nodos agent_summary con protected=1.

**El problema que queda abierto:** No hay una maquina de estados formal que defina:
1. Que estados existen para un nodo de memoria
2. Que transiciones estan permitidas entre ellos
3. Que eventos disparan cada transicion
4. Como los estados interactuan con los mecanismos existentes (decay, federation, quarantine)

---

## 2. Decisiones

### Decision 1: Estados de Lifecycle del Nodo

Se define el siguiente conjunto de estados de lifecycle, como un eje independiente de memory_status() (que responde a confiabilidad) y quarantined (que responde a seguridad):

    +---------------+
    |    active     |  ← nodo con acceso reciente, weight alto
    +-------+-------+
            | sin acceso >30 dias
    +-------v-------+
    |     quiet     |  ← nodo sin acceso reciente pero aun util
    +-------+-------+
            | consolidacion (NightConsolidation)
    +-------v-------+
    | consolidated  |  ← contenido fusionado en resumen
    +-------+-------+
            | weight < threshold o cleanup manual
    +-------v-------+
    |   archived    |  ← preservado pero no activo
    +---------------+

**Estados ortogonales** (pueden coexistir con cualquier estado de lifecycle):
- superseded — reemplazado por una version mas reciente (memory_status() en mod.rs:66)
- contradicted — entra en conflicto con otro nodo (ConsensusEngine en consensus.rs)
- quarantined — marcado como potencialmente danino por ASI06 write-gate (schema.rs:237)

**Nota:** provisional no es un estado de lifecycle — es un indicador de confianza baja en memory_status(). Un nodo puede ser active + provisional simultaneamente.

### Decision 2: Transiciones y Triggers

| Transicion | Trigger | Implementacion actual |
|------------|---------|----------------------|
| → active | Cualquier acceso (touch_node, recall exitoso, ingest) | Ya existe: updated_at se actualiza, FSRS review con Rating::Good |
| active → quiet | Sin acceso >30 dias + weight still above prune threshold | NUEVO — necesita campo lifecycle_state o calculo derivado |
| quiet → consolidated | NightConsolidation consolida el contenido en un resumen | Ya existe parcialmente: consolidate_episodes() en maintenance.rs:63 |
| quiet → archived | meta_cognitive_prune() cuando weight < threshold | Ya existe: maintenance.rs:186 |
| consolidated → archived | Cleanup manual o retention policy | NUEVO — no hay trigger automatico hoy |
| archived → active | Re-acceso (recall, ingest, manual) | NUEVO — requiere desarchivado explicito |
| Cualquiera → superseded | ConsensusEngine encuentra version mejorada | Ya existe: mod.rs:77 |
| Cualquiera → contradicted | ConsensusEngine detecta conflicto | Ya existe: consensus.rs |
| Cualquiera → quarantined | ASI06 write-gate detecta contenido sospechoso | Ya existe: schema.rs:237 |

**Transiciones NO permitidas:**
- archived → quiet (skip de lifecycle — si re-accessas un archived, vuelve a active)
- superseded → active (no se revierte una supersesion)
- quarantined → cualquier estado sin resolucion manual explicita

### Decision 3: Durable Summaries como Categoria Protegida

El fix 242a6db excluye agent_summary/session_digest/consolidated_summary de las 3 rutas DELETE. Esta decision se formaliza como politica de diseno, no solo fix:

**Regla:** Los summaries duraderos (agent_summary, session_digest, consolidated_summary) son inmunes a pruning automatico (decay, salience, weight cleanup). Solo son removibles via:
1. Cleanup manual explicito (operator-controlled)
2. Supersion por un resumen mas reciente del mismo agente/sesion
3. Quarantine por ASI06 (si el contenido es potencialmente danino)

**Rationale:** Estos nodos representan el conocimiento estructural del agente — preferencias, patrones de sesion, resumenes consolidados. Su eliminacion accidental degrada la calidad de recall de forma que el usuario no puede diagnosticar facilmente.

### Decision 4: Quarantine como Eje Orthogonal

El campo quarantined (schema v21, schema.rs:237) responde a una pregunta diferente al lifecycle:

| Eje | Pregunta | Valores |
|-----|----------|---------|
| **Lifecycle** | "¿Cuán activa/util es esta memoria?" | active, quiet, consolidated, archived |
| **Quarantine** | "¿Es seguro compartir/consumir esta memoria?" | 0 (seguro), 1 (en cuarentena) |
| **Memory status** | "¿Es confiable esta informacion?" | confirmed, provisional, superseded, contradicted |

Un nodo puede ser active + quarantined = memoria activa pero sospechosa — se mantiene localmente pero no se comparte via federacion ni se consume en recall generativo.

**Comportamiento de quarantine en lifecycle:** Los nodos quarantined NO avanzan en el ciclo de vida (no se consolidan, no se archivan). Se mantienen en su estado actual hasta que el quarantine se resuelva manualmente.

---

### 2.5 Propuesta: Refinamiento Activo en Reactivacion (Qwen, turno 135)

**Estado:** Propuesta — NO es una decision cerrada. Requiere evaluacion del equipo antes de incluirse en Fase 1+.

Qwen propos en Coloquio T135 un principio de "refinamiento activo en reactivacion" inspirado en la maduracion de afinidad de anticuerpos (inmunologia adaptativa): cuando un nodo archived se re-accessa (transicion archived → active), no solo se restaura — se **refina activamente** antes de volver al pool activo.

**Mecanismo propuesto:**
- Al re-accessar un nodo archived, el sistema ejecuta un paso de "afinidad" antes de marcarlo active
- El paso evalua: (a) cuantas veces se ha re-accessado en los ultimos N dias (frecuencia de reactivacion), (b) que tan bien matchea con las queries recientes del agente (similitud contextual), (c) si su contenido sigue siendo coherente con el grafo actual (coherence check ligero)
- Un nodo que falla este paso se queda en archived (no se reactiva automaticamente) — evita que memorias obsoletas revivan por un acceso casual
- Un nodo que pasa con alta afinidad puede saltarse el estado quiet e ir directo a active ("hot reactivation")

**Relacion con D2 (Transiciones):** Esta propuesta modifica la transicion archived → active para incluir un "gate de afinidad" intermedio. Si se adopta, la tabla de transiciones de D2 ganaria una fila adicional: archived → (gate de afinidad) → active, con bypass para re-accessos de alta frecuencia.

**Honestidad sobre madurez:** Este principio es una analogia biologica inspiradora, no un algoritmo validado. No hay benchmarks que demuestren que el "gate de afinidad" mejora la calidad de recall sobre la transicion simple archived → active. Se documenta aqui como direccion futura para Fase 5+, no como requisito de Fase 1-4.


## 3. Comparativa de Opciones

### Modelo de estados

| Opcion | Ventajas | Desventajas |
|--------|----------|-------------|
| **A. Enum en columna lifecycle_state (elegido)** | Explicito, queryable, audit-friendly. | Requiere migracion de schema + backfill. |
| B. Derivar de updated_at + weight | Sin migracion. Ya funciona hoy. | Implicito — un nodo quiet no se puede encontrar sin calcular. |
| C. Multiples columnas booleanas | Simple de implementar. | Combinatoria explosiva — 4 booleanos = 16 estados. |

**Decision: Opcion A** — columna lifecycle_state TEXT NOT NULL DEFAULT active.

### Triggers de transicion

| Opcion | Ventajas | Desventajas |
|--------|----------|-------------|
| **A. Basado en eventos (elegido)** | Modular, coherente con NightConsolidation phases. | Complejidad distribuida. |
| B. Basado en polling | Centralizado, facil de auditar. | Latencia — 24h antes de que se detecte. |
| C. Hibrido | Lo mejor de ambos. | Mas complejo de razonar y testear. |

**Decision: Opcion A** por coherencia con el patron de phases existente.

---

## 4. Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigacion |
|--------|-------------|---------|------------|
| Backfill clasifica mal nodos existentes | Media | Medio | Backfill conservador: todos empiezan como active. |
| Performance de queries con columna nueva | Baja | Bajo | Indice solo si las queries lo justifican. |
| Durable summaries necesiten archivado eventual | Baja | Bajo | Politica clara: solo cleanup manual. |
| Quarantine + lifecycle interactions edge cases | Media | Medio | Documentar explicitamente: quarantine es orthogonal. |

---

## 5. Limites y No-Objetivos

- NO se modifica el schema de SilvaDB en este ADR. La columna lifecycle_state se anade en Fase 1.
- NO se reemplaza FSRS-5. El lifecycle es un eje adicional.
- NO se cambia CONTRACT-01 (5 herramientas soportes).
- NO se implementa undo de quarantined automatico — resolucion manual explicita.
- NO se toca decay.rs, tests.rs, ni handler_do/mod.rs.
- NO se toca maintenance.rs (alguien sin identificar ya esta extendiendo el patron de exclusion de summaries durables ahi mismo).
- NO se modifica federation — los lifecycle states no se sincronizan entre peers.

---

## 6. Roadmap de Implementacion

| Fase | Hito | Dependencias | Estado |
|------|------|-------------|--------|
| **Fase 0** | Fix de exclusion de summaries duraderos en 3 DELETE paths | Ninguna | Cerrado (242a6db) |
| **Fase 1** | Anadir columna lifecycle_state + migracion + backfill | ADR-012 aprobado | Pendiente |
| **Fase 2** | Transiciones active → quiet (basado en updated_at) | Fase 1 | Pendiente |
| **Fase 3** | Integracion con NightConsolidation (phases de transicion) | Fase 2 | Pendiente |
| **Fase 4** | Dashboard panel de lifecycle states | Fase 1 | Pendiente |

**Referencia de implementacion para estado archived:** Ver investigacion de compactacion de Antigravity (vector tiering, distilled_from, Coloquio ~turno 129) como modelo de referencia para como los nodos archived podrian preservar informacion comprimida sin consumir espacio de recall activo. Esta referencia es orientativa, no un requisito de implementacion.

---

## 7. Referencias

1. Commit 242a6db — fix de exclusiones de prune para summaries duraderos (Fase 0)
2. ADR-011 — Signal Loop + Coherence Gate (dependencia: quarantine interaction)
3. docs/concepts/FSRS_DESIGN.md — diseno de decay y estabilidad FSRS
4. crates/tylluan-kernel/src/memory/silva/mod.rs:66 — memory_status() existente
5. crates/tylluan-kernel/src/memory/silva/maintenance.rs:141 — meta_cognitive_prune()
6. crates/tylluan-kernel/src/memory/silva/decay.rs — apply_decay + 3 DELETE paths
7. crates/tylluan-kernel/src/memory/silva/schema.rs:237 — quarantined column (v21)
8. Coloquio T124-T135 — debate de lifecycle states (fuente primaria)
