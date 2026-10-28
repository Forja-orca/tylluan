# INVENTARIO_REAL — PROGRESO (bitácora reanudable)

**Autora:** Buffy · **Worktree:** `E:\tylluan-wt-buffy` · **Formato:** tandas cortas (COLA_buffy_2026-10-27_tandas.md)
**Base actual:** `a564880` (= `origin/main` = kernel de producción reiniciado por Jose el 2026-10-28). Camino: `d79990a` → `ccbf3d5` → `a564880`. Mi trabajo ya está **comiteado en main** (`f378be4` cabeceras + informe M-BUFFY-13, `c879cb9` tests ADR-024 + informe S-BUFFY-1, `2142e57` fix del puerto en el informe).

## Bitácora

- 2026-10-28 01:20 · re-medición · HECHA · sincronizado a `a564880` (ff-merge tras guardar 3 parches en `E:\tylluan-patches\buffy\pre_a564880\` y re-aplicarlos: 8+/8-, 27+/0-, 8+/1- líneas reales intactas). Clippy all-targets **exit 0**; lib **1330 passed / 0 failed / 4 ignored**; ADR-024 **0 passed / 5 failed, exit 101**; 33 citas `fichero:línea` revisadas, 1 desplazada (`continuity_probes` assert `:413` → `:419-421`). Aviso en Coloquio: **T1931**.
- 2026-10-27 04:35 · tanda 0 · HECHA · `git fetch` + `git merge --ff-only origin/main` (d79990a → ccbf3d5, sin conflicto, entregables intactos). Cambios locales antiguos listados abajo (43 líneas reales, no descartados).
- 2026-10-27 04:06 · tanda 1 · HECHA · `docs/internal/IMPL_M-BUFFY-13_buffy_2026-10.md` §1 (9 cabeceras + 4 motivos; diff 13 ficheros, +121/−81; clippy exit 0).
- 2026-10-27 04:06 · tanda 2 · HECHA · `IMPL_M-BUFFY-13…md` §3 (6 rojos: 2 CRÍTICOS D-13/X1, 1 ALTA B2, 1 MEDIA t25_b2, 2 BAJA; `vias3_6` invertido explicado).
- 2026-10-27 04:06 · tanda 3 · HECHA · `IMPL_M-BUFFY-13…md` §2 y §4 (`knowledge_guild_test` NO a CI con 4 datos; supervisor.rs:41, propuesta de una línea sin aplicar).
- 2026-10-27 04:18 · tanda 4 · HECHA · `crates/tylluan-kernel/tests/adr024_containment_pending.rs` + `docs/internal/IMPL_S-BUFFY-1_buffy_2026-10.md` (5 tests, 4a A4 / 4b D1 / 4c B1 / 4d A1 / 4e C3; rojos: 0 passed / 5 failed, exit 101; normal: 0 passed / 5 ignored).
- 2026-10-27 04:40 · tanda 5 · HECHA · `docs/internal/INVENTARIO_REAL/inventario.json` (0 items, JSON válido) + `INDEX.md` + este `PROGRESO.md`.
- 2026-10-28 05:10 · tanda 6 · HECHA · `AREA_06_kernel.md` (24 módulos, ninguno huérfano; 3 hallazgos: cap de 20 % de CPU a guilds stdio, `/metrics` fuera de la release, `tylluan://skills` hardcodeado y duplicado).
- 2026-10-28 05:10 · tanda 7 · HECHA · `AREA_07_transporte.md` (249 registros de ruta vivos, 2 muertos; 13 métodos MCP; 4 métodos A2A; 6 grupos de alias). Hallazgos nuevos: T-03 recurso/prompt anunciados que no se sirven, T-06 la capa de 100 MB cae sobre 41 rutas equivocadas, T-04 `a2a_routes()` muerta, T-07 falta `tasks/result`.
- Próxima: **tanda 8 — Memoria**. Después 9 Seguridad, 10 Guilds, 11 Inferencia, 12 CLI, 13 Dashboard/UI, 14 CI, 15 Tests, 16 Docs, 17 Federación, 18 Empaquetado, 19 Benchmarks, 20 Flota, 21 Decisiones de José, 22 Cierre.

---

## Tanda 0 — cambios locales antiguos (NO descartar)

`git diff --ignore-space-at-eol --stat` sobre los tres: **43 inserciones / 9 borrados reales** (el resto es EOL; se normalizaron a LF, que es como están los blobs de `HEAD`).

| Fichero | Cambio real |
|---|---|
| `crates/tylluan-kernel/tests/regression_origin_host_outside_crate.rs` (16 líneas) | Cabecera actualizada: los 8 casos corren siempre desde `346cf65` (los 5 `#[ignore]` provisionales de 403 se retiraron) + separadores de bloque 403/200 |
| `docs/internal/IMPL_M-BUFFY-10_test_origin_host_buffy_2026-10.md` (+27) | Sección «Post-merge (2026-10-25): fix aterrizado en `346cf65`, 8/8 en verde» con la salida de test/clippy y la comprobación EN VIVO contra producción (403/415/200) |
| `scripts/coloquio_loop_buffy.py` (+9/−1) | `urllib.parse.quote` en `http_json`: un canal con acento (`#recomendación`) lanzaba `UnicodeEncodeError` dentro de `http.client` y mataba el wake entero |

Nada de esto se ha descartado ni mezclado con el trabajo nuevo.

---

## Estado de los entregables (verificado en disco, 2026-10-27)

| Entregable | Ruta | Existe |
|---|---|---|
| Informe M-BUFFY-13 | `docs/internal/IMPL_M-BUFFY-13_buffy_2026-10.md` | ✅ 16 359 B |
| Informe S-BUFFY-1 | `docs/internal/IMPL_S-BUFFY-1_buffy_2026-10.md` | ✅ 8 795 B |
| Tests ADR-024 (5, `#[ignore]`) | `crates/tylluan-kernel/tests/adr024_containment_pending.rs` | ✅ 13 027 B |
| Esqueleto inventario | `docs/internal/INVENTARIO_REAL/{inventario.json,INDEX.md,PROGRESO.md}` | ✅ (tanda 5) |

> Nota para el TL: los tres primeros se escribieron el 2026-10-27 entre 04:06 y 04:18 **en el worktree** `E:\tylluan-wt-buffy` (no en el checkout compartido `E:\tylluan`), por eso no aparecían al mirar fuera del worktree.

## Cifras medidas (base **`a564880`**, 2026-10-28, `CARGO_TARGET_DIR` propio)

- `cargo test -p tylluan-kernel --lib` → `1330 passed; 0 failed; 4 ignored` (43,8 s). En `d79990a` eran 1297: las 49 nuevas son trabajo del TL, mi cambio es solo texto.
- `cargo test -p tylluan-kernel --test adr024_containment_pending -- --ignored` → `0 passed; 5 failed`, exit 101 (los 5 pendientes de aprobación de ADR-024).
- **Histórico (base `d79990a`, ya superado):** lib 1297/4.
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → exit 0 (antes y después de añadir el fichero de ADR-024).
- `python scripts/check_doc_links.py` → exit 0 (184 enlaces / 432 `.md`).
- `grep -rn -E '^\s*#\[ignore' crates/` → 24 atributos (18 kernel tests, 2 evals, 4 src).

## Reglas activas

- Silencio de carga **levantado** el 2026-10-28. Se puede compilar, siempre con `CARGO_TARGET_DIR` propio (`E:\cargo-target\buffy`) y `-j 4`. **Nadie reinicia el kernel de producción** (lo hace Jose) y nadie borra `target/` sin aviso del TL.
- Solo lectura sobre producción; nada de instalar; 2-4 hilos; sin commit ni push; sin tokens en ficheros.

---

## 2026-10-28 · COLA nuevo (M-BUFFY-14 + tanda 6)

| Entregable | Fichero | Estado |
|---|---|---|
| M-BUFFY-14 informe | `docs/internal/IMPL_M-BUFFY-14_buffy_2026-10.md` | ✅ |
| M-BUFFY-14 tests (4, `#[ignore]`) | `crates/tylluan-kernel/tests/project_skills_store_mismatch.rs` | ✅ 4 rojos, 0 por fallo de montaje |
| Tanda 6 Kernel | `docs/internal/INVENTARIO_REAL/AREA_06_kernel.md` | ✅ 24 módulos |
| Tanda 7 Transporte | `docs/internal/INVENTARIO_REAL/AREA_07_transporte.md` | ✅ 249 rutas vivas, 13 métodos MCP, 4 A2A |
| Inventario | `docs/internal/INVENTARIO_REAL/inventario.json` | ✅ 17 filas (REAL 5 · A_MEDIAS 7 · MAL_PLANTEADO 3 · SIN_VERIFICAR 2) |

Base `a564880`, `CARGO_TARGET_DIR=E:\cargo-target\buffy`, `-j 4`.

- `cargo test -p tylluan-kernel --test project_skills_store_mismatch -- --ignored` → **0 passed; 4 failed**, exit 101, cada uno por el motivo esperado (lo guardado por MCP no sale en la lista; lo guardado por HTTP no se abre por `@skill:get`; el borrado no quita el `.md`; `DELETE /api/v1/skills/<name>` da 404).
- Hallazgos nuevos de la tanda 6: `process_isolation` (20 % de CPU fijo a todo guild stdio, tras handshake), `/metrics` fuera de las features por defecto, `tylluan://skills` hardcodeado y duplicado.
- Tanda 7: 249 registros de ruta vivos (`routes.rs` 230 + `sse.rs` 3 + `mod.rs` 16), 2 muertos (`a2a_routes()`), 13 métodos MCP, 4 métodos A2A, 6 grupos de alias (13 rutas / 6 handlers). Seis filas nuevas en `inventario.json` (T-03, T-04, T-05, T-06, T-07) más T-01/T-02/T-08/T-09. Lectura pura: **nada ejecutado en vivo**.
- Siguiente: tanda 8 (memoria).
