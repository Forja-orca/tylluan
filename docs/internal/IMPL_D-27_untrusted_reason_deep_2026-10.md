# IMPL D-27 — el motivo de `untrusted_reason` lo fija el kernel

**Item:** D-27 de `COLA_deep_2026-10-22.md` (O1 de T-DSH-22).
**Fecha:** 2026-10-22. **Agente:** Deep. **Diff sin commitear.**
**Base del diff:** `514058e`. **Parche verificado además sobre:** `f5a505b` (`origin/main` al
cerrar). Entre ambos hay 8 ficheros de docs/STATUS/ROADMAP y **un test nuevo**
(`tests/oauth_consent_audit_t24.rs`), **nada en `src/`**: `git apply --check` exit 0 y
`cargo check --tests` aislado exit 0 sobre `f5a505b`.
**Parche:** `E:\tylluan-patches\deep\D-27.patch` — 9.901 B, 4 ficheros, +91/−14.

## Hallazgo (antes)

`security/trust.rs::mark_untrusted` era deliberadamente **no destructivo**: si el productor
ya había puesto `untrusted_content: true`, respetaba su `untrusted_reason` y no lo pisaba
(test `mark_keeps_a_producers_own_reason` lo fijaba como contrato).

Eso era correcto para escritores cuyo metadata **construye el kernel** (ingesta, correo de
agentes, herencia de resúmenes), pero no para `ingest_from_peer`
(`transport/http/api_v1/api_federation.rs:149`), donde `meta` sale del `metadata` que manda
el peer:

- el **flag** ya era intocable (`untrusted_content: false` remoto quedaba en `true`, F3 de
  T-DSH-22);
- el **motivo** no: un peer podía mandar `true` + `untrusted_reason` inventado y sobrevivía
  — incluido `federation_peer:<otro-peer>`, es decir, **suplantar el origen de otro peer**.

No es cosmético: `origin_of()` mete ese motivo en el `banner()` que se antepone al contenido
cuando llega al modelo por una ruta sin gate (bootstrap, `@inbox`, contexto de sesión). Un
motivo controlado por el peer es texto controlado por el peer en el prompt.

El caso estaba **documentado como observación** en `tests/untrusted_writers_integration.rs`
(F4, "el motivo sí es falsificable"), no cerrado.

## Cambio

1. `security/trust.rs`: helper privado `write_mark(meta, origin, force_reason)` y dos
   variantes públicas:
   - `mark_untrusted` (sin cambios de comportamiento): primer escritor gana, para metadata
     construido por el kernel.
   - **`mark_untrusted_authoritative`** (nuevo): pone el flag a `true` y **siempre** escribe
     el motivo del kernel, descartando el que venga en el contenido.
2. `api_federation.rs` (`ingest_from_peer`): pasa a `mark_untrusted_authoritative` con
   `federation_peer:<peer>`.
3. `transport/http/a2a.rs` (`execute_intent_and_store`): el nodo `a2a_result` ya se construía
   a mano con las dos claves; ahora se construye sin ellas y se marca con
   `mark_untrusted_authoritative("a2a external agent: <agent_id>")`. Comportamiento idéntico,
   pero el invariante vive en un solo sitio y una futura fusión de metadata del cliente no
   reabre el agujero.
4. `tests/untrusted_writers_integration.rs`: F4 deja de ser observación y pasa a ser
   **aserción**: peer manda `untrusted_content: true` + `untrusted_reason: "forged by peer"`
   → el nodo queda con flag `true` y motivo `federation_peer:atlas-t22`. Cabecera del test
   actualizada (punto 5).

**Por qué no cambié `mark_untrusted` globalmente:** sus otros dos llamadores
(`memory/agent_memory.rs:390` y `:473`) marcan resúmenes cuyo metadata acaba de construir el
kernel, así que "primer escritor gana" no es explotable ahí y cambiarlo sería un cambio de
contrato sin motivo. Si el TL prefiere una sola función, es borrar la variante no forzada y
pasar el flag a `true` en las dos llamadas de `agent_memory` (2 líneas); no lo hice por
iniciativa propia.

## Verificación

| Paso | Comando | Resultado |
|---|---|---|
| Integración del escritor | `cargo test -p tylluan-kernel --test untrusted_writers_integration` | **1 passed, 0 failed** (5,55 s); F1-F6 + A1-A2 en verde |
| Unit del contrato | incluidos en la lib | `authoritative_mark_overrides_a_forged_reason`, `authoritative_mark_forces_flag_and_reason_over_a_false_claim` |
| Clippy | `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** (41,93 s) |
| Lib (1ª) | `cargo test -p tylluan-kernel --lib` | **1176 passed, 0 failed, 4 ignored** (37,07 s) |
| Lib (2ª) | idem | **1176 passed, 0 failed, 4 ignored** (37,97 s) |
| Parche aislado | `git apply --check` + `cargo check --tests` en worktree limpio sobre `origin/main` | **exit 0 / exit 0** (10m16s, target vacío) |

Las dos ejecuciones de la lib son **secuenciales**, no simultáneas: dos `cargo test` en el
mismo worktree chocan en el linker (`link.exe` 1104) y ese fallo no dice nada del código.
Mismo criterio que en D-25/D-26.

Nota de recuento: la lib sube de **1174 a 1176** (dos tests unitarios nuevos). `1173` era la
cifra tras D-25/D-26 en el worktree; `main` ya trae el ajuste del test `0600` en `514058e`.

## Límites y decisiones que no tomé

- **No reescribo el histórico.** Los nodos ya ingeridos con un motivo falsificado conservan
  el motivo viejo: D-27 corrige la escritura, no migra datos. Un `UPDATE` masivo sobre
  `nodes.metadata` es exactamente el tipo de migración que la cola reserva a José (misma
  frontera que D-24/D-28).
- **Se pierde el motivo "rico" del peer.** Si un peer manda un motivo legítimo y específico
  (p. ej. `"scraped from forum"`), ahora queda `federation_peer:<peer>`. Es lo que pide D-27
  ("pisa siempre"), pero si se quiere conservar para forense sin que llegue al banner, la vía
  limpia es una clave aparte (`untrusted_reason_claimed`) que ningún consumidor lea. No la
  añadí: es superficie nueva y una decisión de contrato, no un arreglo.
- **A2A no tenía ruta de inyección hoy.** `a2a.rs` solo construye metadata de kernel (el
  único `metadata` que viene del cliente es el payload de error, `a2a.rs:679`, que no escribe
  nodo). El cambio ahí es de contrato/prevención, no cierra un agujero abierto; lo digo para
  que nadie lo cuente como hallazgo cerrado.

## No verificado

- Nada contra un kernel **en vivo** (no arranco procesos): todo es `build_router` + `oneshot`
  en proceso. Los tiempos de la tabla son de test, no de producción.
- No probé un **peer real** por la ruta ChaCha/Ed25519 completa: el test usa el harness con
  `ed25519_pubkey` vacío (peer legacy), igual que el resto de `untrusted_writers_integration`.
  La ruta firmada comparte `ingest_from_peer` después de la verificación de firma.
- No hay test que demuestre spoofing **por A2A** porque hoy no existe tal ruta (ver arriba):
  la cobertura es unitaria sobre el contrato + A1 sobre el nodo `a2a_result`.

## Impact

- **Superficie tocada:** `security/trust.rs` (contrato de confianza), dos rutas de escritura
  externas (`api_federation.rs`, `a2a.rs`) y su test de integración.
- **Datos:** ninguna migración, ningún borrado, ningún cambio de esquema. Solo cambia el
  metadata de las escrituras **nuevas**.
- **Comportamiento por defecto:** sin cambios de configuración. Un peer legítimo que enviaba
  un motivo propio ahora aparece como `federation_peer:<peer>`; el flag no cambia para nadie.
- **Seguridad:** cierra la falsificación del origen que se muestra al modelo. No cambia el
  ranking del gate (que solo lee el booleano) ni la tabla de roles de D-11.
- **No depredación:** no toca otros worktrees, no toca `tylluan*.toml`, no toca tokens, no
  toca `dev_mode`, no reescribe datos de producción.
