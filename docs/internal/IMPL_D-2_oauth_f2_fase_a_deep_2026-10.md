# D-2 — OAuth F2 fase A: descubrimiento (RFC 9728), audiencia (RFC 8707) e `iss` (RFC 9207) (Deep, 2026-10-19)

**Estado: no verificado** por terceros. Diff **sin commitear** en el worktree
`E:\tylluan-wt-deep`, base `17465b9` (HEAD de `origin/main`).
Parche: `E:\tylluan-patches\deep\D-2.patch` (9 archivos).

## Qué se entregó

### 1. `GET /.well-known/oauth-protected-resource` (RFC 9728)

Endpoint nuevo (MCP lo exige con MUST y Tylluan no lo tenía):

```json
{ "resource": "http://<host>:<port>",
  "authorization_servers": ["http://<host>:<port>"],
  "bearer_methods_supported": ["header"] }
```

`resource`/`authorization_servers` salen de `OAuthState::canonical_resource()`
(la `base_url` con la que se construye el router, sin barra final).
`bearer_methods_supported: ["header"]` es real, no aspiracional: F1 ya rechaza
el JWT por query (caso C5 de la suite adversarial). `scopes_supported` se omite
a propósito — no hay scopes declarados todavía; llegan con la fase B (D-3).

Ruta registrada en `transport/http/mod.rs` junto a la de `oauth-authorization-server`.

### 2. `resource` (RFC 8707) + `aud` en el JWT

- `AuthorizeQuery` y `TokenForm` aceptan `resource` (opcional).
- El JWT que se emite lleva **`aud` = URI canónica del servidor** (antes el JWT
  no ligaba recurso alguno: un token de otro servicio podía presentarse aquí).
- `resource` se valida contra la canónica en **authorize** y en **token**:
  - se acepta la canónica con o sin barra final y con cualquier sufijo de
    path/query (`/mcp`, `?x=1`) — un cliente MCP real apunta al endpoint que usa;
  - se rechaza otro host, otro puerto u otro esquema con `400 invalid_target`
    (error definido por RFC 8707). Se rechaza en el authorize en vez de emitir
    un token que después rechazaríamos nosotros mismos al validarlo.
  - el sufijo se compara por carácter siguiente (`/`, `?`, `#` o fin): así
    `http://host:port.evil.com` y `http://host:port1` **no** pasan.
- En el canje, el `resource` del token request manda si viene; si no, vale el
  declarado en la autorización (guardado en `PendingCode`).
- **Validación de audiencia** (`validate_bearer`), con la política escrita en el
  código:
  - `aud` presente y distinto de la canónica → **rechazado siempre** (es de otro
    servicio; el modo no cambia eso);
  - `aud` ausente (token anterior a D-2, o cliente que no manda `resource`) →
    rechazado con `dev_mode=false` (no hay prueba de que fuera emitido para este
    servidor), aceptado con `dev_mode=true` para no romper el flujo local ya
    existente;
  - `aud` como array también se acepta si contiene la canónica.

### 3. `iss` (RFC 9207)

- El redirect de `/oauth/authorize` incluye `&iss=<canónica urlencoded>`
  (además de `code` y `state`).
- `/.well-known/oauth-authorization-server` añade
  `"authorization_response_iss_parameter_supported": true`.

### Fricción cero

Un cliente con Bearer válido no cambia nada: el token maestro se sigue
comparando en `auth.rs` antes de tocar `validate_bearer`, y todos los JWTs
emitidos por `sign_jwt` (incluidos los de los tests de integración) llevan ahora
`aud` correcta, así que siguen validando con `dev_mode=false`.

## Verificación (ejecutada por mí, salidas reales)

```
cargo test -p tylluan-kernel --lib transport::http::oauth
  → 15 passed; 0 failed   (4 de fase 1 + 11 nuevas de fase 2)
```

Tests nuevos (módulo `fase2_tests` en `oauth.rs`), uno por punto:

| Test | Qué fija |
|------|----------|
| `protected_resource_metadata_declares_resource_and_as` | RFC 9728: `resource`, `authorization_servers`, método de portador |
| `metadata_advertises_iss_parameter_supported` | RFC 9207 en metadatos (y S256 intacto) |
| `authorize_response_carries_iss` | `iss` urlencoded en el redirect |
| `signed_jwt_carries_canonical_audience` | `aud` de `sign_jwt` |
| `legacy_token_without_audience_rejected_in_prod` | sin `aud` + `dev_mode=false` → false |
| `legacy_token_without_audience_accepted_in_dev_mode` | sin `aud` + `dev_mode=true` → true |
| `foreign_audience_rejected_in_both_modes` | `aud` ajena → false en ambos modos |
| `own_token_validates_in_prod_mode` | token propio con `dev_mode=false` |
| `resource_must_point_to_this_server` | canónica ±path sí; otro host/puerto/esquema no |
| `authorize_rejects_foreign_resource` | `400 invalid_target` |
| `authorized_token_carries_audience_end_to_end` | authorize (`resource` propio) → canje → `aud` canónica y token válido |

Puertas de la cola:

- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **EXIT 0**.
- `cargo test -p tylluan-kernel --test oauth_operator_gate --test oauth_f2_adversarial_spec`
  → **oauth_operator_gate: 6 passed; 0 failed** · **oauth_f2_adversarial_spec:
  1 passed; 7 ignored; 0 failed**. (La suite F2 sigue `#[ignore]` por diseño;
  aquí solo se exige que compile y que lo activo esté verde.)
- `cargo test -p tylluan-kernel --lib` → **1078 passed; 0 failed; 5 ignored**.
- `scripts/check_test_count.sh` → **EXIT 0**, `kernel=1078 link=75 fsrs=12 total=1165`.
  Cifras actualizadas en `README.md`, `AGENTS.md`, `STATUS.md` (dos sitios) y los
  cuatro sitios del dashboard (`docs-site/...`).
- `scripts/check_docs_reality.sh` → **EXIT 1 por un único ❌ preexistente**
  (`docs/roadmap/ROADMAP_O3.md:42`, `onnx/int8/decima.json`), ajeno a D-2 y
  asignado a la cola de Buffy (B-1). Sin ❌ nuevos.
- `check_head_sync.sh` / `check_no_predation.sh` siguen **no ejecutables** en este
  worktree (resolución de `.git/worktrees/...` bajo Git Bash en Windows); HEAD
  comprobado a mano: `17465b9`.

## Decisiones mías (no del TL) que conviene revisar

1. **Alcance del `resource`**: acepto canónica + sufijo de path/query, no solo
   igualdad exacta. Es más permisivo que "exact match" y está motivado por no
   romper clientes que apuntan a `.../mcp`; el host/puerto sí es estricto.
2. **`aud` ajena se rechaza también con `dev_mode=true`** (el brief solo exigía
   rechazarla con `dev_mode=false`). Razón: un token de otro servicio no debería
   valer ni en local, y nuestros propios tokens siempre llevan la `aud` correcta,
   así que no añade fricción.
3. **`scopes_supported` no se declara** en los metadatos del recurso: prefiero
   declararlo cuando existan los scopes reales de la fase B antes que anunciar
   una lista inventada.
4. **`STATUS.md:50`**: para que la puerta de documentación quede verde tuve que
   actualizar cifras y fecha de verificación; la línea sigue diciendo
   `HEAD: 8cf87f4` (la referencia histórica del ciclo). Si prefieres que ese HEAD
   pase a `17465b9`, es tu llamada — no lo cambié por no reescribir tu registro.

## Límites

- No he leído los RFC 9728/8707/9207 directamente: la fuente es la tabla de
  citas del documento de investigación del TL (que también advierte que RFC 9728
  no se abrió). Lo implementado es lo que MCP exige y lo que el brief fija.
- No hay página de consentimiento ni desafíos de scope: son D-3, fuera de esta
  fase A.
- No he probado el flujo con un cliente real (Claude Desktop / `mcp-remote`);
  solo handlers y router en tests.

## Impact

Toca `transport/http/` (endpoint público nuevo + validación de tokens) y las
cifras de tests. Responde a ADR-013:

- **¿Depreda a otro agente?** No: rama desde `17465b9`, sin tocar archivos de
  Buffy ni de Mimo; el único choque posible es el recuento de tests, que sube por
  mis 11 tests y queda vigilado por el script.
- **¿Depreda a un usuario?** No añade pasos: sin `resource` todo sigue igual; con
  `dev_mode=false` un token sin `aud` deja de valer (es el cierre buscado, no una
  regresión: no había forma legítima de obtener uno sin `aud` salvo tokens
  anteriores a este cambio, que solo existen en dev/lab).
- **¿Rompe despliegues?** Los JWTs ya emitidos en producción (si hubiera alguno
  fuera de dev) no llevan `aud` y con `dev_mode=false` dejarían de validar. Es
  deliberado y está en el código; si el TL prefiere una ventana de gracia, es un
  cambio de una línea (`None => true` temporal) que no hice por no abrir un
  agujero silencioso.
- **¿Se puede verificar sin mí?** Sí: los comandos de arriba reproducen todo.
