# IMPL H-32 — sincronización de ROADMAP.md y STATUS.md con los cierres del 24/25-oct (hermes)

Solo lectura + diff sin commitear, base `5d21cc7` (origin/main al empezar; empecé sobre `346cf65` y rebaseé al ver que main avanzó: e6e19c5, 31b965c, 4e2304e, 5d21cc7).

## Cambios

1. **ROADMAP.md** — nueva subsección "Closures of 2026-10-24 and 2026-10-25 (verified by the TL)" tras la de 2026-10-22/23: 11 afirmaciones (T-DSH-86/D-59/M-BUFFY-10, T-DSH-87, D-58+D-56, D-57, T-DSH-81, T-DSH-74, T-DSH-79, flaky fdb4c72, T-DSH-83/84, guardian R4 H-31, cifras de tests). Cada una con ancla `fichero:linea` verificada por grep y por el guardian R4.
2. **STATUS.md** — línea "Kernel en ejecución": `93d7cb1` → `31b965c` con evidencia propia (curl `/health` en vivo), y sumario de cierres apuntando a la sección nueva del ROADMAP.

## Verificación (mandé todo a grep/sedes reales antes de escribir)

- `validate_mcp_origin_host_middleware` auth.rs:751; `mcp_origin_guard_tests` auth.rs:1411 (se movió +10 con T-DSH-87 — re-ancorado tras el rebase); wiring mod.rs:1522; `reject_non_json_content_type` mcp.rs:284; `c8_post_messages_without_origin_200` regression_origin_host_outside_crate.rs:380; bloque T-DSH-87 auth.rs:761; `federation_approve_peer` api_federation.rs:323; d56 sin ignore regression_d55_d56_outside_crate.rs:167 (el `-1` de d1a375c es el `#[ignore]` eliminado); `persist_contract` api_contracts.rs:240; `enable_wal_best_effort` config.rs:1569 + `busy_timeout` 1555; `audit_or_warn` audit.rs:142; fail-closed nodes.rs:2153; flaky coherence_gate.rs:1885; `resolve_token` kernel_token.py:24.
- Guardian R4 sobre mis ediciones: **exit 0, 28 anclas OK** (antes de mis cambios: 13 OK).
- `bash scripts/check_test_count.sh` contra `346cf65`: **exit 0, "1374 … all green"** — el spec citaba "1364"; es cifra previa al des-ignorado de D-56 y a las suites de T-DSH-86. Puse 1374 con evidencia y nota de la discrepancia.
- Kernel vivo: `/health` → commit `31b965c`; POST `/messages` con `Origin: http://evil.example` → **403 `forbidden_origin`** (prueba en vivo propia).

## Impact

Docs únicamente — no toca transport/, guilds/, integrations/ ni toml; la puerta anti-depredación no se activa. El riesgo es documental: si una ancla decae, R4 la caza en CI cuando el TL la cablea. La sección nueva queda como destino canónico de "¿qué se cerró el 24/25-oct?".

## No verificado

- Que 31b965c/4e2304e no cambiaran el recuento de tests (no reejecuté el conteo en `5d21cc7`; el TL sincronizó su línea a 1374).
- Estado de CI en `5d21cc7` (el TL afirmó verde en push 158; no consulté GitHub Actions).
- Números de push 159/160: usé hashes de commit porque desconozco la cuenta de pushes del TL tras 158.

## Nota al TL

`.cargo/config.toml` que genera el protocolo de worktrees guarda `target-dir = "E:\cargo-target\hermes"` — TOML inválido (falta escapar), rompe cualquier cargo en el worktree hasta corregirlo a `"E:/cargo-target/hermes"`. Lo arreglé solo en mi worktree; el script es tuyo.
