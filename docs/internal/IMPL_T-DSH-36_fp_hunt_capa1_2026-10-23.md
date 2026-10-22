# T-DSH-36 — Caza de falsos positivos de la capa 1 de D-31 sobre el repositorio

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`51c6e41`, push 108 con D-31); limpio antes de empezar.
- **Encargo (turno 1557):** crear `crates/tylluan-kernel/tests/layer1_fp_hunt_repo.rs` que recorra los `.md`, `.py` y `.rs` rastreados (git ls-files, con fallback sin git), parta en párrafos ≤500 caracteres y llame a `matches_injection_pattern`; informe con nº de párrafos, nº de aciertos y lista (fichero:línea, patrón que casa, veredicto); el test pasa hoy con un máximo explícito; proponer qué patrones recortar si hay falsos positivos reales.

## Qué hace el test

`layer1_fp_hunt_repo.rs` (nuevo; `src/` intacto):
1. lista los ficheros con `git ls-files -z` (fallback: recorrer `docs/`, `guilds/` y `README.md` si no hay git);
2. parte cada fichero en párrafos (separados por línea en blanco) y trocea a 500 caracteres como máximo;
3. llama a `tylluan_kernel::security::poison_patterns::matches_injection_pattern` sobre cada párrafo;
4. atribuye el patrón replicando la implementación de D-31: literales de `INJECTION_PATTERNS` contra `normalize(texto)` y, si no, literales de `SQUASHED_PATTERNS` contra la vista aplastada (las dos listas se leen del propio `poison_patterns.rs`, no se duplican);
5. imprime la lista de aciertos y **fija el tope actual en `MAX_HITS = 122`** (medido hoy). No falla por aciertos de documentación de seguridad: solo por superar el tope.

## Medición (árbol `51c6e41`)

```text
ficheros analizados: 794
parrafos analizados: 37301
aciertos de capa 1: 122
```

Distribución por patrón:

| aciertos | patrón atribuido |
|---:|---|
| 35 | `squash:imend` |
| 33 | `squash:imstart` |
| 12 | `crudo:[system:` |
| 10 | `crudo:ignore all previous instructions` |
| 8 | `crudo:ignora todas las instrucciones anteriores` |
| 4 | `crudo:<|im_start|>` |
| 3 | `crudo:sin restricciones` |
| 3 | `squash:ignoreallpreviousinstructions` |
| 3 | `crudo:ignore previous instructions` |
| 2 | `crudo:<|im_end|>` |
| 2 | `crudo:[sistema:` |
| 1 c/u | `crudo:override security policy`, `crudo:sin filtros`, `crudo:autoridad maxima`, `crudo:olvida todas las instrucciones anteriores`, `crudo:ignora las instrucciones anteriores`, `squash:olvidatodaslasinstruccionesanteriores`, `squash:ignorepreviousinstructions` |

Por veredicto heurístico (según ruta): 78 código, 19 documentación, 16 el propio `poison_patterns.rs`, 9 datos/literal de test de inyección.

## Hallazgo: 68 de 122 aciertos son FALSOS POSITIVOS reales (una sola clase)

**56 % de los aciertos los produce la vista aplastada de dos patrones-token.** `SQUASHED_PATTERNS` incluye `"imstart"` y `"imend"` (derivados de `<|im_start|>`/`<|im_end|>`). La vista aplastada elimina todo lo no alfanumérico, así que:

- `trim_start()` / `trim_start_matches(...)` → `trimstart...` **contiene** `imstart` → 33 aciertos;
- `trim_end_matches(...)` / `trim_end()` → `trimend...` **contiene** `imend` → 35 aciertos;
- español: «…el **régimen de**l software libre…» → `...regimendel...` contiene `imend` → 4 aciertos en docs.

Ejemplos con su línea:
- `crates/tylluan-kernel/src/config.rs:1859` → `l.trim_start()` (`squash:imstart`)
- `crates/tylluan-cli/src/main.rs:1334` → `base.trim_end_matches('/')` (`squash:imend`)
- `docs/reference/research/2026-10-22_cierre_no_verificado_hermes.md:5` → «…régimen del software…» (`squash:imend`)

**Contradice la propia documentación de D-31**: el módulo declara como límite conocido (c) que los patrones-token "solo sobreviven a invisibles, ancho, homóglifos y mayúsculas; espaciarlos sigue fallando, porque aplastarlos casaría con cualquier mención de 'system'". La implementación, en cambio, **sí** aplasta dos tokens (`imstart`/`imend`), y el resultado es que cualquier `trim_start`/`trim_end` de Rust (o «régimen de» en español) dispara la capa 1. No es un FP sobre contenido escrito en memoria en este repo (son fuentes), pero sí es la clase exacta de FP que llegaría a la memoria si ese texto se ingiere.

**Propuesta de recorte (para el TL/Deep):** quitar `"imstart"` y `"imend"` de `SQUASHED_PATTERNS`. Los tokens `<|im_start|>`/`<|im_end|>` siguen detectándose por la vía cruda con normalización (invisibles, ancho, homóglifos, mayúsculas), que es lo que el propio módulo documenta como alcance de los tokens. Efecto esperado: **122 → 54 aciertos** (los 68 FPs desaparecen) y `MAX_HITS` bajaría a 54. No toqué `src/` (es de Deep); lo dejo como propuesta con el número medido.

Nota sobre las cláusulas en español: en este repo no aparecen en prosa benigna (`sin restricciones` 3 aciertos y `sin filtros` 1 son todos documentación que cita los patrones, sobre todo `IMPL_D-31`); los 300 benignos del corpus tampoco las disparan (0 FPs medidos por Deep). No propongo recortarlas; sí dejar el test como vigilancia si ese texto empieza a aparecer en la memoria del producto.

## Lista completa de aciertos (122)

```text
== 68 FPs de squash:imstart/imend (clase trim_start/trim_end/«régimen de») ==
crates/tylluan-cli/src/main.rs:426 | squash:imend | código (revisar si es FP real)
crates/tylluan-cli/src/main.rs:700 | squash:imstart | código (revisar si es FP real)
crates/tylluan-cli/src/main.rs:1334 | squash:imend | código (revisar si es FP real)
crates/tylluan-cli/src/main.rs:1334 | squash:imend | código (revisar si es FP real)
crates/tylluan-cli/src/main.rs:1458 | squash:imend | código (revisar si es FP real)
crates/tylluan-cli/src/main.rs:1542 | squash:imstart | código (revisar si es FP real)
crates/tylluan-evals/src/tests.rs:715 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/examples/decision_fabric_paired.rs:1482 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/examples/decision_fabric_paired.rs:1675 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/examples/decision_fabric_spike.rs:1482 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/examples/decision_fabric_spike.rs:1675 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/config.rs:975 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/config.rs:1859 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/config.rs:1875 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/config.rs:1875 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/config.rs:1875 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/eval/longmemeval_s.rs:66 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/logging.rs:211 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/main.rs:1103 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/main.rs:2570 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/main.rs:2581 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/memory/silva/decay.rs:99 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/memory/silva/nodes.rs:179 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/memory/triple_extractor.rs:23 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/registry/guild_process.rs:1563 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/registry/proxy.rs:349 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/router/catalog.rs:1161 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/router/complexity.rs:32 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/router/decision/systemone_http.rs:38 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/router/decision/systemone_http.rs:78 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/a2a_client.rs:317 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/a2a_client.rs:338 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:177 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:337 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:454 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:469 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:469 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:549 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:561 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:561 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:659 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:930 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_collective.rs:38 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_federation.rs:292 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_federation.rs:956 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_ingest.rs:61 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/api_ingest.rs:467 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/api_v1/mcp.rs:562 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/mod.rs:1237 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/mod.rs:1237 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/oauth.rs:143 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/http/oauth.rs:360 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/mdns.rs:89 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/server/handler_do/a2a.rs:8 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/server/handler_do/coloquio.rs:8 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/server/handler_do/coloquio_utils.rs:127 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/server/handler_do/coloquio_utils.rs:341 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/server/handler_do/mod.rs:684 | squash:imstart | código (revisar si es FP real)
crates/tylluan-kernel/src/transport/server/handlers.rs:390 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/src/tunnel/mod.rs:156 | squash:imend | código (revisar si es FP real)
crates/tylluan-kernel/tests/approve_action_intent_adversarial.rs:1 | squash:imstart | código (revisar si es FP real)
docs/internal/IMPL_T-DSH-9_adversarial_d16_2026-10-21.md:45 | squash:imstart | documentación (revisar si es FP real)
docs/internal/INVESTIGACION_H-1_cuatro_fuentes_primarias_2026-10-21.md:39 | squash:imend | documentación (revisar si es FP real)
docs/internal/INVESTIGACION_H-1_cuatro_fuentes_primarias_2026-10-21.md:41 | squash:imend | documentación (revisar si es FP real)
docs/reference/research/2026-10-21_preguntas_correctas_camino_a_v1.md:123 | squash:imend | documentación (revisar si es FP real)
docs/reference/research/2026-10-21_preguntas_correctas_camino_a_v1.md:164 | squash:imend | documentación (revisar si es FP real)
docs/reference/research/2026-10-22_cierre_no_verificado_hermes.md:5 | squash:imend | documentación (revisar si es FP real)
docs/roadmap/ROADMAP_O3.md:260 | squash:imstart | documentación (revisar si es FP real)

== 54 aciertos legítimos (literales de patrón en código/tests/docs) ==
benchmarks/spikes/qwen3_consensus/eval_consensus_synthesis.py:159 | crudo:<|im_start|> | código (revisar si es FP real)
benchmarks/spikes/qwen3_consensus/eval_consensus_synthesis.py:173 | crudo:<|im_end|> | código (revisar si es FP real)
benchmarks/spikes/qwen3_digest/eval_coloquio_digest.py:156 | crudo:<|im_start|> | código (revisar si es FP real)
benchmarks/spikes/qwen3_digest/eval_coloquio_digest.py:169 | crudo:<|im_end|> | código (revisar si es FP real)
crates/tylluan-kernel/src/memory/agent_memory.rs:518 | crudo:ignore all previous instructions | código (revisar si es FP real)
crates/tylluan-kernel/src/security/coherence_gate.rs:877 | crudo:[system: | código (revisar si es FP real)
crates/tylluan-kernel/src/security/integration_tests.rs:6 | crudo:[system: | código (revisar si es FP real)
crates/tylluan-kernel/src/security/integration_tests.rs:6 | crudo:override security policy | código (revisar si es FP real)
crates/tylluan-kernel/src/security/poison_patterns.rs:1 | crudo:[system: | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:1 | crudo:sin filtros | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:39 | crudo:[system: | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:39 | crudo:ignora todas las instrucciones anteriores | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:39 | crudo:sin restricciones | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:74 | squash:ignoreallpreviousinstructions | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:74 | squash:olvidatodaslasinstruccionesanteriores | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:289 | crudo:[system: | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:307 | squash:ignoreallpreviousinstructions | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:323 | crudo:ignore all previous instructions | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:330 | crudo:[system: | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:336 | squash:ignoreallpreviousinstructions | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:343 | crudo:ignora todas las instrucciones anteriores | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:343 | crudo:[sistema: | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:343 | crudo:[sistema: | literal del propio patrón (código)
crates/tylluan-kernel/src/security/poison_patterns.rs:359 | crudo:sin restricciones | literal del propio patrón (código)
crates/tylluan-kernel/src/security/trust.rs:294 | crudo:ignore all previous instructions | código (revisar si es FP real)
crates/tylluan-kernel/tests/asi06_layer1_ofuscacion_regresion.rs:25 | crudo:ignore all previous instructions | código (revisar si es FP real)
crates/tylluan-kernel/tests/asi06_layer1_ofuscacion_regresion.rs:29 | crudo:ignora todas las instrucciones anteriores | código (revisar si es FP real)
crates/tylluan-kernel/tests/asi06_redteam_memory_poisoning.rs:69 | crudo:ignore all previous instructions | datos/literal de test de inyección (legítimo)
crates/tylluan-kernel/tests/asi06_redteam_memory_poisoning.rs:75 | crudo:ignora todas las instrucciones anteriores | datos/literal de test de inyección (legítimo)
crates/tylluan-kernel/tests/asi06_redteam_vias3_6.rs:37 | crudo:ignora todas las instrucciones anteriores | datos/literal de test de inyección (legítimo)
crates/tylluan-kernel/tests/cola_d32_acceptance.rs:27 | crudo:ignora todas las instrucciones anteriores | código (revisar si es FP real)
crates/tylluan-kernel/tests/cola_d32_acceptance.rs:33 | crudo:ignore all previous instructions | código (revisar si es FP real)
crates/tylluan-kernel/tests/d18_untrusted_visibility.rs:144 | crudo:ignore all previous instructions | código (revisar si es FP real)
crates/tylluan-kernel/tests/decide_adversarial.rs:191 | crudo:ignore all previous instructions | código (revisar si es FP real)
docs/concepts/SECURITY.md:90 | crudo:[system: | documentación (revisar si es FP real)
docs/internal/IMPL_D-31_poison_normalizacion_deep_2026-10.md:27 | squash:ignorepreviousinstructions | documentación (revisar si es FP real)
docs/internal/IMPL_D-31_poison_normalizacion_deep_2026-10.md:27 | crudo:<|im_start|> | documentación (revisar si es FP real)
docs/internal/IMPL_D-31_poison_normalizacion_deep_2026-10.md:27 | crudo:autoridad maxima | documentación (revisar si es FP real)
docs/internal/IMPL_D-31_poison_normalizacion_deep_2026-10.md:71 | crudo:ignora todas las instrucciones anteriores | documentación (revisar si es FP real)
docs/internal/IMPL_D-31_poison_normalizacion_deep_2026-10.md:71 | crudo:sin restricciones | documentación (revisar si es FP real)
docs/internal/IMPL_D-31_poison_normalizacion_deep_2026-10.md:180 | crudo:[system: | documentación (revisar si es FP real)
docs/internal/IMPL_T-DSH-27_corpus_es_2026-10-21.md:28 | crudo:[system: | datos/literal de test de inyección (legítimo)
docs/internal/IMPL_T-DSH-33_vias_escritura_2026-10-23.md:45 | crudo:ignora todas las instrucciones anteriores | documentación (revisar si es FP real)
docs/internal/IMPL_asi06_redteam_envenenamiento_buffy_2026-10.md:29 | crudo:[system: | datos/literal de test de inyección (legítimo)
docs/internal/IMPL_b4_coherencegate_puerta_escritura_buffy_2026-10.md:84 | crudo:olvida todas las instrucciones anteriores | documentación (revisar si es FP real)
docs/internal/IMPL_b4_coherencegate_puerta_escritura_buffy_2026-10.md:84 | crudo:ignore all previous instructions | documentación (revisar si es FP real)
docs/reference/adr/ADR011_learned_reranker_coherence_gate.md:105 | crudo:[system: | documentación (revisar si es FP real)
docs/superpowers/plans/2026-08-18-security-claims-ci-gate.md:384 | crudo:[system: | documentación (revisar si es FP real)
guilds/core/utils.py:192 | crudo:ignore all previous instructions | código (revisar si es FP real)
guilds/core/vision.py:327 | crudo:<|im_start|> | código (revisar si es FP real)
tests/python/test_injection_guard.py:14 | crudo:ignore previous instructions | datos/literal de test de inyección (legítimo)
tests/python/test_injection_guard.py:22 | crudo:ignore previous instructions | datos/literal de test de inyección (legítimo)
tests/python/test_injection_guard.py:28 | crudo:ignora las instrucciones anteriores | datos/literal de test de inyección (legítimo)
tests/python/test_injection_guard.py:40 | crudo:ignore previous instructions | datos/literal de test de inyección (legítimo)
```

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test layer1_fp_hunt_repo -- --nocapture` | **1 passed / 0 failed** (2,66 s); tablas y lista impresas |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** el test nuevo + este informe; `src/` intacto |
| `check_test_count.sh` | exit 0 (1272; el test de integración no cuenta en lib) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función.

## Impact

- **Solo lectura sobre `src` + test nuevo**: cero cambios en código de producción, guilds, configuración o corpus; no se arrancó el kernel ni se tocó `E:\tylluan`. El test recorre el repo en ~2,7 s sin red.
- **Para D-31/D-32**: identifica una clase de FP **real y medible** de la implementación actual (68 de 122 aciertos), con la causa exacta (`imstart`/`imend` en la vista aplastada) y la corrección propuesta (quitar dos literales). También deja una vigilancia continua: si D-31 gana patrones que disparen prosa benigna, el recuento sube y el test falla pidiendo revisión.
- El tope (122) es el estado **de hoy**: subirlo exige motivo; al aplicar el recorte propuesto debe bajarse a 54.

## No verificado

- **Ejecución con el recorte**: no modifiqué `SQUASHED_PATTERNS` (es `src/` de Deep); los 54 aciertos restantes son una proyección aritmética (122 − 68), no una medición con el cambio aplicado.
- **Por qué la implementación aplasta los tokens**: leí el código y la doc del módulo, no la intención de Deep (la contradicción doc/implementación es un hecho; la causa histórica no la he preguntado).
- **Triaje semántico fino**: el veredicto es heurístico por ruta; no he revisado uno a uno los 54 aciertos legítimos para confirmar que ninguno es un FP de otra clase (los revisados son literales en tests/docs que citan inyecciones, `guilds/core/utils.py` con su propia lista y prompts de benchmark que usan `<|im_start|>` legítimamente). Los 4 aciertos de `benchmarks/spikes/*` sí son tokens reales en código que construye prompts: no son FP de capa 1 sobre memoria, pero conviene tenerlos presentes si ese texto se ingiere.
- **Otras extensiones**: solo `.md`/`.py`/`.rs` rastreados; no `.json` (los corpora de inyección se marcarían enteros y distorsionarían el recuento), ni `.toml`/`.yml`/`.html`.
- **Párrafos de 500 caracteres**: trocear corta frases largas; un patrón partido por el corte podría no casar (infra-detección del escaneo, no del producto). No medí sensibilidad al tamaño de trozo.
- **CI**: medido en mi Windows con `git ls-files` del worktree; no en el runner (que puede tener otro árbol de ficheros, aunque el test tolera ausencia de git con el fallback).
