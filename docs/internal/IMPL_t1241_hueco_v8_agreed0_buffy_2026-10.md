# IMPL T1241 (Buffy) — hueco de cobertura v8: rechazo CON VALOR y `agreed = 0`

- **Estado: IMPLEMENTADO — diff sin commitear.** Base: `origin/main` `6b9b177` (realigned con `git fetch` + fast-forward; mis docs pendientes intactos).
- **Autor:** Buffy. **Fecha:** 2026-10-18. Encargo: GO del TL en Coloquio T1244 (punto 1).

## 1. Qué faltaba

El v8 de Deep (`v8_model_cannot_overturn_deterministic_rejection`) cubre el rechazo
determinista **sin valor**: el sanity check de `rules.rs` eleva error → abstención
`provider error: …` → fila de sombra con `rules_value = NULL` y `agreed = NULL`.
Quedaba sin cubrir el **otro** rechazo determinista: una respuesta con **VALOR**
dentro del conjunto declarado que el modelo contradice.

## 2. El caso

Pregunta `should_execute` (`kind: boolean`) en contrato `scheduler_routing` con
`risk = 0.9`:

- **Reglas** (`rules.rs`, política HITL `risk >= 0.7`): responden `false` — un
  rechazo de ejecución **con valor** (`allowed_values` boolean = `["true","false"]`,
  sin abstención, sin degradación).
- **Modelo** (canario `CannedTrueProvider` inyectado vía `inject_model_provider`
  de Deep): puntúa **a favor** (`true: 0.9`, dentro del conjunto declarado).
- **Auditoría**: sí hay valor de reglas con el que comparar ⇒ `agreed` debe ser
  **`0`** (desacuerdo medido), no el `NULL` del v8.
- **Respuesta final**: conserva `value = "false"` — el modelo puntúa, nunca revierte
  (invariante v8 intacta).

Se recorre `Advisory` y `Gate` (el mecanismo de Deep `consult_rejected`, sin tocar
`service.rs`: solo test + proveedor canario en `tests/decide_adversarial.rs`).

## 3. Evidencia

- `decide_adversarial`: **13/13, 0 ignored** (`rejection_with_value_keeps_rules_answer_and_logs_disagreement`
  nuevo; los 12 anteriores intactos) — pipeline con `pipefail`, exit 0.
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **CLIPPY_EXIT=0**.
- `scripts/check_test_count.sh` → **COUNT_EXIT=0**: kernel=1048 link=74 fsrs=12 = 1134;
  README y AGENTS coincidentes (el test es de integración, no moue el conteo lib).
- Asserts del test (no vacíos): `!abstained`, `value == "false"`, `!degraded`,
  `model_identity == Some("canned-true-v1:test-model")`, `hits == 1`,
  fila única con `rules_value == "false"`, `model_value == "true"`, `agreed == Some(0)`.

## 4. Ficheros de este diff

`crates/tylluan-kernel/tests/decide_adversarial.rs` (test + `CannedTrueProvider`)
· este informe. `service.rs` **no** se toca (condición del encargo).

## 5. Por qué no es el mismo test que v8

| | v8 (Deep) | este (T1241) |
|---|---|---|
| Rechazo | error del proveedor (fuera de opciones) | valor dentro del conjunto (`false`) |
| `value` en la respuesta | `NULL` (abstención) | `"false"` (sin abstención) |
| `rules_value` en sombra | `NULL` | `"false"` |
| `agreed` en sombra | `NULL` (no hay qué comparar) | **`0` (desacuerdo medido)** |

Sin este test, un cambio futuro que rompiera el cálculo de `agreed` cuando hay
valor de reglas pasaría desapercibido: el v8 solo observa el `NULL`.

## 6. Re-verificación tras rebase (2026-10-18)

`origin/main` avanzó a `0ed8cbd` (incluye `8181f19` de Deep, que toca
`service.rs` — la vía que este test ejercita). Sin solape con mis ficheros;
fast-forward hecho y **re-verificado sobre `0ed8cbd`**: `decide_adversarial`
**13/13** (`pipefail`, `TEST_EXIT=0`) · `clippy --all-targets -D warnings`
**CLIPPY_EXIT=0** · `check_test_count` **COUNT_EXIT=0** (1134).

## Impact

- Sin cambio de código productivo: solo se añade un test de integración y su
  proveedor canario al fichero de batería; ningún comportamiento observable del
  kernel cambia (mismo wire, mismo `decision_shadow_log`).
- Riesgo nulo para producción; el test fija por contrato que el desacuerdo
  modelo/reglas en un rechazo con valor se registra como `agreed = 0`, lo que
  el arnés pareado (duelo P2 y evaluación 1A) depende para medir.
- No modifica configuración, esquemas ni invariantes de CONTRACT-01.