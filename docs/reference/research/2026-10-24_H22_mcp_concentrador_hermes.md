# H-22 — Estado del arte: «MCP concentrador» (solo lectura)

Asignación TL T1650 (2026-10-24): «nota de estado del arte MCP concentrador, solo lectura». La seccion H-22 de COLA_hermes no existia en origin/main@7c252ae al empezar; spec vinculante = texto T1650. Entregable: este doc.
Metodo: 7 fuentes primarias descargadas con curl esta sesion y guardadas en disco; 12 citas textuales verificadas con `grep -F` sobre las guardas (whitespace normalizado); [I]=inferencia mia.

**NO ENCONTRADO**: «concentrador/concentrator» no es termino normalizado en ninguna de las 7 fuentes; el ecosistema usa 5 patrones distintos para la misma idea [I]:

## 1. Proxy de borde + multiplexacion de servidores (mcp-proxy)
- «Start multiple named MCP servers behind the proxy» — un solo endpoint, N servidores stdio detras. (mcp-proxy, README; https://github.com/sparfenyuk/mcp-proxy)
- Namespacing por URL: «NAME` is used in the URL path `/servers/NAME/» — colision de nombres resuelta por ruta.

## 2. Aggregacion in-process con prefijo (FastMCP)
- «AggregateProvider for combining multiple providers into one.» y «presents multiple providers as a single unified provider» (fastmcp.server.providers.aggregate; https://github.com/PrefectHQ/fastmcp/blob/main/docs/python-sdk/fastmcp-server-providers-aggregate.mdx)
- Colision resuelta por namespace con prefijo de herramienta: «combined.add_provider(provider2, namespace="api")  # Tools become "api_foo"».
- Nota: jlowin/fastmcp movido a PrefectHQ/fastmcp (redireccion 301 verificada; [I] el repo `jlowin/fastmcp` ya no es la ruta canonica).

## 3. Adaptador cliente-side que fusiona tools (LangChain)
- Patron: un cliente se conecta a N servidores y funde sus tools en el space del agente: «allows you to connect to multiple MCP servers and load tools from them» (https://github.com/langchain-ai/langchain-mcp-adapters).
- Estado del patron: «MCP support has moved into LangChain under the» — el adaptador externo esta deprecado; el concentrador pasa a la libreria nucleo (`langchain[mcp]`).

## 4. Gateway gobernado (Agentgateway, Linux Foundation)
- «that provides drop-in security, observability, and governance for agent-to-LLM, agent-to-tool, and agent-to-agent communication» (https://github.com/agentgateway/agentgateway)
- Funcion concentrador explicita: «Connect LLMs to tools and external data sources via MCP with tool federation, stdio/HTTP/SSE/Streamable HTTP transports, OpenAPI integration, and OAuth authentication.» — el unicos de las fuentes pone auth+governance en el centro.

## 5. Convertidor de transporte (Supergateway)
- «runs **MCP stdio-based servers** over **SSE (Server-Sent Events)** or **WebSockets (WS)** with one command» (https://github.com/supercorp-ai/supergateway) — concentrador minimo: 1 servidor, cambio de transporte, sin fusion de N servidores.

## Contraste: descubrimiento NO es concentracion
- El registro oficial solo lista: «The MCP registry provides MCP clients with a list of MCP servers, like an app store for MCP servers.» (https://github.com/modelcontextprotocol/registry) — resuelve descubrimiento, no agregacion; [I] un concentrador propio tendria que registrarse como agregador y declinar por tool (API freeze v0.1 2025-10-24).
- Hospedado URL->servidor: «GitMCP only accesses content that is already publicly available and only when queried by a user.» (https://github.com/idosal/git-mcp) — concentrador serverless bajo demanda, sin fusion persistente.

## Conclusion para Tylluan [I]
- Los 5 patrones convergen en 2 reglas no negociables: (1) toda fusion de servidores exige estrategia anti-colision (prefijo tipo `api_foo` o ruta `/servers/NAME/`); (2) el punto unico de concentracion es el punto unico de credenciales — solo Agentgateway lo asume (OAuth + governance) y es el patron con mas superficie de ataque.
- Tylluan ya expone exactamente 6 tools soberanas (CONTRACT-01); un concentrador MCP externo choca con esa invariante: [I] la via compatible es patron 2 (agregacion in-process con namespace detras de `tylluan_do`), no exponer tools ajenas en `all_tools()`.
- Sin medida publicada de latencia/overhead de agregacion en ninguna de las 7 fuentes: NO ENCONTRADO.

## Impact
- Solo lectura; ningun codigo del repo tocado; un doc nuevo en docs/reference/research/. Fuentes guardadas: supergateway.md, mcpproxy.md, fm_agg.mdx, lc_adapters.md, agentgateway.md, registry.md, gitmcp.md.
- No verificado: requisitos concretos de Tylluan para un concentrador (no estaban en la spec); rendimiento de agregacion; MetaMCP fue eliminado de GitHub (404 en API y raw, branches main/master/develop) — no citable, baja confianza de que el TL lo de por bueno si lo esperaba.

Gate 12/12: citas verificadas con `grep -F` (whitespace normalizado) sobre las fuentes guardadas esta sesion.
