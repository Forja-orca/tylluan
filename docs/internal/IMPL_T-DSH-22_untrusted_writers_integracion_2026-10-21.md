# T-DSH-22 — Integración de los escritores `untrusted_content` de federación y A2A

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `59d3eaa` == origin/main (push93; T-DSH-20 y T-DSH-21 integradas; worktree en `--detach` limpio).
- **Encargo (turno 1496):** solo tests contra el router real para los escritores que D-18 declaró sin test de integración (`api_v1/api_federation.rs` y `transport/http/a2a.rs`): (1) nodo/mensaje de peer o agente externo queda con `untrusted_content=true` + `untrusted_reason`; (2) nodo local legítimo NO se marca; (3) banner cuando ese contenido llega por bootstrap, `@inbox` y `tylluan_think`; (4) un peer no puede enviar `untrusted_content=false` para saltarse la marca. Tabla escritor × caso × esperado × real; si algo falla, `#[ignore]` con motivo y `archivo:línea`. Informe con `## Impact` y «no verificado».

## Entrega

`crates/tylluan-kernel/tests/untrusted_writers_integration.rs` — un test contra el router de producción (`build_router`): federación por `/api/v1/federation/sync/receive` (peer aprobado con token propio, payload cifrado con su `shared_secret`, envelope firmado) y A2A por `/a2a` `message/send`. **1 passed / 0 failed**; ninguna fila necesitó `#[ignore]`.

```text
rustup run 1.99.1 cargo test -p tylluan-kernel --test untrusted_writers_integration
# test result: ok. 1 passed; 0 failed (5,0 s)
```

## Tabla escritor × caso × esperado × real

| escritor | caso | esperado | real | veredicto |
|---|---|---|---|---|
| federación (`ingest_from_peer`) | nodo normal de peer | flag=true + reason=`federation_peer:atlas-t22` | `mark=true reason=true` | OK |
| federación/local | nodo local por `memory/write` | **NO** marcado | `marcado=false` | OK |
| federación (`ingest_from_peer`) | peer envía `untrusted_content=false` | forzado a true + reason del kernel | `mark=true reason_reescrito=true` | OK (no hay bypass) |
| federación (`ingest_from_peer`) | peer envía true + `untrusted_reason` falso | flag=true (motivo, observación) | `reason='forged by peer'` | OK con observación O1 |
| `tylluan_think` | contenido federado marcado | banner con `origen: federation_peer:atlas-t22` | `banner=true` | OK |
| bootstrap / `@inbox` | contenido federado | no llevan nodos federados (N/A) | `bootstrap_has=false inbox_has=false` | N/A documentado (O2) |
| A2A (`a2a_result`) | resultado de agente externo | nodo `a2a_result` flag=true + reason=`a2a external agent: a2a-t22` | `mark=true reason=true` (task `a2a_…`) | OK |
| A2A (`a2a_result`) | payload intenta `untrusted_content=false` | sigue marcado true | `mark=true` | OK (no hay bypass) |
| `tylluan_think` | resultado A2A (query = contenido) | banner con `origen: a2a external agent: a2a-t22` | `surfacing=true banner=true` | OK |
| `tylluan_recall` | resultado A2A (query = contenido) | si lo surfacea, con banner | `surfacing=false` | OK (no lo surfacea; nada que bannerar, O3) |

## Observaciones

- **O1 — El `untrusted_reason` es falsificable si el peer ya envía `untrusted_content=true`.** `mark_untrusted` solo escribe el motivo si el flag no estaba ya en true (`security/trust.rs`, `mark_untrusted`): un peer puede mandar `{"untrusted_content": true, "untrusted_reason": "forged by peer"}` y el motivo del kernel **no** se impone. El flag (lo que gatea) sí es intocable. Recomendación (requiere `src/`): pisar siempre el motivo en `ingest_from_peer`/`mark_untrusted` con prefijo del kernel, o anexar el original.
- **O2 — bootstrap y `@inbox` no llevan estos tipos de nodo, así que el banner no aplica ahí.** `build_agent_bootstrap` solo lee memorias/resumen del agente (`transport/server/bootstrap.rs:26-56`) y `@inbox` lee mensajes del mailbox; ni la federación ni A2A escriben esos tipos (federación → nodo Silva; A2A → nodo `a2a_result`). El test lo verifica por ausencia; si el requisito era literal (`el contenido debe llegar por esas tres rutas`), eso exigiría consolidación previa del nodo a `agent_memory` (ruta 8 del mapa B-8), fuera de estos escritores.
- **O3 — `tylluan_recall` no devolvió el `a2a_result`** con la query de su propio contenido (surfacing=false), mientras `tylluan_think` sí lo devuelve y con banner. No es un fallo de audit/marca (no hay contenido visible sin banner); sí es una asimetría de superficie de recuperación digna de nota.
- **O4 — A2A marca el RESULTADO, no el mensaje entrante**: `a2a.rs` escribe `a2a_result` tras ejecutar el intent; el texto del mensaje del cliente no se persiste como nodo. Coherente con D-18 (el nodo `a2a_result` es el que lleva `untrusted_reason: "a2a external agent: <id>"`), pero conviene saberlo al leer la tabla.
- **O5 — Firma**: el peer de test no publica `ed25519_pubkey`, así que el camino es el legacy `ChaCha20-Poly1305` con `shared_secret` (mismo harness que `federation_audit.rs`); la marca de desconfianza es independiente de la firma (ver «no verificado»).

## Verificación

| criterio | resultado |
|---|---|
| `cargo test --test untrusted_writers_integration` | **1 passed / 0 failed** (5,0 s), matriz de 10 filas |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1152 passed / 0 failed / 4 ignored** (sin cambios: solo test de integración) |
| `bash scripts/check_docs_reality.sh` | solo el hallazgo **preexistente** de `ROADMAP_O3.md` (no tocado) |

## Impact

- **Diff sin commitear: solo tests** — un fichero nuevo (`tests/untrusted_writers_integration.rs`) y este informe. **Cero cambios en `src/`**, sin kernels arrancados, sin escrituras fuera de directorios temporales, sin secretos reales (identidad/token de test).
- Coste CI: +5,0 s; el intent A2A usado (`whoami`) es un subtool determinista del kernel (no arranca guilds Python).

## No verificado

- **Camino de firma real**: peer con `ed25519_pubkey` (Noise NK + `verify_envelope`) no ejercitado; el peer de test usa el camino legacy ChaCha. La marca no depende de la firma, pero la ruta de verificación sí quedaría por cubrirse.
- **A2A con credencial externa/peer** (no token maestro): el test usa el token maestro; el camino `current_bound_agent_id`/ACL de agente externo no se ejercita.
- **bootstrap/`@inbox` con contenido consolidado**: solo se verifica que estos escritores no escriben esos tipos; no se prueba el caso «el nodo se consolida después en un `agent_summary` y el banner llega por bootstrap» (sería otro test de la ruta 8).
- **`recall` con otras queries**: el surfacing del `a2a_result` puede depender de la query/score (BM25); solo se probó una query derivada de su contenido.
- **Kernel vivo**: no se arrancó (regla de flota); todo va contra el router en proceso.
