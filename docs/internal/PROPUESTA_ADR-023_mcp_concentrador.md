# PROPUESTA, no aprobada — ADR-023: Tylluan como concentrador MCP

Borrador de Hermes (H-23, T1658 + COLA sección H-23 @9b07412). **La aprobación es de José**, punto por punto (§6). Árbol verificado: `bec191b`. Insumos: `docs/reference/research/2026-10-24_H22_mcp_concentrador_hermes.md` (estado del arte, 5 patrones), `docs/reference/adr/ADR022_extension_trust_sandbox.md` (confianza de terceros — no se decide aquí), `docs/internal/MAPA_PROYECTO_vivo.md` I-01/J-04. Ámbito: cómo Tylluan expone tools de servidores MCP externos a agentes que solo ven las 6 soberanas. Fuera: confianza de extensiones (ADR-022), identidad de agente, topes de gasto de nube (J-05).

## 1. Contexto

Un «concentrador MCP» unifica varios servidores MCP tras una sola puerta. H-22 mostró que no hay un patrón canónico: son 5 (proxy de borde con rutas `/servers/NAME/`, agregación in-process con prefijo tipo `api_foo`, fusión client-side, gateway gobernado con OAuth, y convertidor de transporte); el registro oficial solo descubre, no concentra. Tylluan quiere lo segundo: que los agentes sigan viendo exactamente 6 tools («Exactamente 6 (ADR-021). No añadir sin enmienda formal de CONTRACT-01.», `crates/tylluan-kernel/src/transport/server/tools.rs`, línea 48) mientras el kernel habla con N servidores externos.

El concentrador **ya existe a medias** (hechos en `bec191b`):

| # | Hecho | Evidencia |
|---|---|---|
| F1 | Config declarativa de externos: «pub external_mcp: Vec<ExternalMcpConfig>,» | `crates/tylluan-kernel/src/config.rs`, línea 127; campos incluyen `command/args/url/sse_url/post_url` (líneas 793-805) |
| F2 | Credenciales hoy en claro en config: «pub headers: Option<HashMap<String, String>>,» | `config.rs`, línea 807; timeout de transporte sí configurable: «pub timeout_ms: Option<u64>,» (línea 808) |
| F3 | El kernel ya es cliente MCP de los externos: «self.jsonrpc("tools/list", serde_json::json!({})» | `crates/tylluan-kernel/src/registry/proxy.rs`, líneas 539 y 1050 |
| F4 | Dispatch de reintento dentro de `tylluan_do`: «After resolve_guild_name fails, attempt dispatch to a registered external MCP server.» (línea 10) y «keyword-score all tools across all active external servers, pick the best» (línea 14) + «match above MIN_SCORE.» (línea 15) | `crates/tylluan-kernel/src/transport/server/handler_do/external_mcp.rs` |
| F5 | El defecto central (confirmado en código, coincide con I-01): «"query": intent, "text": intent, "content": intent,» (línea 135) y «"prompt": intent, "message": intent, "input": intent,» (línea 136) — el mismo texto plano rellena 8 campos; sin validación contra `inputSchema`; `"timeout_secs": 30` fijado en código (línea 139) | `external_mcp.rs`, `do_external_call` (líneas 133-142) |
| F6 | Colisión silenciosa de nombres: «if seen.insert(t.name.to_string()) {» — el duplicado se descarta sin aviso | `tools.rs`, línea 37 |
| F7 | Con el flag por defecto, nada externo sale como tool MCP directa: «expose_guild_tools: false,» | `config.rs`, línea 641; también `transport/server/mod.rs`, línea 181. Si se activa: «This gives agents without native tools direct access to» (línea 14) + «everything without routing through tylluan_do.» (línea 15) | `tools.rs` |
| F8 | Import de config de clientes YA existe en parte: parsea «mcpServers» (línea 321) y protege contra autorreferencias: «Skip self-references: any command containing tylluan-nexus or url pointing to el puerto propio o al antiguo de Forja» (línea 324); export inverso: inserta la entrada propia «tylluan» (línea 1455) | `crates/tylluan-kernel/src/transport/http/api_v1/api_mcp.rs`; `api_v1/mod.rs` |
| F9 | Descubrimiento real hoy: «by design (CONTRACT-01). A client that only ever discovers tools via» (línea 282) + «told the literal tool name out of band) could never reach them otherwise.» (línea 284); «intent text is the only discovery path for clients that don't already» (línea 312) | `transport/server/handler_do/mod.rs` |
| F10 | Hay precedente de cuarentena con juez en el kernel (capa ASI06): «ASI06 (D-9/H5): un resumen NO puede citar texto de nodos en» (línea 360) + «cuarentena. El juez de la capa 2 marca `quarantined = 1` después de la» (línea 361) | `crates/tylluan-kernel/src/memory/agent_memory.rs` |
| F11 | Hay store de planes de aprobación con persistencia y caducidad (`GrantRequest`, líneas 32; D-30 persiste planes pendientes con TTL) | `crates/tylluan-kernel/src/security/grants.rs` |

## 2. Decisión propuesta (conjunto de 6 piezas; cada una es aprobable aparte)

**P-1 Paso directo con argumentos tipados.** Cambiar `do_external_call` (F5) a forma `tylluan_do(server, tool, args_json)`: el agente puede dar args JSON explícitos y el kernel los **valida contra el `inputSchema`** que ya recibe en `tools/list` (F3) y cachea por (servidor, tool). Si el agente solo da intent natural: rellenar campos *nombre por nombre* desde el schema (no escopeta de 8 campos); si ningún campo es deducible, **aviso explícito** pidiendo args, nunca relleno silencioso. El `timeout_secs` del paso 30 pasa a derivar de `timeout_ms` (F2) o config por servidor; no se baja de los defaults actuales.

**P-2 Importación de configuración de clientes.** Completar el import existente (F8) para poblar `[[external_mcp]]` desde configs `mcpServers` de clientes (Claude/Cursor/etc.), conservando la protección anti-autorreferencia (F8), marcando lo importado como `active=false` hasta aprobación manual. [I] El export ya funciona (F8); el import queda a medias y debe respetar la lista de permitidos del ADR-022 (punto 8: «nunca un marketplace propio; todo lo que devuelven es dato no confiable», `ADR022_extension_trust_sandbox.md`, línea 25).

**P-3 Bóveda OAuth/secreta para externos.** Hoy `headers` va en claro en la config (F2). Propuesta: referencia a secreto, no valor — `token_ref` a un almacén cifrado local (patrón H-16/H-18: file `0600`/DPAPI, nunca valor en `tylluan.toml`), con los endpoints OAuth ya existentes (`transport/http/oauth.rs`: consent store, secreto persistido) reutilizados solo en su dirección actual (Tylluan como servidor); para externos salientes NO hay bóveda hoy [I: verificado por ausencia — `grep` de bóveda para externos da 0 en el código leído].

**P-4 Permisos por agente.** Hoy el dispatch externo recibe `agent_id` pero no hay matriz (agente × servidor externo) en el camino (F4: el filtro es keyword-score). Propuesta: ACL declarativa en el estilo de `agents.toml`/`GrantRequest` (F11): cada agente declara qué servidores externos puede tocar; denegar por defecto fuera de lista. [I]

**P-5 Cuarentena y comprobación de envenenamiento de descripciones.** Las descripciones/`inputSchema` de un externo son **dato no confiable** (ADR-022 punto 8). Propuesta: al importar `tools/list`, las descripciones nuevas entran «en cuarentena» (no se usan para selección keyword-score ni se inyectan en textos del agente) hasta que el juez las apruebe — reaprovechando el patrón ASI06 ya existente (F10: juez capa 2 marca cuarentena, y el consolidado jamás cita nodos cuarentenados). Diff de descripción en re-scan = re-cuarentena automática. [I: análogo propuesto, no existe aún — grep de "tool poisoning" da 0 en el kernel]

**P-6 Búsqueda de herramientas.** El descubrimiento real hoy es el texto del intent (F9), y el keyword-score elige tool sin schema (F4). Propuesta: la puntuación consume el (descripción aprobada + schema) cacheados de P-1/P-5; con `tylluan_do(server, tool, args)` explícito el agente se salta la heurística. Un modo `discover` que liste servidores+tools **aprobados** (no cuarentenados) sigue dentro de las 6 soberanas — no añade tools MCP directas (respeta F7).

## 3. Alternativas descartadas

- **A-1 Status quo (escopeta de 8 campos).** Fallo activo documentado (F5 + I-01). No.
- **A-2 Exponer tools externas como MCP tools directas.** Exige `expose_guild_tools=true` (F7), que publica TAMBIÉN todo lo demás; viola CONTRACT-01 (6 exactas, línea 48 en `tools.rs`). No.
- **A-3 Gateway externo de proceso aparte (Agentgateway/Supergateway, patrones de H-22).** Proceso y puerto extra: choca con puerto único y soberanía MIT. No (se puede usar externamente por quien quiera, pero no forma parte de Tylluan).

## 4. Riesgos

- Validar args contra schema ajeno puede fallar con schemas defectuosos o inexistentes: el fallback con aviso (P-1) evita el silencio, pero un externo sin schema útil queda en modo "el agente pasa JSON crudo" — más fricción.
- La cuarentena de descripciones (P-5) puede dejar inutilizable un externo recién importado hasta la aprobación: fricción por seguridad, aceptable si el aviso lo dice.
- P-3 toca secretos: si la bóveda se implementa mal, es peor que headers en claro (ya lo están); exigir revisión y pruebas antes de migrar configs existentes. Sin medición de overhead de agregación en ninguna fuente (H-22): NO ENCONTRADO.

## 5. No verificado

- Términos de la guía de seguridad MCP ("confused deputy", "tool poisoning", "token passthrough"): la guía oficial no se pudo descargar en esta sesión (6 rutas probadas, 404); se citan como nombre de riesgo, sin cita literal. Ampliación posible de H-22.
- Comportamiento en vivo de `external_mcp` (ejecutado): no ejecuté nada; F1-F11 son lectura de código en `bec191b`.
- `inputSchema` reales de los externos que José quiere instalar (Whisper en `E:\MCP-SERVERS`, etc.): medidos cero.
- Existencia de bóveda/ACL para externos: verificada por ausencia en el código leído (greps 0), no por diseño documentado.

## 6. Seis decisiones para José

1. ¿Se adopta **P-1** (paso directo `tylluan_do(server, tool, args)` + validación por `inputSchema`) como dirección del ADR-023?
2. ¿**P-2** completo: importar `mcpServers` de clientes con `active=false` hasta aprobación manual?
3. ¿**P-3**: bóveda local de secretos para externos y prohibir `headers` en claro en configs nuevas?
4. ¿**P-4**: ACL agente × servidor externo con deniega-por-defecto?
5. ¿**P-5**: cuarentena de descripciones de externos con el patrón de juez ASI06?
6. ¿**P-6**: `discover` de servidores+tools aprobados dentro de las 6 soberanas, y prefijo interno `servidor_tool` anti-colisión (arregla F6 de paso)?

---

## Impact

- Solo lectura + este borrador nuevo en `docs/internal/`. Cero código tocado, cero ejecuciones, sin commitear. Formato "línea N" según spec de cola. El borrador previo en `docs/reference/adr/ADR023_mcp_concentrador_hermes.md` queda sustituido por este fichero (nombre vinculante de la COLA).
