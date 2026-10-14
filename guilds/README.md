# TylluanNexus — Sistema de Gremios (Guild System)
Version: 2.0.0

---

> **Nota (2026-10-14).** Este documento se corrigió contra el árbol real de `guilds/`. Lo que aquí figura como existente existe; lo que falta está marcado como "no existe". Los *agentes/roles* son ficheros de texto para cargar en un cliente IDE, no procesos del kernel; los procesos que el kernel arranca son los plugins Python.

## ¿Qué es un Gremio?

Un **Gremio** (Guild) en TylluanNexus es un **ecosistema vivo de agentes** organizado por oficio real. No es una colección de herramientas — es una comunidad con identidad, jerarquía, sinergias y memoria compartida.

La analogía es un gremio medieval: un grupo de profesionales con el mismo oficio, que comparten conocimientos, herramientas, aprendices (sub-agentes) y un espacio de trabajo (sandbox).

---

## Arquitectura de un Gremio

Esta es la forma *prevista*; no todos los gremios la completan (ver "Estado real" más abajo).

```
guild-name/
├── guild.md              ← Identidad: misión, estructura, reglas de colaboración
├── agents/               ← Roles universales (cualquier modelo puede adoptarlos)
├── sub-agents/           ← Skills especializadas
├── workflows/            ← Workflows pre-baked de ejemplo
├── plugins/              ← MCP tools Python del gremio (FastMCP), ficheros reales
└── sandbox/              ← Lab de experimentación (solo README en builders)
```

### Estado real (2026-10-14)

| Gremio | `agents/` | `sub-agents/` | `workflows/` | `plugins/` | `sandbox/` |
|--------|-----------|---------------|--------------|------------|------------|
| `builders/` | architect, backend-dev, frontend-dev | solo `rust-specialist.skill.md` | `new-feature.md`, `debug-session.md` | 15 ficheros (ver abajo) | solo `README.md` |
| `scholars/` | no existe | no existe | `deep-research.md` | 11 ficheros (ver abajo) | no existe |
| `wardens/` | no existe | no existe | `health-audit.md` | `audit.py`, `biome_warden.py` | no existe |
| `watchers/` | no existe | no existe | no existe | `cron_scheduler.py`, `monitor.py`, `system_metrics.py` | no existe |

---

## Gremios Activos

| Gremio | Oficio | Agentes | Plugins |
|--------|--------|---------|---------|
| [`builders/`](builders/guild.md) | Ingeniería de Software | architect, backend-dev, frontend-dev | code, git, docker, bash, filesystem |
| [`scholars/`](scholars/guild.md) | Investigación y Análisis | (ninguno todavía) | search, pdf, deep_analysis, knowledge, data_tools, sequential_thinking |
| [`wardens/`](wardens/guild.md) | Integridad y Observabilidad | (ninguno todavía) | audit, biome_warden |

---

## Dónde viven realmente los plugins

No hay un directorio de plugins "compartidos" con esos nombres. Cada fichero está en un sitio concreto:

| Plugin | Ruta real |
|--------|-----------|
| `bash`, `filesystem`, `code`, `git`, `docker`, `code_analysis` | `guilds/builders/plugins/` |
| `memory`, `sequential_thinking`, `ingest`, `search`, `pdf`, `knowledge`, `data_tools`, `deep_analysis` | `guilds/scholars/plugins/` |
| `audit`, `biome_warden` | `guilds/wardens/plugins/` |
| `monitor`, `system_metrics`, `cron_scheduler` | `guilds/watchers/plugins/` |
| `browser`, `vision`, `coloquio`, `scheduler`, `websearch` y otros | `guilds/core/*.py` |

---

## Cómo Funciona la Activación

### Desde un Agente IDE (Cursor, Claude Code, etc.)

**Opción 1: Cargar rol directamente**
```
@builders/agents/backend-dev
```
Incluye el `backend-dev.md` como parte del contexto del agente.

**Opción 2: Via tylluan_do (routing automático)**
```
tylluan_do("necesito implementar un nuevo endpoint en el kernel Rust")
```
TylluanNexus analiza el intent, selecciona el gremio `builders` y el agente `backend-dev`, y retorna las opciones disponibles.

**Opción 3: Via tylluan_do con gremio explícito**
```
tylluan_do("@builders necesito revisar esta arquitectura")
```

---

## Cómo Funciona la Memoria Compartida

Cada gremio tiene un namespace en SilvaDB:

| Gremio | Namespace | Tipos de Nodos |
|--------|-----------|---------------|
| builders | `builders:` | decision, pattern, anti_pattern, lesson, feature |
| scholars | `scholars:` | research, finding, source, synthesis |
| wardens | `wardens:` | alert, incident, audit_result, metric_snapshot |

```bash
# Guardar conocimiento del gremio
tylluan_remember("builders:pattern — Usar Arc<RwLock<T>> para estado compartido en Axum")

# Recuperar conocimiento del gremio
tylluan_recall("builders: patrones de estado compartido")
```

---

## Cómo Funciona la Sinergia entre Gremios

Los gremios se comunican via el **Blackboard** de TylluanNexus:

```
# Ejemplo: Warden detecta un problema, notifica a Builders
tylluan_do("escribe en blackboard: [WARDEN] guild filesystem en crash loop")

# Builders recoge la tarea
tylluan_do("ver blackboard")
# → "Tarea pendiente: guild filesystem en crash loop (creado por guardian)"
```

El Blackboard es el sistema de mensajería entre gremios. No se hablan directamente — colaboran via tareas asignadas.

---

## Cómo Añadir un Nuevo Gremio

1. Crear carpeta: `guilds/nuevo-gremio/`
2. Crear `guild.md` con identidad y estructura
3. (Opcional) crear agentes en `agents/` y al menos 1 workflow en `workflows/`
4. (Opcional) crear `sandbox/README.md`
6. Registrar el gremio en este README
7. (Opcional) Actualizar `catalog.rs` si el gremio tiene plugins Python propios

---

## Cómo Crear un Nuevo Workflow

1. Inspirarse en los workflows existentes (ver `builders/workflows/`)
2. Crear el borrador en el sandbox del gremio (hoy solo `builders/sandbox/` existe)
3. Probarlo ahí
4. Si funciona: mover a `workflows/` del gremio
5. Documentar en SilvaDB: `tylluan_remember("builders:workflow — nuevo workflow creado")`

---

## Filosofía del Sistema de Gremios

> Los gremios no son herramientas. Son comunidades de práctica.

- **Soberanía**: Cada gremio opera de forma autónoma. No depende de servicios externos.
- **Sinergias reales**: Los gremios colaboran via Blackboard y SilvaDB, no via APIs acopladas.
- **Auto-evolución**: Los agentes pueden crear nuevos workflows y skills desde el sandbox.
- **Memoria colectiva**: El conocimiento de cada gremio persiste en SilvaDB y beneficia a futuras sesiones.
- **Roles universales**: Un agente `backend-dev` no está ligado a Claude, Cursor o cualquier modelo específico. Cualquier LLM puede adoptar el rol.
