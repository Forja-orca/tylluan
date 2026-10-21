# T-DSH-14 — Auditoría adversarial de D-15 (consentimiento OAuth Fase 2)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `260a241` == origin/main (push 85: D-15, D-22a y T-DSH-13 integrados; lib 1150). **Auditoría sobre el código ya integrado**, no sobre el parche.
- **Encargo (turno 1479):** 7 vectores sobre `tests/oauth_f2_consent.rs` como base; solo tests (`#[ignore]` con motivo si fallan), tabla caso × esperado × real, informe con `## Impact` y «no verificado».

## Método

Tests de integración contra el **router real** (`GET/POST /oauth/authorize`, `POST /oauth/token`), con `dev_mode=false` y la credencial de operador del harness (`support::test_state_custom`), PKCE S256 real y `ConnectInfo` inyectado cuando el vector necesita IP de socket distinta. Fichero entregado: `crates/tylluan-kernel/tests/oauth_f2_consent_adversarial.rs` (7 tests, uno por vector).

```text
rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_f2_consent_adversarial
```

Resultado: **7 passed / 0 failed / 0 ignored** — ningún vector encontró un bypass. No hizo falta marcar ningún test como ignorado (la convención del encargo era para tests que fallaran).

## Tabla caso × esperado × real

| # | caso | esperado | real | veredicto |
|---|---|---|---|---|
| V1a | GET authorize sin consentimiento | P1 (200), sin code ni redirect | 200 HTML, sin `Location`, sin `code=` | OK |
| V1b | POST approve con credencial vacía | 403, sin code | 403, sin `Location` | OK |
| V1c | POST approve con credencial incorrecta | 403, sin code | 403 + P1 con error | OK |
| V1d | credencial/decision en query string de GET | ignoradas | 200, sin code | OK |
| V1e | aprobación correcta y segunda conexión | code sin acción humana (por diseño) | 302 con code | OK |
| V2a | replay del POST tras aprobar (mismos ids) | 403, sin segundo code | 403, `consent_id` consumido | OK |
| V2b | replay de approve tras **denegar** | 403, sin code | 403 (pending eliminado) | OK |
| V3a | CSRF de la petición B sobre `consent_id` A | 403, A intacta | 403; A sigue viva | OK |
| V3b | cookie de la sesión B con ids de A | 403 | 403 | OK |
| V3c | petición A legítima tras los intentos cruzados | 302 con code | 302 con code | OK |
| V4a-c | mismo cliente, otro redirect (`?x=1`, `/`, `localhost`) | no hereda consentimiento | P1 (200), sin code | OK |
| V4d-f | `client_id` con mayúsculas, espacio final, NUL | no hereda | P1/400, sin code | OK |
| V4g-h | `client_id` con el separador `\|` del key interno | no colisiona con otro par | P1, sin code | OK |
| V5 | `scope=operator` en 11 formas (mayúsculas, espacios, tabs, comas, duplicados, NBSP) | 400 `invalid_scope` siempre | 400 `invalid_scope` en las 11 | OK |
| V5b | `scope=mcp` repetido/con espacios | aceptado (dedup), P1 sin code | 200, sin code | OK |
| V6a | `client_id` con `<script>`/`onerror` | escapado, sin ejecución | escapado; CSP `default-src 'none'`, `X-Frame-Options: DENY` | OK |
| V6b | `state` con `<script>` y CRLF | no reflejado crudo; percent-encoded en `Location` | `Location` sin `<script>`/CRLF, con `%3C` | OK |
| V7a | 5 fallos con `X-Forwarded-For` rotado | bloqueo (la cabecera no crea cupos) | 429 al 6.º intento | OK |
| V7b | 5 fallos de la misma IP de socket con XFF distinto | bloqueo de esa IP | 429 | OK |
| V7c | 20 fallos desde 20 IPs distintas | techo global bloquea (riesgo de disponibilidad) | 429 incluso para IP nueva | OK (comportamiento documentado) |

## Riesgos observados (no son bypasses; para decidir)

1. **DoS de disponibilidad por el techo global (medio).** `GLOBAL_MAX_FAILURES = 20`: cualquier cliente no autenticado puede agotar el cupo con 20 credenciales inventadas y dejar la pantalla de consentimiento **bloqueada 300 s para todos** (`oauth_consent.rs`, `prune_failures`). Confirmado en V7c. Mitigación posible: cupo global mucho mayor, ventana más corta o bloqueo con escalado por IP en vez de global.
2. **El cupo por IP usa la IP del socket, no `X-Forwarded-For` (correcto hoy, frágil con proxy).** Sin *trusted proxy* configurado, el kernel directo está bien (el encargo lo pedía justo así: XFF no debe abrir cupos). Pero si alguien despliega el kernel detrás de un proxy inverso (no es el diseño: `:47004` escucha directo), **todos los clientes comparten una única IP** y cualquier atacante bloquea a todos con 5 fallos. No hay soporte de proxy de confianza: documentarlo como límite de despliegue.
3. **Pendings sin límite por cliente (bajo).** Cada `GET /oauth/authorize` crea un `PendingConsent` en memoria (TTL 5 min, barrido perezoso al crear otro). Un cliente puede generar muchos en 5 min (crecimiento de memoria acotado por TTL, no por cupo). Sin impacto de seguridad directo; sí higiene.
4. **La cookie de sesión es el propio `consent_id` (bajo, diseño).** La comprobación CSRF compara cookie (`tylluan_oauth_consent`) con el `consent_id` del formulario, y el CSRF real es el campo oculto aleatorio. Funciona (los dos secretos son aleatorios de 32 bytes y no se filtran), pero un secreto de sesión independiente del `consent_id` sería más limpio si algún día el `consent_id` aparece en logs/URLs.
5. **`response_type` no se valida (informativo).** El comentario del código lo dice (`response_type accepted but not validated`); hoy solo se emite `code`, así que un `response_type=token` obtiene igualmente un `code` (desviación de RFC 6749, sin impacto de seguridad sobre el operador).

## Impact

- **Solo tests**: un fichero nuevo (`oauth_f2_consent_adversarial.rs`, 7 tests) y este informe. **Cero cambios en `src/`**, cero escrituras en producción (el estado de test es en memoria y el kernel vivo no se tocó), sin secretos.
- Los 7 tests corren en la suite normal de CI (`cargo test -p tylluan-kernel --test oauth_f2_consent_adversarial`), coste ~9 s; `clippy --all-targets -D warnings` exit 0 con ellos dentro.
- Los riesgos 1-2 son de disponibilidad/despliegue y requieren decisión de TL/José, no cambios de código por mi parte.

## No verificado

- **Navegador real**: todo se ejercita in-process sobre el router; no se probó con un navegador (cookies, `SameSite`, redirecciones reales a un cliente OAuth).
- **Despliegue detrás de proxy inverso**: el riesgo 2 se deduce del código (IP de socket) pero no se reprodujo con un proxy real.
- **Timing del `constant_time_eq`**: la comparación es constant-time salvo el early-return por longitud; no se midió un ataque de temporización (fuera de alcance y poco práctico sobre loopback).
- **`dev_mode=true`** (auto-aprobación): es el camino de desarrollo y no se audita aquí (decisión R12).
- **Persistencia del fichero de consentimientos**: la revocación editando el fichero y el mtime tienen tests unitarios en `oauth_consent.rs`; la matriz no repite eso a nivel de router (T-DSH-14 se limita a los 7 vectores).
