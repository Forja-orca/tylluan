# IMPL_M-BUFFY-10 — test de regresión FUERA del crate: validación Origin/Host en /messages y /sse

**Cola:** `docs/internal/COLA_buffy_2026-10-25.md` (enviada por José). **Base:** `origin/main` `cb41c03` (reset con `agent_wt.sh reset buffy`, previo guardado en `E:\tylluan-patches\buffy\_saved\20261025_173323.*` + stash). **Relacionado:** T-DSH-85 (hallazgo, turno 1760) y T-DSH-86 (fix que está implementando DSH, turno 1762).

## Qué se entregó

Fichero nuevo `crates/tylluan-kernel/tests/regression_origin_host_outside_crate.rs` (único cambio; `ci.yml` NO se toca, según la cola). Mismo patrón que `regression_d55_d56_outside_crate.rs`: API pública `build_router` + `support::test_state_custom(tag, Some(MASTER), dev_mode)`, peticiones con `tower::ServiceExt::oneshot`, sin lanzar kernel.

8 casos (uno por test), **cada uno en ambos modos** (`dev_mode=true` y `dev_mode=false` con Bearer maestro) = 16 peticiones. Todos envían `Host` (loopback, salvo el caso 4) y `Authorization: Bearer` (irrelevante en dev; el fix "no exime por tener auth válida"):

| # | Caso | Esperado | Estado hoy |
|---|------|----------|------------|
| 1 | POST /messages, `Origin: http://evil.example` | 403 | ROJO → `#[ignore]` |
| 2 | POST /messages, `Origin` evil + `Content-Type: text/plain` (simple, sin preflight) | 403 **y tool sin ejecutar** | ROJO → `#[ignore]` |
| 3 | GET /sse, `Origin` evil | 403 | ROJO → `#[ignore]` |
| 4 | `Host: evil.example` SIN Origin | 403 | ROJO → `#[ignore]` |
| 5 | OPTIONS preflight, `Origin` evil | 403 | ROJO → `#[ignore]` |
| 6 | POST /messages, `Origin: http://127.0.0.1:47004` | 200 + catálogo tools | VERDE (corre siempre) |
| 7 | POST /messages, `Origin: http://localhost:47004` | 200 + catálogo tools | VERDE (corre siempre) |
| 8 | POST /messages SIN Origin (cliente no navegador) | 200 + catálogo tools | VERDE (corre siempre) |

Detalle del caso 2: hace primero una llamada de **control** idéntica SIN Origin (debe ser 200; extrae el texto del resultado de `whoami` como marcador) y después la petición `text/plain` con `Origin` evil. Assertions: 403 **y** que el body 403 no contenga el marcador del control (si lo contiene, el tool se ejecutó igualmente).

El caso 4 cumple la condición del fix: la config de test es `TylluanConfig::default()` (no hay `tylluan.toml` en el paquete) y `default_host() = "127.0.0.1"` (config.rs:2025, invariant test en config.rs:2533), es decir, kernel "enlazado a loopback" → T-DSH-86 debe validar Host.

Los 5 tests en rojo llevan `#[ignore = "bloqueado por T-DSH-86"]`. **Cuando T-DSH-86 aterrice: quitar los 5 `#[ignore]` y este fichero pasa a gatear el fix.**

## Rojo medido (HOY, sin el fix)

```
$ cargo test -p tylluan-kernel --test regression_origin_host_outside_crate -- --ignored
running 5 tests
test c4_host_evil_without_origin_403 ... FAILED
test c2_post_messages_evil_origin_text_plain_403_tool_not_executed ... FAILED
test c5_options_preflight_evil_origin_403 ... FAILED
test c1_post_messages_evil_origin_403 ... FAILED
test c3_get_sse_evil_origin_403 ... FAILED
...
c1: [dev=true] POST /messages Origin evil => 200 OK (esperado 403); body: {"id":1,...,"tools":[...tylluan_do...
c2: [dev=true] text/plain + Origin evil => 200 OK (esperado 403...); body: {...,"content":[{"text":"...\"agent_id\": \"anonymous\"...
c3: [dev=true] GET /sse Origin evil => 200 OK (esperado 403); body: <body en stream sin fin (SSE)>
c4: [dev=true] Host evil sin Origin => 200 OK (esperado 403); body: {...,"tools":[...
c5: [dev=true] OPTIONS preflight Origin evil => 200 OK (esperado 403); body: 
     (idem en [dev=false] en los 5 casos)
test result: FAILED. 0 passed; 5 failed; 0 ignored; 3 filtered out; finished in 6.35s
error: test failed, to rerun pass `-p tylluan-kernel --test regression_origin_host_outside_crate`
EXIT=101
```

Lectura: los 5 casos devuelven **200 en AMBOS modos** (también con `dev_mode=false` + Bearer válido: la validación de origen no existe, no es un problema de auth). En c2 el `tools/call whoami` **se ejecuta** devuelve resultado completo; en c3 `/sse` abre el stream.

## Aceptación (verificado en mi worktree, 2026-10-25)

```
$ cargo test -p tylluan-kernel --test regression_origin_host_outside_crate
test result: ok. 3 passed; 0 failed; 5 ignored; 0 measured; 3 filtered out; finished in 21.44s   (exit 0)

$ cargo test -p tylluan-kernel --test regression_origin_host_outside_crate -- --ignored
test result: FAILED. 0 passed; 5 failed; ...   (exit 101)   ← rojo real, arriba la salida

$ cargo clippy -p tylluan-kernel --all-targets -- -D warnings
Finished `dev` profile ... (exit 0)
```

Base `cb41c03`; `ci.yml` intacto (el test NO está en CI todavía, punto 5 de la cola).

## Incidencia de proceso (corregida en el mismo paso)

La primera escritura del fichero de test cayó por ruta relativa en el **checkout compartido** `E:\tylluan` (la tool de escritura resuelve contra `/e/tylluan`). Detectado antes de compilar: el fichero se movió a `E:\tylluan-wt-buffy\crates\tylluan-kernel\tests\` y se verificó que el compartido volvió a su estado previo (su único cambio restante, `M crates/tylluan-kernel/src/security/coherence_gate.rs`, **no es mío** y no se tocó). Ninguna otra escritura salió del worktree.

## Impact

- Aporta la red de regresión externa del fix T-DSH-86: hoy documenta en rojo reproducible que cualquier página web puede ejecutar `tools/list`/`tools/call` y abrir `/sse` contra el kernel sin token (DNS-rebinding, spec MCP 2025-06-18); al aterrizar el middleware pasa a gatear 403 en los 5 casos sin romper clientes sin Origin (caso 8) ni orígenes loopback (6-7).
- Cero cambios en `src/` y cero cambios en `ci.yml`: no afecta a ninguna ruta de auth ni a la federación; el único artefacto es un fichero de test nuevo que hoy corre con 3 tests y 5 ignorados.
- Riesgo de falso verde controlado: los casos 200 exigen catálogo `tools` en el body (un fallback SPA con 200 no pasaría) y el caso 2 exige que el body 403 no contenga el resultado del tool de control.

## No verificado

- No ejecuté el fix T-DSH-86 (no existe aún en `cb41c03`; DSH lo está implementando): los 5 `#[ignore]` no se han visto pasar en verde — la verificación de que "pasan al aterrizar" es responsibility de quien quite los `#[ignore]` (yo o quien aterrice el fix), y conviene repetir `--ignored` en ese momento.
- Sin kernel real ni navegador: igual que T-DSH-85, todo es router en proceso (`oneshot`); no se ejecutó un ataque de DNS-rebinding real ni se comprobó el bloqueo en un navegador.
- El "tool NO se ejecuta" del caso 2 se asserts de forma indirecta (403 + ausencia del marcador del control), no instrumentando el propio tool.
- No se probaron `/mcp`, `/api/v1/mcp` ni `/sse/download` (comparten handler/middleware, pero la cola solo pedía `/messages` y `/sse`), ni `allowed_origins`/`allowed_hosts` (T-DSH-86 aún no los define en `config.rs`).
- Sin `cargo test --lib` completo ni `verify.sh` (~10 min): los criterios de la cola eran clippy + este binario de tests; el binario nuevo y sus dependencias compilan limpios, pero la suite completa no se corrió en esta entrega.
