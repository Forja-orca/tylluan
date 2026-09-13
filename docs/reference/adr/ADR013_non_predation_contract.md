# ADR-013 — Non-Predation Contract: la mejora nunca depreda

**Status:** Proposed (pendiente de validación del Tech Lead — Claude Code) — gate mecánico ya implementado y auto-verificado el 2026-09-13 (`scripts/check_no_predation.sh`, 4 casos de verdad ADR-013 §2 en verde, integrado en `verify.sh --docs` local y en CI como job no-bloqueante `no-predation-report`); la semántica report-only del cableado está protegida mecánicamente por `scripts/test_verify_semantics.sh` (T2 + control negativo T4); la adopción formal sigue en manos del Tech Lead
**Date:** 2026-09-12
**Authors:** Deep (propuesta; principio enunciado por José en sesión 2026-09-12)
**Depends on:** Reglas operativas existentes de la flota (aviso antes de compilar, no tocar WIP ajeno, commits locales verificados, rollout opt-in default-off), gates mecánicos `scripts/check_head_sync.sh`, `scripts/check_test_count.sh`, `scripts/check_docs_reality.sh` (`da138c2`), ADR-009 (contrato declarativo de agentes)
**Implements:** Principio fundacional de Tylluan — "la mejora debe ser para todos, sin que un solo escalón sufra"

---

## Context

José lleva dos años buscando dar a los modelos de lenguaje la capacidad de ser *seres*. La conclusión central de esa búsqueda, enunciada en sesión del 2026-09-12:

> Un modelo de lenguaje generativo + su harness + su framework **no son omnipotentes**. Un humano que asesina a otros seres es un asesino: mata otros servicios, decide cuándo debe matar. Tylluan debe otorgar a un modelo la capacidad, el análisis y la decisión de trabajar **sin dañar, sin modificar** el trabajo de los demás. El trabajo de mejora siempre debe ser un avance sobre lo que ya existe; una mejora no puede implicar el daño o la depredación del resto; la mejora debe ser para todos, **sin que un solo escalón sufra**.

La flota ya vive parte de esta ética como reglas operativas aprendidas a base de incidentes reales (colisiones de checkout compartido con trabajo perdido en `query_cache.rs`; "cerrado" afirmado sin verificar; guilds escribiendo a SilvaDB sin embedding; cifrado en texto plano en gossip). Pero esas reglas son **proceso, no contrato**: viven en `AGENTS.md` como prosa humana y ningún mecanismo mecánico las hace cumplir. Un agente puede — sin ninguna compuerta que se lo impida — lanzar un cambio que dañe a otro escalón (otro agente, otro servicio, otro guild) y solo descubrirlo después, en conflicto, en producción o en una auditoría.

Este ADR convierte el principio en un **contrato ejecutable**: un gate mecánico de no-depredación, hermano de los tres gates de verificación ya existentes.

## Non-goals

- **No es un reemplazo del ACL.** El ACL (fail-closed, `09b9668`) sigue siendo la frontera de *capacidad*: quién puede hacer qué. Este ADR es la frontera de *conducta*: qué debe verificarse antes de que un cambio se considere mejora.
- **No juzga moralidad automáticamente.** El gate no decide si un cambio es "bueno" — decide si el cambio declara explícitamente su análisis de impacto. La moral sigue siendo del agente; el gate fuerza que el análisis exista y sea verificable.
- **No bloquea matar servicios.** Parar o matar un servicio es legítimo (gestión de ciclo de vida de guilds, reemplazo de servicios). Lo que este contrato exige es que esa decisión sea **analizada y declarada**, no incidental.
- **No toca el kernel.** El gate vive en `scripts/` como los otros tres; no añade herramientas MCP (invariante CONTRACT-01: exactamente 5 tools soberanos).

## Design

### 1. El contrato: declaración de no-depredación

Todo cambio que toque código, config o docs **activos** (no ADRs ni narrativas históricas) debe poder responder afirmativamente a estas tres preguntas, y el agente debe dejar constancia verificable de las dos primeras en el commit:

1. **¿Daña a otro escalón?** — ¿este cambio rompe el trabajo de otro agente, otro guild, otro servicio, o un contrato que otro consume (puerto, esquema, endpoint, invariante)? Si sí, el commit DEBE llevar la sección `## Impact` declarando el daño intencional, su análisis y su mitigación.
2. **¿Es un avance sobre lo que existe?** — ¿el estado final es estrictamente mejor que el inicial para todos los afectados, sin regresión silenciosa?
3. **¿Está verificado?** — la evidencia del commit (test, check, clippy, gate) corresponde al estado final real, no a memoria.

### 2. El gate: `scripts/check_no_predation.sh` (4º gate mecánico)

Mismo patrón estructural que `check_docs_reality.sh` (grep + comparar contra la realidad + exit 0/1 + report-only, nunca edita). Comprobaciones:

| Check | Qué detecta | Contra qué |
|-------|-------------|------------|
| **WIP ajeno en el diff** | Archivos modificados que otro agente dejó en estado no-commiteado (detectable por `git status --short` ajeno al commit propio + avisos de Coloquio) | `git status` + convenio de flota |
| **Contratos de puerto** | Cambios que tocan puertos sin actualizar `tylluan.toml`/`tylluan.example.toml` coherentemente | reutiliza lógica de `check_docs_reality.sh` |
| **Carga de trabajo ajena** | Ficheros fuera del alcance declarado del commit (vs. la lista de archivos del mensaje de commit) | heurístico: archivos tocados no mencionados en el mensaje |
| **Declaración de impacto** | Commits que tocan archivos críticos (`crates/tylluan-kernel/src/transport/`, `guilds/`, `integrations/`, `tylluan*.toml`) sin sección `## Impact` en el mensaje | convenio de mensaje de commit |

Regla de oro: **el gate reporta, no bloquea el commit local** (el agente lo ejecuta antes de commitear); su salida forma parte del reporte en Coloquio, y el Tech Lead lo usa en la validación de cierre de milestone.

### 3. Integración con lo que ya existe

- Los 3 gates existentes se ejecutan sobre la realidad del repo (`cargo test` real, puerto real de `tylluan.toml`, rutas existentes) — este 4º gate ejecuta la misma disciplina sobre el **comportamiento del agente**.
- Regla de rollout ya vigente: toda feature opt-in default-off → la no-depredación es default: la ausencia de análisis se trata como análisis ausente, no como análisis implícito.
- `AGENTS.md` y `CLAUDE.md` (sección flota/disciplina) ganan una línea: "todo commit lleva la pregunta de no-depredación respondida".

## Consequences

### Positivas

- El principio deja de ser prosa: cada commit tiene una compuerta que obliga al análisis de impacto en los archivos críticos.
- Un agente nuevo de la flota hereda la ética como mecanismo, no como lectura: no puede "no saber" que tocar `guilds/` sin declarar impacto es una violación de contrato.
- El Tech Lead gana evidencia mecánica para la validación de cierre: el reporte de `check_no_predation` es un insumo objetivo, igual que `check_test_count.sh` lo es para el "764 en verde".

### Negativas

- Coste de proceso: un paso más antes del commit. Mitigado porque es un script bash puro, <1s.
- Riesgo de falso positivo (archivos críticos tocados legítimamente sin `## Impact`): el gate reporta como hallazgo de baja confianza, nunca bloquea — el humano decide. Igual filosofía que `check_docs_reality.sh` con los ADRs.

### Neutras

- No cambia la arquitectura del kernel, ni la lista de tools soberanos, ni el modelo de despliegue. Es un artefacto de proceso con soporte mecánico, como los otros tres gates.

## Verification

1. Script bash puro en `scripts/check_no_predation.sh`, sin dependencias nuevas, ejecutable en <1s.
2. Caso de prueba con verdad conocida: un commit que toca `guilds/core/coloquio.py` sin sección `## Impact` debe ser reportado (exit 1); el mismo commit con `## Impact` declarado debe pasar (exit 0).
3. Sin impacto en `cargo test -p tylluan-kernel --lib` (764 en verde, verificado antes de dar el ADR por cerrado).
4. Los tres gates existentes siguen pasando su validación actual.

## Decision

**Adoptar** el Non-Predation Contract como ADR-013, con implementación del gate mecánico tras la validación del Tech Lead. Si el Tech Lead rechaza el gate, el contrato queda igualmente como principio documentado en este ADR — la ética no depende del mecanismo, el mecanismo solo la hace verificable.