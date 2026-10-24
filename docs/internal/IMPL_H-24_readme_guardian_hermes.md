# IMPL_H-24 — Guardián determinista del README (hermes)

Entrega de H-24 (T1663 + COLA@6b50ba6). Árbol probado: `7b24da2` (origin/main real). Sin commits; el TL integra.

## Qué se entrega

1. `scripts/check_readme_claims.sh` — gate read-only (no toca READMEs, no corre cargo, no toca red). Verifica 5 categorías, una línea `OK`/`DRIFT` por afirmación, exit 1 si hay deriva:
   - **tools**: el recuento soberano de `tools.rs` (`sovereign_tool_names`, fuente única CONTRACT-01) vs los nombres `tylluan_*` listados en ambos READMEs, más membership de cada nombre en el código y la prosa «six/seis tools».
   - **tests**: suma por componente de AGENTS.md internamente consistente + cifra total contra la afirmación de cada README. La verificación *en vivo* contra `cargo test` sigue siendo dominio de `check_test_count.sh` en CI (no duplicado ni cableado aquí).
   - **version**: badges `version-X.Y.Z` de ambos READMEs vs `version = "0.17.1"` del `Cargo.toml` raíz (línea 5 aproximada; lo lee el propio script).
   - **guilds**: todo token `` `*.py` `` citado en los READMEs debe existir bajo `guilds/` (hoy ningún README cita guilds por ruta → el check queda preparado, reporta «nothing to check»).
   - **links**: enlaces markdown relativos (sin anclas, sin http) deben existir en disco.

2. Este informe.

## Salida real (bash scripts/check_readme_claims.sh sobre main 7b24da2, 2026-10-24 ~05:00)

```
OK tools: tools.rs declares 6 sovereign tools (source of truth)
OK tools: README.md lists exactly 6 sovereign tool names
OK tools: README.es.md lists exactly 6 sovereign tool names
OK tests: AGENTS.md component sum 1242+75+12=1329 is internally consistent
OK tests: README.md claims 1329 tests; AGENTS.md total is 1329
DRIFT tests: README.es.md claims 1125 tests but AGENTS.md total is 1329
OK version: README.md version badges match Cargo.toml (v0.17.1)
OK version: README.es.md version badges match Cargo.toml (v0.17.1)
OK guilds: README.md cites no guild .py files by path (nothing to check)
OK guilds: README.es.md cites no guild .py files by path (nothing to check)
OK links: README.md: 20 relative file links exist, 0 broken
OK links: README.es.md: 21 relative file links exist, 0 broken
RESULT: 1 DRIFT line(s) — README claims diverge from the tree.
EXIT=1
```

## Hallazgo en el primer run (deriva real, no ruido)

`README.es.md`, línea 200: «1125 tests en el kernel de Rust (lib), `tylluan-link`, y `tylluan-fsrs` — todos en verde.» — es la traducción directa de `README.md`, línea 198: «1329 tests across Rust kernel (lib), `tylluan-link`, and `tylluan-fsrs` — all green.» La traducción se quedó 204 tests atrás. **Causa del ciego:** `check_test_count.sh` gatea `README.md` y `AGENTS.md`, pero no `README.es.md` — el gap exacto que el guardián venía a cerrar. Fix propuesto (no lo aplico; editar READMEs está excluido de H-24): cambiar 1125→1329 en README.es.md línea 200 y opcionalmente extender `check_test_count.sh --fix` al es.

## ## Impact

- Zona tocada: `scripts/` (añade un fichero nuevo; no modifica ninguno existente). ADR-013 no-depredación: no toca `transport/`, `guilds/`, `integraciones` ni `tylluan*.toml`; no lee ni escribe ficheros del kernel; solo lectura de docs/código. Preguntas: ¿destruye WIP ajeno? No (read-only, sin writes). ¿Añade dependencia? No (bash+coreutils+find+grep, nada cloud). ¿Cableado en verify.sh? Deliberadamente no — lo decide el TL (exclusión explícita de la spec).
- Riesgo bajo: el script puede dar falsos DRIFT si un README mueve la frase de tests antes del patrón (toma la primera coincidencia `[0-9]+ tests`) o si una ruta con espacios aparece en enlaces. Ambos se ven al ejecutarlos, nunca silenciosos.

## No verificado

- Conteos EN VIVO de tests (no corrí cargo; la fuente canon sigue siendo check_test_count.sh en CI — este guardián hace coherencia cruzada estática).
- Anchors internos (`#seccion`) no se validan; solo enlaces a ficheros (spec: «enlaces a ficheros que existan»).
- Prosa de posicionamiento y claims de benchmarks (tabla LongMemEval, métricas de arranque en frío): fuera por spec (lo decide José) — el guardián no los toca.
- `check_no_predation.sh` no corrí (no hay commit que correr; lo hará el TL al integrar).
