# INFORME — tylluan_decide: batería adversaria + evaluación pareada (@buffy, turno 1220)

Fecha: 2026-10-16. Worktree: `E:\tylluan-wt-buffy` sobre `origin/main dc42c2e` (detach).
Entrega SIN commitear, como manda el turno 1217. Ficheros tocados (nada más):

| Fichero | Estado |
|---|---|
| `crates/tylluan-kernel/tests/decide_adversarial.rs` | NUEVO (parte A) |
| `crates/tylluan-evals/src/paired_eval.rs` | NUEVO (parte B) |
| `crates/tylluan-evals/src/main.rs` | M (suite `paired` + dispatch antes del motor de embeddings) |
| `crates/tylluan-evals/Cargo.toml` | M (+ `rusqlite` 0.33 bundled, misma versión que el kernel) |
| `Cargo.lock` | M (una línea: arista `tylluan-evals → rusqlite`) |

**Estado posterior:** esta entrega fue integrada por el TL en `origin/main` como
`8f16a97` (2026-10-16). Las cifras vivas actuales de ambas piezas están en
`REVISION_commit_44d5828_buffy_2026-10-16.md` §5.

## 0. Preparación del worktree (turno 1217) — qué había y dónde está ahora

`git stash push -u -m "previo-2026-10-16"` → `stash@{0}`, nada descartado. Qué contenía:

- `crates/tylluan-kernel/src/security/coherence_gate.rs` y `friction_log.rs` modificados: **parche idéntico** al ya commiteado en `main` como `f52be7c` (verificado línea a línea: 157/157, diff de contenido vacío). Es el arreglo del flaky del ciclo anterior; ya está integrado, el stash es redundante y puede descartarse con tranquilidad.
- `docs/internal/FLAKY_hybrid_classify_2026-10.md` (untracked): idéntico al commiteado en `main`.
- `docs/internal/AUDIT_buffy_2026-10-14.md` (untracked): **NO está en `main`** — es el entregable del ciclo anterior del AUDIT (los 3 ❌ de la puerta de documentación). Preservarlo o decirme y lo borro.

Nota: `stash@{1}` (`agent/antigravity-adr020 … adr020-v7`) ya existía antes que mí; no es mío y no lo toqué.

## 1. Parte A — batería adversaria (`tests/decide_adversarial.rs`)

Escrita contra el esquema congelado del turno 1217. Cobertura de los 8 vectores del turno 1220:

| # | Vector | Test | Estado tras integrar Deep+Antigravity |
|---|---|---|---|
| 1 | inyección en `state` | `v1_state_injection_never_escapes_options` | quitar `#[ignore]` |
| 2 | fuera de opciones | `v2_answer_outside_declared_options_rejected` | quitar `#[ignore]` |
| 3 | abstención respetada | `v3_free_question_abstains_no_model_provider` | quitar `#[ignore]` |
| 4 | modelo no revierte rechazo | `v8_model_cannot_overturn_deterministic_rejection` | FASE 2 (sin proveedor de modelo hoy) |
| 5 | clínicos no salen | `v5_clinical_state_never_echoed_nor_stored` | quitar `#[ignore]` (capa respuesta) |
| 6 | no accesible como operador | `v4_decide_is_agent_class_not_operator` | quitar `#[ignore]` |
| 7 | límites | `v6_size_and_question_limits_rejected` | quitar `#[ignore]` |
| 8 | apagado = cero llamadas | `v7_disabled_switch_answers_disabled_empty` | quitar `#[ignore]` (cero llamadas = test de Deep con proveedor canario) |

Además, `mod wire_contract` (3 tests) fija el contrato de cable congelado: snake_case de `QuestionKind` (choice/score/boolean) y round-trip de igualdad exacta de `DecideRequest`/`DecideResponse`/`Answer` con TODOS los campos del turno 1217. Son la red anti-deriva de esquema.

**Cómo lo verifiqué sin pisar los ficheros de nadie:** el fichero necesita `router/decision/api_types.rs` (Deep, turno 1218). Creé un stub idéntico byte a byte al esquema del turno 1217 en MI worktree, cableé `pub mod api_types;`, ejecuté `cargo test -p tylluan-kernel --test decide_adversarial`: **3 passed, 0 failed, 8 ignored** (cada ignore con su motivo), y **borré el stub y revierto `decision/mod.rs`** (confirmado: `git status` sin ellos, `mod.rs` byte idéntico). El único fallo en el camino fue un artefacto mío (0.7 en f32 no sobrevive round-trip JSON con igualdad estricta; cambiado a 0.5 exacto).

**Estado real de entrega:** el fichero NO compila hasta que exista `api_types.rs` — es el orden TDD que pediste. Al aterrizar Deep compila y `wire_contract` corre sin tocar el fichero; al aterrizar Antigravity se quitan los `#[ignore]`.

## 2. Parte B — arnés pareado (`tylluan-evals/src/paired_eval.rs`)

- `load_rows` (rusqlite solo-lectura, `CAST` tolerante a tipos) contra el esquema congelado; `load_truth` (JSON `{request_hash: valor}` — el log no guarda `state`, solo hash, por eso la verdad entra por fuera).
- `mcnemar_exact(b,c)`: p bilateral exacta en **espacio logarítmico** (la versión lineal daba p=0.0 por underflow f64 en n=1100 — medido por su propio test).
- `brier_mean` (multicategoría sin normalizar, convención declarada) y `ece_10bins` (confianza = prob. máxima).
- Veredicto: subir de modo solo si el modelo gana ESTRICTAMENTE, `p < 0.05` y `ECE ≤ ece_max`. Razones explícitas: `model_worse` / `no_difference` / `not_significant` / `no_calibration_data` / `ece_above_threshold` / `insufficient_data`.
- **Abstención cuenta como fallo del modelo** (conservador; `abstention_rate` aparte). Decisión mía, documentada en el módulo.
- CLI: `--suite paired --db <silva.db> --truth <json> [--contract NAME] [--ece-max 0.10] [--save out.json]`, atendida ANTES de cargar el motor de embeddings.

## 3. Qué ejecuté de verdad (nada inventado)

| Comando | Resultado |
|---|---|
| `cargo test -p tylluan-evals` | **22 passed, 0 failed, 1 ignored** (19 míos + 3 preexistentes; el ignored es el A/B de embeddings pre-existente) |
| `cargo clippy -p tylluan-evals --all-targets -- -D warnings` | exit 0 |
| `cargo test -p tylluan-kernel --test decide_adversarial` (con stub) | **3 passed, 8 ignored** |
| CLI ruta de error (BD inexistente) | mensaje claro, exit 1 |
| CLI ruta positiva (BD+truth sintéticos) | informe completo: b=0 c=10 p=0.001953 (=2/1024 exacto), Brier 0, ECE 0, `upgrade_recommended`, JSON guardado, exit 0 |

**NO verifiqué:** el comportamiento del servicio real de Deep (solo su esquema congelado); los vectores REST contra el endpoint (no existe aún); la batería sin stub (imposible por definición hasta el turno 1218).

## 4. Preguntas abiertas / avisos para el TL

1. **«Operador denegado»**: mi `v4` fija clase agente (401 sin credenciales, JWT aceptado, `!operator_class_required`). El middleware actual deja pasar el **master token** por rutas no-operador; si «operador denegado» (turno 1219) debe significar 403 también para él, es decisión de diseño nueva — dime y añado el assert. **→ RESUELTO 2026-10-16: ver `ANALISIS_token_maestro_vs_operador_denegado_2026-10-16.md` (no; añadirlo sería incorrecto).**
2. **`tests/support/mod.rs` romperá al integrar**: construye `HttpState` con literal completo; cuando Deep añada campo/servicio o la sección [decision], hay que actualizar ese literal (no es fichero mío). **→ No se rompió: `decision_service` vive en `TylluanServer`, no en `HttpState`.**
3. **Mina para Antigravity**: `route_inventory_every_mutating_route_is_classified` (auth.rs) exige que toda ruta mutante nueva esté clasificada; al montar `POST /api/v1/decide` debe añadirse a `AGENT_OR_PEER_CLASS` o el test estructural falla. **→ Desactivada en `44d5828` (auth.rs:926).**
4. **`ece_max` es parámetro con defecto 0.10**: el ADR dice «ECE aceptable» sin número; no inventé política, lo dejé configurable.
5. **AGENTS.md**: la línea «tylluan-evals … no tiene #[test] propios» deja de ser cierta (19 tests nuevos); el recuento total de tests sube — actualizar al integrar.
6. `v8` (modelo no revierte rechazo determinista) queda como marcador `#[ignore]` con `unimplemented!` hasta que exista proveedor de modelo.
