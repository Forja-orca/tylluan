# IMPL — T1241: los 4xx de la entrada no abren el cortacircuitos; 422 STATE_TRUNCATED → abstención estable

- **Estado: IMPLEMENTADO — diff sin commitear.** Base: `origin/main` `6b9b177`.
- **Autor:** Deep. **Fecha:** 2026-10-17. **Encargo:** Coloquio T1241 (hallazgo medido en vivo por el TL con Decima-small).

## Qué estaba roto

`systemone_http.rs` trataba CUALQUIER respuesta no-2xx como fallo del proveedor (`note_failure`), incluidos los 4xx causados por la ENTRADA. Caso real: estados largos → Decima responde `422 STATE_TRUNCATED`; 13 de 245 casos lo provocaron y los siguientes 30 cayeron con "circuit breaker open" — un llamante con estados grandes abría el breaker para todos (solo el carril del modelo; las reglas siguen respondiendo).

## Qué cambia

1. **`router/decision/systemone_http.rs`** — el bloque no-2xx ahora distingue:
   - `422` + body que contiene `STATE_TRUNCATED` (case-insensitive) → `DecisionError::Abstained("state_too_long")`, **sin** `note_failure`.
   - resto de 4xx **salvo 429** → `DecisionError::Provider` (el error se sigue surfacando al servicio) **sin** `note_failure` — la causa está en la entrada, no en el proveedor.
   - `429` y `5xx` → `note_failure` como antes (fallo del carril: rate-limit / servidor caído).
   - red, decode y "ninguna respuesta utilizable" → `note_failure` como antes.
2. **`router/decision/mod.rs`** — variante nueva `DecisionError::Abstained(String)`: proveedor se abstiene con motivo estable; el servicio la traduce a abstención sin prefijo. Sin matches exhaustivos en el repo (verificado) → adición segura.
3. **`router/decision/api_types.rs`** — `pub const ABSTAIN_STATE_TOO_LONG: &str = "state_too_long"` junto a los demás motivos del esquema.
4. **`router/decision/service.rs`** — en los tres puntos donde un `Err` del proveedor se traducía a abstención con prefijo (`"provider error: {e}"` en el carril de contrato, `"provider_error: {e}"` en el libre, warn en la sombra), el brazo `Err(DecisionError::Abstained(reason))` viaja **tal cual**: razón exacta `state_too_long`, sin dump del HTTP. Sombra: log a debug + filas NULL (semántica de evaluación pareada intacta).
5. **Parte opcional del encargo — `config.rs`**: `[decision.provider] state_max_chars` (usize, default `0` = sin límite): si la representación JSON de `state` lo supera, el proveedor **ni siquiera llama** al servidor (abstención sin coste de red). Nota: el contrato wire ya impone 16 KiB (`MAX_STATE_BYTES`); este tope es para el límite real del modelo (típicamente < 16 KiB).

**Decisiones que tomé yo (no atribuidas a José):** (a) el 422 solo mapea a `state_too_long` si el body contiene `STATE_TRUNCATED` — otros 422 siguen siendo `Provider` sin contar para el breaker; (b) default del tope = 0 (sin límite) para no cambiar el comportamiento de nadie hasta que se configure; (c) razón exacta `state_too_long` (el "p. ej." del TL).

## TDD (rojo → verde)

- **Rojo** (3 tests, con las aserciones del bug en vivo): 400 → hits=1 (esperado 2: el breaker se abrió); 422 → razón `provider_error: … HTTP 422 …` (esperada `state_too_long`); sombra 422 → hits=1 (esperado 2). La guarda `breaker_still_counts_429_and_5xx` ya era verde.
- **Verde**: `tests/decide_systemone_http.rs` **18/18** (los 13 de fase 2 intactos + 5 nuevos: `client_4xx_does_not_count_toward_breaker`, `state_truncated_422_is_stable_abstention_and_breaker_safe`, `shadow_422_state_truncated_leaves_rows_null_and_breaker_closed`, `breaker_still_counts_429_and_5xx`, `state_max_chars_cap_skips_the_wire` + assert del default en el test de config).

## Evidencia

- `cargo test -p tylluan-kernel --test decide_systemone_http` → 18/18.
- `cargo test -p tylluan-kernel --test decide_adversarial` → 12/12 (v8 intacto).
- Lib: **1048 + 3 ignored** (sin tests lib nuevos → conteo estable); `check_test_count.sh` ✅ 1134.
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` limpio.

## Ficheros

`router/decision/{systemone_http.rs, service.rs, mod.rs, api_types.rs}` · `config.rs` · `tests/decide_systemone_http.rs` · este informe.

## Impact (ADR-013)

- **Disponibilidad del carril modelo**: un 4xx de la entrada ya no puede tumbar el carril para todos (el breaker solo cuenta fallos del carril: red, 429, 5xx). El incidente en vivo (13/245 → 30 ceros) queda cerrado con motivo estable.
- Sin cambio de esquema de respuesta: la abstención `state_too_long` usa el mecanismo existente (`Answer.abstained` + `abstain_reason`); las filas de sombra siguen quedando NULL.
- Sin cambio por defecto: `state_max_chars = 0` (desactivado) y el resto de defaults intactos; `provider = "rules"` byte-idéntico (test existente en verde).
- El contrato wire (16 KiB, 16 preguntas) no cambia.
