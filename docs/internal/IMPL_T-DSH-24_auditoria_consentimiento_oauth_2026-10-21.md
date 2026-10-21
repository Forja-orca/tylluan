# T-DSH-24 — Auditoría del consentimiento OAuth tras D-25 (solo lectura + tests)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** worktree `E:\tylluan-wt-dsh` en `--detach origin/main` (`a14dcdd`); limpio antes de empezar (T-DSH-23 ya integrada; copias idénticas descartadas).
- **Encargo (turno 1504):** auditar el consentimiento OAuth tras D-25, con los tests `oauth_f2_consent_adversarial` y `oauth_consent_file_adversarial` ya en main: (1) V7c debe seguir probando que 20 fallos desde 20 IPs distintas NO bloquean a una IP nueva ni al operador legítimo; (2) con `127.0.0.1` como único bucket, cuántos fallos de terceros locales bloquean al operador y por cuánto tiempo (tabla con el escalado 30/60/120/300 s); (3) `MAX_PENDING=128`: ¿un atacante local que abre 128 pendientes echa fuera el pendiente legítimo? mide; (4) `consents.json` inválido ⇒ fail-closed revoca en memoria: caso de ida y vuelta (corrompo, arreglo, ¿vuelven los consentimientos?). Tabla caso × esperado × real; `#[ignore]` con motivo si falla; informe con `## Impact` y no verificado.

## Entrega

`crates/tylluan-kernel/tests/oauth_consent_audit_t24.rs` — 4 tests, **4 passed / 0 failed**; ninguna fila necesitó `#[ignore]` (lo medido coincide con lo que el código promete). Además se ejecutaron las suites que ya estaban en main: **7 + 9 + 7 passed**.

```text
rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_consent_audit_t24
# test result: ok. 4 passed; 0 failed (3,4 s)
rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_f2_consent_adversarial --test oauth_consent_file_adversarial --test oauth_f2_consent
# 7 + 9 + 7 passed; 0 failed
```

## Tabla caso × esperado × real

| # | caso | esperado | real | veredicto |
|---|---|---|---|---|
| 1 | 20 fallos desde 20 IPs distintas (tercero), luego operador `127.0.0.1` e IP nueva | ninguno de los dos bloqueado | `is_locked(127.0.0.1)=false`, `fresh=false`, `retry_after=0` | OK (V7c sigue en pie) |
| 2 | fallos propios en el bucket `127.0.0.1`, 1→12 | escalado 0/0/0/0/30/30/60/60/120/300/300/300 s | exactamente eso (real medido = esperado) | OK |
| 2b | lo mismo con techo global saturado (≥ 20 fallos) | +2 s solo a quien ya falló | `302 s`; IP nueva sin bloqueo | OK |
| 3 | pendiente legítimo + 128 pendientes de atacante (legítimo primero) | el tope acota memoria; el más antiguo (el legítimo) se desaloja | `count=128`, legítimo `alive=false` | OK según diseño (DoS de ventana) |
| 3b | 128 del atacante y luego el legítimo | sobrevive el legítimo; cae el más antiguo del atacante | `legítimo=true`, `primer_atacante=false`, `count=128` | OK |
| 4 | `consents.json` corrupto → `valid_consent` | fail-closed: revocado en memoria | `revoked=true`; y la consulta **no** reescribe el fichero roto | OK |
| 4c | arreglo el fichero con el contenido válido | los consentimientos vuelven (reload por mtime) | `returned=true` | OK |
| 4b | corrompo y **apruebo otro cliente** antes de arreglar | el estado en memoria (vacío) se persiste | el consentimiento anterior **no** está en disco | observación (ver O2) |

## Hallazgos

- **F1 — V7c se mantiene.** El techo global (`GLOBAL_MAX_FAILURES = 20`) ya no bloquea por sí mismo: `lock_until` devuelve `None` si la IP no tiene fallos propios (`oauth_consent.rs`, línea 486-490), y el extra `GLOBAL_THROTTLE_EXTRA_SECS = 2` solo se suma a quien ya está bloqueado. Verificado en unidad (20 fallos/20 IPs ⇒ operador e IP nueva libres) y con el test de router `v7_forwarded_for_does_not_bypass_the_rate_limit` (sub-caso c) que sigue verde: la aprobación legítima desde una IP sin fallos propios completa con `307` + `code`.
- **F2 — En un despliegue solo-local el bucket de la IP es el del operador.** Con `127.0.0.1` como único bucket, **5 fallos de terceros locales bastan para dejar al operador 30 s fuera**; el escalado sube 60 s (7-8), 120 s (9) y **300 s (10+)** por fallos propios en la ventana de 300 s (`lockout_secs`, `oauth_consent.rs`, línea 53-61; `record_failure`/`lock_until`, línea 477-532). Con el techo global saturado el bloqueo del operador es 302 s. No hay exención de loopback: la decisión de eximir loopback en el consentimiento está abierta y es de José (la mencionaste como decisión pendiente de D-24); aquí queda el dato medido, sin recomendación.
- **F3 — `MAX_PENDING=128` acota memoria, y el pendiente legítimo es desalojable.** El desalojo es del más antiguo (`create_pending`, `oauth_consent.rs`, línea 406-422): si el legítimo se creó antes del flood, cae él; si se crea después, sobrevive y cae el pendiente más antiguo del atacante. Consecuencia: un atacante local que abra 128 pendientes en la ventana de 5 minutos puede tirar la petición legítima en curso (el cliente reintenta; **no** se concede ningún consentimiento sin credencial). Es el compromiso declarado del tope; no hay prioridad por origen.
- **F4 — Fail-closed correcto, con un matiz de recuperación.** Con el fichero corrupto, `reload_if_changed` limpia el estado en memoria (`oauth_consent.rs`, línea 242-249) y `valid_consent` devuelve `None` **sin reescribir** el fichero roto (el operador conserva la evidencia y puede arreglarlo). Al arreglarlo con contenido válido, el reload por `mtime` restaura los consentimientos: **ida y vuelta OK**.
- **O1 — `mtime` es la única señal de recarga.** La detección depende de `modified()`; el test usa esperas de 1,1 s (un operador real tarda más). No se probó el caso de dos escrituras en el mismo tick de reloj (ver «no verificado»).
- **O2 — El fail-closed es en memoria: cualquier `persist` posterior hace permanente la pérdida.** Si tras corromper el fichero se aprueba *cualquier* consentimiento nuevo, `persist` reescribe `consents.json` desde el estado ya vacío y el consentimiento anterior desaparece del disco (no solo queda revocado). Recuperación garantizada solo si el operador arregla el fichero **antes** de que ocurra otro `persist`. Es coherente con el diseño («el peor caso es volver a pedir consentimiento»), pero conviene saberlo: en producción, un JSON corrupto + una aprobación nueva = consentimientos previos perdidos.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test --test oauth_consent_audit_t24` | **4 passed / 0 failed** (3,4 s), matriz de 8 filas |
| `cargo test --test oauth_f2_consent_adversarial --test oauth_consent_file_adversarial --test oauth_f2_consent` | **7 + 9 + 7 passed / 0 failed** |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1173 passed / 0 failed / 4 ignored** (coincide con AGENTS.md; sin cambios: solo test de integración) |

## Impact

- **Diff sin commitear: solo tests** — un fichero nuevo (`tests/oauth_consent_audit_t24.rs`) y este informe. **Cero cambios en `src/`**; no se modificó ningún test existente; no se tocó el kernel `:47004` ni `E:\tylluan`; sin secretos (los tests usan stores en memoria y directorios temporales propios en `%TEMP%\tylluan_t24_*`).
- Coste CI: +3,4 s. Los tests de fichero hacen 2-3 esperas de 1,1 s a propósito (resolución de `mtime`); no hay flakiness observada en 2 corridas.
- **Dato para decisión abierta (José/TL), no cambio de producto:** en despliegue solo-loopback el operador comparte bucket con cualquier proceso local (5 fallos = 30 s, 10 = 300 s), y el pendiente legítimo puede ser desalojado por un flood de 128. Ambos comportamientos son los que el código implementa hoy; el informe no decide sobre la exención de loopback ni sobre priorizar pendientes legítimos.

## No verificado

- **Camino de router para (2):** la tabla de escalado se midió en la unidad (`ConsentStore::record_failure` / `retry_after_secs`); que el handler HTTP aplique el `Retry-After` de esos segundos al operador bloqueado por fallos locales **no** se ejercitó con router (el V7c existente cubre la IP nueva/techo global, no ese caso). Replicarlo exige montar el router de consentimiento con cookie de sesión (harness de `oauth_f2_consent.rs`).
- **Recarga por `mtime` en el mismo tick:** no se probó corromper y arreglar dentro del mismo instante (depende de la resolución del sistema de ficheros); el test usa 1,1 s. Si la resolución fuese gruesa, una revocación seguida de arreglo inmediato podría no detectarse hasta el siguiente cambio de `mtime`.
- **Ventana de 300 s y expiración:** no se esperó a que caduquen los fallos ni los pendientes (sin time-travel): la retirada por ventana se leyó en el código, no se observó vencer.
- **Concurrencia del tope de pendientes:** la medición de `MAX_PENDING` es secuencial (un hilo); no se estresó con creación concurrente (existe `v4_concurrent_approvals_do_not_lose_or_duplicate_records` para el fichero, no para el tope).
- **Proxy real:** el bucket es la IP del socket (no `X-Forwarded-For`), ya cubierto por V7a/V7b; no se probó detrás de un proxy real que colapse todo en una IP (mismo efecto que el caso loopback).
- **No se ejecutó `verify.sh` completo** (solo el subconjunto Rust de consentimiento + clippy + lib).
