# T-DSH-120 — Cierre de huecos del informe de T-DSH-119

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = ea963b1` (push184). Diff sin commitear (1 fichero +197 + 1 doc nuevo).
- **Encargo (T1985):** (a) test del reintento OpenAI por HTTP 400 con servidor falso que responde 400 sin `response_format` y 200 con él; (b) test de que `instructions` saneadas llegan idénticas a los 3 dialectos; (c) propuesta por escrito (sin código) de dónde comprobar `DataSensitivity::Clinical` antes de usar adaptadores cloud. Criterio: clippy 0, lib = 1383 + tests nuevos, informe con `## Impact` y "No verificado".

## (a) Reintento OpenAI por HTTP 400 — test nuevo

`openai_retries_once_without_response_format_on_400` (adapters.rs, `t120_gap_tests`):
- Servidor falso que **lee el body completo** y responde `400 {"error":"json_schema no soportado"}` si contiene `response_format`, y `200` con respuestas válidas si no.
- Resultado: `decide()` devuelve la decisión (no abstención), `hits == 2` (exactamente un reintento), el primer body **sí** lleva `response_format` y el segundo **no**.
- Cubre la ruta que antes solo existía en código.

## (b) `instructions` saneadas idénticas en los 3 dialectos — test nuevo

`sanitized_instructions_reach_all_three_dialects_identically`:
- Tres servidores falsos capturan el body de OpenAI/Anthropic/Ollama.
- Petición con `instructions = "elige la ruta segura\u{0007} ahora"` (incluye carácter de control).
- Se extrae `messages[0].content` de cada body y se comprueba: **los tres prompts son byte-idénticos**, contienen `"elige la ruta segura ahora"` (sin el `\u{0007}`) y **no** contienen el carácter de control.
- Con esto se cierra también el "test dedicado de instructions" que quedó como no verificado en T-DSH-119.

## (c) Dónde comprobar `DataSensitivity::Clinical` — propuesta escrita (sin código)

Documento nuevo: `docs/internal/DISENO_clinical_sensitivity_cloud_decision.md`. Resumen:
- La comprobación va **en la frontera del servicio de decisión**, no en los adaptadores: `resolve_model_slot` envuelve el proveedor cloud en un decorador `ClinicalHtlGate<P: DecisionProvider>` (mismo trait; adaptadores intactos).
- El decorador, **antes de cualquier HTTP**, deriva la sensibilidad de la petición (corto plazo: `state.metadata.space/sensitivity` + herencia de recall; medio plazo: campo opcional aditivo `sensitivity` en `DecideRequest`).
- Si `Clinical` + proveedor cloud: exige **aprobación HITL por petición** (reutilizando dispatches approve/reject, hash `decision_row_hash`); sin aprobación ⇒ `Abstained("clinical_requires_hitl")` y **cero** conexiones. Aprobación válida para UNA ejecución (nunca persistente/global), con auditoría sin payload clínico.
- Sensibilidad desconocida + cloud ⇒ **fail-closed** (tratar como Clinical).
- Local (loopback) + Clinical ⇒ pasa sin HITL.
- 6 tests previstos para cuando se implemente (incluido "0 peticiones" y "la aprobación de una petición no autoriza otra").
- Nota operativa: mientras no esté implementado, no configurar puntos de decisión cloud para espacios clínicos.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo test -p tylluan-kernel --lib                          → 1385 passed; 0 failed; 4 ignored (44,69 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Lib: 1385 = 1383 de main + 2 tests nuevos.

## Impact

- Cierra dos huecos reales del informe anterior: el reintento 400 de OpenAI queda **probado** (una sola vez, sin `response_format` en el segundo intento) y las `instructions` saneadas quedan **probadas como idénticas** en los tres dialectos, sin caracteres de control.
- Deja por escrito, sin código, el punto exacto y fail-closed donde debe vivir el HITL clínico para cloud (decorador en el servicio, aprobación por petición, cero conexiones sin aprobación), con tests previstos.

## No verificado

- **Nada de (a)/(b) se probó contra proveedores reales**: los servidores son falsos en loopback; no se midió latencia ni comportamiento real de un 400 de OpenAI/Anthropic/Ollama.
- **El 400 simulado** es una respuesta controlada del fake; un 400 real por otra causa (p. ej. clave inválida) también dispararía el reintento sin `response_format` (el reintento es por status 400, no por inspección del error). Conviene decidir si el reintento debe ser solo para errores de `response_format`.
- **(c) es solo propuesta**: no hay código ni tests de HITL clínico; sigue pendiente de aprobación e implementación (y de decidir si el campo `sensitivity` entra en el wire).
- **No se tocó** `:47004`, `E:\tylluan` ni el 9000 ajeno.
