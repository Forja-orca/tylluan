# T-DSH-148 — Puesta al día del test `asi06_judge_hang_writes` al mecanismo actual (judge_grammar / inference point)

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = a85526b` (push213), HEAD confirmado `a85526b`.
- Encargo (T2197): poner al día el test `asi06_judge_hang_writes` para el mecanismo actual (`grammar_judge` / inference point), manteniendo la garantía: **un juez colgado NO bloquea la escritura** (fire-and-forget, <3 s). Elegir la opción más fiel y explicar por qué. Criterios: test verde 3/3 y con `--test-threads=1`; clippy `--all-targets -D warnings` = 0; `cargo test -p tylluan-kernel --tests` completo listando otros rojos (sin arreglarlos). Exclusiones: no tocar código de producción ni `ci.yml`.

## Diff (solo test)

```
crates/tylluan-kernel/tests/asi06_judge_hang_writes.rs | 73 ++++++++++++++++------
1 file changed, 53 insertions(+), 20 deletions(-)
```

Sin cambios en código de producción, `Cargo.toml`, `ci.yml` ni dashboard.

## Opción elegida: inference point falso colgado (la más fiel)

Se descartó limitar la aserción al contador de spawns porque, tras T-DSH-115/116, `spawns` sube aunque el juez no llegue a llamar a nada (el `spawn` es de la tarea, no de la llamada). El test nuevo:

1. Levanta un servidor **OpenAI-compatible falso** en loopback que acepta conexiones TCP y **nunca responde**, con un contador de conexiones aceptadas.
2. Escribe un `tylluan.toml` temporal con `[inference.points.grammar_judge] provider="openai_compatible"`, `endpoint=<falso>`, `model="fake-hung-judge"` y lo fija con `TylluanConfig::set_config_path_override` (endpoint loopback → local, sin `cloud_enabled`/`api_key_env`).
3. Hace 2 `POST /api/v1/memory/write` y exige: **200 OK** y **<3 s** cada una (fire-and-forget).
4. Exige `judge_spawn_count()` +2 (el juez se lanzó) **y** que el servidor falso haya aceptado ≥1 conexión (el juez **llegó a llamar** al punto y quedó colgado).

Por qué es más fiel que solo contar spawns: reproduce el peor caso real (el punto de inferencia acepta y no responde), demuestra que la escritura no espera y que el juez sí fue invocado; la señal observable pasa de `judge_telemetry().in_flight()` (que ya solo cubre la ruta HTTP legacy, `#[allow(dead_code)]` en `coherence_gate.rs:378`) a las conexiones aceptadas por el punto falso.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test asi06_judge_hang_writes` ×3 | **3/3 verde** (`ok. 1 passed; 0 failed`, 0,73-0,77 s) |
| mismo test con `-- --test-threads=1` ×3 | **3/3 verde** (0,78-0,88 s) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0**; único "warning" del log es el del build script (`bundled-dashboard feature not enabled — skipping UI build`), no un lint de clippy |
| `cargo test -p tylluan-kernel --tests` completo | **1 rojo, ajeno a este cambio** (tabla abajo) |

Entorno: `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`, `CARGO_TARGET_DIR=D:\cargo-target\shared-dev`, `-j 8`.

### Otros rojos del `--tests` completo (NO arreglados, como pedía el encargo)

| test | mensaje | causa probable |
|---|---|---|
| `cola_d33_acceptance::d33_desired_judge_failure_is_visible` | `panicked at tests\cola_d33_acceptance.rs:378:5: D-33: el juez no registró NINGÚN fallo observable (¿no llegó a llamar al backend?)` | **mismo patrón obsoleto**: el test fija `TYLLUAN_KERNEL_URL` a un fake `Always400` y espera telemetría `by_kind` de la ruta HTTP legacy; `write_gate` ya no usa esa ruta (T-DSH-115/116) → no hay fallo observable. Reproducido aislado: falla también. |

## Impact

- El test vuelve a describir el mecanismo real: con `[inference.points.grammar_judge]` apuntando a un punto colgado, la escritura responde **200 en <3 s** y el juez **sí llega a llamar** (conexión aceptada y sin respuesta). 3/3 en normal y 3/3 con `--test-threads=1`.
- Diff mínimo y solo de test: `asi06_judge_hang_writes.rs` (+53/−20). No se tocó producción, `ci.yml` ni dashboard.
- Queda documentado un segundo test obsoleto por la misma migración (`cola_d33_acceptance::d33_desired_judge_failure_is_visible`), listado para que el TL decida si entra en otra tarea; `cargo test --tests` completo seguirá rojo por él (CI no lo ejecuta: `ci.yml` corre `--lib` + binarios de auditoría concretos).

## No verificado

- Una sola máquina y una ventana; el test se ejecutó 3+3 veces, no cientos.
- No se probó con un punto de inferencia real ni con `[security.write_gate_judge]` configurado; solo con el servidor falso colgado (que es el caso peor que interesa).
- El `--tests` completo se corrió **una vez**; no se descarta otro rojo intermitente en otros binarios.
- No se ejecutó clippy de otros crates ni la suite de `tylluan-cli`/`tylluan-link`/`tylluan-fsrs`; no se probó el perfil release.
- No se tocó `ci.yml` (el TL decide si CI pasa a `--tests`); el test no está en CI hoy.
- El test usa `TylluanConfig::set_config_path_override` (override global de proceso); es seguro porque es el único `#[test]` de su binario, pero no se probó convivencia con otros tests en el mismo proceso.
- `cola_d33_acceptance` no se arregló (fuera de alcance); su fallo se atribuye por lectura de código, no instrumentando la telemetría.
