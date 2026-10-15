# DISEÑO — Frontera de autenticación/operador (OAuth) — v2

> **Asignación:** TL, turnos 1189 y 1193 (aprobada por José). **SOLO DISEÑO — cero código**. Prerequisito de `dev_mode=false` con LAN y del paso 3 de ADR-019.
> **v2 (2026-10-15):** correcciones del turno 1193 aplicadas: (1) **rediseño de la Fase 1** — la v1 no cerraba la cadena RCE verificada (§2-T1); (2) adoptados de la revisión de Antigravity: DELETE `silva/nodes`, `silva/edge` y `consolidate` → clase Operador, `AuthKind` tipado, RFC 8252 explícito, tests R13-R15; (3) tests de la cadena RCE; (4) CORS en LAN con solución operativa sin `Any`.
> **v2.1 (2026-10-15):** nota CONTRACT-01 / sexta tool (§8, turno 1197) — este diseño **no asume** el invariante de 5 tools.
> **Autor:** Deep. **Worktree:** `E:\tylluan-wt-deep` (fichero NUEVO; el diff del paso 2 permanece congelado). **Nada en main.**
> **Base de verificación:** `origin/main` = `682d428`; cada afirmación con `fichero:línea` releído hoy. Sin leer/imprimir `.tylluan-token`; sin tocar `tylluan.toml` ni producción.
> **Nota de método (recuento de rutas):** mi cifra cuenta RUTAS de la tabla `api_v1/routes.rs` con ≥1 verbo mutante (104) + registros directos en `build_router()`/`sse_routes()` (oauth token/revoke, messages/mcp ×3, a2a, sse ≈ 7) ≈ **111**; la del TL (119) cuenta declaraciones `.post/.put/.delete/.patch` por línea. **Ambas son aproximadas**; el diseño no depende del número exacto.

---

## 0. Resumen ejecutivo

**La v1 quedaba corta: rechazar el JWT OAuth en la clase Operador NO cierra el riesgo.** Cadena verificada hoy, eslabón a eslabón:

- LAN → `GET /oauth/authorize` (público, auto-aprueba SIEMPRE; el comentario dice "in dev_mode" pero el código no lo comprueba: `oauth.rs:3-4` vs `:151-201`) → JWT (aceptado como bearer, `auth.rs:273-278`) → `POST /messages` (protegido solo por bearer genérico, `mod.rs L1176-1178`) → `tylluan_do` → **guild `bash`**.
- En ese camino, el gate ACL del dispatch **es condicional**: `if !acl.roles.is_empty() || !acl.tokens.is_empty()` (`handler_do/mod.rs:745`) — y `AclConfig::default()` tiene `roles` y `tokens` **vacíos** (`config.rs L1595-1605`) ⇒ **con la config por defecto el gate ni se consulta**. Con roles configurados, `default_role=admin` (`config.rs L1829`) pasa además por el atajo admin de `acl_can_access` (`auth.rs:93-94`).
- `bash.py` permite `python/python3/node/npx/cargo/git/…` (`guilds/builders/plugins/bash.py:76-98`) ⇒ **ejecución arbitraria de código con los privilegios del usuario del kernel, desde la LAN**. El filtro de intención (`check_dangerous_intent`, usado en `handler_do/mod.rs:199`; default true `config.rs L1285`) solo bloquea patrones listados — no detiene `python -c …`.

**Fase 1 rediseñada (adoptando la propuesta del TL, evaluada y afinada):** `/oauth/authorize` auto-aprueba **solo con `dev_mode=true`**; con `dev_mode=false` responde **403 hasta que exista consentimiento de operador (Fase 2)**. Cierra la cadena de raíz (sin dev_mode no hay JWT que obtener) sin tocar nada del modo local. Consecuencia documentada: `mcp-remote`/Claude Desktop no podrá conectar en modo LAN hasta la Fase 2; los clientes con header Bearer siguen funcionando.

## 1. Estado verificado hoy (hechos, con fichero:línea)

| Pieza | Realidad verificada |
|---|---|
| Rutas OAuth | Públicas (fuera del middleware): `.well-known/oauth-authorization-server`, `/oauth/authorize` (GET), `/oauth/token`, `/oauth/revoke` (POST) (`mod.rs L1155-1168`). |
| `authorize_handler` | Auto-aprueba incondicionalmente; código TTL 60s (`oauth.rs:151-201`). `client_id` no se parsea/valida (`:104-111`); `redirect_uri` sin allowlist, usado para el 302 (`:190-197`). Solo exige PKCE S256 (`:156-162`). Divergencia comentario/código (`:3-4` vs `:151`). |
| `token_handler` | Código+PKCE (`:216-255`), `redirect_uri` debe coincidir (`:237-243`). JWT HS256 `sub="mcp"` sin scopes; TTL 3600s; secreto HMAC por arranque (`:44`, `:55-70`, `:257-259`). Sin refresh tokens. |
| `revoke_handler` | Sin auth (estándar); revoca por `jti` del token presentado (`oauth.rs:271-285`). |
| Middleware | Autoriza si bypass público || `dev_mode` || token maestro (header o query) || JWT OAuth (`auth.rs:259-303`); query-token con `warn` de fuga (`:382-425`). Sin token y `dev_mode=false` → deniega todo (`:299-302`). Rate-limit por IP (`:241-257`) + por `agent_id` autodeclarado (`:316-359`). |
| Token maestro | `ensure_auth_token()`: dev_mode→None; env `TYLLUAN_TOKEN` → `.tylluan-token` → fallback en el mismo fn (`config.rs L1983-2005`; `main.rs:560-568` fatal sin token y `dev_mode=false`). |
| Rol | `acl.tokens` → rol; si no, `default_role` (**admin**, `config.rs L1829`); override por contrato de agente (`auth.rs:141-169`). Un JWT OAuth nunca está en `acl.tokens` → cae siempre a `default_role`. |
| **Gate de guilds en `tylluan_do`** | **Condicional**: solo actúa si `acl.roles` o `acl.tokens` NO están vacíos (`handler_do/mod.rs:742-767`; condición en `:745`). Default = vacíos (`config.rs L1595-1605`) ⇒ sin gate. Con roles, `admin` pasa por `auth.rs:93-94`. |
| Enforcement de rol | Guild-access en 3 sitios (`api_guilds.rs:56-61`, `handler_do/mod.rs:746-751`, `a2a.rs:56-62`). **Cero checks de rol en endpoints de gestión.** |
| Ejecución vía bash | Allowlist de binarios (git/cargo/python/node/npm/npx/…) `guilds/builders/plugins/bash.py:76-98`; sandbox off por defecto (`config.rs L1563`). |
| Guardas LAN | `dev_mode=true` + host no-localhost: fuerza/deniega (`config.rs L1924-1935`, `main.rs:407-415`); `main.rs:390-406` exige token o override. |
| CORS | Allowlist solo origins localhost (`mod.rs L1128-1152`). **Corrección a mi v1:** un navegador que abre el dashboard en `http://<ip-LAN>:47004/` hace peticiones **same-origin** (el SPA se sirve del propio kernel, `mod.rs L1193-1206`) y **no pasa por CORS**; la allowlist solo afecta a usos CROSS-origin (Vite dev 5173, o servir el dashboard desde otro origen). Solución operativa en §3.4. |

## 2. Modelo de amenaza (v2)

Actores: operador humano · agente local legítimo · **proceso/dispositivo en la LAN** (nuevo con LAN) · cliente MCP legítimo · par federado aprobado.

| # | Amenaza | Cadena/evidencia | Sev. |
|---|---|---|---|
| **T1** | **LAN → JWT autoregalado → ejecucción de herramientas (RCE)** | §0 completa: authorize auto (`oauth.rs:151-201`) → JWT bearer (`auth.rs:273-278`) → `/messages` (`mod.rs L1176`) → `tylluan_do` → gate condicional/vacío (`handler_do/mod.rs:745`, `config.rs L1599-1600`) o admin-by-default → `bash` con python/node (`bash.py:76-98`), sandbox off (`config.rs L1563`) | **CRÍTICA** |
| T2 | `redirect_uri` arbitraria: código hacia URL atacante (PKCE no protege: el atacante controla su propio challenge) | `oauth.rs:104-111`, `:174-197` | **ALTA** (mitigada en Fase 1 al no emitir sin dev_mode) |
| T3 | Cualquier bearer (incluido JWT) hereda `default_role=admin`; gestión sin checks de rol | `auth.rs:104-106`, `config.rs L1829`; enforcement solo guilds (§1) | **ALTA** |
| T4 | `agent_id` autodeclarado evade el rate-limit por agente | `auth.rs:316-359` (mitigado por el de IP `:241-257`) | Media |
| T5 | Token por query-string en logs/historial | `auth.rs:382-425` (+`warn`) | Media |
| T6 | Divergencia comentario/código en oauth (falsa sensación de que dev_mode gatea) | `oauth.rs:3-4` vs `:151` | Media |
| T7 | `revoke` sin auth (estándar; solo revoca jti del token presentado) | `oauth.rs:271-285` | Baja |
| T8 | CORS: el "arreglo fácil" de LAN sería `Any` — abriría el API a orígenes maliciosos en el navegador del operador | `mod.rs L1128-1152` | Media (evitable, §3.4) |

## 3. Opciones y recomendación (v2, rediseñada)

### Fase 1 — NÚCLEO REDISEÑADO (cierra T1-T3 antes de LAN; 100% kernel, pequeña)
Adopto la propuesta del TL (a)-(d), evaluada:
- **(a) `/oauth/authorize` auto-aprueba SOLO con `dev_mode=true`; con `dev_mode=false` → 403** con error claro ("operator consent not implemented in this mode; see DESIGN_oauth_operator"). Es el cierre REAL de T1: sin dev_mode no existe forma de obtener un JWT vía el flujo. Sin efecto en el modo local (R12).
- **(b) Clase Operador rechaza JWT OAuth** — defensa en profundidad (cinturón si algún día dev_mode=false volviera a permitir emisión antes de tiempo). Con **`AuthKind` tipado** (adoptado de Antigravity): el middleware etiqueta cómo se autenticó (`MasterToken | OAuthJwt | DevBypass | Internal`), y la clase Operador exige `MasterToken|Internal` explícitamente.
- **(c) `redirect_uri` restringida a loopback** (`127.0.0.1`/`localhost`, cualquier puerto estilo RFC 8252) — defensa en profundidad para T2 en dev_mode.
- **(d) Comentario de `oauth.rs:3-4` corregido** a la semántica real (dev_mode gatea la emisión; consentimiento en Fase 2).
- **Clases (§4 inventario):** Operador = admin/kill, mantenimiento destructivo, mcp/external, config/*, guilds lifecycle, dispatch, a2a CRUD, federation peers/permissions/sharing, bash directo, autoresearch, **`silva/nodes/{id}` DELETE, `silva/edge` POST, `silva/consolidate` POST** (adoptado de la revisión de Antigravity). Agente = memoria/coloquio/contratos/nodos/sesiones. Par = sync/receive/export (con su propio bearer de par).
- **Consecuencia aceptada y documentada:** con `dev_mode=false`, **Claude Desktop vía `mcp-remote` no conecta** (su flujo OAuth recibirá 403 en authorize) hasta la Fase 2. Clientes con header Bearer (Claude Code, OpenClaw, Hermes, VS Code, Cursor) siguen. Esto NO rompe dev_mode=true sobre 127.0.0.1 (R12).

### Fase 2 — consentimiento real (con el paso 3 o inmediatamente después)
- Página de consentimiento servida por el kernel cuando `dev_mode=false`, que exige la **credencial de operador** (token maestro introducido por el humano en el navegador). OAuth deja de poder auto-aprobar en LAN.
- **Registro de clientes/redirect_uri** con **RFC 8252 explícito** (loopback con puerto efímero permitido para clientes nativos; lista estática para el resto).
- **Scopes mínimos** en el JWT (`scope: "mcp"` vs `"operator"`; hoy `sub="mcp"` sin scopes, `oauth.rs:58-63`). El rol de un JWT se deriva de su scope — nunca del `default_role` global.
- Reapertura de `mcp-remote` en LAN una vez exista consentimiento.

### Opción descartada
Gateway/proxy externo con auth propia — contradice CONTRACT-01 #4 ("sin proxy") y no arregla nada dentro del kernel.

### 3.4 CORS en LAN — solución operativa sin `Any`
1. **Regla:** el dashboard se usa en el MISMO origen del kernel (`http://<ip-LAN>:47004/`) — same-origin, CORS no interviene (corregido el diagnóstico de mi v1).
2. Si un operador necesita un origen extra (p. ej. otro hostname del mismo equipo o un dev-server en otra máquina), se añade **explícitamente** a la allowlist mediante configuración (propuesta de diseño: `[nexus] extra_cors_origins = [...]`, solo papel) — nunca comodín.
3. Prohibido `Any`/reflejar `Origin`: la allowlist es una defensa del navegador del operador (T8).
4. Documentar que IPs LAN cambian (DHCP) ⇒ la vía soportada es el punto 1, no ampliar allowlist con IPs fijas.

## 4. Inventario de endpoints que mutan estado (cifra ~aprox. §nota de método) y clase propuesta

**Rol exigido hoy: ninguno específico** (bearer genérico del middleware) salvo guild-access en los 3 sitios citados. Tabla por clúster con la clase propuesta (v2: cambios adoptados marcados ✱):

| Clúster | Ejemplos | Clase |
|---|---|---|
| Admin/kill (5) | `admin/reload`, `meta-prune`, `shutdown`, `emergency-kill`, `kill-guild/{name}` | **Operador** |
| Config (9) | `config/device`, `config/inference-llama`, `config/sandbox-profile*` (4), `config/wsl`, `setup-hint/apply` | **Operador** |
| Mantenimiento (9) | `purge`, `purge-lessons`, `clean-orphans`, `onnx-clean`, `logs-compact`, `vacuum`, `checkpoint`, `decay`, `export` | **Operador** |
| Federación (9) | `peers/{name}` DELETE, `approve`, `permissions` PUT, `sharing/*` | **Operador** (sync = **Par**, bypass propio) |
| MCP externo (2) | `mcp/external/discover`, `mcp/external/{name}` DELETE | **Operador** |
| Ejecución directa (2) | `bash`, `guilds/{name}/tools|call/{tool}` | **Operador** (directo) / *Agente vía ACL de guild* |
| Guilds lifecycle (7) | `register`, `request`, `start|stop|reset-backoff|test`, `guild-start`, `doctor/repair` | **Operador** |
| Dispatch (4) | `dispatch/execute`, `dispatch/remote`, `dispatches/{id}/approve|reject` | **Operador** |
| Memoria/Silva (10) | `memory/write|reindex`, `ingest*`, `silva/node` POST, **✱`silva/nodes/{id}` DELETE, ✱`silva/edge` POST, ✱`silva/consolidate`** | **Agente** (los ✱ → **Operador**) |
| Coloquio (6) | `channels/{id}/post|read|typing`, `documents/{id}/append`, `repair-msgids` | **Agente** |
| Agentes/sesiones/nodos/journal (10) | `agents/session/*`, `heartbeat`, `nodes/*`, `journal/checkin`, `agent-memories/{id}` DELETE, `sessions/{id}` DELETE | **Agente** |
| Contratos/BWC (7) + mailbox/canvas/blackboard/stigmergy | … | **Agente** |
| Eval/embed/mesh/gossip (5) | `eval/run`, `embed`, `mesh/refresh`, `gossip` | **Operador/Agente** (a decidir) |
| A2A (3), Autoresearch (3) | CRUD + test/start/stop/evaluate | **Operador** |

## 5. Plan de tests de rechazo (v2: +R13-R15 cadena RCE)

- R1-R2: JWT OAuth → 403 en TODA la clase Operador (admin, purge, mcp/external, config, bash directo, federation approve/permissions/sharing, **✱silva/nodes DELETE, silva/edge, consolidate**, a2a CRUD, autoresearch).
- R3: token maestro → 200 en los mismos (control positivo).
- R4: bearer de par → 401 en `federation/peers` POST (un par no se auto-aprueba).
- R5: rol `reader` → guild no permitida denegada / permitida OK (regresión del gate de guilds + clase Operador).
- R6: authorize con method `plain` o ausente → 400.
- R7: token con code expirado / reusado / `redirect_uri` distinta → `invalid_grant`.
- R8: JWT expirado / revocado / firmado con otro secreto → 401.
- R9: `dev_mode=false` sin token configurado → todo protegido 401 (fail-closed).
- R10 (F2): authorize sin consentimiento → 403; `redirect_uri` fuera del registro → 400.
- R11 (F2): JWT `scope=mcp` → 403 en Operador aunque el rol fuese admin (el scope manda).
- R12: **NO regresión** — `dev_mode=true` + `127.0.0.1`: mcp-remote completo + query-token SSE intactos.
- **R13 (cadena RCE, núcleo F1):** con `dev_mode=false`, `GET /oauth/authorize` → **403** (no se puede obtener JWT por el flujo).
- **R14 (cadena RCE, extremo):** sin credencial válida (ni maestro ni JWT), `POST /messages` (tylluan_do→bash) → **401**; con master token → 200 (control).
- **R15 (F1-c):** `redirect_uri` fuera de loopback → **400** (aunque dev_mode=true).
- ✱ Extra adoptado: test de `AuthKind` — la clase Operador exige `MasterToken|Internal` explícito; JWT/DevBypass no cualifican (unit de la tabla de clases).

## 6. Compatibilidad MCP (v2 — impacto de la Fase 1 revisada)

| Cliente | Auth hoy | Depende del auto-approve | Fase 1 (dev local) | Fase 1 (LAN, dev_mode=false) | Fase 2 |
|---|---|---|---|---|---|
| Claude Desktop vía `mcp-remote` (`CLIENT_INTEGRATION_GUIDE.md:43+`) | Flujo OAuth (auto) | **Sí** | Sin cambio | **NO conecta hasta F2 (consecuencia aceptada y documentada)** | Conecta con consentimiento |
| Claude Code / OpenClaw / Hermes / VS Code / Cursor | Header Bearer (docs citadas en v1 §4) | No | Sin cambio | Funciona | Sin cambio |
| Antigravity / Qwen / LM Studio | sin headers en sus JSON | No consta | Sin cambio | Según soporte de headers (**NO VERIFICADO**) | Igual |
| Todos con `dev_mode=true` + `127.0.0.1` | Bypass | Sí | **Sin ningún cambio (R12)** | n/a | **Sin cambio (R12)** |

## 7. NO VERIFICADO (explícito)

- Que `mcp-remote` maneje bien un 403 en authorize en LAN (mensaje de error al usuario) — F2 lo validará con cliente real.
- Soporte de `headers` Bearer en Antigravity/Qwen/LM Studio (sus JSON no los usan).
- Semántica del store de sesiones (`sessions/{id}` DELETE) — inventariado, no diseccionado.
- Fallback final de `ensure_auth_token()` tras env/fichero (leído parcialmente, `config.rs L1983-2005`).
- Flujo actual del dashboard para obtener el token que pegará el operador en la UI (revisar al implementar F1/F2).
- DCR (RFC 7591): no hay `registration_endpoint` en el metadata (`oauth.rs:136-148`); no probé un cliente que lo requiera.

---

**Decisión pedida al TL:** ¿la consecuencia de Fase 1 (mcp-remote sin LAN hasta F2) se acepta tal cual, o se quiere adelantar F2 (consentimiento) al mismo ciclo para no dejar a Claude Desktop fuera de LAN? Mi recomendación: aceptar (los clientes con header cubren LAN; F2 va con el paso 3).

## 8. Nota CONTRACT-01 — sexta tool (turno 1197, decisión de José)

Este diseño **no asume** el invariante "exactamente 5 tools". José decidió (2026-10-15, vía TL) crear una **sexta tool real**; el contrato y la enmienda formal de CONTRACT-01 los redacta el TL en ADR-021. Impacto sobre ESTE diseño:

1. La sexta tool será una llamada MCP normal por `/messages`, autenticada como cualquier otra (bearer de agente) — **clase agente**, nunca Operador.
2. Por mandato explícito no tendrá autoridad para ejecutar ni conceder permisos ⇒ no añade ninguna superficie a `operator_class_required` (Fase 1/§3) ni cambia la tabla del §4.
3. Los tests R1-R15 no cambian; R12 (no-regresión del modo local) sigue siendo el contrato.
4. **Nada de código de la tool** hasta que el ADR-021 esté aprobado.
