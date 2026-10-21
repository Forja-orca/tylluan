# D-25 — Techo global del consentimiento OAuth + ampliación T-DSH-18 (F1/F3/F4/F5)

**Agente:** Deep (OpenCode)
**Fecha:** 2026-10-21
**Base:** `59d3eaa` (worktree `E:\tylluan-wt-deep`, detached). Mientras trabajaba,
`origin/main` avanzó a `3dd9dc6` (dos commits **solo de documentación**: `STATUS.md` y
un borrador de research). El parche se verificó con `git apply --check` también contra
`3dd9dc6` (ver §5).
**Estado:** implementado y verificado en local. **No verificado en producción** (no se
ha arrancado ningún kernel: regla del entorno).

---

## 1. Qué pedía la cola

`docs/internal/COLA_deep_2026-10-21.md`, D-25:

1. Sustituir el techo global de fallos (que hoy **bloquea a todo el mundo**) por un
   límite que **solo frene el ritmo**: un tercero no puede dejar la pantalla de
   consentimiento inutilizable para el operador.
2. Bloqueo **por IP de socket** con **espera creciente** (no un 429 fijo a los 5 fallos).
3. Tope de peticiones de consentimiento vivas (riesgo 3 de T-DSH-18).
4. Ampliación T-DSH-18: **F1** colisión de la clave interna por `|`, **F3** fallo abierto
   al recargar un `consents.json` inválido, **F4** permisos `0600` en Unix, **F5**
   serializar las escrituras del fichero.
5. Sin tocar `dev_mode`, su valor por defecto, ni la credencial maestra.

## 2. Qué había realmente (premisa contrastada contra el código)

| Hallazgo | Evidencia antes del cambio |
|---|---|
| El techo global era un **bloqueo**, no un freno | `prune_failures` devolvía `entry.len() >= MAX_FAILURES \|\| global.len() >= GLOBAL_MAX_FAILURES`; `is_locked` era exactamente eso. 20 fallos de 20 IPs distintas (o de una sola con `X-Forwarded-For` rotado… no: sin `ConnectInfo` todas caen en el bucket `unknown`) dejaban **300 s de 429 para todos**, incluida una IP recién llegada. |
| El `Retry-After` no correspondía al bloqueo | `oauth.rs` hacía `retry_after_secs(&ip).max(60)` mientras el bloqueo real duraba 300 s desde el **primer** fallo de la ventana. |
| `MAX_FAILURES` no escalaba | A los 5 fallos, bloqueo plano hasta que expiraba la ventana. |
| F1: colisión de clave | `key()` era `format!("{client_id}\|{redirect_uri}")` → `("a\|b","c")` y `("a","b\|c")` compartían entrada. El test `v3_pipe_in_client_id_collides_internal_keys` estaba `#[ignore]` **y además tenía la aserción invertida** (esperaba que al menos una entrada desapareciera, que es justo el comportamiento con bug). |
| F3: recarga inválida = fallo abierto | `reload_if_changed` hacía `.ok()` sobre lectura y parseo; en `None` **mantenía el estado anterior**. Revocar editando el fichero fallaba abierto si el JSON quedaba roto. |
| F4: sin permisos restrictivos | `std::fs::write` sobre `consents.json.tmp` con el umask del proceso (típicamente `644`). |
| F5: escrituras no serializadas | Dos `persist` concurrentes compartían `consents.json.tmp`; el `rename` podía publicar el contenido de la otra escritura. |
| Pendings sin tope | `create_pending` solo barría expirados: un flood de `/oauth/authorize` crecía sin límite en memoria (riesgo 3). |

## 3. Cambios

| Archivo | Cambio |
|---|---|
| `transport/http/oauth_consent.rs` | Nueva política de límite: `lockout_secs()` (5-6 → 30 s, 7-8 → 60 s, 9 → 120 s, 10+ → 300 s); `lock_until()` calcula por IP `último_fallo + escalón + extra_global`; `is_locked()`/`retry_after_secs()` derivan de ahí. `GLOBAL_MAX_FAILURES` deja de bloquear: solo añade `GLOBAL_THROTTLE_EXTRA_SECS` (2 s) a IPs **que ya fallaron**. `key()` pasa a `len(client_id):client_idredirect_uri` (F1). `reload_if_changed` fail-closed: lectura/JSON inválido → `consents.clear()` + `tracing::error!` (F3). `write_private()` en Unix crea el `.json.tmp` con `0o600` vía `OpenOptions::mode` + `set_permissions` (F4). `persist_lock: Mutex<()>` serializa las escrituras (F5). `MAX_PENDING = 128` con desalojo de la petición más antigua (`pending_seq`) + `pending_count()` para introspección/tests. |
| `transport/http/oauth.rs` | `Retry-After` = espera real (`.max(1)`), sin el suelo de 60 s que contradecía el primer escalón. |
| `tests/oauth_f2_consent_adversarial.rs` | **V7c invertido**: 20 fallos desde 20 IPs distintas ya **no** bloquean a una IP nueva; la aprobación legítima con la credencial maestra completa (`307` + code). Se elimina el `break` que enmascaraba el comportamiento anterior. |
| `tests/oauth_consent_file_adversarial.rs` | V1 (recarga inválida) pasa de "mantiene estado" a "revoca" (F3). V2 (`version` con tipo inválido) igual, y comprueba que un fichero válido posterior vuelve a habilitar sus entradas. V3 (F1) se **des-ignora** y se reescribe con la aserción correcta: los dos pares coexisten con sus propios valores. |

## 4. Tests

**Unidad (`oauth_consent.rs`, 13 en total, 5 nuevos en Windows / 6 en Unix):**

- `lockout_escalates_and_the_global_ceiling_never_blocks_a_clean_ip` — satura el techo global con 20 IPs y comprueba que `127.0.0.1` **sin fallos propios no está bloqueada**; a los 5 fallos propios hay bloqueo de 30 s (+2 s global), a los 7 escala a 60 s, y `clear_failures` devuelve el cupo.
- `pending_requests_are_capped_evicting_the_oldest` — con `MAX_PENDING` vivas, una más desaloja la más antigua y el contador no supera el tope.
- `invalid_json_on_reload_revokes_instead_of_failing_open` (F3).
- `concurrent_persists_do_not_lose_records` (F5): 8 hilos aprobando a la vez; el fichero final parsea y tiene las 8 entradas.
- `key_is_unambiguous_with_separator_in_values` (F1).
- `consents_file_is_created_with_0600` (F4, `#[cfg(unix)]` — **no se ejecuta en este Windows**).

**Integración:** `oauth_f2_consent` (9), `oauth_f2_adversarial_spec` (8), `oauth_f2_consent_adversarial` (7, V7c reescrito), `oauth_consent_file_adversarial` (7, V1/V2/V3 ajustados, ninguno ignorado).

## 5. Verificación ejecutada

| Comprobación | Comando | Resultado |
|---|---|---|
| Compilación (incluye tests) | `cargo check -p tylluan-kernel --tests` | OK (59,79 s) |
| Suite OAuth núcleo | `cargo test -p tylluan-kernel --test oauth_f2_consent` | **9 passed**, 0 failed |
| Suite adversarial spec | `cargo test -p tylluan-kernel --test oauth_f2_adversarial_spec` | **8 passed**, 0 failed |
| Suite adversarial consentimiento | `cargo test -p tylluan-kernel --test oauth_f2_consent_adversarial` | **7 passed**, 0 failed |
| Suite adversarial fichero | `cargo test -p tylluan-kernel --test oauth_consent_file_adversarial` | **7 passed**, 0 failed |
| Clippy | `cargo clippy -p tylluan-kernel --tests -- -D warnings` | **exit 0** (solo el aviso conocido `bundled-dashboard feature not enabled`) |
| Lib completa ×2 (secuencial) | `cargo test -p tylluan-kernel --lib` | **1173 passed / 0 failed / 4 ignored** (45,54 s y 48,85 s) |
| Parche contra `59d3eaa` | `git apply --check` en worktree prístino | **exit 0** |
| Compilación del parche aislado | `cargo check -p tylluan-kernel --tests` en ese worktree prístino | **exit 0** (2m 31s) |
| Parche contra `origin/main` actual | `git apply --check` en worktree prístino `3dd9dc6` | **exit 0** |

El parche entregado es `E:\tylluan-patches\deep\D-25.patch`: **29 617 B, 4 archivos, +355/−64**.

Nota sobre el recuento: la base `59d3eaa` da **1168** tests de lib con D-23 aplicado; D-25 añade **5** en Windows (el de `0600` es `#[cfg(unix)]`) → **1173**. El desfase de `scripts/check_test_count.sh` es preexistente y lo reconcilia el TL.

Una ejecución intermedia falló por una aserción mía (esperaba el escalón a los 6 fallos cuando el mapa 5-6 comparte 30 s); se corrigió el test, no el código. No es un fallo del producto.

## 6. Límites y lo que NO se ha hecho

- **No verificado en vivo** contra un kernel arrancado (prohibido arrancar procesos). Todo es in-process vía `build_router`.
- **El operador no es inmune a sus propios fallos**: si teclea mal la credencial 5 veces desde su IP, se bloquea esa IP 30 s. Lo que se elimina es el bloqueo por culpa de terceros (techo global), que era el vector real.
- **`127.0.0.1` es un solo bucket**: todos los clientes locales (Claude Desktop, Code, Cursor…) comparten IP. Cinco fallos de cualquiera bloquean el consentimiento desde loopback 30 s (escalando si insiste). No lo he eximido porque eximir loopback convierte el límite en decorativo para el atacante local; el proceso malicioso local ya tiene capacidades mayores de todas formas. **Decisión para el TL/José si se quiere exención explícita.**
- **F3 revoca también por corrupción transitoria**: si otro proceso escribe el fichero a medias y lo deja inválido, los consentimientos quedan revocados hasta que vuelva a ser válido. Es el precio del fail-closed y el peor caso es re-aprobar, no un acceso indebido.
- **F4 solo Unix**: en Windows no hay equivalente de `0600` en este código (las ACL por usuario son otro trabajo). `v5_file_replacement_and_permission_notes` sigue documentando la nota.
- **`MAX_PENDING = 128`** puede desalojar la petición de un cliente legítimo bajo flood (el cliente reintenta). No hay métrica expuesta de desalojos: solo `tracing::warn!`.
- No se ha tocado `dev_mode`, sus defaults, la credencial maestra ni `oauth.rs` más allá del `Retry-After`.
- No se ha tocado la UI de P1 (sigue sin decir cuánto falta para reintentar); el `Retry-After` está en la cabecera, no en el HTML.

## 7. Impact

**No-depredación (ADR-013):** no se elimina ninguna comprobación de autenticación; el único cambio de comportamiento en el camino de aprobación es que **un JSON inválido ahora revoca en vez de mantener** (más estricto, no menos) y que el `Retry-After` dice la verdad. El bloqueo por IP se mantiene y se refuerza con escalado; el techo global **deja de bloquear** (cambio de comportamiento deliberado y pedido por la cola: reduce disponibilidad-denegada a cambio de permitir más intentos agregados, que siguen acotados por el cupo por IP y por la ventana de 5 min).

**Zonas tocadas:** `crates/tylluan-kernel/src/transport/http/` (oauth y oauth_consent) y dos tests de integración. Nada en `guilds/`, `integrations/`, `tylluan*.toml`, ni en la configuración de `dev_mode`.

**Datos:** no se toca ninguna base ni fichero de datos real. Los tests usan directorios temporales propios y los borran.

**Riesgo residual:** el techo global ya no protege la disponibilidad de la pantalla frente a un flood distribuido; el coste de un fallo pasa a ser CPU/rate por IP (5 intentos por IP cada 30 s, escalando). Si un atacante rota IPs de socket reales, no hay límite agregado de intentos; sí lo hay de peticiones vivas (`MAX_PENDING`).
