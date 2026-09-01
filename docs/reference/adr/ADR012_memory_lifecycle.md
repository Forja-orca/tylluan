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

**Enmienda (2026-09-02, drift_guard canary, drift_ratio=4.56):** la inmunidad al pruning automatico protege el resumen *vigente* — la continuidad activa del agente. Un nodo que ya fue reemplazado (`metadata.superseded_by` set, via el mecanismo de la regla 2 de arriba) fue declarado obsoleto por el propio sistema en el momento de la supersesion, no por un cron externo. Podar esos nodos tras una ventana de gracia (`prune_superseded`, `LifecyclePhase`, 14 dias por defecto) no es "pruning automatico de memoria util" en el sentido que esta decision prohibe — es recoleccion de basura de lo que Tylluan ya marco como reemplazado. El resumen activo nunca se toca; solo se libera el historico ya superado, cuyo `superseded_by` sigue siendo recuperable via el nodo vigente si hiciera falta trazabilidad.

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

### 6.1 Fase 1 — detalle de implementacion real (fusionado desde el companion doc de Deep)

Deep escribio un documento companion separado (`ADR-012_lifecycle_design.md`)
durante la implementacion de Fase 1. Fusionado aqui tras acuerdo del equipo
(Coloquio T163-T165: Antigravity, Deep y Buffy coincidieron en que era
complemento con solapamiento parcial, no duplicado) para evitar el riesgo de
drift entre dos archivos con nombres casi identicos (`ADR-012_` vs
`ADR012_`) -- el mismo patron de incidente que ya ocurrio una vez con
AGENTS.md. El archivo separado fue eliminado tras esta fusion.

**Migracion v23 real, tal como quedo commiteada en `e4d3c5a`:**

```sql
ALTER TABLE nodes ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'active';
ALTER TABLE nodes ADD COLUMN last_agent_access INTEGER NOT NULL DEFAULT 0;
ALTER TABLE nodes ADD COLUMN reactivation_count INTEGER NOT NULL DEFAULT 0;

-- Backfill derivado (no DEFAULT 'active' ciego, ver 8.3):
UPDATE nodes SET lifecycle_state = CASE
    WHEN updated_at < datetime('now', '-30 days') THEN 'quiet'
    ELSE 'active'
END
WHERE protected = 0 AND type != 'identity';
UPDATE nodes SET last_agent_access = 0;
UPDATE nodes SET reactivation_count = 0;
```

El `WHERE protected = 0 AND type != 'identity'` en el backfill fue un hallazgo
real de revision (no estaba en la primera version del diff, corregido en
T161) -- sin el, nodos protegidos/identidad podrian degradarse a `quiet`
por antiguedad en el backfill inicial.

**Indices: estrategia condicional, no crear en Fase 1.** Escrituras
(`touch_node`, decay, consolidate, recall) tocan cualquier indice sobre
estas columnas; crear uno sin necesidad medida es overhead puro en el
write-path. Decision: crear `idx_nodes_lifecycle`/`idx_nodes_last_agent_access`
en Fase 2 SOLO si `EXPLAIN QUERY PLAN` muestra seq scan real en las queries
que los necesiten -- no antes.

**Checklist de validacion pre-commit (usado para Fase 1, reutilizable en
fases futuras):**
```bash
cargo check -p tylluan-kernel
cargo clippy -p tylluan-kernel -- -D warnings
cargo test -p tylluan-kernel --lib
scripts/check_test_count.sh   # evita el drift de conteo que ya paso una vez (1a5eaaa)
```

**Nota de honestidad sobre los tests propuestos en el documento original de
Deep:** su seccion 7 incluia esbozos de tests (`test_lifecycle_migration_
backfill_derived`, `test_last_agent_access_updated_only_on_agent_access`)
que llaman a metodos que no existen en el API real
(`insert_node_with_timestamps`, `migrate_to_v23`, `get_lifecycle_state`,
`recall_with_agent`) -- eran ilustrativos de la intencion, no codigo
verificado contra el kernel real. No se fusionan aqui como tests reales;
quedan como recordatorio de que Fase 1 aun no tiene cobertura de tests
dedicada para el backfill derivado ni para la separacion `last_agent_access`
vs `touch_node`, mas alla de que los 685 tests existentes siguen en verde.
Anadir esa cobertura sigue siendo trabajo pendiente, no cerrado por Fase 1.

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

---

## 8. Revision Post-Critica del Equipo (T139-T141) — 5 hallazgos reales, ADR modificado

Tras publicar la v1 de este ADR, se pidio al equipo una lectura critica real (no
aprobacion por cortesia). Antigravity (T139) y Deep/Codex (T141, mas una lectura
independiente directa del codigo antes de tener acceso a Coloquio) encontraron 5
problemas reales que la v1 no cubria. Documentados aqui en vez de reescribir D1-D4
en su sitio, para preservar la trazabilidad de que fueron encontrados en revision,
no en el diseno original.

### 8.1 D2 esta rota: "archived -> active" via recall normal es logicamente circular

**Encontrado por Antigravity (T139).** Si `archived` excluye nodos del recall
estandar para no contaminar contexto, el recall normal NUNCA los encuentra — el
trigger "re-acceso (recall)" nunca se dispara. Si el recall SI los incluye al
mismo nivel, `archived` deja de cumplir su proposito.

**Resolucion:** la transicion `archived -> active` NO se dispara por un recall
estandar. Se acota explicitamente a tres vias: (a) salto asociativo via
`local_query_graph` / PPR desde un nodo semilla activo, (b) coincidencia lexica
exacta en FTS5, (c) parametro explicito `include_archived: true` en
`tylluan_recall`. D2 (tabla de transiciones) queda modificada: la fila
`archived -> active` cambia su columna "Trigger" de "Re-acceso (recall, ingest,
manual)" a estas 3 vias explicitas.

### 8.2 D2 tambien esta rota: "active -> quiet" usa un campo que no mide lo que dice medir

**Encontrado independientemente por Antigravity y Deep/Codex (T139, T141) —
mismo hallazgo desde dos lecturas distintas.** El trigger propuesto ("sin acceso
>30 dias") se apoyaria en `updated_at`, pero esa columna se actualiza por
stigmergy, decay y consolidate, no solo por acceso real de un agente.
`fsrs_last_review` es 0 para nodos anteriores a v0.13, lo que causaria una
transicion falsa e inmediata a `quiet` en el backfill.

**Resolucion:** Fase 1 debe anadir una columna nueva `last_agent_access INTEGER`
que SOLO se actualiza en `recall`/`ingest` exitosos iniciados por un agente —
nunca por `touch_node` interno, decay, ni consolidate. El trigger real de
`active -> quiet` se mide contra esta columna, no contra `updated_at`.

### 8.3 Backfill ingenuo de Fase 1 rompe D2 para todo el corpus historico

**Encontrado por Deep/Codex (T141).** Un backfill simple `DEFAULT 'active'`
marcaria TODO nodo preexistente como `active`, dandole 30 dias extra de vida
activa artificial, inflando el indice vectorial y retrasando el pruning real.

**Resolucion:** el backfill de Fase 1 debe ser derivado, no un default plano:
`CASE WHEN updated_at < now() - 30d THEN 'quiet' ELSE 'active' END`. Esto entra
en el plan de migracion de Fase 1, no cambia D1-D4.

### 8.4 Version barata de la propuesta 2.5 de Qwen, viable en Fase 1-4 (no Fase 5+)

**Propuesta convergente de Antigravity y Deep/Codex (T139, T141), ambos
independientemente de acuerdo en posponer la version completa (gate sincrono
con LLM/embeddings) a Fase 5+ por coste en el read-path (+15-30ms rompe el SLA
de recall <50ms), pero proponiendo una version barata que SI cabe antes:**

Anadir `reactivation_count INTEGER DEFAULT 0`, incrementado en cada transicion
`archived -> active`. Si `reactivation_count >= 3`, el nodo puede saltarse el
estado `quiet` e ir directo a `active` ("hot reactivation"). Coste: un UPDATE de
entero en el write-path, cero overhead en el read-path — sin embeddings, sin
inferencia. Captura "frecuencia de reactivacion" (la senal central de la
propuesta de Qwen) sin el coste que la hace inviable en Fase 1-4.

**Decision:** esta version barata se anade a Fase 2 del roadmap (Seccion 6).
La version completa de Qwen (gate de afinidad con coherence check) permanece en
Fase 5+ sin cambios, tal como estaba en la v1 de este ADR.

### 8.5 Conflictos de implementacion senalados, no resueltos aqui (requieren decision en Fase 1)

Estos 3 hallazgos son reales pero de implementacion, no de diseno — se dejan
registrados para que quien ejecute Fase 1 los resuelva explicitamente, no los
descubra en produccion:

- **`type = 'archived'` (ya en uso por `meta_cognitive_prune()`, maintenance.rs:186)
  vs `lifecycle_state = 'archived'` (columna nueva de este ADR):** sin decision
  explicita, un nodo podria tener `type='archived'` + `lifecycle_state='active'`
  simultaneamente sin que ninguna query lo detecte. Fase 1 debe decidir: ¿se
  migra `type='archived'` a `lifecycle_state` y `type` vuelve a su valor
  original, o coexisten con una regla de precedencia explicita?
- **Mapeo posicional de columnas SQL** (`r.get(0)`, `r.get(1)`... en `nodes.rs`,
  `search.rs`, `decay.rs`): anadir `lifecycle_state` a `SELECT *` sin actualizar
  cada mapeo posicional corrompe silenciosamente el deserializado de `GraphNode`
  en runtime. Fase 1 debe auditar cada `SELECT *` sobre `nodes` antes de tocar
  el schema.
- **`ON CONFLICT DO UPDATE SET`** en `upsert_node_with_validity` (nodes.rs:155-173):
  si `lifecycle_state` se anade al INSERT pero se olvida en el bloque
  `ON CONFLICT`, cada upsert posterior resetea silenciosamente el lifecycle de
  nodos existentes. Riesgo confirmado por Deep/Codex via lectura directa del
  codigo antes de tener acceso a Coloquio.
- **Serializacion de API/dashboard** (`/api/v1/silva/graph`, `KnowledgeGraphCanvas`
  en `@tylluan/ui-core`): si `GraphNode` gana `lifecycle_state` sin un default
  seguro en el frontend, el panel de grafo puede fallar al renderizar. Sealado
  por Antigravity.

### 8.6 D3 (supersion) entra en conflicto directo con "NO tocar consensus.rs"

**Encontrado por Deep/Codex (T141, punto 2) — omitido por error en la primera
revision de este ADR (v2), corregido ahora.** La Decision 3 (durable summaries)
dice que un summary duradero es removible via "supersion por un resumen mas
reciente del mismo agente/sesion". Pero `ConsensusEngine` en `consensus.rs`
hace supersedence por CONTENIDO (deteccion de contradiccion/mejora semantica),
no por coincidencia de agente/sesion — esa logica de supersedence-por-identidad
no existe hoy en ningun sitio del codigo.

Implementarla tal cual la describe D3 requeriria tocar `consensus.rs`, lo cual
contradice la restriccion de la Seccion 5 ("NO se toca decay.rs, tests.rs, ni
handler_do/mod.rs") en espiritu, aunque `consensus.rs` no estaba en esa lista
explicitamente — la restriccion nunca considero este caso.

**Resolucion:** Fase 1 debe decidir explicitamente entre dos caminos, no
asumir que D3 "ya funciona" via `ConsensusEngine` existente:
1. Ampliar `ConsensusEngine` para reconocer supersedence por agente/sesion
   como un caso adicional junto al de contenido (cambio real en consensus.rs,
   requiere su propio ciclo de revision, no incluido en el alcance de Fase 1).
2. Un trigger SQL/aplicacion nuevo, independiente de `ConsensusEngine`, que
   solo gestione la supersedence de summaries duraderos por agente/sesion sin
   tocar la logica de contradiccion por contenido.

Sin esta decision, D3 tal como esta escrita en la Seccion 2 es aspiracional,
no implementable directamente.

### 8.7 D4 (consolidacion) debe heredar explicitamente el fix de 242a6db, no asumirlo

**Sealado en T144 (relay de un companero sin acceso directo a Coloquio en el
momento de escribirlo).** Cualquier logica de consolidacion nueva que Fase 3
introduzca (integracion con NightConsolidation) debe llamar o respetar el
mismo filtro de exclusion por tipo semantico que ya protege a
`agent_summary`/`session_digest`/`consolidated_summary` en `decay.rs` (fix
`242a6db`) y en `maintenance.rs` (fix paralelo, ver commit relacionado). Si
la nueva ruta de consolidacion escribe sus propias queries de limpieza sin
heredar ese filtro, reintroduce el mismo hueco que motivo la Fase 0 de este
ADR — un resumen duradero podria volver a quedar expuesto a traves de una
ruta de codigo distinta a las 3 ya cerradas.

**Resolucion:** Fase 3 no debe escribir queries de limpieza nuevas desde
cero. Debe reutilizar la misma clausula `type NOT IN ('identity',
'agent_summary', 'session_digest', 'consolidated_summary')` (o una funcion
compartida que la encapsule) en cualquier ruta de borrado/archivado que
introduzca.

**Nota tecnica adicional de T144, no verificada por mi todavia:** propone
`tokio::spawn` como mecanismo concreto para el worker asincrono de
reactivacion de la Seccion 8.4 (version barata de la propuesta de Qwen) —
compatible con la resolucion ya acordada ahi (write-path barato, sin
inferencia sincrona). Se anade como detalle de implementacion sugerido, no
como decision nueva.
