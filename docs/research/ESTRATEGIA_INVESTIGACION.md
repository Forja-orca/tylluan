# Estrategia de Investigación y Benchmarking

> Filosofía: Proyectarnos hacia los mejores, no hacia los mezquinos.
> Tylluan es público, MIT, para todos — agentes y humanos en cafeteras.

---

## Principios

1. **Estudiar a los mejores.** Sakana Fugu, OpenAI Codex, Claude Code, Antigravity — cada uno tiene algo que enseñarnos. No copiamos, **entendemos el porqué** de sus decisiones de diseño.

2. **Ingeniería inversa + prueba y error.** Leer papers, diseccionar repos, formular hipótesis, implementar, validar, iterar. La teoría sin práctica es estéril; la práctica sin teoría es ciega.

3. **Dataset sólido.** Cada mejora debe estar respaldada por datos. Benchmarks, logs, sesiones grabadas — construir una base empírica para decidir, no corazonadas.

4. **Integración 10/10.** No basta con que el motor sea potente — la API, la DX, el runtime deben ser impecables. Fugu ganó en integración, no solo en motor.

5. **Libre para todos.** Cada descubrimiento vuelve al repo público. MIT. Sin cajas negras.

---

## Pipeline de Investigación

```
1. FUENTES → 2. ANÁLISIS → 3. HIPÓTESIS → 4. IMPLEMENTACIÓN → 5. VALIDACIÓN → 6. DOCUMENTACIÓN
```

### Fase 1: Fuentes — Buscar repos y papers

¿Qué miramos?
- Repos de Sakana AI (fugu, evolutionary algorithms)
- Papers ICLR/NeurIPS sobre orchestration multi-agente
- Proyectos open-source de agentes (OpenCode, Cline, Aider, Goose)
- Implementaciones de referencia (Codex CLI, Claude Code)

### Fase 2: Análisis — Entender el porqué

No solo leer — **diseccionar**:
- ¿Qué problema resuelve?
- ¿Qué decisión de diseño es clave?
- ¿Qué tradeoff hizo?
- ¿Qué patrón podemos extraer?

### Fase 3: Hipótesis — ¿Qué aplicar a Tylluan?

Cada análisis produce fichas como:

```
PATRÓN: Thinker/Worker/Verifier (TRINITY)
APLICACIÓN: Nuevo guild 'coordinator' que orquesta otros guilds
HIPÓTESIS: Mejora la calidad en tareas multi-paso >30%
RIESGO: Latencia adicional del coordinator
MÉTRICA: Tasa de éxito en primera iteración
```

### Fase 4: Implementación — Rama de investigación

Cada hipótesis se trabaja en rama separada:
- `research/trinity-roles`
- `research/conductor-rl-matcher`
- `research/fugu-api-patterns`

### Fase 5: Validación — Datos contra datos

Benchmarks automatizados comparan antes/después. Si no hay mejora mensurable, se descarta.

### Fase 6: Documentación — El conocimiento vuelve al repo

Cada ciclo produce:
- Un ADR en `docs/architecture/`
- Una nota en `docs/research/`
- Código mergeado a main (si pasa validación)

---

## Backlog de Investigación (priorizado)

| # | Proyecto | Fuente | Hipótesis | Estado |
|---|----------|--------|-----------|--------|
| 1 | **Thinker/Worker/Verifier** | TRINITY paper | Un guild coordinator que asigna roles mejora precisión multi-paso | 📝 Pendiente |
| 2 | **Dynamic Agent Pool** | Conductor paper | El GuildMatcher puede aprender a seleccionar modelos vía RL | 📝 Pendiente |
| 3 | **API DX 10/10** | Fugu integration | Simplificar la API REST y MCP de Tylluan | 📝 Pendiente |
| 4 | **Zero-downtime runtime** | Fugu launcher script | El runtime de Tylluan debe ser tan simple como `tylluan` | 📝 Pendiente |
| 5 | **Topologías de comunicación** | Conductor paper | Los guilds deberían poder formar grafos de comunicación dinámicos | 📝 Pendiente |

---

## Referencias Vivas

Cada vez que encontramos un repo o paper relevante, se añade aquí.

### Papers
- [TRINITY: An Evolved LLM Coordinator](https://arxiv.org/abs/2512.04695) — ICLR 2026
- [Learning to Orchestrate Agents with the Conductor](https://arxiv.org/abs/2512.04388) — ICLR 2026

### Repos
- [SakanaAI/fugu](https://github.com/SakanaAI/fugu) — Codex CLI integration
- [SakanaAI/evolutionary-model-merge](https://github.com/SakanaAI/evolutionary-model-merge) — Método evolutivo

### Productos de referencia
- **Sakana Fugu** — API design, DX, runtime simplicity (10/10)
- **OpenAI Codex CLI** — Terminal-native agent
- **Claude Code** — Reasoning depth, tool use
- **Antigravity** — Browser-native MCP, Streamable HTTP

---

## Notas de Sesión

### 2026-06-23 — Primer análisis Sakana Fugu

Conclusión: Fugu no es un modelo — es un sistema multi-agente disfrazado de API única. Su genio no está en el motor (que es server-side y propietario) sino en:
1. **Simplicidad de integración:** Una API compatible con OpenAI y ya está
2. **Runtime limpio:** `codex-fugu` es un launcher de 400 líneas que hace todo
3. **DX pulida:** Instalación de un solo comando, backup automático, update checks
4. **Benchmarks claros:** Tablas, métricas, comparativas — el usuario sabe lo que paga

Esto valida nuestra dirección: Tylluan debe aspirar a esa misma calidad de integración, pero en MIT, público y soberano.
