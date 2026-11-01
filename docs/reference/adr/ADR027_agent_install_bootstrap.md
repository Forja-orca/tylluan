# ADR-027: Instalación de Tylluan por parte del propio agente (F8)

- **Estado:** ⏸️ **APLAZADO (José, 2026-11-01)** — se retoma cuando exista el instalador para personas (H4) y las releases firmadas. No se publica ningún plugin ni marketplace antes.
- **Frente:** F8 del roadmap. Va **después** de F6 (instalador para personas): primero la abuela, luego el agente.

## 1. Contexto
El usuario debe poder pasarle a *su* agente un comando o una dirección y que el agente instale un Tylluan limpio como plugin o skill en su IDE o CLI, de modo que cada agente tenga a Tylluan integrado como sistema vivo. La mayoría de IDEs y CLIs admiten ya plugins, skills o configuración MCP.

## 2. Qué hay hoy (fuentes abiertas en esta sesión; lo no confirmado, marcado)
| Cliente | Mecanismo | Empaqueta | Instalable por URL |
|---|---|---|---|
| Claude Code | Plugin con marketplace: `plugin marketplace add owner/repo#tag` y `plugin install` | skills, agentes, hooks y servidores MCP | Sí, no interactivo, con versión fijada por etiqueta |
| GitHub Copilot (CLI y VS Code) | Plugin con `plugin.json` | agentes, skills, hooks, MCP | Parcial; comando exacto no verificado |
| Cursor | Plugin con `plugin.json`, revisión manual en su marketplace | reglas, skills, agentes, comandos, MCP, hooks | Parcial |
| Gemini CLI / Antigravity | Extensiones con `gemini-extension.json` | MCP, contexto, comandos, skills | Sí, por URL de GitHub |
| OpenCode | Plugins JS/TS o paquetes npm | hooks y herramientas | Parcial |
| Cline, Continue, JetBrains | Solo configuración MCP | MCP | Sí para MCP; no para un paquete completo |
| Windsurf, Codex | **No verificado** | — | — |

Formato recomendado para que lo lea un agente: `skill.md`. `install.md` figura como deprecado en la fuente consultada.

## 3. Decisión propuesta
**Un repositorio propio de distribución** con tres elementos, y un protocolo de verificación que el agente cumple *antes* de instalar:
1. `skill.md` con los pasos, el nombre exacto del propietario y la huella de la clave pública.
2. El manifiesto del plugin, los hooks y el `.mcp.json`, siempre con `127.0.0.1` (nunca `localhost`).
3. `SHA256SUMS` del artefacto, **firmado** con la clave de Tylluan y con procedencia verificable.

### 3.1 Comprobaciones previas del agente
- La fuente es el propietario exacto y una **etiqueta firmada** (o un commit fijado), nunca una rama.
- El hash y la firma del artefacto coinciden; si no, se aborta sin instalar.
- El agente **muestra al usuario** el origen, `plugin.json`, los hooks y el `.mcp.json` antes de instalar.
- Se rechaza cualquier fuente de tipo comando sin confirmación explícita; nunca se usa un «sí a todo» automático.
- El MCP expuesto contiene solo las herramientas propias de Tylluan (invariante de soberanía de herramientas).
- La instalación no mata procesos ni toca un kernel en marcha.

### 3.2 Anti-suplantación
Un marketplace propio con nombre no confundible con los oficiales; la huella de la clave se publica por un canal distinto del que trae el comando; ningún plugin se instala solo por su nombre.

## 4. Orden de clientes
1) Claude Code, 2) Copilot, 3) Cursor. Después Gemini/Antigravity y OpenCode. Cline, Continue y JetBrains con configuración MCP. Windsurf y Codex solo cuando se verifiquen sus mecanismos.

## 5. Prueba de aceptación
Un agente de prueba instala desde una única dirección; **rechaza** un artefacto alterado (hash distinto) y una etiqueta sin firma; el plugin instalado expone solo las herramientas de Tylluan; y desinstalar lo deja todo como estaba. Se repite en Windows y Linux.

## 6. Riesgos
Ejecución de código al instalar (hooks y servidores MCP) → de ahí la verificación y la lectura previa por el usuario; configuración del repositorio que se ejecuta sola al abrir una carpeta → ninguna se arranca sin aprobación ligada a su huella; dependencia de un marketplace de terceros.

## 7. No verificado
Windsurf y Codex; el comando exacto de instalación de Copilot; si los plugins de OpenCode pueden registrar MCP; la firma de releases con procedencia (aún no existe); el nombre del propietario del marketplace (por definir).

## Impact
Documento de diseño: no cambia código. Fija el protocolo de confianza antes de publicar nada.
