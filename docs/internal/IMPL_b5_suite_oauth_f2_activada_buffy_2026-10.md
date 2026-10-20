# INFORME B-5 — Suite adversarial OAuth F2 activada — Buffy (2026-10-20)

**Asignación:** ítem B-5 de `docs/internal/COLA_buffy_2026-10-19.md` — quitar los `#[ignore]` de `crates/tylluan-kernel/tests/oauth_f2_adversarial_spec.rs` que correspondan, ejecutarlos y reportar cuáles pasan y cuáles no. Condición de activación ("cuando Deep termine las fases A y B"): **CUMPLIDA** — `d88512d` (en `origin/main`) trae D-2 (RFC 9728/8707/9207) y D-3 (desafíos de scope, JWT con claim `scope=mcp`), es decir las fases A y B, verificado con `git fetch` + ff al empezar la sesión (el worktree estaba 38 commits por detrás; mis entregables B-1..B-4 ya estaban commiteados byte a byte en `origin/main`, con copia de seguridad de los locales antes de darles paso).

**Fichero tocado (1; diff SIN commitear — solo el TL commitea):** `crates/tylluan-kernel/tests/oauth_f2_adversarial_spec.rs` — 34+/24−: 4 atributos `#[ignore]` retirados y doc-comentarios actualizados a la realidad verificada. **CERO toques en `crates/tylluan-kernel/src/`** (exclusión de la cola respetada).

## Qué activé y qué no — criterio: la condición escrita en el propio `#[ignore]`

| Caso | Condición escrita en su `#[ignore]` | ¿Cumplida hoy? | Decisión |
|---|---|---|---|
| C2b redirect con userinfo | (sin `#[ignore]` desde `1a2b6a0`, que además lo arregló) | — | ya activo |
| C3 scope mcp → clase operador | "activar cuando existan los claims de scope" | **SÍ** — `sign_jwt` lleva `scope: mcp` desde `d88512d` | **ACTIVADO** |
| C4a code de uso único | "comportamiento F1 que F2 debe preservar" | **SÍ** — F2 ya existe; ahora sí hay preservación que vigilar | **ACTIVADO** |
| C4b binding code ↔ redirect | ídem | **SÍ** | **ACTIVADO** |
| C5 JWT OAuth por query | ídem | **SÍ** | **ACTIVADO** |
| C1 authorize sin credencial de operador | "activar cuando el contrato de consentimiento esté congelado" | NO — `docs/internal/PLAN_oauth_f2_consentimiento_deep_2026-10.md` está **sin implementar** (5 decisiones de José pendientes; el propio mensaje de `d88512d` lo declara) | sigue `#[ignore]` |
| C2a redirect_uri hostil | "activar cuando el contrato esté congelado" | NO (mismo motivo) | sigue `#[ignore]` |
| C2c redirect no registrada sin consentimiento | "el registro de clientes llega con el diseño de consentimiento" | NO — no existe registro de clientes | sigue `#[ignore]` |

## Resultados — ejecución real, exit capturado sin pipe que lo oculte

**Línea base (ANTES de tocar nada, mismo día):**

| Corrida | Resultado | Exit |
|---|---|---|
| normal | 1 passed (C2b), 0 failed, 7 ignored | 0 |
| `--ignored` | 7 passed, 0 failed | 0 |

**Después de activar C3/C4a/C4b/C5:**

| Corrida | Resultado | Exit |
|---|---|---|
| normal | **5 passed, 0 failed**, 3 ignored (C1, C2a, C2c) | 0 |
| `--ignored` | 3 passed, 0 failed | 0 |

**Cuáles pasan y cuáles no: PASAN LOS 8.** Cero fallos. El único que falló alguna vez fue C2b el 2026-10-19 (el hallazgo de userinfo que originó la suite); ya está cerrado en `1a2b6a0` y corre activo desde entonces.

Logs crudos (con los cuatro `test result:` y los exits): `test-run-gate/work/b5/` (`baseline_active.log`, `baseline_ignored.log`, `active_after.log`, `ignored_after.log`, `clippy.log`) — directorio auto-ignorado por el gate de ADR-013, igual que en B-4.

## Higiene de documentación dentro del propio fichero

- El módulo decía "Todos están `#[ignore]` mientras T1281 no aterrice" y "C2b FALLA hoy": falso desde `d88512d`/`1a2b6a0`. Reescrito con el estado de activación por caso, su motivo y el resultado verificado en vivo (con fechas, para no borrar el historial del hallazgo).
- El comentario del cuerpo de C3 decía "JWT con el formato actual (sin claim scope)": el JWT lleva `scope=mcp` desde `d88512d`; corregido.

## Verificaciones

- `cargo test -p tylluan-kernel --test oauth_f2_adversarial_spec` → **EXIT 0** (5 passed / 3 ignored)
- `cargo test -p tylluan-kernel --test oauth_f2_adversarial_spec -- --ignored` → **EXIT 0** (3 passed)
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` (flags exactos de CI) → **EXIT 0**
- `git diff --stat` → 1 fichero, 34+/24−; `git status` → solo este diff + informes sin commitear

## Gate ADR-013 (`check_no_predation.sh`)

`bash scripts/check_no_predation.sh` → **EXIT 1**, con un único hallazgo: el propio diff de este ítem (`crates/tylluan-kernel/tests/oauth_f2_adversarial_spec.rs`) clasificado como "WIP ajeno" porque está modificado en el working tree sin formar parte de ningún commit del rango. Es el comportamiento esperado del gate para la entrega de Buffy (diff sin commitear; solo el TL commitea, y el gate no puede saber que el diff es mío). Se resuelve al commitear: el mensaje del commit debe llevar la sección `## Impact` de este informe (ya redactada más abajo) — el propio gate lo indica ("declara '## Impact' vía commit"). Cero hallazgos de depredación sobre trabajo de otros agentes.

## No verificado / limitaciones

- Son tests **in-process** (router real `build_router` con `support::test_state*`, mismo patrón que la batería F1/`oauth_operator_gate.rs`): no levantan un kernel en proceso ni tocan la red ni la base de producción.
- Mantener C1/C2a/C2c ignorados es mi lectura literal de la condición escrita en cada atributo. Como los 3 pasan hoy (evidencia arriba), si el TL prefiere activarlos igualmente antes de congelar el contrato de consentimiento, es un diff de 3 líneas.
- No cambié recuentos de tests de lib (`scripts/check_test_count.sh` no aplica: esto es un integration test) ni STATUS/README/AGENTS (los sincroniza el TL).

## Impact

- **ADR-013 / pre-push:** diff sin commitear entregado al TL, como manda la regla 9 (solo el TL commitea); el único fichero tocado es un integration test del kernel, no `transport/`, `guilds/`, `integrations/` ni `tylluan*.toml`.
- **Superficie de producción:** CERO — ni una línea de `src/` cambia; el efecto es sobre la red de seguridad del CI.
- **Efecto en CI:** la corrida normal de la suite pasa de 1 a 5 tests (3,5 s). Si F2 (consentimiento, registro de clientes, scopes) rompe algo de lo que ya existe — uso único del code, binding code↔redirect, JWT por query, scope mcp que nunca satisface la clase operador — el CI grita ahora, no después.
- **Para agentes legítimos:** cero pasos nuevos; nada de umbrales bajados ni gates tocados.
