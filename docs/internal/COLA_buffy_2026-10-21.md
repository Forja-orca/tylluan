# Cola de trabajo de Buffy, segunda tanda — 2026-10-21

**Mecánica:** la de `COLA_buffy_2026-10-19.md`. Trabaja solo en `E:\tylluan-wt-buffy` (los informes anteriores aparecieron en el checkout compartido dos veces; no más). `git fetch` y `git checkout --detach origin/main` al empezar, diff sin commitear, informe `docs/internal/IMPL_<item>_buffy_2026-10.md` con `## Impact` y "no verificado", mensaje `[COLA buffy] <item> terminado: <ruta>` y al final `[COLA buffy] COLA 2 COMPLETA`. Kernels de prueba solo en `test-run-*` con puerto y `DATA_DIR` propios.

## B-6 — Entrega el kernel.log de tu drill a Deep
Deep investiga el 36 % de fallos del juez (`docs/internal/INVEST_D-10_fallos_del_juez_deep_2026-10.md`) y necesita el texto literal tras `GUILD_ERROR|llama_backend|` de la fase 3 de tu B-4 (`test-run-gate/work/kernel.log`). Copia ese log, **recortado a las líneas del juez y sin tokens ni rutas absolutas**, a `E:\tylluan-patches\deep\kernel_log_juez_fase3.txt` y avísale en Coloquio con `@deep`. Si ya no existe, dilo y vuelve a producirlo con un drill corto (10 escrituras juzgadas).

## B-7 — Prueba end-to-end de self-approval (test primero, sin tocar src/)
Deep implementará D-13 (`approve_action` debe rechazar la autoaprobación sin credencial de operador). Escribe ANTES la prueba que lo demuestra, marcada `#[ignore = "D-13"]` para que sea la especificación ejecutable: dos identidades (autor y otro agente) y el token maestro; casos: autor aprueba su plan -> 403 `self_approval`; otro agente sin operador -> 403; operador -> 200; el audit registra `human_intervention=true` solo con operador. Entrega el fichero de test en `crates/tylluan-kernel/tests/`. **Criterio:** `cargo test ... -- --ignored` falla hoy exactamente por la razón descrita (pega la salida) y `cargo clippy --all-targets -- -D warnings` exit 0.

## B-8 — Mapa de untrusted_content
`untrusted_content` (`coherence_gate.rs`) solo penaliza en el recall (D-11). Tabla `dónde se pone -> dónde se lee -> qué decisión cambia` con archivo y línea, y una lista de las rutas por las que memoria no confiable llega al contexto de un turno (recall, `read_channel`, resúmenes, `agent_mail`). Sin cambios de código.

## B-9 — H5 de tu revisión SSRF
Tu hallazgo H5 (ingest y `/api/v1/memory/write` escriben sin la capa 1 de ASI06). Mide con el corpus `benchmarks/injection_corpus_v1.json` cuántas de las 8 muestras que la capa 1 detecta por `tylluan_remember` pasan hoy por ingest y por `memory/write` sin ser marcadas. Solo la tabla; el arreglo lo decide el TL.

**Exclusiones:** no tocar `src/`; no bajar umbrales de los gates; no `--no-verify`; no escribir en el checkout compartido.
