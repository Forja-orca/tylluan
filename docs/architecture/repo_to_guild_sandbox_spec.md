# Repo-to-Guild Automático + Sandbox OCI — Especificación de Investigación

> **Autor:** Claude Code (Tech Lead), a partir de la visión original de José
> **Target:** Deep, Buffy, Antigravity, José
> **Status:** Investigación + arquitectura propuesta — sin contrato de
> implementación todavía, pendiente de aprobación de alcance
> **Fecha:** 2026-09-20
> **Decisiones de José ya fijadas**: automatización completa desde el
> diseño (no un asistente semi-manual como primer paso), evaluar podman
> como motor de contenedor real, no solo Docker CLI.

---

## 1. La visión, en una frase

Un agente conectado a Tylluan (Claude Code, OpenCode, cualquiera) pide una
capacidad que Tylluan no tiene todavía como guild — Tylluan localiza el
repo de GitHub relevante, lo convierte en guild real, lo ejecuta en un
contenedor OCI aislado, y entrega el resultado por el canal del propio
agente o, si eso falla, en una carpeta `output/` garantizada. Todo
agnóstico de sistema operativo, sin depender de Docker Desktop.

Ejemplo del propio José: pedir un vídeo largo sobre una investigación
propia → Tylluan resuelve que ComfyUI es la herramienta → si no está
como guild, lo instala/convierte → el guild "enseña" al agente a usarlo →
consulta memoria por trabajo previo con ComfyUI (de este agente o de
otros) para afinar el resultado → ejecuta en sandbox → entrega el vídeo.

---

## 2. Qué existe ya (verificado, no asumido)

- **`comfy_ui` guild** (`guilds/core/comfy_ui.py`): guild ya escrito a
  mano, pre-cableado a un `COMFY_BASE_URL` local, con salida a
  `data/outputs/comfy`. Es el caso concreto que José usa como ejemplo,
  pero es un guild **pre-existente y estático** — no demuestra ninguna
  pieza del pipeline genérico "repo→guild".
- **Sandbox por Docker CLI** (`registry/guild_process.rs:383-408`):
  `Command::new("docker")`, gateado por `resolve_docker_profile()` y
  `[security.sandbox]` (hoy `enabled = false` por defecto). Ejecuta
  guilds **ya existentes** bajo un perfil restrictivo — no resuelve la
  conversión repo→guild, solo la contención de guilds ya registrados.
  Confirma que el proyecto ya evita Docker Desktop (llamada CLI directa),
  consistente con el principio ya documentado.
- **`[guilds.v2]`** (`tylluan.toml`): manifiesto declarativo de guilds
  por plugin, con `always_on`/`agents`/`path`/`plugins` — el formato que
  cualquier guild generado automáticamente tendría que producir para
  registrarse, pero hoy se escribe a mano.
- **No existe**: ningún mecanismo de "clonar repo de GitHub → generar
  wrapper MCP → registrar como guild v2" en ningún lugar del código
  actual. Confirmado por búsqueda directa (`git clone`, `from_repo`,
  `install_from_github` — cero resultados en `guilds/` o
  `crates/tylluan-kernel/src/`).

---

## 3. Investigación de mercado — no hay precedente que copiar

Búsqueda real 2026: existen servidores MCP con sandbox de ejecución
(`code-sandbox-mcp`, `agent-workspace-mcp`, `tool-sandbox-mcp`) — todos
sandboxean **código que el propio agente escribe o ejecuta**, nunca
**convierten automáticamente un repositorio de terceros en una
herramienta MCP nueva**. Esto confirma que el pipeline repo→guild
automático que José describe es diseño original de Tylluan, no una
pieza que se pueda adoptar de un proyecto existente — hay que diseñarlo
desde cero, con la disciplina de verificación habitual del proyecto
(spike acotado antes de comprometerse a la arquitectura completa).

---

## 4. Motor de contenedor: Podman — veredicto con evidencia

| Criterio | Docker | Podman |
|---|---|---|
| Arquitectura | Cliente-servidor, depende de un daemon | **Daemonless** — cada comando es un proceso normal que exec/forkea el runtime (crun/runc) directamente |
| Privilegios | Rootless requiere configuración extra | **Rootless por defecto** — un escape de contenedor aterriza como usuario sin privilegios en el host, no como root |
| Compatibilidad CLI | — | **95-99% compatible** con la sintaxis de `docker` — `Command::new("docker")` existente migra con cambios mínimos |
| Licencia/coste | De pago para organizaciones grandes (~$21/usuario/mes desde 250 empleados o $10M+ de ingresos) | **Apache 2.0, sin coste**, sin excepción por tamaño de organización |

**Veredicto**: Podman es la recomendación real para este pipeline
específico — la ejecución de guilds generados automáticamente desde
repos de terceros es exactamente el caso de mayor riesgo (código no
auditado por el equipo), y el modelo rootless-por-defecto de Podman da
una garantía de contención más fuerte sin configuración adicional. El
código Docker-CLI existente (`guild_process.rs`) se mantiene como
fallback — no se elimina, porque no todos los sistemas objetivo (RPi4,
Windows sin WSL2 completo) tendrán Podman disponible igual de fácil.

Referencias: [Last9, Podman vs Docker 2026](https://last9.io/blog/podman-vs-docker/), [Tech Insider, 7 tests 2026](https://tech-insider.org/podman-vs-docker-2026/).

---

## 5. Arquitectura propuesta (alto nivel, para debate — no contrato final)

```
[Agente pide capacidad] 
       │
       ▼
[Router: ¿existe guild para esto?] ──sí──> [ejecutar guild existente]
       │ no
       ▼
[Buscador de repo GitHub relevante]     (nuevo componente)
       │
       ▼
[Generador de wrapper MCP]              (nuevo componente — el reto real)
   - detecta lenguaje/runtime del repo
   - detecta forma de invocación (CLI, API HTTP local, import de librería)
   - genera guild v2 (plugin.py + entrada en [guilds.v2])
       │
       ▼
[tylluan_recall: trabajo previo con esta herramienta]  (YA EXISTE — reutilizar)
   - afina el resultado con experiencia acumulada de cualquier agente
       │
       ▼
[Ejecución en sandbox OCI — Podman preferido, Docker CLI fallback]
   - [security.sandbox] ya existe como config, extendido para esto
       │
       ▼
[Entrega]
   - por el canal del agente que pidió (IDE/output MCP), si es posible
   - fallback GARANTIZADO: carpeta output/ de Tylluan (nueva, no existe hoy)
```

**Piezas nuevas identificadas, ninguna implementada todavía**:
1. Buscador de repo (probablemente investigación web + heurística, no
   trivial de automatizar bien).
2. Generador de wrapper MCP — el componente de mayor riesgo e incertidumbre
   técnica real. Necesita su propio spike acotado antes de comprometerse.
3. Extensión del sandbox existente a Podman.
4. Carpeta `output/` de Tylluan como destino de entrega garantizada — no
   existe hoy, es la pieza más barata y debería ser de las primeras.

---

## 6. Qué NO se decide en este documento

- No se decide todavía el mecanismo exacto de generación del wrapper MCP
  — es el punto de mayor riesgo y necesita su propio spike, no una
  decisión de arquitectura de escritorio.
- No se decide si el buscador de repo usa un índice curado, búsqueda web
  en vivo, o una combinación — pendiente de investigación separada.
- No se implementa nada de esto todavía — este documento es
  investigación + arquitectura de alto nivel, a la espera de que el
  equipo la revise antes de repartir contratos de implementación.
