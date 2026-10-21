# Cómo integran los grandes el «ordenador sandbox» del agente, y cómo adaptarlo a Tylluan — 2026-10-22

**Autor:** TL (Claude). **Encargo:** José, 2026-10-22: investigar cómo lo hacen ChatGPT, Codex, Claude, Grok Build y Z.ai y resumir la mejor adaptación para nosotros. Entrada del ADR-022 (modelo de confianza de extensiones y sandbox). **No vinculante.**
**Método:** páginas oficiales abiertas en esta sesión, con cita literal. Lo que no se pudo abrir consta como no verificado.

## 1. Qué hace cada uno (con fuente)

| Quién | Dónde corre | Aislamiento | Red | Control humano |
|---|---|---|---|---|
| **OpenAI Codex (local: CLI, app, IDE)** | En tu máquina | macOS: Seatbelt (`sandbox-exec`); Linux: `bwrap` más `seccomp`; Windows: «MXC» (término de la fuente, no verificado qué es) | «By default, the agent runs with network access turned off.» | Tres modos: `read-only`, `workspace-write` (por defecto) y `danger-full-access` («No sandbox; no approvals»); políticas `on-request`, `never`, `granular` |
| **OpenAI Codex (nube)** | Un contenedor por tarea | «Codex creates a container and checks out your repo» | Fase de preparación con red; fase del agente «blocks internet access during the agent phase» por defecto | Resultado como «answer and a diff of any files it changed» |
| **Claude, API (herramienta de ejecución de código)** | Contenedor Linux alojado por Anthropic | Linux x86_64, 5 GiB de RAM, 5 GiB de disco, 1 CPU | «Internet access: Completely disabled»; solo bibliotecas preinstaladas | Contenedor reutilizable por ID; caduca a los 30 días; tras unos 5 minutos de inactividad se guarda un punto de control |
| **Claude Code (local y nube)** | Escalera de seis niveles (abajo) | Desde un sandbox por comando hasta una VM completa | Allowlist de dominios; en la nube, proxy con lista por defecto | `--dangerously-skip-permissions` solo dentro de contenedor, VM o runtime de sandbox |
| **xAI Grok Build** | En tu máquina; sandbox oficial, **desactivado por defecto** | Landlock (Linux) y Seatbelt (macOS); cuatro perfiles: `workspace`, `read-only`, `strict` (repositorios no confiables) y `devbox` | El bloqueo de red de los procesos hijos se aplica «Enforced on Linux only»; en macOS no hace nada en `read-only` ni `strict` | «Permissions gate whether a tool call runs; the sandbox limits what an approved call can do»; los perfiles base no protegen `~/.ssh`: hace falta una lista `deny` propia |
| **Perplexity SPACE (código abierto)** | Plataforma de sandboxes para agentes | «running code in isolated Firecracker microVMs», Apache-2.0, escrita en Rust, runtimes enchufables, instantáneas, colas de sandboxes ya arrancados en Kubernetes | Control de salida de red y credenciales fuera del sandbox: **no figuran en el README** (solo prensa) | — |
| **Z.ai / AutoGLM** | Nube de Zhipu | Índice del MIT: «Sandboxed cloud computers GUI with Internet access» | Con internet | «Human users can take over the cloud computer GUI»; el índice apunta «Sandboxing and containment approaches: None found» |
| **ChatGPT (modo agente)** | Un ordenador virtual alojado | Solo fuentes secundarias | — | **No verificado**: la tarjeta del sistema devolvió 403 |

Fuentes: https://docs.x.ai/build/features/sandbox · https://github.com/perplexityai/SPACE · https://learn.chatgpt.com/docs/agent-approvals-security · https://learn.chatgpt.com/docs/cloud/internet-access · https://learn.chatgpt.com/docs/environments/cloud-environment · https://platform.claude.com/docs/en/agents-and-tools/tool-use/code-execution-tool · https://code.claude.com/docs/en/sandbox-environments.md · https://hub.docker.com/r/sbx/grok-kit · https://aiagentindex.mit.edu/2025/autoglm

## 2. La escalera de Claude Code (lo más transferible)

Anthropic no ofrece un sandbox, ofrece una **escalera según el modelo de amenaza**: (1) herramienta Bash con sandbox, sin Docker; (2) «sandbox runtime», que envuelve el proceso entero, incluidas herramientas de fichero, servidores MCP y hooks; (3) dev container; (4) contenedor propio; (5) máquina virtual («microVMs such as Firecracker»; Docker Sandboxes es una microVM, gratuita y sin Docker Desktop); (6) sesión en la nube en una VM gestionada.

Frases que conviene tener a la vista:
- «Sandbox isolation reduces the impact of a breach, but it does not eliminate risk.»
- «Permission modes decide whether a tool call runs […]. Isolation restricts what a command can access once it runs.» Son dos ejes, no uno.
- El clasificador del modo automático «is a per-action control, not an isolation boundary».
- El sandbox de Bash por sí solo no cubre los servidores MCP ni los hooks, que «run unconstrained on the host». Para cubrirlos hay que meter el proceso entero.
- El runtime bloquea sin configuración `.git/hooks`, `.mcp.json`, `.claude/commands`, `.claude/agents` y los ficheros de arranque del shell, porque un agente que los escribiera persistiría ejecución fuera del sandbox.
- Nube: «a separate proxy holds your GitHub token outside the sandbox while issuing scoped credentials».

## 3. Patrones que se repiten en todos

1. **Dos fases de red:** preparar con red, actuar sin ella. Codex nube: los secretos «are only available to setup scripts» y «are removed before the agent phase starts».
2. **Salida denegada por defecto** y, si se abre, lista de dominios y de métodos: Codex recomienda solo «GET, HEAD, and OPTIONS» y nombra los riesgos: «Prompt injection from untrusted web content», «Code or secret exfiltration», «Downloading malware or vulnerable dependencies».
3. **El secreto nunca entra en el sandbox:** un proxy lo guarda y el sandbox recibe un marcador (Docker Sandboxes: «the key itself never enters the sandbox»; Claude en la nube, igual para el token de GitHub).
4. **Rutas de configuración de solo lectura:** Codex protege `.git`, `.codex`, `.agents` y `.aws` de forma recursiva; Claude protege sus hooks y comandos.
5. **Un contenedor efímero por tarea**, con caché acotada (Codex: hasta 12 horas) y resultado como diff.
6. **Aislamiento con primitivas del sistema en local** (Seatbelt, `bwrap`) y **VM o contenedor en la nube**. Nadie usa una distro completa dentro del proceso del agente.
7. **Toma de control humana** cuando el agente maneja un ordenador gráfico (AutoGLM).
8. **Aprobaciones y aislamiento como dos ejes** lo dicen ya dos fabricantes con casi las mismas palabras (Anthropic y xAI).
9. **El valor por defecto es una decisión de producto:** Codex trae el sandbox activado y Grok Build desactivado. Cada perfil tiene un coste de fricción distinto.

## 4. Cómo lo adaptaría Tylluan (propuestas para el ADR-022, no decisiones)

| # | Propuesta | Por qué encaja con nosotros |
|---|---|---|
| A1 | Una interfaz común de sandbox con una escalera de cuatro niveles: **N0** nativo (perfil ligero, sin código ajeno); **N1** primitivas del sistema (`bwrap` en Linux, Seatbelt en macOS); **N2** contenedor sin privilegios con runtime configurable (Docker o Podman); **N3** microVM opcional | Es la escalera de Anthropic aplicada a nuestra regla de que lo que ejecuta código ajeno se desactiva si no hay sandbox. El N0 protege el hardware modesto |
| A2 | **Ejecución en dos fases** para conectores: instalar con red y lista de dominios, ejecutar sin red por defecto | Patrón 1; cierra la vía de exfiltración más barata |
| A3 | **Corredor de secretos:** Tylluan guarda los tokens y el sandbox solo ve marcadores; nunca se monta `.tylluan-token` ni el directorio de datos | Patrón 3; respeta la regla de José de no perder acceso, porque los secretos no se mueven |
| A4 | **Salida por conector:** vacía por defecto, dominios y métodos declarados en el manifiesto | Patrón 2; mismo manifiesto de capacidades que ya pidió Deep |
| A5 | **Rutas protegidas de solo lectura** dentro de cualquier espacio de trabajo: `.git`, la configuración de Tylluan, el directorio de datos, el token y los scripts de arranque | Patrón 4; evita que un conector deje persistencia fuera del sandbox |
| A6 | **Espacio de trabajo en copia y entrega como diff** que el operador aprueba | Patrón 5; coincide con el «copia más diff» de Deep y con nuestro modo plan |
| A7 | **Dos ejes separados:** aprobaciones (si se ejecuta) y aislamiento (qué alcanza). El juez ASI06 y los clasificadores no cuentan como frontera | La lección explícita de Anthropic; ya tenemos aprobaciones y modo plan |
| A8 | **Los servidores MCP externos arrancan envueltos** por nuestro lanzador | La advertencia de Anthropic: sin eso corren sin restricción en el anfitrión |
| A9 | **Entorno efímero por tarea con caducidad**, arranque perezoso y punto de control por inactividad | Referencia: Codex 12 horas; API de Claude 30 días y ~5 minutos de inactividad. En «tostadora» interesa arrancar solo si se pide |

| A10 | **Para el nivel de microVM, estudiar SPACE antes de escribir algo propio:** Apache-2.0 (compatible con nuestra MIT), Rust y con runtimes enchufables | Filosofía de aprovechar lo existente; requisitos del anfitrión (KVM, Linux) sin verificar |
| A11 | **Lista `deny` propia además de los perfiles:** los perfiles base de Grok no protegen `~/.ssh` | Los nuestros deben nombrar de serie el directorio de datos, el token y las claves de federación |

**Lo que no copiaría:** una VM gestionada por nosotros en la nube (exige infraestructura y no es soberana), ni el ordenador gráfico con toma de control (no hay caso de uso nuestro que lo pida hoy).

## 5. No verificado

- La tarjeta del sistema de ChatGPT (modo agente): 403; solo hay fuentes secundarias sobre su arquitectura.
- Grok Build: en la primera versión de esta nota dije que no había documentación oficial; **era un error mío**: existe y la leí después (https://docs.x.ai/build/features/sandbox).
- Perplexity SPACE: la licencia, el lenguaje y Firecracker están en el README; la latencia de unos 60 ms, las credenciales fuera del sandbox, el control de salida de red y la prueba de 108 intentos de fuga solo figuran en prensa.
- Imágenes de Docker de Anthropic para el uso de ordenador, «Sandbox Mode» de ChatGPT Work y los proveedores del SDK de OpenAI (E2B, Modal, Daytona): citados en un texto que pegó José, sin comprobar.
- AutoGLM: el índice del MIT dice «None found» sobre contención; no hay documento del desarrollador.
- Qué significa «MXC» en Windows para Codex: solo la cita de la página.
- Google Gemini, Manus y otros no se han leído en esta nota.
- Ninguna cifra de rendimiento (arranque, memoria) de Firecracker, gVisor o Podman está medida por nosotros; la nota anterior del coloquio ya marcó como sin fuente las del mapa pegado. La medición con nuestra carga real (`cargo build` y una guilda Python) decide el backend por sistema operativo.
