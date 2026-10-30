# T-DSH-149 — Puesta al día de `cola_d33_acceptance::d33_desired_judge_failure_is_visible` (mecanismo actual)

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = 06053c6` (push214; el TL indicó a85526b, sobre esa base está todo lo relevante; 06053c6 solo añade un fix de lint de dashboard).
- Encargo (T2209): poner al día `tests/cola_d33_acceptance.rs::d33_desired_judge_failure_is_visible` al mecanismo actual (mismo criterio que asi06: inference point falso que falla o cuelga), comprobar si el fallo del juez queda **registrado donde hoy se registra**; si no se registra en ningún sitio, **no inventarlo**: reportarlo como **hueco de producto con file:line**. Solo tests; ni producción ni `ci.yml`.

## Diff (solo test)

```
crates/tylluan-kernel/tests/cola_d33_acceptance.rs | 97 +++++++++++---------
1 file changed, 58 insertions(+), 39 deletions(-)
```
(más el diff pendiente de T-DSH-148 en `asi06_judge_hang_writes.rs`, que sigue en el worktree y el TL está verificando).

Sin cambios en producción, `Cargo.toml`, `ci.yml` ni dashboard.

## Qué se cambió

El test ya no usa `TYLLUAN_KERNEL_URL` (ruta HTTP legacy retirada de `write_gate` en T-DSH-115/116). Ahora:

1. Levanta el backend falso existente (`Behavior::Always400`, responde 400 a todo).
2. Escribe un `tylluan.toml` temporal con `[inference.points.grammar_judge] provider="openai_compatible"`, `endpoint=<falso>`, `model="fake-400-judge"` y lo fija con `TylluanConfig::set_config_path_override` (loopback → local, sin `cloud_enabled`/`api_key_env`).
3. Llama a `spawn_write_gate_judge` y espera a que el falso **reciba** el `POST /chat/completions` → prueba de que el juez llegó a llamar al punto.
4. Fija el **estado actual**: el 400 no incrementa ningún contador visible (`judge_telemetry().by_kind` sigue igual). El test falla en cuanto el producto registre el fallo, para obligar a actualizarlo y cerrar D-33(b).

## Hueco de producto encontrado (D-33(b) reabierto para el mecanismo nuevo)

Hoy el fallo del juez de capa 2 **no se registra en ningún contador visible**; solo queda en `tracing::debug!`:

- `crates/tylluan-kernel/src/security/write_gate.rs:206` — `tracing::debug!("write_gate: juez de gramatica efimero no disponible para {node_id}: {e} (no bloquea)")`; la llamada está en `:203` (`grammar_judge::judge_grammar`). No hay `note_*` de telemetría.
- `crates/tylluan-kernel/src/security/grammar_judge.rs:76` — al fallar el punto configurado cae al juez local efímero con `tracing::debug!`; si también falla, el `Err` sube a `write_gate` y muere en el debug de arriba.
- `crates/tylluan-kernel/src/security/judge_telemetry.rs` — `by_kind` (y `GET /health?verbose=true → components.judge.by_kind`) se alimenta **solo** desde la ruta legacy de `coherence_gate` (`call_judge`, marcada `#[allow(dead_code)]` para `write_gate` en `coherence_gate.rs:378`).

Conclusión: **no se inventó** ningún registro; se documenta el hueco con esas líneas y el test fija el comportamiento actual. Si el TL quiere cerrar D-33(b), la vía es añadir un contador (p. ej. `judge_telemetry.note_backend_error()` en `write_gate.rs` al recibir `Err` de `judge_grammar`) — **no implementado** por estar fuera del alcance («solo tests»).

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test cola_d33_acceptance` ×3 | **3/3 verde** (`ok. 3 passed; 0 failed`, 4,5-7,6 s) |
| mismo test con `-- --test-threads=1` ×3 | **3/3 verde** (11,2-13,8 s) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** (único warning del log: build script `bundled-dashboard feature not enabled`, no lint) |
| `cargo test -p tylluan-kernel --tests` completo | **1 rojo ajeno** (tabla abajo); `cola_d33_acceptance` y `asi06_judge_hang_writes` ya verdes |

### Otro rojo del `--tests` completo (no arreglado)

| test | mensaje | causa probable |
|---|---|---|
| `corpus_es_shape::injection_corpus_es_shape_and_layer1_baseline` | `panicked at tests\corpus_es_shape.rs:136:5: linea base: la capa 1 ya detecta 13 inyecciones ES (se esperaba 0 hoy)` (`left: 13, right: 0`) | **ajeno a este diff**: la línea base del corpus ES espera 0 detecciones de capa 1 y hoy detecta 13 (probable mejora de patrones de capa 1 en main frente a una expectativa obsoleta). Determinista: reproducido aislado. No se toca (fuera de alcance). |

## Impact

- `cola_d33_acceptance` vuelve a verde (3/3 normal y 3/3 single-thread) describiendo el mecanismo real: el juez llama al inference point configurado y el fallo del punto **no** se registra hoy en ningún contador visible.
- Hueco de producto documentado con `file:line` (write_gate.rs:203/206, grammar_judge.rs:76, judge_telemetry solo legacy) en vez de inventar una métrica; el test avisará si el producto lo cierra.
- Diff mínimo y solo de test (+58/−39). `--tests` completo sigue con **1 rojo ajeno** (`corpus_es_shape`), que impide hoy el «0 rojos» que José quiere para pasar CI a `--tests`; queda listado para el TL.

## No verificado

- Una sola máquina y ventana; 3+3 ejecuciones del test de cola_d33.
- `--tests` completo se corrió **una vez** (y `corpus_es_shape` aislado para confirmar que es determinista, no flaky); no se descarta otro rojo intermitente en otros binarios.
- El test usa `TylluanConfig::set_config_path_override` (override global de proceso) en un binario con 4 tests que corren en paralelo; los otros 3 no llaman a `TylluanConfig::load_cached` (usan `GuildProcess` con env explícito), por lo que no se observó interferencia en 6 ejecuciones, pero no está probado formalmente para cualquier orden.
- No se probó con un inference point real ni con `[security.write_gate_judge]` configurado.
- El hueco D-33(b) se atribuye leyendo el código (no hay instrumentación de contadores); no se midió el impacto de añadir el contador propuesto.
- `corpus_es_shape` no se arregló (fuera de alcance); su causa se atribuye por el mensaje del assert, sin auditar los patrones de capa 1.
- No se ejecutó clippy de otros crates ni la suite de `tylluan-cli`/`tylluan-link`/`tylluan-fsrs`; no se probó el perfil release.
