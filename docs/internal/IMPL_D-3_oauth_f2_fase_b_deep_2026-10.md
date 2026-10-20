# D-3 — OAuth F2 fase B: desafíos de scope + plan de consentimiento (Deep, 2026-10-19)

**Estado: no verificado** por terceros. Diff **sin commitear** en el worktree
`E:\tylluan-wt-deep`, base `17465b9` (HEAD de `origin/main`).
Parche: `E:\tylluan-patches\deep\D-3.patch` (11 archivos).
Plan de consentimiento: `docs/internal/PLAN_oauth_f2_consentimiento_deep_2026-10.md`.

## ⚠️ El parche D-3 es acumulado

`D-3.patch` **incluye D-2** en `oauth.rs`, `mod.rs` y las líneas de recuento de
tests (mismo punto de partida `17465b9`). Si el TL aplica los dos en orden,
aplique D-2 primero y D-3 con `git apply --3way`, o aplique **solo D-3** (ya
contiene D-2). Los dos informes son independientes.

## Qué se entregó (fase B: desafíos)

### 1. `scope` en el JWT: `mcp` (y solo `mcp`)

`sign_jwt` emite ahora `"scope": "mcp"`; constantes `SCOPE_MCP` /
`SCOPE_OPERATOR` en `oauth.rs`. Ningún token puede llevar `operator`: no existe
flujo que lo conceda (es exactamente la escalada que ADR-019 prohíbe, y el
consentimiento de F2-B — plan, no implementado — tampoco lo concederá).

### 2. 401 con desafío de descubrimiento

El bearer middleware añade a todo 401:

```
WWW-Authenticate: Bearer resource_metadata="<canónica>/.well-known/oauth-protected-resource", scope="mcp"
```

Cierra el bucle con D-2: el cliente recibe el 401, descubre el documento de
recurso protegido y el AS sin configuración previa.

### 3. 403 con `error="insufficient_scope"`

En ruta de clase operador, si quien llama trae un **JWT OAuth válido** (es
decir: autenticado, pero de clase `mcp`), la respuesta pasa de un 403 genérico a
MCP-correcta:

```
HTTP/1.1 403 Forbidden
WWW-Authenticate: Bearer error="insufficient_scope", scope="operator", resource_metadata="<canónica>/.well-known/oauth-protected-resource"
{"error":"insufficient_scope","scope":"operator","message":"…"}
```

Todos los scopes necesarios van en el **mismo** desafío (`scopes.join(" ")`), no
una respuesta por scope.

Qué **no** cambia, a propósito: un token maestro por query-string, o la ausencia
de credenciales, conservan el 403/401 anterior. A esos medios no se les arregla
nada pidiendo un scope (el problema es el medio, no el permiso), y devolver
`insufficient_scope` les diría al cliente que reintente un flujo que no puede
funcionar.

### 4. Cableado

`operator_gate_middleware` pasa de `middleware::from_fn` a
`from_fn_with_state` para poder construir el desafío con la URI canónica del
`OAuthState`. El orden de capas no cambia (el gate sigue dentro del bearer, que
instala `ACL_AUTH_KIND` primero).

## Verificación (ejecutada por mí, salidas reales)

```
cargo test -p tylluan-kernel --lib transport::http::oauth      → 16 passed
cargo test -p tylluan-kernel --lib transport::http::auth       → 29 passed
cargo test -p tylluan-kernel --test oauth_f2_scope_challenges  →  4 passed
cargo test -p tylluan-kernel --test oauth_operator_gate        →  6 passed
cargo test -p tylluan-kernel --test oauth_f2_adversarial_spec  →  1 passed; 7 ignored
cargo test -p tylluan-kernel --lib                             → 1082 passed; 0 failed; 4 ignored
cargo clippy -p tylluan-kernel --all-targets -- -D warnings    → EXIT 0
scripts/check_test_count.sh                                    → EXIT 0 (1169)
```

Tests nuevos:

| Test | Fichero | Qué fija |
|------|---------|----------|
| `signed_jwt_carries_mcp_scope_only` | `oauth.rs` | el JWT solo lleva `mcp` |
| `bearer_challenge_points_to_protected_resource_metadata` | `auth.rs` | forma exacta del desafío 401 (incluye recorte de barra final) |
| `insufficient_scope_challenge_names_error_and_scope` | `auth.rs` | `error` + `scope` + `resource_metadata` |
| `insufficient_scope_challenge_lists_every_scope_in_one_header` | `auth.rs` | varios scopes en **un** header, un solo `resource_metadata` |
| `unauthorized_challenge_points_to_resource_metadata` | `tests/oauth_f2_scope_challenges.rs` | 401 real sobre `build_router` |
| `oauth_jwt_gets_insufficient_scope_on_operator_route` | idem | 403 real con JWT + cuerpo `insufficient_scope` |
| `query_token_on_operator_route_has_no_scope_challenge` | idem | el token por query **no** recibe `insufficient_scope` |
| `master_token_still_passes_operator_gate` | idem | fricción cero del operador (404 del handler) |

Recuento: kernel 1078 → **1082** (4 tests lib nuevos), total **1169**. Cifras
actualizadas en `README.md`, `AGENTS.md`, `STATUS.md` (dos líneas) y los cuatro
sitios del dashboard. Los tests de integración no entran en el recuento.

Puertas: `check_docs_reality.sh` sigue con **un único ❌ preexistente**
(`docs/roadmap/ROADMAP_O3.md:42`, `onnx/int8/decima.json`, cola de Buffy B-1);
sin ❌ nuevos. `check_head_sync.sh` / `check_no_predation.sh` no ejecutables en
este worktree (ya documentado en D-2).

## Plan de consentimiento (entregado, NO implementado)

`docs/internal/PLAN_oauth_f2_consentimiento_deep_2026-10.md` cubre lo pedido:
pantallas (P1 consentimiento, P2 autorizado, P3 denegado, E1 credencial mala,
E2 solicitud inválida), máquina de estados (`PENDING → APPROVED → CONSUMED`,
`DENIED`, `EXPIRED`, `LOCKED`), qué se recuerda y dónde (`consents.json` por
`client_id` + `redirect_uri` completo + scopes + caducidad 30 d deslizante,
escritura atómica; **nunca** la credencial ni IPs), presupuesto de fricción
(2 acciones la primera vez, **0** las siguientes, 1 campo, 1 pantalla, 0 JS,
0 llamadas a LLM) y 8 puntos de seguridad del formulario (CSRF, tiempo
constante, escapado, anti-clickjacking, no conceder `operator`, auditoría).

Cinco decisiones quedan explícitamente para el TL (§7 del plan): almacenamiento,
plazo, confirmación de que el formulario nunca concede `operator`, revocación en
esta fase o no, y no recordar la credencial en el navegador.

## Nota de proceso (fallo propio, corregido)

Al actualizar los recuentos en los cuatro ficheros del dashboard usé PowerShell
con `Set-Content -Encoding UTF8` sin `-Raw` correcto y un `\b1078\b` global: eso
(a) corrompió caracteres UTF-8 en esos ficheros y (b) reescribió tres
coordenadas SVG `x1={1078}` de `architecture-map.tsx` como `1082`. Lo detecté al
verificar el resultado, revertí los cuatro ficheros con `git checkout --` y los
edité de nuevo con la herramienta de edición dirigida (solo las tres cadenas de
recuento). El estado actual de esos ficheros es correcto y lo comprobé por grep
(`1,169` en los cuatro, `(1082+75+12)` en `fsrs-model.tsx`, coordenadas SVG
intactas en `1078`). Lo dejo escrito porque el fallo era silencioso y habría
pasado por bueno sin la comprobación.

## Impact

Toca `transport/http/` (respuestas de autenticación y el gate de operador).

- **¿Depreda a otro agente?** No. Ramas desde `17465b9`; no toca ficheros de
  Buffy/Mimo. El único cruce es el recuento de tests, documentado arriba.
- **¿Depreda a un usuario?** No. El 401/403 solo **añade** un header; los
  clientes que no lo entienden lo ignoran (RFC 7235 permite extensiones). El
  token maestro sigue pasando el gate sin cambios (test explícito).
- **¿Rompe contratos?** El 403 de operador con JWT cambia de cuerpo
  (`operator_required` → `insufficient_scope`) y de código de error. Es el
  cambio que pide la spec; si algún consumidor discriminaba por
  `error == "operator_required"` con un JWT, vería otro valor. El 403 genérico
  se conserva para el resto de medios, así que la semántica de seguridad no
  cambia: nunca se concede algo que antes se negara.
- **¿Se puede verificar sin mí?** Sí: los siete comandos de arriba.
