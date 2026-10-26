# IMPL H-37 — ventana de arranque del CLI acorde al preload (hermes)

Spec T1821 (TL, verbatim vía `GET /turn/1821`): "Arregla en tylluan-cli: ventana de espera acorde (NO reducir ningun timeout del kernel/guilds; solo ampliar la del CLI, p.ej. hasta que /health responda o 180s) + test o reproduccion con kernel test-run-* en puerto propio. Base origin/main tras push163. Diff sin commitear + informe con ## Impact y 'no verificado'." Base real usada: `0a8a29c` (T1838 fija "base 0a8a29c"; push165b ya publicado).

## Diagnóstico (verificado contra el código, 0a8a29c)

- `crates/tylluan-cli/src/main.rs:239-252` (pre-cambio): bucle fijo `for i in 1..=30`, 1 poll/s al `/health` del puerto pedido; al agotar los 30 prints imprimía "⚠️ Kernel started but not ready within 30s" y el CLI terminaba. Con kernel default-profile (preload mxbai) el arranque real midió ~52 s (H-35, 2x reproducido, Xeon E5-2680 v4) → el CLI abandonaba un arranque sano.
- Corrección a mi propio hallazgo H-35: escribí "el Job Object del CLI mata el kernel". grep completo del crate (`kill_on_drop|CREATE_NEW|Job|creation_flags|process_group`) = **0 hits** en `crates/tylluan-cli/src/` — el CLI no tiene Job Object. La muerte observada del kernel hijo vino de la capa shell (MSYS/job del comando), no del código CLI. El fix no depende de la causa de la muerte: con ventana suficiente el CLI ya no abandona el arranque.
- El spawn hijo y `find_kernel_exe` (main.rs:1358+: exe del kernel junto al CLI, luego `~/.tylluan/bin`) quedan intactos.

## Cambio (diff sin commitear, solo `crates/tylluan-cli/src/main.rs`, +113/−13)

1. `const STARTUP_WAIT_SECS: u64 = 180;` con comentario citando la medición H-35 (52 s cold preload) y el porqué seguro: ampliar la ventana del lado CLI no baja ningún timeout del kernel ni de guilds; sigue 1 poll/s con 2 s de timeout HTTP por request.
2. Extraído a `async fn wait_for_health(client, url, cap_secs) -> bool`: sondea cada 1 s hasta 2xx o cap; informa cada ~15 s el tiempo restante (DX: el usuario sabe que el arranque sigue vivo).
3. El brazo `Start` llama a `wait_for_health(..., STARTUP_WAIT_SECS)`; en timeout, el mensaje dice explícitamente que un preload cold puede legítimamente pasar de un minuto y sugiere `tylluan-cli status` para re-check — sin matar ni asumir fallo.
4. No se tocó ningún otro timeout del crate (los `from_secs(2)`/`from_secs(5)`/`from_secs(30)` restantes pertenecen a health-check de `status`/`doctor`/requests de setup, no a la ventana de arranque; y el `from_secs(300)` es de benchmark) — ningún timeout de kernel/guild modificado.

## Tests añadidos (mismo fichero, `mod startup_wait_tests`)

- `startup_wait_window_exceeds_cold_preload`: guarda la regresión — `STARTUP_WAIT_SECS` debe superar los ~52 s medidos (afirmado en 180).
- `wait_for_health_honors_cap_when_nothing_listens`: sin listener → `false`, y respeta el cap (≥2 s y ≤5 s con cap=2) — contrato temporal paramétrico.
- `wait_for_health_turns_ready_once_the_server_answers`: servidor one-shot (std thread, 200 a la primera conexión) → `true` en el primer poll (1-3 s con cap=30).

Resultados de `cargo test -p tylluan-cli` y `cargo clippy -p tylluan-cli --all-targets -- -D warnings`: <!-- RESULT:TESTS -->

## Reproducción end-to-end con binario real (kernel `test-run-*`, puerto propio 47016, NUNCA 47004)

Plan: CLI de este diff (dev build) + kernel publicado v0.17.1 (sin cambios de código) en un dir de scratch nuevo con `FASTEMBED_CACHE_DIR` apuntado a la caché; `tylluan-cli start --port 47016` sin profile portable → el arranque cold (~52 s) debe completar con "ready" en vez del abandono a los 30 s del código viejo.

Resultado real: <!-- RESULT:REPRO -->

## Impact

Solo el lado CLI del comando `start`: usuarios con arranques lentos (preload cold, discos lentos, AV escaneando) dejan de ver "not ready within 30s" y de perder el kernel durante un arranque sano. Ninguna semántica más cambia: `stop`/`status`/`doctor`/`install` intactos; ningún timeout del kernel o guilds tocado (prohibición explícita del TL respetada — verificado con grep de la diff: los únicos `from_secs` añadidos son el cap 180 y sleeps/timeout preexistentes de poll). Riesgo acotado: un `start` sobre un kernel REALMENTE muerto ahora bloquea hasta 180 s en vez de 30 s antes de devolver (mitigado por el reporte de cuenta atrás cada 15 s; Ctrl+C sigue disponible). Sin cambios en transporte/guilds/toml → no aplica gate ADR-013, se declara igualmente.

## No verificado

- La causa exacta de la muerte del kernel-hijo en H-35 (shell-job de MSYS vs lifetime del parent en cmd puro): no la persigo porque el fix no depende de ella; lo corrijo explícitamente arriba.
- Reproducción en Windows puro sin bash (el repro se hace desde bash de git; la capa shell podría influir en el lifetime del hijo).
- Comportamiento con `--setup` wizard interactivo previo (no cubierto por el repro).
