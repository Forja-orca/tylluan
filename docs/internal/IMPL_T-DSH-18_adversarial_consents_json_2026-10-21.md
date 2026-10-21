# T-DSH-18 — Auditoría adversarial de `consents.json` (D-15, `oauth_consent.rs`)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `fc480d6` == origin/main (push89; T-DSH-17 integrada; lib 1152).
- **Encargo (turno 1487):** solo tests sobre `consents.json`: (1) fichero corrupto/truncado/JSON inválido al arrancar y en recarga por mtime; (2) `expires_at` futuro lejano/negativo/desbordado, `revoked_at` raro, `version` desconocida; (3) `client_id` con separador `|` colisionando claves; (4) escrituras concurrentes sin perder ni duplicar; (5) symlink/permisos legibles por otros (nota Windows); (6) ni credencial, ni csrf, ni consent_id en el fichero. Tabla caso × esperado × real, `#[ignore]` con motivo si falla + hallazgo con archivo:línea, `## Impact` y «no verificado».

## Entrega (solo tests)

`crates/tylluan-kernel/tests/oauth_consent_file_adversarial.rs` — 7 tests contra la **API pública** del store real (`ConsentStore`): no toca `src/`, no necesita acceso a campos privados (para los vectores internos se manipula el fichero, que es justo la superficie atacada).

```text
rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_consent_file_adversarial
# 6 passed / 1 ignored  (0,2 s)
rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_consent_file_adversarial -- --ignored
# 1 failed (el #[ignore] es la demostración del hallazgo V3)
```

## Tabla caso × esperado × real

| # | caso | esperado | real | veredicto |
|---|---|---|---|---|
| V1a | arranque con fichero corrupto / truncado / vacío / `null` / tipos erróneos | no cae y **no concede** consentimiento | store arranca vacío, `valid_consent` = `None`, sin panic; aprobar después reescribe un fichero válido | OK |
| V1b | recarga por mtime con fichero corrompido | no cae; el corrupto no concede nada nuevo | no cae, **mantiene el estado anterior en memoria** (no concede nada nuevo, pero tampoco revoca) | OK + observación F3 |
| V2a | `expires_at` futuro lejano (9e9) | sin panic; vigente o caducado (decisión del fichero) | vigente (no hay tope superior) | OK (documentado) |
| V2b | `expires_at` negativo | caducado, no vigente | `valid_consent` = `None` | OK |
| V2c | `expires_at = i64::MAX` (desbordado) | sin overflow ni panic | vigente, sin desbordar | OK |
| V2d | `revoked_at` = `0`, `-1`, `1` (cualquier `Some`) | revocado | revocado en los tres | OK |
| V2e | `revoked_at = null` | vigente | vigente | OK |
| V2f | `version` desconocida (12345) | ¿rechazar por versión? | **se acepta sin validar** (no hay gate de versión) | observación F2 |
| V2g | `version` con tipo inválido (`-1`) | parse error sin caída | parse error → mantiene estado en memoria | OK |
| V3a | par `(a\|b, c)` y par `(a, b\|c)` en memoria | pares distintos, sin herencia | **la clave interna `a\|b\|c` colisiona: `(a, b\|c)` devuelve el registro de `(a\|b, c)`** | **HALLAZGO F1** (`#[ignore]`) |
| V3b | dos entradas colisionantes en el fichero | ambas cargan como pares distintos | solo sobrevive una (HashMap: última gana); ambas claves resuelven a la misma entrada | **HALLAZGO F1** |
| V4a | 4 aprobaciones simultáneas × 10 rondas (CI) | sin pérdidas ni duplicados | 40/40 en disco, 0 duplicados | OK |
| V4b | estrés: 4 × 200 rondas (800 aprobaciones) | sin pérdidas | 800/800, sin duplicados (18,6 s; no va en CI) | OK |
| V4c | misma clave aprobada 2 veces a la vez | una sola entrada | 1 entrada | OK |
| V5a | fichero sustituido por otro fichero válido | se relee (vía de revocación) | relee y aplica | OK |
| V5b | permisos legibles por otros (POSIX) | `mode & 0o077 == 0` | **no ejecutable en esta máquina (Windows)**: no hay bits POSIX; el fichero hereda la ACL del directorio | NO VERIFICADO (F4, solo Unix) |
| V5c | fichero sustituido por symlink | comportamiento definido | no reproducible aquí: crear symlink en Windows exige `SeCreateSymbolicLinkPrivilege` (no se intenta) | NO VERIFICADO |
| V6 | tras aprobar y con un pending vivo | ni credencial, ni csrf, ni consent_id en el fichero | ninguno aparece; las claves del JSON son solo las 8 de `ConsentRecord` | OK |

## Hallazgos

**F1 — Colisión de claves por el separador `|` (bajo; `#[ignore]` con motivo).**
`ConsentStore::key` concatena `format!("{client_id}|{redirect_uri}")` sin escapar (`oauth_consent.rs:151-153`) y tanto `reload_if_changed` (`:177-181`) como `valid_consent` (`:239-269`) usan esa clave plana: dos pares distintos `(a|b, c)` y `(a, b|c)` comparten entrada y uno puede heredar el consentimiento del otro. **Por HTTP no es explotable hoy** porque `oauth.rs` valida el `redirect_uri` (loopback, esquema, host) antes de llegar al store y un `b|c` no es URI loopback válida; pero la API del store es insegura por construcción y cualquier llamador futuro que valide distinto la expone. Fix sugerido en código (no aplicado, solo tests): clave con longitud prefijada (`format!("{}:{}", client_id.len(), client_id)` + redirect) o clave tupla `(String, String)` en un `HashMap<(String,String), _>`.

**F2 — `version` sin validación (bajo, forward-compat).** `ConsentFile.version` se parsea y se ignora (`oauth_consent.rs:86-96`, `:171-182`); un fichero de una versión futura con semántica distinta se carga como si fuera v1. Impacto actual nulo (solo hay v1); anotado por si el formato crece.

**F3 — Corrupción en recarga = revocación que falla abierta (bajo/medio).** Si el operador intenta revocar editando el fichero y deja el JSON inválido, `reload_if_changed` avisa por `tracing::warn!` pero **mantiene el consentimiento anterior en memoria** (`oauth_consent.rs:171-187`): la revocación no surte efecto hasta que el fichero vuelva a ser válido. No concede nada nuevo (el vector se cumple), pero conviene documentarlo en el pie de P1 o validar el JSON antes de editar.

**F4 — Permisos del fichero (solo Unix, no verificado aquí).** `persist` escribe con `std::fs::write` sobre `consents.json.tmp` y renombra (`oauth_consent.rs:208-209`) sin fijar permisos: en Unix el modo resultante es el del umask (típicamente `0644`, legible por grupo/otros). El contenido son metadatos (sin secretos, ver V6), así que el impacto es bajo, pero un `0600` explícito sería lo correcto. **En Windows no aplica**: no hay bits POSIX y el fichero hereda la ACL del directorio de datos; el test lo documenta y no lo intenta.

**Observación F5 — carrera teórica de `persist` (no reproducida).** `persist` toma la foto del `HashMap` bajo mutex pero serializa/escribe/renombra **fuera** del lock, sobre un `.tmp` de nombre fijo (`oauth_consent.rs:190-223`): dos aprobaciones concurrentes pueden intercalar la foto y el rename y dejar en disco la foto más vieja (pérdida de un registro, recuperable de memoria hasta la siguiente recarga). No se reprodujo con 800 aprobaciones concurrentes en 200 rondas (Windows). Queda anotado; si se quiere cerrar del todo, serializar `persist` con un mutex propio o escribir el JSON bajo el lock del estado.

## Verificación

| criterio | resultado |
|---|---|
| suite normal | **6 passed / 1 ignored** (0,2 s) |
| `-- --ignored` | 1 failed = demostración de F1 (por diseño) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1152 passed** (sin cambios: todo el diff son tests de integración) |

## Impact

- **Solo tests**: un fichero de integración nuevo (7 tests, 6 vivos + 1 `#[ignore]` por hallazgo) y este informe. **Cero cambios en `src/`**, cero escrituras fuera de directorios temporales de test, sin secretos.
- Coste CI: +0,2 s (la variante de estrés de 800 aprobaciones, 18,6 s, se midió a mano y no va en la suite).

## No verificado

- **Permisos POSIX y symlink (V5b/V5c)**: esta máquina es Windows; no hay bits POSIX y crear symlinks requiere privilegio. El bloque `#[cfg(unix)]` del test quedará sin ejecutar aquí (en Unix diría si el modo es `0600`).
- **Arranque real del kernel con `OAuthState::with_consent_dir`**: el test usa `ConsentStore::with_file` directamente; no se arrancó kernel (regla de flota) ni se comprobó qué directorio cablea `transport/http/mod.rs` en producción.
- **Carrera de `persist` (F5)**: no reproducida en Windows con 200 rondas × 4 hilos; la semántica de `rename` en Unix puede comportarse distinto (reemplazo atómico), así que la probabilidad real no está medida.
- **`mtime` con resolución gruesa**: los tests duermen 20 ms antes de reescribir; en un FS con resolución de 1 s la recarga podría no dispararse (el test de `oauth_consent.rs` ya duerme por este motivo).
