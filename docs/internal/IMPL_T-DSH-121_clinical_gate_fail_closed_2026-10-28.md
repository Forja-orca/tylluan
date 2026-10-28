# T-DSH-121 — ClinicalHtlGate fail-closed (sin cola HITL)

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = ea963b1` + T-DSH-120 (diff sin commitear, en verificación del TL). Diff de este paso: 3 ficheros modificados (+269/−2) + `clinical_gate.rs` nuevo.
- **Encargo (T1993):** implementar **solo** la versión fail-closed del `ClinicalHtlGate`: decorador en `resolve_model_slot`; proveedor cloud + sensibilidad `Clinical` **o desconocida** ⇒ `Abstained("clinical_requires_hitl")` **sin abrir conexión**; local/loopback o `Public`/`Private` conocido ⇒ delega. **Sin cola HITL** (paso siguiente). Tests 1, 3, 4, 5 de la propuesta, con 0 peticiones en el caso bloqueado. Criterio: clippy 0, lib = 1385 + tests nuevos, informe con `## Impact` y "No verificado".

## Implementación (`router/decision/clinical_gate.rs`)

- `Sensitivity { Public, Private, Clinical }` + `sensitivity_from_state(state)`:
  - `state.sensitivity` (string), `state.metadata.sensitivity` (string), o `state.metadata.space` que contenga `"clinical"` (herencia de recall) ⇒ `Clinical`.
  - Sin campo reconocible ⇒ `None` (desconocida).
- `ClinicalHtlGate { inner: Arc<dyn DecisionProvider>, cloud: bool }` implementa `DecisionProvider`:
  - `cloud == true` y sensibilidad `Clinical` **o** `None` ⇒ `DecisionError::Abstained("clinical_requires_hitl")` **antes de llamar al proveedor** (cero conexiones), con `tracing::warn`.
  - `cloud == true` y `Public`/`Private` ⇒ delega en `inner`.
  - `cloud == false` (local/loopback) ⇒ delega siempre.
  - `provider_id()` devuelve el del proveedor envuelto (la identidad del slot no cambia).
- **`resolve_model_slot`**: envuelve con el gate los adaptadores OpenAI/Anthropic/Ollama **cuando el punto resuelve a nube**, y también `systemone_http` remoto (nube). Los proveedores locales/loopback no se envuelven (o van con `cloud=false`), así que no se bloquean.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo test -p tylluan-kernel --lib                          → 1392 passed; 0 failed; 4 ignored (44,60 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Lib: 1392 = 1385 de main + **7 tests nuevos** (5 unitarios del gate + 2 de integración con el slot).
- Test 1 (cloud + Clinical) ⇒ `Abstained`, **0 llamadas** al proveedor (CountingProvider) y en integración con endpoint TEST-NET (no hay servidor: si intentara conectar fallaría con `Provider`, y no lo hace).
- Test 3 (cloud + Public/Private) ⇒ delega (2 llamadas contadas).
- Test 4 (local + Clinical) ⇒ delega (1 llamada); integración: loopback no se envuelve y falla por red (`Provider`), no por política.
- Test 5 (cloud + desconocida) ⇒ `Abstained`, 0 llamadas.
- Extra: parseo de sensibilidad (`sensitivity`, `metadata.sensitivity`, `metadata.space` clínico, ausente).

## Impact

- Un proveedor de decisión **cloud** no puede recibir una petición `Clinical` ni de sensibilidad desconocida: el gate se aplica en la frontera del servicio (decorador en `resolve_model_slot`) y **corta antes de cualquier HTTP** (fail-closed), sin tocar los adaptadores.
- Lo local/loopback y lo `Public`/`Private` conocido siguen funcionando igual; la identidad del proveedor en el slot no cambia.
- Cierra el hueco de seguridad señalado en el diseño: sin cola HITL todavía, el sistema bloquea en vez de arriesgar.

## No verificado

- **Sin cola HITL** (por orden del TL): no hay aprobación por petición; todo cloud+Clinical/desconocida queda bloqueado (no hay forma de aprobarlo todavía).
- **La sensibilidad solo se deriva de `state`** (`sensitivity`, `metadata.sensitivity`, `metadata.space` clínico); el wire `DecideRequest` sigue sin campo `sensitivity`, así que la heurística puede no cubrir todos los casos (p. ej. sensibilidad declarada en otro sitio) — en esos casos cae a desconocida y bloquea (fail-closed, correcto pero puede bloquear de más).
- **Sin prueba en vivo contra proveedores reales**: el caso bloqueado se prueba con CountingProvider y con endpoint TEST-NET sin servidor; el caso delegado con proveedor local solo verifica que no bloquea por política.
- **Sin contador/métrica** de peticiones bloqueadas por el gate (solo `tracing::warn`); no se añadió a `/health` ni a auditoría.
- **`provider_id()` del gate** devuelve el del proveedor interno, así que la identidad del slot no distingue "gated"; si se quiere trazabilidad explícita, habrá que añadir un sufijo/identidad propia.
- **No se tocó** `:47004`, `E:\tylluan` ni el 9000 ajeno.
