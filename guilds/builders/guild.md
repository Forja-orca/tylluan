# 🔨 Gremio de Constructores (Builders Guild)
Version: 1.0.0  
Oficio Real: **Ingeniería de Software Soberana**

---

> **Nota (2026-10-14).** Este documento se corrigió contra el árbol real de `guilds/`. Lo que aquí figura como existente existe; lo que falta está marcado como "no existe". Los *agentes/roles* son ficheros de texto para cargar en un cliente IDE, no procesos del kernel; los procesos que el kernel arranca son los plugins Python.

## Identidad del Gremio

Los Constructores son el gremio central de TylluanNexus. Su oficio es **crear, mantener y evolucionar software** con criterios de soberanía: sin dependencias externas innecesarias, sin cajas negras, sin lock-in.

Un Constructor no escribe código por escribirlo. Cada línea tiene una razón, cada abstracción tiene un coste conocido.

---

## Misión

> "Construir sistemas que cualquier agente pueda operar, cualquier humano pueda leer, y ningún proveedor pueda controlar."

---

## Estructura del Gremio

```
builders/
├── guild.md                    ← Este archivo (identidad del gremio)
├── agents/
│   ├── architect.md            ← Agente: Arquitecto de Sistemas
│   ├── backend-dev.md          ← Agente: Desarrollador Backend
│   └── frontend-dev.md         ← Agente: Desarrollador Frontend   (devops.md: no existe)
├── sub-agents/
│   └── rust-specialist.skill.md   (python-specialist y api-designer: no existen)
├── workflows/
│   ├── new-feature.md          ← Workflow: Nueva Feature Completa
│   └── debug-session.md        ← Workflow: Sesión de Debugging   (release.md: no existe)
├── plugins/                    ← MCP tools del gremio: ficheros reales (no son symlinks)
│   ├── code.py, code_analysis.py, git.py, docker.py, bash.py, filesystem.py, database.py
│   └── sandbox.py, formatter.py, audio_tools.py, ffmpeg_tools.py, screenshot_tools.py,
│       clipboard_tools.py, local_llm_proxy.py, utils.py
└── sandbox/
    └── README.md               ← Cómo usar el sandbox lab (no hay subdirectorio experiments/)
```

---

## Agentes del Gremio

| Agente | Rol | Especialidad Principal |
|--------|-----|----------------------|
| `architect` | Arquitecto de Sistemas | Diseño de APIs, schemas, ADRs, revisión técnica |
| `backend-dev` | Desarrollador Backend | Rust/Python, kernel, guilds, performance |
| `frontend-dev` | Desarrollador Frontend | React/TS, dashboards, UX soberana |
| `devops` | Ingeniero DevOps (rol previsto; **no existe** `agents/devops.md`) | Docker, CI/CD, monitoreo, deployments |

**Nota**: Los agentes son **roles universales** — no están ligados a un modelo LLM específico. Cursor, Claude, Qwen o cualquier cliente puede adoptar estos roles cargando el `agent.md` correspondiente.

---

## Plugins del Gremio

Herramientas MCP disponibles para todos los agentes de este gremio:

| Plugin | Descripción |
|--------|-------------|
| `code` | Edición y refactorización de código |
| `git` | Control de versiones |
| `docker` | Orquestación de contenedores |
| `bash` | Comandos de shell y scripting |
| `filesystem` | Operaciones de archivos y directorios |

---

## Reglas de Colaboración (Sinergias)

1. **Arquitecto Primero**: Para features nuevas, el `architect` siempre diseña antes de que cualquier `backend-dev` o `frontend-dev` implemente.
2. **Revisión Cruzada**: El `backend-dev` revisa PRs de `frontend-dev` en la capa de API y viceversa en la capa de UI.
3. **DevOps en el Loop**: El `devops` debe aprobar cualquier cambio en `Dockerfile`, `docker-compose.yml` o scripts de deployment.
4. **Blackboard Compartido**: Los agentes se comunican tareas y bloqueos via el Blackboard de TylluanNexus (`tylluan_do` → blackboard guild).
5. **Sandbox Primero**: Cualquier experimento técnico NO probado va al `sandbox/` antes de tocar código de producción.

---

## Memoria Compartida del Gremio

Los agentes de este gremio comparten un namespace en SilvaDB:
- **Namespace**: `builders:`
- **Tipos de nodos compartidos**: `decision`, `pattern`, `anti_pattern`, `lesson`
- **Acceso**: Cualquier agente del gremio puede leer y escribir con `tylluan_remember` usando el prefijo `builders:`

Ejemplo:
```
tylluan_remember("builders:pattern — Para APIs Axum, usar State<Arc<AppState>> en lugar de closures")
tylluan_recall("builders: patrón de error handling en Rust")
```

---

## Workflows Pre-Baked

Workflows de ejemplo que los agentes pueden usar directamente o como base para crear nuevos:

| Workflow | Cuándo Usarlo |
|----------|--------------|
| `new-feature.md` | Implementar una feature completa desde diseño hasta tests |
| `debug-session.md` | Debugging estructurado de bugs en producción |
| `release.md` | (no existe todavía) |

Para crear un nuevo workflow basado en los existentes, un agente puede:
```
tylluan_recall("builders:workflow")  # Recupera patrones de workflow del gremio
# → Adapta el template al nuevo caso
# → Valídalo en el sandbox del gremio
# → Si funciona, propone como nuevo workflow al gremio
```

---

## Sandbox Lab

El sandbox es el espacio seguro de experimentación del gremio. Los agentes pueden:
- Probar integraciones de plugins antes de usarlos en producción
- Crear workflows experimentales
- Validar scripts y automatizaciones en entorno aislado
- Compartir hallazgos con el gremio via `tylluan_remember`

Ver `sandbox/README.md` para instrucciones de uso.

---

## Activación (para Agentes IDE)

Para activar el modo Constructor en tu cliente IDE (Cursor, Claude Code, etc.):
```
@builders/agents/backend-dev  # Carga el rol de Backend Developer
```
O manualmente incluir el contenido del `agent.md` correspondiente en tu SYSTEM prompt.
