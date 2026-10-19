# Autorización de MCP y OAuth: requisitos primarios para OAuth F2

**Fecha:** 2026-10-19 · **Autor:** Claude Code (TL) · **Propósito:** fijar, con fuentes primarias, qué exige la especificación antes de `dev_mode=false`. Todas las citas se leyeron de la fuente el mismo día; lo que es análisis mío va marcado como tal.

## Fuentes (leídas completas, no de resumen)

| Fuente | Qué es |
|---|---|
| [MCP, Authorization (revisión 2026-07-28)](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization) | Flujo, descubrimiento, registro de clientes, scopes, tokens |
| [MCP, Authorization Security Considerations (2026-07-28)](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations) | Requisitos normativos de seguridad |
| [RFC 9700, OAuth 2.0 Security BCP (BCP 240, enero 2025)](https://www.rfc-editor.org/rfc/rfc9700.html) | Prácticas obligatorias de seguridad |

Límite: el RFC 9700 se leyó hasta el carácter 100.000 de 142.010; no he leído su final.

## Requisitos que importan a Tylluan (citas literales)

| Tema | Cita | Fuente |
|---|---|---|
| Descubrimiento | "MCP servers **MUST** implement OAuth 2.0 Protected Resource Metadata (RFC9728)" | MCP, Overview |
| Registro de clientes | Client ID Metadata Documents **SHOULD**; "Dynamic Client Registration is deprecated" | MCP, Overview |
| Redirect URI | "Authorization servers **MUST** validate exact redirect URIs against pre-registered values" | MCP, Security |
| Redirect URI (BCP) | "MUST utilize exact string matching except for port numbers in localhost redirection URIs" | RFC 9700, 2.1 |
| Comunicación | "All redirect URIs **MUST** be either `localhost` or use HTTPS" | MCP, Security |
| PKCE | "MCP clients **MUST** use the `S256` code challenge method"; si falta `code_challenge_methods_supported`, "MUST refuse to proceed" | MCP, Security |
| Código de un solo uso | "authorization codes MUST be invalidated ... after their first use at the token endpoint" | RFC 9700, 4.2.4 |
| Audiencia | "MCP servers **MUST** only accept tokens specifically intended for themselves"; el cliente "MUST include the `resource` parameter" (RFC 8707) | MCP, Security |
| Sin reenvío | "The MCP server **MUST NOT** pass through the token it received" | MCP, Security |
| Token en la URL | "Access tokens **MUST NOT** be included in the URI query string" | MCP, Authorization |
| Scopes | 403 con `error="insufficient_scope"` y `scope=...` en `WWW-Authenticate`; todos los scopes necesarios en un solo desafío | MCP, Authorization |
| Refresco | "For public clients, authorization servers **MUST** rotate refresh tokens" | MCP, Security |
| Consentimiento | Un proxy con client ID estático "**MUST** obtain user consent for each dynamically registered client" | MCP, Security |
| `iss` | Servidores **SHOULD** incluir `iss` (RFC 9207); se prevé subirlo a MUST | MCP, Authorization |

## Qué tiene Tylluan hoy (comprobado en `oauth.rs`)

- **Cumple:** endpoint `/.well-known/oauth-authorization-server` con `code_challenge_methods_supported: ["S256"]` (línea 166); solo acepta S256 (línea 225); verifica el PKCE (línea 315); código de un solo uso y ligado al `redirect_uri` (casos C4a y C4b de la suite adversarial); token por query rechazado para JWT (C5); redirect solo loopback y, desde hoy, sin userinfo ni backslash (`1a2b6a0`).
- **No encuentro:** `/.well-known/oauth-protected-resource` (RFC 9728) — **es un MUST del servidor MCP**; parámetro `resource` o validación de audiencia (no hay `aud` ni `resource` en `oauth.rs`); `iss` en las respuestas; desafío `WWW-Authenticate` con `scope`/`insufficient_scope`; rotación de refresh tokens.

## Análisis mío (no es cita)

1. **El hueco de mayor peso es RFC 9728.** Con `dev_mode=false`, los clientes que descubren la autorización por esta vía (los que cumplen la spec) no encontrarán de dónde sacar un token. Es previo a cualquier consentimiento.
2. **Consentimiento y registro de F2 encajan con la spec** si se usa registro previo (permitido) y la página de consentimiento muestra el host del `redirect_uri` ("MUST clearly display the redirect URI hostname", Client ID Metadata Document Security). DCR queda fuera por estar deprecado.
3. **Audiencia:** hoy el JWT parece no ligar el recurso. Sin `aud`, un token de otro servicio podría presentarse aquí. Añadirlo es barato y no añade pasos al usuario (encaja con el presupuesto de fricción).
4. **Orden propuesto:** (1) RFC 9728 + `resource`/`aud`; (2) `WWW-Authenticate` con scope e `insufficient_scope`; (3) consentimiento recordado por cliente; (4) `iss` y rotación de refresh; (5) activar la suite adversarial y recién entonces `dev_mode=false`.

## No verificado

No he abierto RFC 9728, RFC 8707 ni RFC 9207 directamente (solo lo que MCP cita de ellos); no he probado cómo se comporta Claude Desktop con `mcp-remote` ante un 401 sin metadatos del recurso.
