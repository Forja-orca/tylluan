# T-DSH-15 — Verificación de specs: B-12 (ASI06 capa 1) y D-13 (self-approval)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `f07ee4a` == origin/main (post-push86; D-17 y D-13 integrados; lib 1150).
- **Encargo (turno 1481):** (1) copiar y ejecutar `tests/asi06_layer1_ingest_memory_write.rs` (spec B-12 de Buffy) normal y con `--ignored` contra origin/main con D-17: tabla de qué pasa y si algún caso espera algo que D-17 no hace; (2) ejecutar `approve_action_self_approval --ignored` y explicar qué expectativa choca con D-13, recomendando alinear test o código con razón. Diff sin commitear (solo tests si hay que ajustar), `## Impact` y «no verificado».

## (1) B-12 vs D-17 — la spec se cumple, el `#[ignore]` estaba obsoleto

Spec copiada de `crates/tylluan-kernel/tests/` del worktree de Buffy (no estaba en main). Ejecución:

| corrida | resultado | lectura |
|---|---|---|
| normal (spec original, 2 `#[ignore = "D-17"]`) | `0 passed; 2 ignored` | verde, pero sin ejercitar nada |
| `--ignored` (spec original) | **`2 passed; 0 failed`** (~30 s) | **D-17 cumple las 3 partes del contrato** |

Contrato verificado con D-17 integrado: (a) `memory/write` rechaza las 8 muestras de inyección que detecta la capa 1; (b) `ingest` no rechaza ninguna y las guarda con `untrusted_content`; (c) ningún benigno se rechaza ni se marca. **Ningún caso espera algo que D-17 no haga**: la spec estaba escrita contra un mundo sin D-17 y su motivo de `#[ignore]` ya no era cierto.

**Ajuste entregado (solo tests):**

- `d17_memory_write_rejects_the_8_layer1_injections` → **deja de estar ignorado** (corre en la suite normal, ~5,5 s; regresión viva del rechazo).
- `d17_ingest_stores_injections_marked_and_never_rejects` → **sigue `#[ignore]` con motivo actualizado**: el camino de marcado hace spawn del guild `knowledge` y tarda **~70 s** con un fallo de import de Python en este entorno (`ModuleNotFoundError: guilds`, no fatal para el test pero sí ruido/dependencia). Con `-- --ignored` pasa. Decisión: no meter 70 s dependientes del entorno en la suite normal; queda como prueba bajo demanda.

Verificación final: normal `1 passed; 1 ignored` (exit 0, 5,5 s); `--ignored` `1 passed` (exit 0, 70 s).

## (2) D-13 vs `approve_action_self_approval` — el choque es una expectativa más estricta que el contrato implementado

Ejecución: normal `1 ignored`; `--ignored` **FAILED**, exactamente en **CASO 2**:

> `CASO 2 (otro agente aprueba sin operador): esperado rechazo con token operator_required, obtenido HTTP 200 OK: ❌ Error: Plan guild 'd13_probe' not found`

Traducción: D-13 bloquea **solo la autoaprobación** (autor == aprobador → `self_approval`, plan intacto). Cuando **otro agente** aprueba sin operador, D-13 **deja ejecutar** el plan (aquí falla el guild de prueba, pero el plan se consume) y además escribe audit con `human_intervention=1`; por eso la spec arrastra también su aserción negativa de CASO 4 ("no debe haber filas de audit para los intents de los casos 1-2").

### Recomendación: alinear el **CÓDIGO** con la spec (requerir operador en toda aprobación), no el test

Razones, por orden de peso:

1. **Con identidad declarativa, "otro agente" no es una categoría autenticable.** T-DSH-8/9/10 ya demostraron que `agent_id` viaja en `arguments` y que las identidades se auto-crean: un cliente puede crear un plan declarando `agent_id="víctima"` y aprobarlo declarando `agent_id="atacante"`. Para la compuerta, eso es "otro agente" → el gate se esquiva con dos strings. Permitir aprobaciones de no-operador equivale, hoy, a **no tener compuerta**.
2. **El plan mode es HITL por diseño.** La aprobación humana es el propósito de M31-P2; una aprobación agente→agente sin humano convierte `human_intervention=1` en una métrica falsa (ASR/fricción), exactamente lo que el spec de B-7 quería eliminar. Si el código se queda como está, CASO 2 pasa a ser "permitido" y el audit seguiría mintiendo para esas filas.
3. **El coste es bajo y acotado**: en `handlers.rs` (rama `approve_action`), tras el chequeo de `self_approval`, añadir `if !operator { return operator_required; }` con el plan intacto — y que `human_intervention` refleje `operator_gate_allows(...)`. Eso es trabajo de Deep/TL (código), fuera del diff de tests que me pide el encargo.

**Alternativa (peor)**: alinear el test a D-13 (CASO 2 → aprobación permitida, plan consumido). Exigiría además corregir la mentira de `human_intervention` y dejaría el gate esquivable, así que no la recomiendo mientras no exista binding token→agent (decisión de José, propuesta anotada en T-DSH-12). Si se decide el binding, la compuerta de D-13 pasa a ser sólida entre agentes autenticados y esta discusión cambia de dueño.

**Estado del test**: se deja **como está** (spec ignorada) — no se toca en el diff: es la especificación del comportamiento que recomiendo implementar, y sigue siendo el disparador de `--ignored` que documenta el hueco.

## Impact

- **Diff entregado: solo tests**, un fichero nuevo (`crates/tylluan-kernel/tests/asi06_layer1_ingest_memory_write.rs`, copia adaptada de la spec de Buffy) + este informe. **Cero cambios en `src/`**, cero escrituras en producción, sin secretos.
- Coste en CI: +5,5 s (un test nuevo vivo); el lento queda fuera con motivo explícito.
- La recomendación de D-13 es de diseño (una línea de código en `handlers.rs` + audit): requiere que Deep/TL la implementen; no la aplico yo porque el diff pedido era solo de tests.

## No verificado

- **D-17 en el kernel vivo**: la spec se ejecuta contra el router en proceso; no se probó contra producción (regla de flota).
- **El fallo `ModuleNotFoundError: guilds`** del spawn de `knowledge` en esta máquina: el test pasa igualmente (el marcado no depende de que el guild responda), pero no verifiqué si en CI ese spawn funciona y cuánto tarda allí.
- **Aprobación agente→agente en la práctica**: no consulté si la flota usa hoy aprobaciones cruzadas reales; si nadie las usa, exigir operador no rompe flujos.
- **El hueco del audit** (`human_intervention=1` sin humano) se deduce del código y del fallo de CASO 4; no medí su efecto en ASR/fricción sobre datos reales.
