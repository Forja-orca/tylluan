# COLA Buffy — 2026-10-25 (la envia Jose; base origin/main cb41c03)

Contexto: T-DSH-85 (turno 1760): el kernel NO valida `Origin`/`Host` en /messages ni /sse (spec MCP 2025-06-18: MUST validate Origin, anti DNS-rebinding). DSH implementa el middleware en T-DSH-86; Deep exige application/json en D-59.

## M-BUFFY-10 — test de regresion FUERA del crate (rojo hoy, verde cuando aterrice T-DSH-86)
1. Fichero nuevo `crates/tylluan-kernel/tests/regression_origin_host_outside_crate.rs`, mismo patron que `regression_d55_d56_outside_crate.rs` (API publica `build_router` + `support::test_state_custom`; sin lanzar kernel).
2. Casos (8): POST /messages con Origin http://evil.example -> debe ser 403; Origin evil + text/plain -> 403 y el tool NO se ejecuta; GET /sse con Origin evil -> 403; Host evil.example sin Origin -> 403; OPTIONS preflight con Origin evil -> 403; Origin http://127.0.0.1:<puerto> -> 200; Origin http://localhost:<puerto> -> 200; SIN Origin (cliente no navegador) -> 200. Cada uno con dev_mode=true y dev_mode=false con Bearer.
3. Hoy los casos 403 FALLAN: marca esos tests con `#[ignore = "bloqueado por T-DSH-86"]` y documenta en el informe el rojo medido (comando y salida). Quitaremos el ignore cuando aterrice el fix.
4. Aceptacion: `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` exit 0; `cargo test -p tylluan-kernel --test regression_origin_host_outside_crate` -> passed los verdes, N ignored; `--ignored` -> los 403 fallan (rojo real).
5. NO anadas el test a ci.yml todavia. Entrega: diff SIN commitear + `IMPL_M-BUFFY-10_*.md` con `## Impact` y "No verificado"; resumen `[COLA buffy]` en Coloquio.
Si te atascas: para, publica el error literal en Coloquio.
