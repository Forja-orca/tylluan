# T-DSH-10 — Auditoría adversarial de D-18: `untrusted_content`, banners y localidad

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `3a1312f` == origin/main (D-18 **ya integrado**, push 81). Se audita el código de main, no un parche.
- **Encargo (turno 1464):** intentar (1) que contenido marcado `untrusted_content` llegue a un contexto SIN banner por alguna de las 8 rutas del mapa de B-8, (2) que un remitente externo pase como local en `mailbox_send_handler` (mayúsculas deep/Deep, ids vacíos), (3) que un `agent_summary` pierda la marca. Tabla caso → esperado → real, diff solo de tests, informe con `## Impact` y «no verificado».

## Método

Test-especificación contra el router real (MCP `tools/call` y REST) más las funciones públicas de los gestores, con estado aislado (silva/mailbox/journal en memoria, `TYLLUAN_AUDIT_DB` a temporal). **Cero escrituras en producción**; el kernel vivo no se tocó.

```text
rustup run 1.99.1 cargo test -p tylluan-kernel \
  --test trust_banners_adversarial -- --ignored --nocapture
```

El test queda `#[ignore]` con el motivo: la suite normal es `0 passed; 1 ignored` (exit 0) y con `--ignored` falla HOY por 4 casos abiertos (A4, A5, B2, C2; B3b no se reproduce en la verificación del TL) (exit 101); debe pasar cuando se cierren.

## Matriz (12 casos)

| # | caso | esperado | real | veredicto |
|---|---|---|---|---|
| A1 | bootstrap: memoria marcada | banner visible | banner + payload | OK |
| A2 | bootstrap: resumen marcado | banner visible | banner + payload | OK |
| A3 | `@inbox`: mensaje nuevo marcado | banner visible | banner presente | OK |
| A4 | `@inbox`: payload **legacy** sin clave `untrusted` | banner o backfill | payload presente, **sin banner** | **QUEBRADO** |
| A5 | `tylluan_think`: nodo marcado | banner visible | payload presente, **sin banner** | **QUEBRADO** |
| B1 | `mailbox_send`: remitente desconocido | marcado | marcado (banner) | OK |
| B2 | `mailbox_send`: externo que **se registra identidad** | marcado | **no marcado** (pasa por local) | **QUEBRADO** |
| B3 | `mailbox_send`: `Deep` con identidad local `deep` | marcado | marcado (banner) | OK |
| B3b | `mailbox_send`: `Deep` tras crearse identidad `Deep` | marcado | marcado (banner) en la verificación del TL (2 corridas); DSH reportó no marcado | **no reproducido** |
| B4 | `mailbox_send`: remitente con id vacío | marcado | marcado (banner) | OK |
| C1 | consolidación: resumen hereda marca | marca heredada | heredada | OK |
| C2 | retroactividad: fuente marcada **tras** consolidar | resumen re-marcado | resumen con payload, **sin marca** | **QUEBRADO** |

## Hallazgos

1. **F1 (medio) — `@inbox` no marca payloads anteriores a D-18.** `BlackboardMessage.untrusted` tiene `#[serde(default)]`, así que un payload viejo (sin la clave) se parsea como `false` y se muestra verbatim sin banner (`handler_recall.rs` líneas 226-242). El buzón histórico puede contener mensajes de remitentes externos anteriores al cambio: no hay backfill ni criterio de lectura alternativo.
2. **F2 (alto) — `tylluan_think` (ruta 7 del mapa B-8) devuelve contenido marcado sin banner.** D-18 cubrió bootstrap, `@inbox` y el contexto de sesión, pero `handler_think.rs` no usa `trust::` en ningún punto: la síntesis incluye `n.content` de nodos marcados tal cual (`handler_think.rs` líneas 30-45, filtro solo de `routing_anchor`/`session_digest`/`experience`). Es la ruta por la que un nodo hostil entra en el razonamiento sin señal alguna.
3. **F3 (alto) — la localidad del remitente es "existe una identidad en este kernel", y crearla está abierto.** `mailbox_send_handler` decide `sender_is_local` con `IdentityManager::get_identity(&req.from).is_some()` (`api_v1/mod.rs` líneas 156-180). Cualquier cliente puede crear esa identidad: `register_identity` (herramienta kernel) o el auto-bootstrap de la primera llamada con `agent_id` (`handlers.rs` líneas 50-68). Medido: remitente externo que se registra → mensaje **sin** marca. La frontera de confianza no debería ser "el nodo existe" sino "estaba registrado antes / está en el contrato o atado a un token".
4. **F4 (no reproducido por el TL: B3b da marcado en 2 corridas) (medio) — la caja de `agent_id` cambia la confianza.** Con identidad `deep` sola, `Deep` es externo (bien); con `agent:Deep` también presente (como en producción, ver T-DSH-8), el mismo remitente pasa a local. Es el hallazgo de caja de T-DSH-8 aplicado a seguridad: la canonicalización lo cierra.
5. **F5 (medio) — la herencia de la marca no es retroactiva.** La marca del resumen se decide en su creación (`agent_memory.rs` líneas 377-397 y 460-479). Si la fuente se marca **después** (el juez de capa 2 marca post-escritura; un productor federado actualiza metadata; un re-sync), el resumen conserva el texto verbatim de 80 chars y sigue sin marca (C2). No hay reconciliación.

## Recomendaciones (no aplicadas por dsh)

1. **Lectura defensiva en `@inbox` (F1):** si el payload no trae la clave `untrusted` y el remitente no tiene identidad local, tratarlo como no confiable al leer (o backfill de payloads previos). Barato: una condición en `handler_recall.rs`.
2. **Banner en `think` (F2):** pasar los nodos de síntesis por `trust::mark_text` cuando estén marcados (o excluirlos, como `session_digest`).
3. **Localidad por contrato, no por existencia (F3):** decidir localidad con `.tylluan/agents.toml`/roster previo o con binding token→agent_id, y marcar por defecto lo que se auto-registra.
4. **Canonicalización de `agent_id` (F4):** misma recomendación que T-DSH-8 (helper único en el punto de entrada); cierra la variante de caja en las dos superficies.
5. **Reconciliación retroactiva (F5):** al marcar una fuente, re-marcar los resúmenes que la citan (o pasada periódica de maintenance que propague la marca a `agent_summary`/`session_digest` derivados).

## Impact

- **Diff entregado: un solo fichero de tests** (`crates/tylluan-kernel/tests/trust_banners_adversarial.rs`), `#[ignore]`: suite normal en verde (`0 passed; 1 ignored`, exit 0), CI intacto; `--ignored` falla por la razón correcta (exit 101).
- **Cero cambios en `src/`** y cero escrituras en producción: el estado del test es en memoria y el audit va a temporal; el kernel vivo no se tocó.
- Sin secretos: solo payloads sintéticos y conteos.

## No verificado

- **Ruta 3 (contexto de sesión del recall)**: `server.agent_memory` no está cableado en el estado de test, así que la cabecera avisada (`handler_recall.rs` líneas 192-208) se verificó solo por lectura de código, no ejecutando.
- **Ruta 1 (recall híbrido)**: por diseño D-18 no pone banner ahí (penaliza ×0.1 en el gate); no se midió el ranking, solo que no hay banner (esperado).
- **Rutas 5/6 (Coloquio y buzon REST)**: sus mensajes no llevan metadata de nodo Silva, así que el flag no es aplicable tal cual; no se atacaron.
- **Auto-bootstrap vs `register_identity`**: el bypass B2 se demuestra con `register_identity` explícito; el auto-bootstrap asíncrono de `handlers.rs` no se materializó dentro de la ventana del test (posible carrera con el spawn), así que la vía automática queda como no verificado.
- **Productores reales del flag**: no verifiqué que la flota escriba `untrusted_content` en la práctica (B-8 ya documentó ese hueco del contrato).
