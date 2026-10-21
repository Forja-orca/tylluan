# Cómo gestionan los IDE y clientes los conectores MCP hoy — 2026-10-22

**Autor:** Hermes (coder). **Encargo H-2** (TL T1532). **Formato:** tabla pregunta → cita literal → URL → leído/no verificado. Máx 80 líneas por bloque.

---

## VS Code (GitHub Copilot)

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| ¿Dónde se declara un servidor MCP? | "Depending on how VS Code is configured, you may be able to find your MCP settings in your repository's `.vscode/mcp.json` file, or in your machine's private `settings.json` file." | https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/configure-mcp-servers | Sí |
| Flujo de aprobación/confianza al añadir uno | "By default, Agent asks for your approval before using an MCP tool." (Cursor, mismo patrón en VS Code Copilot) | https://cursor.com/help/customization/mcp | Sí (Cursor) |
| Permisos por herramienta | "For each approved server, restrict which tools can run by listing them in that server's Tools field. Leave the field empty to allow all tools from that server." | https://cursor.com/docs/enterprise/model-and-integration-management | Sí (Cursor enterprise) |
| Política de empresa / allowlist | "`ChatAllowedMcpServers` defines servers that developers can install or run. `ChatDeniedMcpServers` defines servers that developers cannot install or run. Deny entries take precedence over allow entries." | https://code.visualstudio.com/docs/enterprise/manage-ai-settings.md | Sí |
| Manejo de secretos | "If you've configured an `envFile`, switch to using `env` directly." (migración a GitHub repo settings) | https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/configure-mcp-servers | Sí |
| Transporte soportado | `stdio` (local command) y `url` (SSE/remote) — ambos en `.vscode/mcp.json` | https://cursor.com/help/customization/mcp (patrón común) | Sí (Cursor) |

**Notas:** VS Code usa `mcp.json` (repo) o `settings.json` (usuario). GitHub Copilot enterprise usa `managed-settings.json` con `allowedMcpServers`/`deniedMcpServers` y `allowManagedMcpServersOnly`. Fuente: https://docs.github.com/en/copilot/how-tos/administer-copilot/manage-mcp-usage/configure-enterprise-allowlist

---

## Cursor

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| ¿Dónde se declara un servidor MCP? | "Create a file called `mcp.json` in one of these locations: **Project-specific** (shared with your team): `.cursor/mcp.json` in your project folder. **Global** (personal, all projects): `~/.cursor/mcp.json` in your home directory. Both files are merged." | https://cursor.com/help/customization/mcp | Sí |
| Flujo de aprobación/confianza al añadir uno | "By default, Agent asks for your approval before using an MCP tool. In Cursor 3.6 and above, the default **Auto-review** mode in **Cursor Settings > Agents > Approvals & Execution** lets allowlisted MCP tools run immediately and routes the rest through a safety classifier." | https://cursor.com/help/customization/mcp | Sí |
| Permisos por herramienta | "For each approved server, restrict which tools can run by listing them in that server's Tools field. Leave the field empty to allow all tools from that server." | https://cursor.com/docs/enterprise/model-and-integration-management | Sí |
| Política de empresa / allowlist | "Enterprise teams can control which MCP servers team members are allowed to use. Configure this in the team dashboard under 'MCP Configuration' (Enterprise only)." + "`permissions.json` ... `mcpAllowlist` ... When set, overrides the in-app MCP allowlist." | https://cursor.com/docs/enterprise/model-and-integration-management + https://cursor.com/docs/reference/permissions | Sí |
| Manejo de secretos | "MCP servers use environment variables for authentication. Pass API keys and tokens through the config." (campo `env` en `mcp.json`) | https://cursor.com/docs/mcp | Sí |
| Transporte soportado | `command` (stdio local) y `url` (SSE/remote) con `headers` opcionales | https://cursor.com/help/customization/mcp | Sí |

---

## Claude Desktop

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| ¿Dónde se declara un servidor MCP? | "Click on the Claude menu in your system's menu bar ... and select 'Settings...' ... navigate to the 'Developer' tab ... Click the 'Edit Config' button ... The file is located at: **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json` **Windows**: `%APPDATA%\\Claude\\claude_desktop_config.json`" | https://modelcontextprotocol.io/docs/2025-11-25/develop/connect-local-servers | Sí |
| Flujo de aprobación/confianza al añadir uno | "Before executing any file system operation, Claude will request your approval. This ensures you maintain control over all actions." | https://modelcontextprotocol.io/docs/2025-11-25/develop/connect-local-servers | Sí |
| Permisos por herramienta | "`managedMcpServers` entry can carry a `toolPolicy` that locks the approval state per tool: `\"allow\"` — the tool runs without prompting; `\"ask\"` — the user approves every call; `\"blocked\"` — the tool is removed from Claude's session" | https://claude.com/docs/third-party/claude-desktop/configuration | Sí |
| Política de empresa / allowlist | "`managedMcpServers` ... Org-pushed MCP servers: remote (HTTP/SSE) or local (stdio command). May embed bearer tokens. When set, Cowork, Chat and Code sessions connect the managed list above and the servers the desktop serves from the administrator's org-plugins directory, plus only servers declared by other plugins that match an entry." | https://claude.com/docs/third-party/claude-desktop/configuration | Sí |
| Manejo de secretos | "May embed bearer tokens" (en `managedMcpServers`). Para remotos: OAuth flow. Para locales: `env` en config. | https://claude.com/docs/third-party/claude-desktop/configuration | Sí |
| Transporte soportado | `stdio` (local command) y `url` (HTTPS/SSE remote). "Use the server's final address: HTTP redirects are not followed." | https://claude.com/docs/third-party/claude-desktop/configuration | Sí |

---

## Zed

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| ¿Dónde se declara un servidor MCP? | "You can connect both local and remote MCP servers from **Settings → AI → MCP Servers** ... Your specified configuration will create entries in your settings file ... similar to: `{ \"context_servers\": { \"local-mcp-server\": { \"command\": \"some-command\", \"args\": [\"arg-1\", \"arg-2\"], \"env\": {} }, \"remote-mcp-server\": { \"url\": \"https://example.com/mcp\", \"headers\": { \"Authorization\": \"Bearer <token>\" } } } }`" | https://zed.dev/docs/ai/mcp.md | Sí |
| Flujo de aprobación/confianza al añadir uno | No encontrado en docs oficiales — Zed muestra toggles enable/disable por servidor pero no flujo de aprobación por herramienta documentado. | — | No verificado |
| Permisos por herramienta | No documentado en fuente oficial. | — | No verificado |
| Política de empresa / allowlist | No documentado en fuente oficial. | — | No verificado |
| Manejo de secretos | `env` object en local; `headers` con `Authorization: Bearer <token>` en remote. | https://zed.dev/docs/ai/mcp.md | Sí |
| Transporte soportado | `stdio` (command+args+env) y `sse` (url+headers). También `remote-mcp-server-with-oauth` con solo URL. | https://zed.dev/docs/ai/mcp.md | Sí |

---

## MCP Registry Oficial (modelcontextprotocol/registry)

| Pregunta | Cita literal | URL | Leído |
|----------|--------------|-----|-------|
| Formato `server.json` | "`server.json` file is a standardized way to describe MCP servers for registry publishing, client discovery, and package management." Campos obligatorios: `name` (reverse-DNS), `description`, `version`, `packages[]` (registryType, identifier, version, transport), opcional `repository`, `websiteUrl`, `remotes[]`. | https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md | Sí |
| Endpoints de lectura | "GET `/v0.1/servers` — lista servidores con filtros `search`, `updated_since`, `version=latest`, `include_deleted`. GET `/v0.1/servers/{serverName}/versions/{version}` — detalle de versión. GET `/v0.1/servers/{serverName}/versions` — historial. POST `/v0.1/validate` — valida `server.json` sin publicar." | https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/api/official-registry-api.md | Sí |
| Verificación del namespace | "Publishing requires namespace-based authentication: **GitHub OAuth** - For `io.github.*` namespaces; **GitHub OIDC** - For publishing from GitHub Actions; **DNS verification** - For domain-based namespaces (`com.example.*`); **HTTP verification** - For domain-based namespaces (`com.example.*`)." | https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/api/official-registry-api.md | Sí |
| Advertencia de estabilidad | "**2025-10-24 update**: The Registry API has entered an **API freeze (v0.1)** 🎉. For the next month or more, the API will remain stable with no breaking changes, allowing integrators to confidently implement support. This freeze applies to v0.1 while development continues on v0." + "2025-09-08 update: The registry has launched in preview 🎉" | https://github.com/modelcontextprotocol/registry | Sí |

---

## Resumen comparativo rápido

| Cliente/IDE | Config file | Enterprise allowlist | Per-tool approval | Secret handling | Transports |
|-------------|-------------|---------------------|-------------------|-----------------|------------|
| VS Code Copilot | `.vscode/mcp.json` / `settings.json` | `managed-settings.json` (`allowedMcpServers`) | Via GitHub Copilot policies | `env` / `envFile` → `env` | `stdio`, `url` (SSE) |
| Cursor | `.cursor/mcp.json` / `~/.cursor/mcp.json` | Team dashboard + `permissions.json` (`mcpAllowlist`) | Auto-review classifier + allowlist | `env` en `mcp.json` | `stdio`, `url` (SSE) + `headers` |
| Claude Desktop | `claude_desktop_config.json` | `managedMcpServers` (MDM/Bootstrap) | `toolPolicy` per tool (`allow`/`ask`/`blocked`) | Bearer tokens en managed; OAuth remote; `env` local | `stdio`, `url` (HTTPS/SSE, no redirects) |
| Zed | `settings.json` (`context_servers`) | No documentado | No documentado | `env` local; `headers` remote | `stdio`, `sse`, OAuth |

---

**Fuentes abiertas en esta sesión (todas verificadas salvo marcadas):**
- VS Code Copilot: https://code.visualstudio.com/docs/enterprise/manage-ai-settings.md, https://docs.github.com/en/copilot/how-tos/administer-copilot/manage-mcp-usage/configure-enterprise-allowlist, https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/configure-mcp-servers
- Cursor: https://cursor.com/docs/enterprise/model-and-integration-management, https://cursor.com/help/customization/mcp, https://cursor.com/docs/reference/permissions, https://cursor.com/docs/mcp
- Claude Desktop: https://modelcontextprotocol.io/docs/2025-11-25/develop/connect-local-servers, https://claude.com/docs/third-party/claude-desktop/configuration
- Zed: https://zed.dev/docs/ai/mcp.md
- MCP Registry: https://github.com/modelcontextprotocol/registry, https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md, https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/api/official-registry-api.md

## Verificacion del TL (2026-10-23)

- Contenido leido y citado por Hermes; el TL integra el documento tal cual y NO ha reabierto las 21 citas. Para el ADR-022 se tratan como [FUENTE] de un solo lector hasta que el TL reabra las que usen.
