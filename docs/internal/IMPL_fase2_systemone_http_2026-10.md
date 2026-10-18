# IMPL — Fase 2 de `tylluan_decide`: proveedor HTTP `/v1/systemone` (ADR-020)

- **Estado: IMPLEMENTADO — diff sin commitear** (espera revisión/commit del TL).
- **Autor:** Deep. **Fecha:** 2026-10-16. **Base:** `origin/main` `8c9287b`.
- **Encargo:** aprobación por José del `DISENO_fase2_proveedor_http_deep.md` con cuatro decisiones explícitas: (1) migración v29 aditiva + escritura asíncrona en dos tiempos; (2) cliente aislado en `router/decision/`, solo loopback; (3) abstención de reglas como estado final; (4) `sensitivity` y `/v1/decisions` quedan solo como diseño. Tests con servidor HTTP simulado, nunca un modelo real.

---

## 1. Qué se implementó (por pieza, con file:line)

| Pieza | Fichero | Detalle |
|---|---|---|
| Config `[decision.provider]` | `crates/tylluan-kernel/src/config.rs` | Enum untagged `DecisionProviderConfig::{Name, Table}` + `DecisionProviderTable::SystemoneHttp(SystemoneHttpConfig)`; la forma string `provider = "rules"` queda byte-idéntica. Defaults documentados: `request_timeout_ms=5000`, `shadow_timeout_ms=30000`, `max_inflight=1`, `breaker_failures=3`, `breaker_cooldown_s=60` |
| Migración v29 | `src/memory/silva/schema.rs` | `SCHEMA_VERSION 28→29`; bloque `ALTER TABLE decision_shadow_log ADD COLUMN model_identity TEXT / model_latency_ms INTEGER` (aditivo, NULL). Assert de migración extendido en `src/memory/silva/tests.rs` |
| Cliente aislado | `src/router/decision/systemone_http.rs` (NUEVO) | `SystemOneHttpProvider` sobre `reqwest` (ya en árbol, sin deps nuevas): POST `{endpoint}/v1/systemone`, redirects none, connect timeout 1 s; mapeo de preguntas/respuestas **portado del spike de Fase 0** (probado contra servidor real) con sus lecciones T1004 (instructions obligatorio, pregunta inválida no tumba el lote, respuesta parcial no invalida). Loopback-only: `from_config` rechaza cualquier endpoint no-loopback (`is_loopback_endpoint`: localhost/127.x/::1). Semáforo `max_inflight` + cortacircuitos por fallos consecutivos |
| Servicio (dos tiempos) | `src/router/decision/service.rs` | `resolve_model_slot` en construcción (config inválida/no-loopback → warn + reglas, fail-closed); contratos: reglas responden (autoridad intacta) → `INSERT` devuelve rowids → sombra **asíncrona** (`tokio::spawn`, deadline `shadow_timeout`) → `UPDATE` con `model_value`/`model_probs_json`/`agreed`/`model_identity`/`model_latency_ms` juntos o ninguno; consultas libres: carril síncrono con deadline `min(max_latency_ms‖2000, request_timeout_ms)`; `drain_shadow_tasks()` para tests/apagado |
| Semántica del diseño | `service.rs` + `systemone_http.rs` | **Abstención de reglas = estado final** (la sombra solo consulta preguntas con valor de reglas; test con `hits == 0`); respuesta del modelo fuera del conjunto cerrado → RECHAZADA (`model_value` NULL, sin contaminar paired_eval); modo `off` → cero llamadas (ADR-021 §3.2, test con `hits == 0`); timeout → filas NULL y la respuesta sale inmediata; `agreed` = 1/0 solo con ambos valores, NULL en el resto |
| Módulo | `src/router/decision/mod.rs` | `pub mod systemone_http` |
| Ejemplo | `tylluan.example.toml` | Bloque `[decision]` documentado (comentado) con el ejemplo de `[decision.provider]` loopback |
| Tests | `crates/tylluan-kernel/tests/decide_systemone_http.rs` (NUEVO) + 5 unitarios en `systemone_http.rs` | **Servidor simulado** (axum en `127.0.0.1:0`, contador de hits): 13 tests de integración + 5 unitarios (mapeo wire + validación loopback) |

## 2. Evidencia (todo medido en este worktree)

- **Integración: 13/13 verde** en el primer run tras compilar: dos tiempos (agreed 1 y 0), fuera-de-opciones → NULL, timeout sombra → NULL + respuesta < 300 ms, abstención de reglas terminal (hits 0), carril libre OK + fuera-de-opciones + timeout `provider_timeout`, modo off (hits 0), no-loopback → reglas + `no_model_provider`, compat string `"rules"`/desconocido, breaker (3 fallos → 4ª no sale, hits==3), parseo de las dos formas con defaults.
- **Lib: 1046 passed + 3 ignored** (`cargo test -p tylluan-kernel --lib`), total **1132** (1046+74+12); `scripts/check_test_count.sh` ✅; test de migración v29 (columnas presentes + `user_version = 29` + idempotencia) ✅.
- **`cargo clippy -p tylluan-kernel --all-targets -- -D warnings` limpio.**
- Conteo actualizado en el mismo pase (disciplina MD-6): `AGENTS.md`, `STATUS.md`, `CLAUDE.md`, `README.md`, `README.zh.md`.

## 3. Fuera de alcance (acordado con José — solo diseño, NO implementado)

- Campo/máquina de `sensitivity` en el wire y toda la matriz de sensibilidad de ADR-020 §3.2.
- Adaptador `/v1/decisions` (OpenAI): queda como diseño del §7 del DISENO.
- Modos `advisory`/`gate` — **ya NO fuera de alcance** (2026-10-17, `5a4a806`): la consulta ASÍNCRONA al modelo cubre en esos modos TAMBIÉN las preguntas rechazadas/abstencidas por las reglas (`consult_rejected`; `rules_value: Option<String>`, `agreed` solo cuando hay valor de reglas con el que comparar). La respuesta emitida NUNCA se altera (invariante v8). Lo que sigue pendiente es el efecto en respuesta tipo ADR-021 §3.2 (señal visible en el wire / `gate` bajando `confidence`) — decisión del TL, no implementado.
- HITL, dual-space, absorción del shim por el Inference Gateway (cuando aterrice ADR-020 Fase 1), integración con `InferenceBudget` (el semáforo propio cubre la ranura local; documentado en `systemone_http.rs`).
- Nota honesta registrada en código: los timeouts bajo el deadline del *llamante* no cuentan para el cortacircuitos (solo los fallos que el proveedor observa dentro de su propio bound).

## 4. Choreografía para el TL

El worktree acumula **tres entregas sin commitear** sobre `8c9287b`: (a) T1228 (diseño fase 2 + `README.zh.md` + switchers + nota ROADMAP); (b) TC-03 (`[observability]` + conteo 1127); (c) esta Fase 2. `README.md` mezcla hunks de las tres (switcher del zh; y las líneas de conteo que ya van a 1132). Ficheros nuevos: `README.zh.md`, `docs/internal/DISENO_fase2_proveedor_http_deep.md`, `docs/internal/IMPL_fase2_systemone_http_2026-10.md` (este), `crates/tylluan-kernel/src/router/decision/systemone_http.rs`, `crates/tylluan-kernel/tests/decide_systemone_http.rs`, `crates/tylluan-kernel/tests/slo_target_config.rs`.

## 5. Impact (ADR-013 — para el mensaje de commit)

- **Comportamiento por defecto byte-idéntico**: sin `[decision.provider]` la resolución cae a reglas y los tests de Fase 1 pasan sin cambios; ninguna llamada de red nueva se produce jamás sin configuración explícita del operador.
- **Salida de red nueva**: solo por loopback, solo con `[decision.provider]` configurado, nunca con el servicio apagado ni en modo `off`; endpoints no-loopback son rechazados en construcción (fail-closed).
- **Datos**: el `state` sigue sin guardarse en claro (solo hash); las columnas nuevas son aditivas y NULL; ninguna herramienta/módulo existente cambia de contrato.

## 6. Verificación sugerida

```bash
cargo test -p tylluan-kernel --test decide_systemone_http
cargo test -p tylluan-kernel --lib && bash scripts/check_test_count.sh
cargo clippy -p tylluan-kernel --all-targets -- -D warnings
```
