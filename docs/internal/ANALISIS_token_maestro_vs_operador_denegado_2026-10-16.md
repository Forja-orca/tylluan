# ANÁLISIS — Token maestro vs «operador denegado» (@buffy, 2026-10-16)

Pregunta abierta 1 del INFORME del turno 1220: si «operador denegado» (turno 1219)
debe significar 403 también para el **token maestro**. Base: código releído hoy en
`crates/tylluan-kernel/src/transport/http/auth.rs` y `transport/http/mod.rs`
(worktree `E:\tylluan-wt-buffy` sobre `origin/main` = `8c9287b`).

## Veredicto

**No, y añadir ese 403 sería incorrecto.** «Operador denegado» no puede aplicarse
al token maestro: el gate de clase Operador (ADR-019/OAuth Fase 1(b)) decide por
**clase de autenticación**, no por identidad ni rol, y el token maestro ES la
clase de operador (`AclAuthKind::MasterToken`, `auth.rs:58-68`;
`operator_gate_allows` solo acepta `MasterToken | DevBypass`, `auth.rs:318-324`).
Denegarlo en rutas operador dejaría al operador legítimo sin acceso a
`/api/v1/bash`, `/api/v1/admin/*` ni `guild-start`: no es una función de bloqueo,
es un cierre de puerta con el operador dentro.

## Matriz verificada hoy (cada celda con su código)

La capa del gate corre POR DENTRO del bearer (el último `.layer()` es el más
externo: bearer instala `ACL_AUTH_KIND` y luego el gate lo consulta —
`mod.rs:1176-1185`, comentario incluido).

| Credencial presentada | Ruta clase Operador | Ruta clase Agente (p. ej. `POST /api/v1/decide`) |
|---|---|---|
| Master token (header `Authorization: Bearer`) | **pasa** (`auth.rs:318`) | pasa |
| Master token (query `?token=`) | **pasa** — ver exposición E1 | pasa |
| JWT OAuth | **403 `operator_required`** (`auth.rs:335-348`) | pasa (`auth.rs:273-278`) |
| Bypass de par/pública (solo sus 4 rutas designadas) | 403 si coincidiera con ruta operador (no coincide: son sync/receive, sync/export, ping, mcp probe — `auth.rs:214-246`) | pasa solo en esas 4 rutas |
| `dev_mode=true` (cualquier red) | **pasa** (`DevBypass`, `auth.rs:265-267`) — ver exposición E2 | pasa |
| Sin token + `dev_mode=false` | 401 `unauthorized` (`auth.rs:299-305`) | 401 |
| Sin `ACL_AUTH_KIND` instalado (fail-closed) | 403 (`auth.rs:318-324`, `None` → denegado) | n/a — el bearer siempre instala el kind en rutas protegidas |

Estados que SÍ existen y su semántica exacta:

1. **401 `unauthorized`** — no hay credencial válida (falta o no coincide).
   `auth.rs:299-305`. El token maestro solo recibe 401 si está mal presentado:
   deja de ser «el token maestro».
2. **403 `operator_required`** — credencial VÁLIDA pero de clase insuficiente
   (JWT OAuth o bypass de par en ruta operador). Cuerpo canónico en
   `auth.rs:338-345`. Ausencia de kind = fail-closed 403.
3. **429 `rate_limit`** — el limitador por IP corre ANTES de la autenticación
   (`auth.rs:236-257`) y también aplica al token maestro (300 req/min por
   defecto). Un operador puede verse «denegado» por volumen, no por clase.

Por qué el diseño es así (design doc `docs/internal/DESIGN_oauth_operator_2026-10.md`
§0/§2): el JWT OAuth es obtenible por cualquier proceso local en dev_mode (cadena
RCE verificada LAN→authorize→JWT→/messages→tylluan_do→bash), así que la defensa
es negar la CLASE operador a todo lo que no sea el maestro. Contra el maestro no
hay nada que defender: es la credencial cuya posesión define operador.

## Consecuencia para mi `v4` (batería adversarial)

`v4_decide_is_agent_class_not_operator` se queda como está: fija
`!operator_class_required("POST", "/api/v1/decide")` (verificado: la ruta está en
`AGENT_OR_PEER_CLASS` — `auth.rs:924-927` — y fuera de la tabla de operador). Un
JWT OAuth puede llamar a `tylluan_decide` por diseño (riesgo bajo, sin efectos
más allá de la fila de auditoría). **No** añado el assert de 403 para el maestro:
sería falso contra el diseño aprobado. Si el turno 1219 quería denegar al
operador en un caso concreto, que lo concrete — sería mecanismo NUEVO
(revocación/lockout), no este gate.

## Exposiciones reales detectadas (decisión del TL, no las cambio yo)

- **E1 [media]: el token maestro por query-string satisface el gate operador.**
  `has_valid_query` (`auth.rs:284-297`) produce `MasterToken`, igual que el
  header. Resultado: acciones de operador vía URL (`/api/v1/bash?token=…`)
  quedan en logs de proxy, historial de navegador y Referer. Ya hay `warn!` de
  fuga en `extract_token` y `sanitize_query` roja nuestros logs, pero el gate
  no distingue el medio. Fix natural: en `operator_gate_middleware`, exigir
  `AclAuthKind::MasterToken` proveniente del header (el kind no guarda hoy el
  medio — habría que propagarlo). Afecta a clientes SSE/dashboard que usan
  query-token en rutas NO operador: ninguna.
- **E2 [documentada]: `dev_mode=true` satisface el gate** (`DevBypass` pasa
  siempre). La única defensa contra LAN RCE sigue siendo la regla AGENTS.md
  «NUNCA `host=0.0.0.0` + `dev_mode=true`»; el gate no la refuerza. Correcto
  según diseño (el bypass es explícito del operador), pero conviene recordarlo:
  en dev_mode NO existe «operador denegado» para nadie con acceso de red.
- **E3 [informativa]: el gate ignora el rol.** Mapear el token maestro a un rol
  restringido en `acl.tokens` restringe el dispatch de guilds
  (`acl_can_access`, `auth.rs:115-123`) pero **no** las rutas operador (el gate
  solo mira el kind). «Operador mínimo» por rol no existe hoy; sería diseño
  nuevo.

## Límites de este análisis

Código a `8c9287b`; no ejecuté el kernel real con red (los tests de fase1 en
`auth.rs:904-935` cubren la tabla y el gate con funciones puras, y la batería
REST del turno 1220 cubre la clase agente de `/api/v1/decide`). No leí
`.tylluan-token` ni toqué `tylluan.toml`.
