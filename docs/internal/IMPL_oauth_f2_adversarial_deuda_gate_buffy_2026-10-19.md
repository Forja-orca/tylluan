# INFORME — Lote B1/B2/B3 (suite adversarial OAuth F2, deuda evals, fixtures del gate en CI) — Buffy

**Asignación:** TL, 2026-10-19 (orden B2 → B3 → B1). **Diff SIN commitear** en el worktree de Buffy (`E:\tylluan-wt-buffy`), detachado de `origin/main = a220fb8` (fetch + checkout verificados al empezar; el diff de T1275 que quedaba sin commitear en mi worktree estaba ya en main byte a byte — comprobado con `git diff origin/main` por fichero antes de tocar nada; el fixture `scripts/test_check_no_predation.sh` local era idéntico al commiteado). Solo el TL commitea.

**Ficheros tocados (3):**

| Fichero | Qué |
|---|---|
| `crates/tylluan-evals/src/tests.rs` | B2: helper `bench_report_path` (tests.rs:16) + 4 call-sites redirigidos a tempdir (tests.rs:143, :393, :543, :764). Diff 20+/32− |
| `.github/workflows/ci.yml` | B3: step "Run non-predation gate fixtures" en el job `no-predation-report` (ci.yml:265-272) |
| `crates/tylluan-kernel/tests/oauth_f2_adversarial_spec.rs` | B1: NUEVO, 428 líneas, 8 tests `#[ignore]` (155, 195, 241, 281, 321, 355, 384, 414) |

---

## B2 — `cargo test -p tylluan-evals` reescribía `benchmarks/*.json` trackeados

**Escritores localizados** (único punto del workspace que ejecuta `std::fs::write` sobre artefactos trackeados de forma automática; `main.rs`/`runner.rs` solo escriben con `--output` explícito del CLI, `paired_eval_e2e.rs` ya usaba tempdir):

1. `baseline_v090_benchmark` → `benchmarks/baseline_v0.9.0.json` (corre SIEMPRE)
2. `benchmark_v0_10_0_quality_delta` → `benchmarks/benchmark_v0.10.0.json` (SIEMPRE; es el artefacto citado en AGENTS.md)
3. `mlp_routing_accuracy_benchmark` → `benchmarks/benchmark_v0.13.0_mlp_routing.json` (SIEMPRE)
4. `ab_embedding_models_benchmark` → `benchmarks/benchmark_ab_embedding_models.json` (`#[ignore]`, solo con `--ignored`)

Los tres primeros reescriben en cada run porque los percentiles de latencia son no deterministas ⇒ el gate de ADR-013 los ve como WIP ajeno en todo worktree tras cada `cargo test` (check 1a).

**Fix (mínimo, como pidió el TL):** `bench_report_path()` escribe a `%TEMP%/tylluan-evals-bench-<pid>/` (colisión imposible entre worktrees: pid por proceso) y cada test imprime el path para que un humano copie el informe a mano si quiere guardar récord. Los JSON trackeados quedan como lo que son: récords históricos inmutables (con fecha y versión), no salidas de test.

**Verificación (exit status capturado explícitamente, sin pipe que lo oculte):**
- `cargo test -p tylluan-evals -- --nocapture` → **24 passed; 0 failed; 1 ignored; EXIT=0**
- `git status --porcelain benchmarks/` → **vacío**; `git diff HEAD -- benchmarks/` → **vacío** tras el run (el criterio de aceptación)
- Informes reales en `...\Temp\tylluan-evals-bench-27668\` (3 ficheros, rutas impresas por los tests)

---

## B3 — fixtures del gate cableadas en CI

`scripts/test_check_no_predation.sh` añadido como step **dentro del job existente `no-predation-report`** (ci.yml:247), que ya era `continue-on-error: true` — hereda la no-bloqueidad "igual que el gate" (ADR-013 §2). Sin job nuevo: ahorra un runner y mantiene el gate y su meta-test juntos.

**Por qué no necesita `fetch-depth: 0` ni nada más:** el fixture es hermético — construye repos desechables bajo `mktemp` con su propio `git config` (user.email/name, gpgsign off), apunta `refs/remotes/origin/main` con `update-ref` (sin red) y ejecuta una copia viva del gate. Verificado en el runner local: **7/7 fixtures, exit 0**.

**Validación del YAML:** `yaml.safe_load(encoding='utf-8')` parsea; el job resultante tiene `continue-on-error: True` y los steps `[<uses>, gate, fixtures]`; fin de línea CRLF preservado (432 CRLF = 432 LF tras la edición).

---

## B1 — suite adversarial OAuth F2 escrita como especificación

**Archivo:** `crates/tylluan-kernel/tests/oauth_f2_adversarial_spec.rs` (integration test sobre el router real `build_router`, patrón de `oauth_operator_gate.rs`). Los 8 casos están `#[ignore]` mientras T1281 (diseño de Codex, en curso) no aterrice — cada uno con resultado esperado y por qué en su doc-comentario, anclado en `DESIGN_oauth_operator_2026-10.md` §§2/5 (R6/R7/R10/R11, T1/T2/T3/T5) y en el código F1 releído hoy. No duplica lo ya cubierto por F1 (R12/R13/R14/R15 y E1 viven en `oauth_operator_gate.rs` + `fase1_tests` de `oauth.rs`).

| Caso | Línea | Resultado esperado | Por qué (resumen) | Estado HOY (ejecutado en vivo) |
|---|---|---|---|---|
| C1 authorize sin credencial de operador | :155 | 403, sin Location, sin code intercambiable | cierra T1 (dispensador JWT LAN→RCE); F2 debe preservarlo cuando falte el consentimiento | **PASA** (F1 ya cumple) |
| C2a redirect_uri hostil | :195 | 400 en todos los vectores (attacker domain, lookalike, nip.io→loopback, LAN IP, file:, javascript:) | T2: el 302 entregaría el code al atacante, que eligió su propio challenge PKCE | **PASA** |
| C2b redirect_uri userinfo | :241 | 400 en `http://127.0.0.1:9999@evil.example/cb` | host real = evil.example; el código solo hace `split(':')` y no mira el '@' | **FALLA — hallazgo real (ver abajo)** |
| C2c redirect no registrada sin consentimiento | :281 | 403, ningún code; registro+consentimiento son la cota suficiente de F2 | sin esto, "es loopback" ≡ "emito" (estado T1) | **PASA** |
| C3 scope mcp→operator | :321 | 403 en clase operador aunque `default_role=admin`; rol SIEMPRE derivado del scope | T3: cualquier bearer hereda admin por defecto hoy | **PASA** (belt F1(b): `auth.rs:338-358` rechaza `OAuthJwt`) |
| C4a replay de code | :355 | 1er canje 200; replay 400 `invalid_grant` | uso único (RFC 6749 §4.1.2); sin eso, code filtrado = token de 1h | **PASA** |
| C4b binding code↔redirect | :384 | canjear con redirect distinto → 400 | canje cruzado entre clientes; ancla del registro F2 | **PASA** |
| C5 token por query | :414 | JWT por `?token=` o `?access_token=` → 401 | T5 (logs/historial); E1 solo ampara al token maestro | **PASA** |

**Evidencia de ejecución (exit capturado, sin pipe):**
- Compilación + suite activa: `cargo test -p tylluan-kernel --test oauth_f2_adversarial_spec` → **0 passed; 8 ignored; EXIT=0** (compila limpio; cero coste en CI)
- Ejecución real: `... -- --ignored --nocapture` → **7 passed; 1 failed (C2b); exit 101** — el 101 es esperado y es el hallazgo, no un test roto. Duración 41.79s (construcción de `HttpState` + RepoMap por test, mismo coste que la batería F1 existente).
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` (flags exactos de CI, ci.yml:29) → **EXIT=0**, sin warnings.

### Hallazgo (C2b): `is_loopback_redirect_uri` acepta userinfo con apariencia loopback

`oauth.rs:173-184` parte la autoridad con `host_port.split(':').next()` — para `http://127.0.0.1:9999@evil.example/cb` ve "127.0.0.1" y devuelve true, pero en la gramática RFC 3986 el host real es `evil.example` (`127.0.0.1:9999` es userinfo). El Location 307 generado (verificado en vivo: **left: 307**) entregaría el code al atacante. Igual con `http://localhost:8080@evil.example/cb`.

- **Acotación honesta:** solo emite con `dev_mode=true` (F1 ya cierra `dev_mode=false`, y con `dev_mode=true` el kernel se ata a 127.0.0.1), así que el explotador real es un proceso local o una página web que fuerza la petición — no un host de LAN contra un kernel en loopback. Gravedad **media**, no crítica.
- **Corrección propuesta (para F2, en kernel src — fuera de mi alcance por la exclusión de la asignación):** rechazar '@' en la autoridad, o parsear con el crate `url` en vez de `split`. El test C2b ya está escrito con el resultado correcto: quien lo arregle lo activa y pasa.

---

## Verificaciones cruzadas

- `bash scripts/test_check_no_predation.sh` → 7/7, exit 0 (re-ejecutado tras el edit de ci.yml; el fixture no lee ci.yml pero el gate vive en el mismo repo)
- `bash -n` sobre los scripts tocados → OK (no toqué scripts en este lote; el fixture ya existía)
- Los gates de docs (`check_docs_reality.sh`, `check_doc_links.py`) no leen los 3 ficheros tocados (tests de Rust + step de CI sin rutas nuevas citadas en docs salvo este informe); comprobado en iteraciones anteriores de este worktree para ci.yml. Este informe es el único fichero docs nuevo.
- `cargo test -p tylluan-kernel --lib` NO se corrió completo en este lote (el lib no cambió: los 3 ficheros son tests/CI/docs; el recuento de 1057 lib tests de AGENTS.md queda intacto). El test-binario del kernel SÍ se compiló y ejecutó (evidence arriba).

## Limitaciones

- Los 8 tests son `#[ignore]` por mandato del TL (spec mientras F2 no existe); se activan cuando T1281 congele el contrato. Riesgo de bit-rot mitigado: se COMPILAN en cada CI (`--all-targets`).
- `scripts/verify.sh` no corre los fixtures (solo CI, como pidió la asignación); candado opcional para el TL.
- C2b fallará siempre que alguien ejecute `--ignored` hasta que se arregle `oauth.rs` — es intencionado y está documentado en el propio test.

---

## Impact

- **ADR-013 / pre-push:** se cierra la deuda que disparaba el gate como WIP ajeno tras cada `cargo test -p tylluan-evals` (B2). El gate gana fixtures en CI no bloqueantes (B3) — meta-protección del gate sin añadir fricción (nada bloqueante, fixtures herméticas, 0 red/0 cargo).
- **Superficie de producción:** CERO. No se toca `crates/tylluan-kernel/src` (exclusión respetada: el escritor de B2 estaba en tylluan-evals). Los 3 ficheros son tests de Rust, un step de CI no bloqueante y este informe.
- **Contrato de puertos:** intacto (ningún `.toml` ni puerto tocado).
- **Seguridad:** hallazgo real de severidad media en `is_loopback_redirect_uri` (C2b), acotado y con fix propuesto para F2; la suite fija por escrito el contrato adversarial que T1281 debe cumplir (8 casos, cada uno con resultado esperado y por qué).
- **Para agentes legítimos:** cero pasos nuevos — correr los tests ya no ensucia el worktree, y el CI cubre el gate.
