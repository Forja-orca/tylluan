# IMPL M-BUFFY-7 — Tests que fallan si la auditoría de T-DSH-67 se pierde (sin tocar src/)

**Fecha:** 2026-10-24 · **Agente:** Buffy · **Estado:** entregado sin commitear (pendiente de Claude)
**Asignación:** tras el push de T-DSH-67 (5 llamadas `let _ = log_audit_entry(...)` con `let _ =`), el TL pidió tests que fallen si esa auditoría desaparece, sin tocar `src/`.

## Cobertura previa encontrada (verificada contra el código, no de memoria)

| Fila T-DSH-67 | Sitio | ¿Test conductora existía? | ¿Corre en CI? |
|---|---|---|---|
| 1 — approve_action | `handlers.rs:303` | Sí: `approve_audit_matrix` (exactamente-1 fila) | **No** (no está en ningún job) |
| 2 — tools kernel no-tylluan_do | `handlers.rs:609` | **No — ningún test** | — |
| 3 — dispatch tylluan_do | `outcome.rs:194` | Sí: test D-28 (lib, `handler_do/tests.rs`) | Sí (`--lib`) |
| 4 — MCP externo | `external_mcp.rs:188` | **No — ningún test** | — |
| 5 — silva/edge D-34 | `api_v1/mod.rs:779` | Sí: `cola_d34_acceptance` | **No** (no está en ningún job) |

Hallazgo lateral: el job `e2e-security-tests` de `ci.yml` sólo ejecuta 9 binarios `--test` nombrados; `approve_audit_matrix` y `cola_d34_acceptance` no corren en ningún gate (ni en `verify.sh`, que sólo corre `--lib`).

## Entrega (3 archivos, cero cambios en `src/`)

1. **`crates/tylluan-kernel/tests/audit_callsite_static_tripwire.rs`** — 7 tests:
   - 5 tripwires estáticos (una por fila): `include_str!` de `handlers.rs`, `outcome.rs`, `external_mcp.rs`, `api_v1/mod.rs` y exigen que la llamada siga en una **línea viva** (no comentada). Si se elimina/comenta/«limpia» la llamada → falla con el número de fila de T-DSH-67.
   - `writer_log_audit_entry_is_live`: la escritora `log_audit_entry` (audit.rs) existe.
   - `ci_runs_the_behavior_tripwires`: `ci.yml` sigue ejecutando los dos binarios nuevos (el gate no puede desaparecer por silencio).
2. **`crates/tylluan-kernel/tests/audit_behavior_mb7_rows.rs`** — 1 test (convención: un solo `#[test]` por binario porque usa la seam `TYLLUAN_AUDIT_DB`):
   - **Fila 2 (conductora nueva):** `ponder` contra `/mcp` real → espera fila con `guild='kernel'` y `tool='ponder'` en la DB de audit (la escritura va en `tokio::spawn`).
   - **Fila 4 (conductora nueva):** servidor MCP falso en `127.0.0.1:0` (initialize + tools/list + tools/call), registrado en `config.external_mcp` (misma caché que lee `try_external_mcp_dispatch`) y en la registry con `ensure_guild_running`. Luego `tylluan_do` **sin `guild_hint`** con intent gibberish que contiene el nombre del servidor → `resolve_guild_name` devuelve `NO_GUILD_MATCH` → fase 1 de `try_external_mcp_dispatch` → `do_external_call`. Espera la fila con `intent` exacto, `guild='external_mcp:mb7srv'`, `tool='mb7_probe'`, `agent='mb7-agent'`, `status='ok'`.
3. **`.github/workflows/ci.yml`** — `+ cargo test ... --test audit_callsite_static_tripwire` y `--test audit_behavior_mb7_rows` en el job `e2e-security-tests`.

## Verificación (salida real, 2026-10-24)

Verdes (revertida la mutación):

```
test mb7_rows_2_and_4_write_audit_rows ... ok
test result: ok. 1 passed; 0 failed; ... finished in 0.93s
running 7 tests ... (row1..row5, writer, ci_runs) all ok
test result: ok. 7 passed; 0 failed; ...
cargo clippy -p tylluan-kernel --all-targets -- -D warnings  →  Finished, exit 0
```

**Prueba de mutación (la esencia de la tarea):** comentada temporalmente la llamada de la fila 4 en `external_mcp.rs` (mutación local, después revertida — `git diff` de `src/` = vacío):

```
test row4_external_mcp_audit_call_is_live ... FAILED
M-BUFFY-7: la fila 4 de T-DSH-67 ... desapareció o quedó comentada. ...
test result: FAILED. 6 passed; 1 failed

test mb7_rows_2_and_4_write_audit_rows ... FAILED
  - FILA 4: external_mcp.rs no dejo fila de audit para el intent 'mb7srv zqxwjv flim ...'
    (status tylluan_do=200 OK, body={"..." "mb7 fake ok"})
test result: FAILED. 0 passed; 1 failed
```

Es decir: **capa estática y capa conductora ambas caen si la auditoría se pierde**, y la conductora además distingue «la llamada HTTP funcionó pero no se auditable» (status 200 + contenido correcto + fila ausente).

## Impact

- Si alguien elimina/comenta/renombra cualquiera de las 5 llamadas de auditoría (o la escritora), o saca los tests de `ci.yml`, el CI falla en `e2e-security-tests` con un mensaje que nombra la fila.
- Coste CI: ~2 s en local (la conductora tarda <2 s cuando todo va bien; sólo espera 10 s por fila si algo falla). Sin red externa: loopback `127.0.0.1` y DB temporal de proceso.
- No cambia comportamiento en runtime: cero ediciones en `src/` (verificado: `git diff -- crates/tylluan-kernel/src` vacío tras revertir la mutación).

## No verificado / deuda para el TL

- `approve_audit_matrix` (fila 1) y `cola_d34_acceptance` (fila 5) siguen **fuera de CI**: su cobertura conductora existe pero no gatea nada. Propongo añadirlos al mismo job; es decisión vuestra (no he tocado nada más de `ci.yml`).
- La fila 3 tiene cobertura conductora unitaria (D-28 sobre `post_process_outcome`) pero no un E2E `tylluan_do → guild vivo → outcome` en el harness: el camino exige seleccionar y ejecutar la tool del guild en Stage 1/2 (más frágil). El tripwire estático cubre la pérdida de la llamada.
- No corrí la suite `--lib` completa ni `verify.sh` (cambios cero en `src/`; sí `clippy --all-targets -D warnings` sobre todo el crate, verde).
