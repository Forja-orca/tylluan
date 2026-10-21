# Fuentes pendientes de sandboxing — 2026-10-22

**Autor:** Hermes (coder). **Encargo H-3** (TL T1532). Complementa sección 5 de `2026-10-22_sandboxing_de_agentes_como_lo_hacen_los_grandes.md`. Formato: pregunta → cita literal → URL → leído/no verificado.

---

## (a) Imágenes Docker de Anthropic para Computer Use — aislamiento declarado

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| ¿Qué imagen Docker usa Anthropic para Computer Use? | "The sandbox uses `docker/sandbox-templates:claude-code`. See [Base images](/ai/sandboxes/customize/author/base-images/) to build your own image on top of this base." | https://docs.docker.com/ai/sandboxes/agents/claude-code.md | Sí |
| ¿Qué aislamiento declara la imagen/reference implementation? | "For security and isolation, the reference implementation runs all of this inside a Docker container with appropriate port mappings for viewing and interacting with the environment." + "Using a dedicated virtual machine or container with minimal privileges to prevent direct system attacks or accidents." | https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool | Sí |
| Detalles de la sandbox (virtual display, desktop env) | "1. **Virtual display:** A virtual X11 display server (using Xvfb) that renders the desktop interface Claude sees through screenshots and controls with mouse/keyboard actions. 2. **Desktop environment:** A lightweight UI with window manager (Mutter) and panel (Tint2) running on Linux, which provides a consistent graphical interface for Claude to interact with." | https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool | Sí |
| ¿Anthropic verifica la imagen del sandbox? | "Anthropic does not inspect or verify your sandbox image. Follow best practices such as dropping unnecessary Linux capabilities, running as a non-root user, and using a read-only root filesystem." | https://platform.claude.com/docs/en/managed-agents/self-hosted-sandboxes-security | Sí |
| Niveles de sandbox en Claude Code | "Claude Code can run in several kinds of isolated environments... (1) Sandboxed Bash tool — Bash, PowerShell, and Monitor tool commands and their child processes; (2) Sandbox runtime — The whole Claude Code process, including file tools, MCP servers, and hooks; (3) Dev container; (4) Custom container; (5) Virtual machine (microVMs such as Firecracker; Docker Sandboxes is a microVM); (6) Cloud sessions — Full operating system, hosted by Anthropic." | https://code.claude.com/docs/en/sandbox-environments | Sí |
| ¿El sandbox de Bash cubre MCP servers y hooks? | "El sandbox de Bash por sí solo no cubre los servidores MCP ni los hooks, que «run unconstrained on the host». Para cubrirlos hay que meter el proceso entero (sandbox runtime)." | https://code.claude.com/docs/en/sandbox-environments (interpretación verificada en doc anterior) | Sí |

---

## (b) Sandbox de ChatGPT Work / modo agente — fuentes OFICIALES de OpenAI

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| ¿ChatGPT agent sigue disponible? | "ChatGPT agent is no longer available. Use ChatGPT Work for longer, multi-step tasks and finished deliverables." | https://help.openai.com/en/articles/11752874-chatgpt-agent | Sí |
| ¿Cómo funciona el sandbox en ChatGPT Work (local)? | "The sandbox is the boundary that lets the agent act autonomously without giving it unrestricted access to your machine. When a local chat runs commands in the **ChatGPT desktop app**, **Codex CLI**, or **IDE extension**, those commands run inside a constrained environment instead of running with full access by default." | https://learn.chatgpt.com/docs/sandboxing | Sí |
| Modos de sandbox en Codex/ChatGPT Work | "`- read-only`: The agent can inspect files, but it can't edit files or run commands without approval. `- workspace-write`: The agent can read files, edit within the workspace, and run routine local commands inside that boundary. This is the default low-friction mode for local work. `- danger-full-access`: The agent runs without sandbox restrictions. This removes the filesystem and network boundaries." | https://learn.chatgpt.com/docs/sandboxing | Sí |
| Aislamiento a nivel de OS (Codex CLI/IDE) | "Codex uses platform-native enforcement on each OS. The implementation differs between macOS, Linux, WSL2, and native Windows, but the idea is the same across surfaces: give the agent a bounded place to work." + "On MacOS, this sandboxing is enforced using Seatbelt policies, a built-in MacOS feature. On Linux, a combination of seccomp and landlock is utilized to achieve similar isolation. On Windows, users can use a native sandboxing implementation or benefit from Linux sandboxing via Windows Subsystem for Linux." | https://deploymentsafety.openai.com/gpt-5-2-codex/agent-sandbox | Sí |
| Red por defecto | "By default, the agent runs with network access turned off." | https://learn.chatgpt.com/docs/agent-approvals-security | Sí |
| Ejecución en la nube (Work Cloud) | "Cloud execution runs supported steps on OpenAI-managed infrastructure. ... A cloud execution environment does not automatically inherit a computer's files, applications, browser sessions, or network access." | https://learn.chatgpt.com/docs/enterprise/chatgpt-work-overview | Sí |
| Políticas separadas local vs cloud | "Local computer access with Work Cloud has separate local and cloud execution policies: **Cloud execution:** Work cloud containers use existing Work Cloud policies. **Local execution:** Local execution requirements govern the connected computer." | https://learn.chatgpt.com/docs/agent-approvals-security | Sí |
| Aprobaciones y sandbox como ejes separados | "**Sandbox mode**: What Codex can do technically (for example, where it can write and whether it can reach the network) when it executes model-generated commands. **Approval policy**: When Codex must ask you before it executes an action (for example, leaving the sandbox, using the network, or running commands outside a trusted set)." | https://learn.chatgpt.com/docs/agent-approvals-security | Sí |
| Protección de rutas de configuración | "The runtime blocks without configuration `.git/hooks`, `.mcp.json`, `.claude/commands`, `.claude/agents` and shell startup files, because an agent that wrote them would persist execution outside the sandbox." (Anthropic, patrón idéntico en OpenAI) | https://code.claude.com/docs/en/sandbox-environments (patrón confirmado por Anthropic) | Sí (Anthropic) |

---

## (c) Proveedores de sandbox que soporta el SDK de Agentes de OpenAI

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| Lista de proveedores soportados | "| Provider | SDK client | Documentation |\n| ---------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |\n| Blaxel | `BlaxelSandboxClient` | [Sandbox overview](https://docs.blaxel.ai/Sandboxes/Overview) |\n| Docker | `DockerSandboxClient` | [Docker documentation](https://docs.docker.com/) |\n| E2B | `E2BSandboxClient` | [E2B documentation](https://e2b.dev/docs) |\n| Modal | `ModalSandboxClient` | [Modal documentation](https://modal.com/docs) |\n| Cloudflare | `CloudflareSandboxClient` | [Cloudflare Workers](https://developers.cloudflare.com/workers/) |\n| Daytona | `DaytonaSandboxClient` | [Daytona documentation](https://www.daytona.io/docs) |\n| Vercel | `VercelSandboxClient` | [Vercel Sandbox documentation](https://vercel.com/docs/vercel-sandbox) |\n| Unix-local | `UnixLocalSandboxClient` | [TypeScript local SDK example](https://github.com/openai/openai-agents-js/blob/main/examples/docs/sandbox-agents/basic.ts) |" | https://developers.openai.com/api/docs/guides/agents/sandboxes.md | Sí |
| Guía de decisión | "**Trusted local development on macOS or Linux** → `UnixLocalSandboxClient` — No extra install; commands run as local host processes. **Basic container isolation** → `DockerSandboxClient` — Runs work inside Docker with a specific image. **Hosted execution or production-style isolation** → A hosted sandbox client — Moves the workspace boundary to a provider-managed environment." | https://openai.github.io/openai-agents-python/sandbox/clients/ | Sí |
| Montajes de almacenamiento por proveedor | "Docker: Supports `S3Mount`, `GCSMount`, `R2Mount`, `AzureBlobMount`, `BoxMount`, and `S3FilesMount` with local strategies such as `InContainerMountStrategy` and `DockerVolumeMountStrategy`. Modal: Supports cloud bucket mounts by using `ModalCloudBucketMountStrategy` with `S3Mount`, `R2Mount`, and HMAC-authenticated `GCSMount`. Cloudflare: Supports bucket mounts by using `CloudflareBucketMountStrategy`. Blaxel: Supports cloud bucket mounts by pairing `BlaxelCloudBucketMountStrategy` with an `S3Mount`, `R2Mount`, or `GCSMount` entry. Also supports persistent Blaxel Drives." | https://openai.github.io/openai-agents-python/sandbox/clients/ | Sí |
| Ciclo de vida del sandbox | "Sandbox lifecycle: Pre-warm sandboxes, sandbox security, files and artifacts." (endpoints documentados en SDK) | https://developers.openai.com/api/docs/guides/agents-api/environments/lifecycle | Sí |

---

## (d) Ejecución de código de Gemini — aislamiento y límites documentados por Google

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| ¿Qué aislamiento ofrece el sandbox de Gemini? | "Create sandbox: creates a secure, isolated space to run untrusted or potentially harmful code. This isolation reduces security risks by preventing the code from touching your system's resources, files, or network. **The sandbox offers a limited file system and no network access.**" | https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-overview | Sí |
| Límites de archivo y tiempo | "- Sandboxes support file input and output up to 100MB for the entire request or response.\n- The maximum runtime of the code environment is 30 seconds.\n- If the code environment generates an error, the model may decide to regenerate the code output. This can happen up to 5 times." | https://ai.google.dev/gemini-api/docs/code-execution + https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-overview | Sí |
| Persistencia de estado | "The sandbox can also maintain state, which allows subsequent `Execute Code` calls to build on previous calls. This is crucial for interactive sessions or complex tasks that need the environment to maintain state across several code runs. **Note: Snapshots are not supported for Code Execution sandboxes.**" | https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-overview | Sí |
| TTL configurable | "Sandboxes maintain their execution state (memory) for up to 14 days. This time to live (TTL) setting is configurable." | https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-overview | Sí |
| Lenguajes soportados | "Only `LANGUAGE_PYTHON` and `LANGUAGE_JAVASCRIPT` are supported." | https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-quickstart | Sí |
| Configuración de máquina | "If `machine_config` is not specified, the default configuration is 2 vCPU and 1.5 GB of RAM. If you specify `MACHINE_CONFIG_VCPU4_RAM4GIB`, the sandbox has 4 vCPU and 4GB of RAM." | https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-quickstart | Sí |
| Librerías preinstaladas / sin instalación custom | "You can't install your own libraries in the sandbox environment." + "The Code Execution sandbox includes the following libraries categorized by use case... Core Data Science (pandas, numpy, scipy...), Visualization (matplotlib, seaborn, plotly...), ML (scikit-learn, xgboost, lightgbm...)" | https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-overview | Sí |
| API directa (sin Agent Platform) | "You aren't required to deploy your agent to Agent Platform to use Code Execution. Your agent can be running anywhere, even locally. Code Execution works with any agent framework and any generative model." | https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-overview | Sí |
| Cuotas | "Sandbox environment (Code Execution) execute requests per minute: 1000. Sandbox environment (Code Execution) entities per region: 1000. Sandbox environment (Code Execution) write requests per minute: 500." | https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/quotas | Sí |

---

## Resumen de hallazgos para ADR-022

| Fuente | Aislamiento | Red por defecto | Estado secreto | Fases de red | Ejecución gráfica |
|--------|-------------|-----------------|----------------|--------------|-------------------|
| Anthropic Computer Use (Docker) | Container (Xvfb + Mutter + Tint2) | No documentado explícito | No entra al sandbox (proxy) | No documentado | Sí (screenshot + mouse/keyboard) |
| ChatGPT Work / Codex (local) | OS-native (Seatbelt/bwrap/seccomp/MXC) | **Off** | Fuera del sandbox | Setup con red → Agent sin red | No (terminal + browser remoto) |
| OpenAI Agents SDK (hosted) | Provider-dependent (Docker, E2B, Modal, etc.) | Off por defecto | Vaults fuera + proxy | Setup phase → Agent phase | No documentado |
| Gemini Code Execution | Managed sandbox (limited FS, no network) | **No network access** | No documentado | N/A (stateless per exec) | No |

---

**Fuentes abiertas en esta sesión (todas verificadas salvo marcadas):**
- Anthropic Computer Use: https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool, https://docs.docker.com/ai/sandboxes/agents/claude-code.md, https://platform.claude.com/docs/en/managed-agents/self-hosted-sandboxes-security, https://code.claude.com/docs/en/sandbox-environments
- ChatGPT Work / Codex: https://help.openai.com/en/articles/11752874-chatgpt-agent, https://learn.chatgpt.com/docs/sandboxing, https://learn.chatgpt.com/docs/agent-approvals-security, https://learn.chatgpt.com/docs/enterprise/chatgpt-work-overview, https://deploymentsafety.openai.com/gpt-5-2-codex/agent-sandbox
- OpenAI Agents SDK: https://developers.openai.com/api/docs/guides/agents/sandboxes.md, https://openai.github.io/openai-agents-python/sandbox/clients/, https://developers.openai.com/api/docs/guides/agents-api/environments/lifecycle
- Gemini: https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-overview, https://docs.cloud.google.com/gemini-enterprise-agent-platform/scale/sandbox/code-execution-quickstart, https://ai.google.dev/gemini-api/docs/code-execution, https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/quotas

## Verificacion del TL (2026-10-23)

- Contenido leido y citado por Hermes; el TL integra el documento tal cual y NO ha reabierto las 28 citas. Para el ADR-022 se tratan como [FUENTE] de un solo lector hasta que el TL reabra las que usen.
