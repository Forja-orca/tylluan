# IMPL — ADR-019/OAuth Fase 1: frontera de autenticación (implementación)

> **Tarea:** TL (diseño aprobado; cola de Deep: "la v2 del diseño de OAuth y después la Fase 1"). Implementación de la **Fase 1** del diseño `DESIGN_oauth_operator_2026-10.md` (v2.1). **Diff SIN commitear** — commitea el TL.
> **Rama:** `agent/deep-oauth-fase1` desde `origin/main` = `cbd4f7c` (paso 2 de ADR-019 ya en main, `742eacf`). **Nada en producción; no se tocó `tylluan.toml`.**
> **Autor:** Deep. **Fecha:** 2026-10-15.

## 1. Qué implementa (mapeo exacto al diseño §3/Fase 1)

| Diseño | Implementación | Fichero:línea |
|---|---|---|
| **(a) authorize solo auto-aprueba con `dev_mode=true`; 403 con `dev_mode=false`** | Gate en `authorize_handler` + `OAuthState.dev_mode` (fail-closed por defecto; `with_dev_mode()` builder). Prod y harness optan explícitamente. | `oauth.rs` (struct `:24-34`, `new/with_dev_mode :56-84`, gate en `authorize_handler`) |
| **(b) clase Operador rechaza JWT, con `AuthKind` tipado** | `AclAuthKind { MasterToken, OAuthJwt, DevBypass, PublicBypass }` + task-local `ACL_AUTH_KIND` seteado por el middleware + `operator_class_required()` (tabla única) + `operator_gate_allows()` + capa `operator_gate_middleware` aplicada DENTRO del bearer. | `auth.rs` (`:19-75` enum/task-local, middleware `:300-345`, tabla/gate antes del bearer) + `mod.rs` (capa en `build_router`, orden: bearer fuera, gate dentro) |
| **(c) redirect_uri solo loopback (RFC 8252)** | `is_loopback_redirect_uri()` con parse estricto de host (`127.0.0.1.evil.com` NO pasa). | `oauth.rs` (helper + gate en `authorize_handler`) |
| **(d) comentario corregido** | Cabecera de `oauth.rs` reescrita (semántica real dev_mode/consentimiento). | `oauth.rs:1-13` |
| **Tabla Operador (inventario §4 + adopciones)** | `admin/*`, `maintenance/*`, `config/*`, `mcp/external*`, `autoresearch/*`, `a2a/agents*`, `dispatches/*`; exactos: `bash`, `doctor/repair`, `guild-start`, `setup-hint/apply`, `guilds/register|request|dispatch/execute|dispatch/remote`; lifecycle `guilds/{n}/start|stop|reset-backoff|test`; federación `peers` (mutaciones, no GET) + `sharing/*`; **✱`silva/nodes/{id}` DELETE, `silva/edge`, `silva/consolidate`** (adoptado de la revisión adversaria). | `auth.rs::operator_class_required` |
| No-regresión dev local | `dev_mode=true` → kind `DevBypass` → gate permite; authorize auto-aprueba en loopback. | R12 |

## 2. Evidencia de validación (real, ejecutada hoy)

| Comando | Resultado |
|---|---|
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | ✅ limpio (0 warnings/errores; `Finished` 26.82 s) |
| `cargo test -p tylluan-kernel --lib` | ✅ **1025 passed; 0 failed; 3 ignored** (1019 base + 6 nuevos Fase 1) |
| Nuevos unitarios (`--lib fase1`) | ✅ **6/6**: tabla operador (cobertura + exclusiones), `operator_gate_allows` fail-closed, R13 (403 sin dev_mode), R15 (loopback estricto), R12 (302 con code), helper de frontera |
| `cargo test -p tylluan-kernel --test oauth_operator_gate` (NUEVO, sobre `build_router` real) | ✅ **3/3**: R13 (403), R14 (JWT→403 / master→404 control / sin credencial→401), R15+R12 |
| `cargo test -p tylluan-kernel --test adversarial_http_battery` | ✅ 1/1 (la capa nueva no rompe la batería WS7) |
| `scripts/check_test_count.sh` | ✅ **1111** = 1025 kernel + 74 link + 12 fsrs (README/AGENTS vía `--fix`; STATUS/CLAUDE a mano — 8 líneas mecánicas) |

## 3. Desviaciones del diseño (conscientes)

1. **`OAuthState::new()` conserva firma con `dev_mode=false` por defecto (fail-closed)**; prod (`mod.rs:518`) y el harness compartido (`tests/support/mod.rs`) optan con `.with_dev_mode(...)`. Los demás constructores en tests (9 binarios) quedan con `false` — verificado por grep que **ninguno ejercita `/oauth/authorize`**, así que no cambia su comportamiento observado.
2. **Tabla de clases como función pura path-based** (el "1 guard + tabla" de mi Opción B), no checks por-handler: un solo punto de verdad, unit-testeado. Quedan FUERA deliberadamente (el diseño los marcó "a decidir"): `eval/run`, `embed`, `mesh/refresh`, `gossip`.
3. **`tests/support/mod.rs` extendido** con `test_state_custom(tag, auth_token, dev_mode)` (aditivo; `test_state` intacto para el resto de binarios).
4. **Fase 2 (consentimiento) NO implementada** — tal cual el diseño: con `dev_mode=false`, `mcp-remote`/Claude Desktop no conecta en LAN hasta F2 (consecuencia aceptada y documentada; clientes con header Bearer siguen).
5. Cifras de tests actualizadas otra vez (1105→1111) por el gate mecánico de recuento.

## 4. ## Impact (propuesto para el commit del TL — toca `transport/`)

```
## Impact
ADR-019/OAuth Fase 1 (docs/internal/DESIGN_oauth_operator_2026-10.md): /oauth/authorize deja de auto-aprobar fuera de dev_mode (403) — cierra la cadena LAN → JWT autoregalado → tylluan_do/bash verificada en el diseño (T1); redirect_uri restringido a loopback; los endpoints de gestión/ejecución (tabla operator_class_required: admin, mantenimiento, config, mcp/external, bash directo, lifecycle de guilds, dispatch approve/execute, gestión de federación, borrados de Silva) exigen el token MAESTRO — un JWT OAuth ya no los satisface (AuthKind tipado en el middleware bearer + capa operator_gate). dev_mode=true sobre 127.0.0.1 intacto (R12). Consecuencia documentada y aceptada: mcp-remote/Claude Desktop no conecta en modo LAN hasta la Fase 2 (consentimiento de operador); clientes con header Bearer sin cambio. Local: +6 tests lib y 3 de integración; kernel 1025+74+12=1111.
```

## 5. NO VERIFICADO

- Que `mcp-remote` real muestre correctamente el 403 de authorize (UX del cliente en LAN) — se validará en F2 con cliente real.
- Suites de integración que construyen `OAuthState::new` directo y no ejercitan authorize: no re-ejecutadas una a una (grep dice que no tocan el flujo; la suite completa `--tests` se corre en el cierre de esta entrega).
- El gate no cubre superficies futuras por sí solo: cualquier endpoint Operador NUEVO debe añadirse a `operator_class_required` (hay unit test de tabla, pero no un test estructural que lo fuerce contra `routes.rs` — candidato a mejora futura).
- Fase 2 completa (consentimiento, registro de clientes, scopes): no existe aún; fuera del alcance de esta entrega.

## 6. Revisión del TL (turno 1210) — atendida

**(1) Test de inventario estructural añadido** (`route_inventory_every_mutating_route_is_classified`, auth.rs, `include_str!("api_v1/routes.rs")`):
- Parsea la tabla de rutas línea a línea (0 `.route(` multilínea — verificado); **121 rutas mutantes** reales (mi extractor anterior daba 104: perdía los métodos encadenados de las líneas `get(a).post(b)/.delete(b)`, de ahí parte del delta con tus ~119).
- Cada ruta mutante debe estar clasificada como **Operador** (`operator_class_required`) o estar en la **allowlist Agente/Par/a-decidir** documentada en el propio test (60 entradas). Una ruta nueva sin clasificar **rompe CI**.
- **En su primera ejecución cazó un hueco real**: `POST /api/v1/config` (guardar config, sin barra final) no lo cubría `starts_with("/api/v1/config/")` → corregido (`path == "/api/v1/config" || starts_with("/api/v1/config/")`). Es exactamente la clase de fallo que pediste que vigile.
- **Clasificaciones nuevas forzadas por el inventario** (todas con test unitario de tabla):
  - `/api/v1/federation/sync`, `/sync/pull`, `/sync/both` → **Operador** (corrección del diseño: solo `sync/receive` es "Par" con bypass propio; push/pull/both son actos de operador sobre NUESTRO estado — el diseño v2 los etiquetó juntos, el inventario forzó la precisión).
  - `/api/v1/inference/providers` (añadir proveedor) → **Operador**; `/api/v1/security/scopes` → **Operador**; `/api/v1/skills` (guardar skills de proyecto) → **Operador**.
  - Restos "a decidir" del diseño (`eval/run`, `embed`, `mesh/refresh`, `gossip`, tests de providers) quedan **explícitamente** en la allowlist como Agente con comentario.
- **Alcance del test**: `api_v1/routes.rs` (donde lo pediste). Las 4 rutas OAuth de `http/mod.rs` (públicas por diseño de Fase 1/2) quedan fuera — anotado en §5.

**(2) Formato roto arreglado**: la firma de `bearer_auth_middleware` volvía a multilínea — la edición de inserción de la Fase 1 colapsó el salto tras `(`, dejando el primer parámetro pegado. Sin efecto funcional (compilaba y clippy pasaba), pero corregido.

**(3) `StaticAclAdmin` / `acl.tokens` — documentación pedida**: el enum `AclAuthKind` NO tiene variante `StaticAclAdmin` **porque hoy `acl.tokens` no autentica a nadie**: el middleware solo acepta (a) el token maestro (`secure_compare` contra `state.auth_token`) o (b) un JWT OAuth (`validate_bearer`); `acl.tokens` se consulta DESPUÉS de autenticar y solo mapea el string autenticado a un rol (`auth.rs:104-106`, `:141-169`). Consecuencia: el único bearer estático que hoy entra es el maestro (kind `MasterToken`); un token listado SOLO en `acl.tokens` recibe 401. Si quieres N tokens de operador estáticos, opciones: (i) extensión pequeña del middleware para aceptar valores de `acl.tokens` como `AclAuthKind::StaticOperator` (fuera del alcance de Fase 1; ~10 líneas + tests), o (ii) llevarlo a la Fase 2 (registro). Decisión tuya.

**Evidencia actualizada tras estos cambios**: clippy `-D warnings` limpio; lib **1026 passed + 3 ignored**; fase1 **7/7**; `oauth_operator_gate` **3/3**; batería adversarial **1/1**; `check_test_count` ✅ **1112** (1026+74+12).
