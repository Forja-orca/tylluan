# D-15 — Consentimiento de operador en `/oauth/authorize` (implementación)

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` = `57d2e7b`
**Plan:** `docs/internal/PLAN_oauth_f2_consentimiento_deep_2026-10.md` (decisiones 1–5 ya aprobadas)
**Estado:** implementado y verificado en tests + navegador simulado. **No verificado en vivo** (ver §7).

---

## 1. Qué cambia para el usuario

Antes (F2): en `dev_mode = false` el kernel respondía `403 access_denied` a cualquier
`/oauth/authorize`, es decir, el flujo OAuth de clientes MCP (Claude Desktop/Code, Cursor…)
estaba cerrado de facto: solo cabía repartir un Bearer estático a mano.

Ahora: el kernel sirve una **página de consentimiento** (P1) que el operador humano aprueba una
vez con su credencial. Tras aprobar, ese cliente y ese `redirect_uri` quedan recordados **30 días
deslizantes** y los siguientes arranques del cliente obtienen token **sin ninguna acción humana**
(0 clics). Revocar = editar `consents.json` en el directorio de datos (no hay endpoint nuevo).

`dev_mode = true` **no cambia** (sigue emitiendo código sin consentimiento, como antes): nadie
puede quedar bloqueado por este cambio.

## 2. Flujo implementado

```
GET /oauth/authorize?response_type=code&client_id=…&redirect_uri=…&code_challenge=…&state=…
  ├─ dev_mode=true ─────────────────────────────► 307 + code (comportamiento previo, intacto)
  ├─ sin client_id (no es un cliente OAuth) ────► 403 access_denied  (RFC 6749 §4.1.1)
  ├─ consentimiento vigente (30 d) ─────────────► 307 + code  (0 acciones humanas)
  ├─ rate-limited (IP) ─────────────────────────► 429 + Retry-After
  └─ si no ─────────────────────────────────────► 200 P1 (HTML, cookie de sesión, CSRF)

POST /oauth/authorize  (consent_id + csrf + credential + decision)
  ├─ cookie ausente/no coincide ────────────────► 403 (no filtra si el consent_id existe)
  ├─ decision=deny ─────────────────────────────► 403 access_denied + auditoría
  ├─ credencial incorrecta ─────────────────────► 403 + P1 re-renderizada con CSRF rotado
  └─ credencial correcta ───────────────────────► 307 + code (+ Location a redirect_uri)
```

Estados: `PENDING` (5 min) → `APPROVED` (code de un solo uso, 60 s) · `DENIED` · `EXPIRED`
(TTLs) · `LOCKED` (5 fallos/IP en 5 min, o 20 globales en la ventana).

## 3. Decisiones del plan, tal como quedaron

| # | Decisión | Implementación |
|---|----------|----------------|
| 1 | Un solo paso, una sola página, sin asistentes | `render_consent_page()` — 1 `<form>`, 2 botones, 0 JS |
| 2 | Mostrar `client_name` + `client_id` corto; **no** hay registro de clientes (DCR deprecado) | Se muestra el `client_id`; si el operador escribe un `client_name` a mano en `consents.json`, se muestra y **sobrevive a re-aprobaciones** (el cliente no puede sobrescribirlo) |
| 3 | Consentimiento persistente 30 d deslizante, revocable | `consents.json` atómico (tmp+rename), `last_used_at` refresca, `revoked_at` no nulo = revocado, recarga por `mtime` (revocar no exige reiniciar) |
| 4 | Nunca `operator` scope desde OAuth | `scope` ausente/vacío/`mcp` → `["mcp"]`; cualquier otra cosa → 400 `invalid_scope` (nunca degrada silenciosamente) |
| 5 | Nunca almacenar credenciales | Solo se compara en memoria (`subtle::ConstantTimeEq`); no se escribe en disco ni en logs |

## 4. Archivos

| Archivo | Δ | Contenido |
|---|---|---|
| `crates/tylluan-kernel/src/transport/http/oauth_consent.rs` | **nuevo**, 688 líneas | `ConsentStore` (memoria/fichero), `ConsentRecord`, `PendingConsent`, rate limiter, `render_consent_page`, `redirect_host`, `html_escape`. **8 unit tests** |
| `crates/tylluan-kernel/src/transport/http/oauth.rs` | +535/−? | `OAuthState` += `operator_credential`, `consents`; builders `with_operator_credential` / `with_consent_dir`; `PreparedAuth`, `parse_scopes`, `validate_authorization`, `issue_code`, `consent_cookie`, `consent_page_response`, `constant_time_eq`; `authorize_handler` reescrito; `authorize_decision_handler` nuevo |
| `crates/tylluan-kernel/src/transport/http/mod.rs` | +21 | `pub mod oauth_consent;`, ruta GET+POST, wiring (`oauth_operator = auth_token.clone()`, `oauth_data_dir`) |
| `crates/tylluan-kernel/tests/support/mod.rs` | +7 | el harness de tests configura credencial de operador |
| `crates/tylluan-kernel/tests/oauth_f2_adversarial_spec.rs` | +48/−? | **3 `#[ignore]` retirados** (C1, C2a, C2c) → 8/8 activos |
| `crates/tylluan-kernel/tests/oauth_f2_consent.rs` | **nuevo**, 420 líneas | 9 tests: estados, lock, fricción, cabeceras |

Parche listo para aplicar: `E:\tylluan-patches\deep\D-15.patch` (**77497 B**, 6 archivos,
**+1630/−89**), `git apply --check` **EXIT 0** contra `origin/main` `57d2e7b` limpio.

## 5. Seguridad

- **P1 sin JS**, CSP `default-src 'none'; style-src 'nonce-…'; form-action 'self'; frame-ancestors 'none'`,
  `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store`.
- **CSRF real**: cookie `tylluan_oauth_consent` (HttpOnly, SameSite=Lax, Path=/oauth, `Max-Age` = TTL del pending)
  + nonce en el formulario, ambos comparados en tiempo constante. Un POST sin cookie no consume el pending.
- **Anti-enumeración**: POST con `consent_id` desconocido y sin cookie → mismo 403 que con cookie válida.
- **Fricción**: 5 fallos de credencial por IP en 5 min → 429 con `Retry-After`; tope global 20 (ventana móvil)
  para que un atacante con muchas IPs no haga crecer el mapa sin límite.
- **Fail-closed**: kernel sin credencial de operador configurada → aprobar devuelve 403 con instrucción de usar
  Bearer estático. Nunca se emite código "porque sí".
- **Auditoría** por `tracing` (target `oauth_consent`): `decision=pending|denied|approved|refused_no_operator_credential|rate_limited`
  con `client_id`, `redirect_uri`, `ip`. Sin credenciales ni código.

## 6. Verificación

Base sincronizada: `origin/main` = **`57d2e7b`** (incluye ya D-18 `9d584cc` y D-19/D-20 `b9ce9e2`,
más T-DSH-10/11 del TL). El worktree se movió a ese commit y el parche se re-aplicó; **todo lo de
abajo es sobre esa base**, no sobre la anterior:

| Comando | Resultado |
|---|---|
| `cargo test -p tylluan-kernel --lib` | **1150 passed, 0 failed, 4 ignored** (34.30 s) |
| `cargo test -p tylluan-kernel --test oauth_f2_adversarial_spec` | **8 passed, 0 ignored** (antes 5 + 3 ignorados) |
| `cargo test -p tylluan-kernel --test oauth_f2_consent` | **9 passed** |
| `cargo test -p tylluan-kernel --test oauth_operator_gate` | **6 passed** (sin regresión del gate C1 previo) |
| `cargo clippy -p tylluan-kernel --tests -- -D warnings` | **EXIT 0** |

**P1 en navegador** (Playwright/Chromium, `setContent` con el HTML real servido y viewport 1280×720):
`scrollHeight = 720 = innerHeight`, `scrollWidth = 1280` → **sin scroll**; un único `input[type=password]`;
botones `Aprobar` / `Denegar`; `<label for=credential>` = "Credencial de operador"; destino del código
(`http://127.0.0.1:9999`) en 18 px; `Tab` desde el documento enfoca la credencial y se puede escribir.
Captura: `E:\tylluan-wt-deep\target\d15_p1_final_1280x720.png`.

## 7. Límites honestos (no verificado)

1. **No se ha probado contra un kernel vivo ni contra un cliente MCP real** (Claude Desktop/Code):
   el entorno no permite arrancar procesos. Todo el flujo está probado in-process con `oneshot`
   contra el router real, y el HTML se midió en navegador con el markup exacto que sirve el kernel,
   pero no hay evidencia de un `authorize` → `token` → `tools/call` end-to-end en vivo.
   Pasos equivalentes para el TL (curl, con cookie porque el POST la exige):
   ```bash
   curl -i "http://127.0.0.1:47004/oauth/authorize?response_type=code&client_id=prueba-deep&redirect_uri=http://127.0.0.1:9999/cb&code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM&code_challenge_method=S256" -c cj.txt
   # copiar consent_id del HTML; luego:
   curl -i -X POST http://127.0.0.1:47004/oauth/authorize -b cj.txt \
     -d "consent_id=<id>&csrf=<csrf>&decision=approve&credential=$(cat .tylluan-token)"
   ```
2. **C1/C2a/C2c**: pasan porque su petición de cable **no envía `client_id`** (un cliente OAuth real
   sí lo envía, RFC 6749 §4.1.1) → 403 `access_denied` antes de llegar a P1. La misma invariante
   **con** `client_id` está cubierta en `tests/oauth_f2_consent.rs`. No se relajó ninguna aserción
   existente; los tres casos dejaron de estar ignorados porque ya no dependen de comportamiento ausente.
3. **Sin registro de clientes**: el nombre amigable solo aparece si el operador lo escribe a mano en
   `consents.json`. DCR está deprecado en MCP (2025-11-25) y el plan lo excluye; el cliente se
   identifica por `client_id`.
4. `scopes_supported` sigue sin anunciarse en `/.well-known/oauth-authorization-server` (fuera del
   alcance de D-15; el kernel acepta `mcp` y rechaza el resto).
5. El redirect usa **307** (`Redirect::temporary`), no 302; los tests asertan 307.
6. La IP solo existe si el router se sirve con `into_make_service_with_connect_info`; en tests
   in-process el bucket es `unknown` (documentado en el código).

## 8. Riesgo de bloqueo para José — respondido

- `dev_mode = true` intacto; Bearer estático intacto; la credencial de operador es **la misma** que
  el `auth_token` (no hay secreto nuevo que rotar).
- Un consentimiento aprobado **nunca** puede conceder `operator` (solo `mcp`).
- Revocar es editar un fichero; si `consents.json` se corrompe, se ignora y se vuelve a pedir
  consentimiento (fail-closed, no fail-open) → nunca deja a José fuera del kernel, solo le pide
  aprobar de nuevo.

## Impact

Toca `crates/tylluan-kernel/src/transport/` (ADR-013: sección obligatoria). No cambia defaults de
`dev_mode`, ni el puerto, ni el token, ni CONTRACT-01 (siguen las 6 tools; P1 solo las **lista**).
No añade endpoints fuera de `/oauth/authorize` (GET ya existía; POST es la decisión sobre la misma
ruta). Sin dependencias nuevas (`subtle` ya estaba en el workspace). Sin procesos lanzados.
No reduce timeouts ni toca embeddings, ACL ni federation. El estado nuevo en disco es un
`consents.json` dentro del directorio de datos, que no contiene credenciales.
