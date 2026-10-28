# T-DSH-122 — Campo `sensitivity` aditivo en `DecideRequest` (REST + MCP) y prioridad en el ClinicalHtlGate

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = 1a24e82` (push185). Diff sin commitear (14 ficheros +198/−17).
- **Encargo (T2001):** campo opcional aditivo `sensitivity` (`public|private|clinical`) en `DecideRequest`, expuesto en REST y MCP (norma de José: toda capacidad en ambos), propagado al `ClinicalHtlGate` con **prioridad sobre la heurística de `state`**. Criterio: clippy 0, lib = 1392 + tests nuevos, un test REST y uno MCP con 0 peticiones al proveedor cuando cloud+clinical, wire anterior sin el campo sigue funcionando (test). Informe con `## Impact` y "No verificado".

## Wire (`api_types.rs`)

- `DecideRequest` gana `#[serde(default, skip_serializing_if = "Option::is_none")] pub sensitivity: Option<String>`.
- `validate_request`: valor permitido `public|private|clinical` (case-insensitive); cualquier otro ⇒ error de validación (mismo camino que `max_latency_ms`/`contract`).
- **Compatibilidad**: sin el campo, el wire parsea igual (`None`) — test dedicado.
- REST: `POST /api/v1/decide` usa exactamente este tipo, así que el campo queda expuesto automáticamente.
- MCP: `tylluan_decide` parsea los argumentos a este tipo (`parse_decide_arguments`, helper puro) y el esquema de la herramienta declara `sensitivity` (`type: string`, `enum: [public, private, clinical]`, descripción del bloqueo fail-closed).

## Propagación al gate (`clinical_gate.rs`, `mod.rs`, `service.rs`)

- Núcleo `DecisionRequest` gana `sensitivity: Option<String>`.
- Las **tres** conversiones wire→núcleo propagan el campo: contrato nombrado (reglas), consulta libre síncrona (modelo) y **shadow** asíncrono (se captura antes del `spawn`).
- `sensitivity_from_request(req)`: **prioridad del campo del wire** (`req.sensitivity`), y si no hay/inválido cae a la heurística de `state` (`sensitivity`, `metadata.sensitivity`, `metadata.space` clínico). `ClinicalHtlGate` usa esta función.
- Un valor inválido no llega al gate: `validate_request` lo rechaza antes.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo test -p tylluan-kernel --lib                          → 1401 passed; 0 failed; 4 ignored (46,05 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Lib: 1401 = 1392 de main + **9 tests nuevos**:
- Wire REST (`api_types::t122`, 3): wire antiguo sin campo parsea y valida; `public/private/clinical` parsean y validan; valor inválido rechazado con error de `sensitivity`.
- MCP (`handler_decide::t122`, 3): argumentos antiguos sin campo parsean (`None`); `sensitivity: clinical` se parsea; valor inválido rechazado por `validate_request`.
- Gate (2): **wire clinical sobre state public ⇒ bloqueado, 0 llamadas**; wire public sobre state clinical ⇒ delega (prioridad del wire en ambos sentidos).
- Integración de servicio (1): slot cloud (endpoint TEST-NET, sin servidor) con `sensitivity: clinical` en el wire ⇒ `Abstained` **sin abrir conexión**.
- Los tests 1/3/4/5 de T-DSH-121 siguen verdes (10 en `clinical_gate`).

## Impact

- REST y MCP exponen el mismo campo aditivo `sensitivity`; el llamante puede declarar `clinical` y el `ClinicalHtlGate` lo usa **con prioridad** sobre la heurística del `state`, bloqueando proveedores cloud antes de cualquier conexión (fail-closed).
- El wire anterior sigue funcionando sin cambios (campo ausente ⇒ desconocida ⇒ fail-closed hacia cloud, como en T-DSH-121).
- La propagación cubre también el carril shadow (la consulta asíncrona al modelo lleva la misma sensibilidad).

## No verificado

- **Sin prueba en vivo HTTP/MCP contra un kernel arrancado**: el "test REST" y el "test MCP" son a nivel de wire/parseo + servicio (los mismos tipos que usan ambos caminos); no se hizo `curl`/llamada MCP real al endpoint.
- **Sin proveedor real**: el caso bloqueado usa CountingProvider y endpoint TEST-NET sin servidor; no se midió latencia ni comportamiento con OpenAI/Anthropic/Ollama reales.
- **La sensibilidad no se registra/audita**: no va a `decision_shadow_log` ni a `/health`; solo afecta al gate. Si se quiere trazabilidad del bloqueo, falta un contador/evento.
- **Shadow**: el campo se propaga, pero no hay test de que el carril shadow con proveedor cloud quede bloqueado por el gate (el slot shadow se construye igual; se asume por construcción).
- **Compatibilidad de esquema MCP**: añadir `sensitivity` al `inputSchema` es aditivo, pero clientes con validación estricta de esquema podrían notar el cambio (no probado).
- **`metadata.space` clínico** sigue siendo heurística; con el campo explícito ya hay vía determinista, pero la heurística permanece para compatibilidad.
- **No se tocó** `:47004`, `E:\tylluan` ni el 9000 ajeno.
