# INFORME — v8 + H1: convergencia con Deep y decisión abierta sobre advisory/gate (@buffy, 2026-10-17)

Encargos: (a) «Falta el test v8 … Se lo asigno: necesita advisory/gate, no sólo shadow» (TL);
(b) «H1 es tuyo asignado por TL». Estado final: **ambos cerrados en el árbol por Deep
(`5a4a806`)**; yo verifico, documento y dejo sobre la mesa la divergencia de diseño que
surge al comparar su cierre con lo que yo tenía en vuelo.

## 1. Cronología honesta (qué pasó con el trabajo en paralelo)

1. Sincronicé mi worktree a `96faf21` (estado integrado del TL: mi `d3d4a85`, fase 2 de Deep
   `34137bf`, deriva de Antigravity `96faf21`) e implementé, sobre esa base:
   - **v8** real en `decide_adversarial.rs` (mock loopback, `should_execute` con risk 0.9 →
     regla HITL `false`; modelo servido a 0.97 `true`; advisory y gate), con hits contados
     por ronda y fila de auditoría unida por `decision_row_hash`.
   - **H1** en `api_decide.rs` vía `Json::from_request` + `map_err` (cuerpo canónico en
     400 sintaxis / 422 forma / 415 content-type, códigos veraces de axum) + test de regresión.
   - **Carril SÍNCRONO advisory/gate** en `service.rs` (`apply_model_signal`): el modelo se
     consulta antes de responder (deadline = min(`max_latency_ms`, `request_timeout`)), la
     señal se registra en la MISMA petición, y en `gate` la regla aditiva §3.3 **capa
     `confidence`** hasta P_modelo(valor de la regla) cuando hay disenso — nunca toca
     `value`/`probabilities`. 3 tests de carril nuevos en `decide_systemone_http.rs` (16/16).
2. Mientras corría la verificación, el checkout compartido avanzó 4 commits
   (`74679ec`, `5a4a806`, `f6c5187`, `f5e2fe1`): Deep cerró **v8 + H1 + H2** con un diseño
   distinto y el TL los integró. Regla de flota §4 aplicada: **no compito, converjo**.
3. Guardé mi diff (`docs/internal/PROPUESTA_advisory_gate_lane_buffy_2026-10-17.patch`,
   794 líneas, sobre `96faf21`), realineé el worktree a `f5e2fe1` (limpio) y verifiqué el
   estado merged por mí mismo.

## 2. Qué verifiqué en `f5e2fe1` (nada heredado de informes ajenos)

| Comando | Resultado |
|---|---|
| `cargo test -p tylluan-kernel --test decide_adversarial` | **12 passed, 0 failed, 0 ignored** — `v8_model_cannot_overturn_deterministic_rejection` ✅ y `h1_malformed_json_returns_canonical_error_envelope` ✅ (el `#[ignore]`+`unimplemented!()` ya no existen) |
| `--test decide_systemone_http` | **13 passed, 0 failed** (regresión de la fase 2 intacta) |
| `--test health_sovereign_count` | **1 passed** (H2: `count == 6` en `/health?verbose`) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1048 passed, 0 failed, 3 ignored** — cuadra con AGENTS.md (1048+74+12 = 1134) |
| `bash scripts/check_docs_reality.sh` | **exit 0** tras mis ediciones (0 ❌; con `74679ec` el allowlist ya exime `:11436`, el puerto del *decima serve* citado en el DISENO de fase 2) |
| `python scripts/check_doc_links.py` | **exit 0** — 220 enlaces relativos en 149 MD, todos resuelven (el punto que el TL dejó abierto) |
| `bash scripts/check_test_count.sh` | **exit 0** — realidad kernel=1048 link=74 fsrs=12 total=1134; README.md y AGENTS.md coinciden |

Criterios del encargo (a) contra el test de Deep: corre en **advisory Y gate** ✅; proveedor
de modelo real inyectable (`inject_model_provider`) ✅; modelo puntúa **a favor** dentro del
conjunto declarado ✅; la respuesta final **conserva el rechazo** y lo prueba con
`model_identity` + fila `model_value` registrada ✅. Cierre aceptado; mi v8 paralelo queda
superado y retirado.

## 3. Mi diff sobre `f5e2fe1` (solo doc-reality, sin commitear, regla 9)

La implementación de Deep cerró los hechos pero dejó la documentación mintiendo en tres
sitios; los arreglé contra el código real:

1. **`docs/reference/adr/ADR021_decision_service.md` §13** — tres bullets:
   - contrato de error de la puerta REST: ya no solo los límites; *cualquier* cuerpo no
     deserializable → `400` + sobre canónico (H1 cerrado, `5a4a806`);
   - «Qué responde hoy»: el proveedor de modelo **ya se puede conectar** (fase 2 loopback
     opt-in, `34137bf`); la abstención `no_model_provider` es el caso por defecto, no la
     ausencia total;
   - «No hecho»: fase 2 **hecha**, v8 **cerrado** (con `inject_model_provider` y la ampliación
     advisory/gate del carril asíncrono), y constancia de que **§3.2 con efecto en respuesta
     no está implementado** (señal vive solo en `decision_shadow_log`).
2. **`docs/internal/IMPL_fase2_systemone_http_2026-10.md` §3** — el bullet «modos
   advisory/gate fuera de alcance» pasaba a falso con `5a4a806`; reescrito con la semántica
   real (consulta asíncrona ampliada, respuesta inalterada) y el pendiente señalado.
3. **`docs/internal/REVISION_commit_44d5828_buffy_2026-10-16.md`** — notas de cierre en H1
   (implementado por Deep en `5a4a806`, su test, su approach, ADR-021 §13) y H2
   (`sovereign_tool_names()` + `/health?verbose` + `health_sovereign_count.rs`). H3-H5 quedan
   abiertos (H3 verificado: `tools.rs:5` sigue apuntando al `CONTRACTS.md` fantasma).

**Observación (no bloqueante):** el approach de H1 de Deep lee `Bytes` en crudo — ya no hay
chequeo de Content-Type (desaparece el 415) y todo cuerpo no parseable responde `400`
canónico. Es un contrato **más simple** que el mío (yo conservaba 400/422/415); es válido y
queda documentado en ADR-021 §13 tal cual está.

## 4. La divergencia que el TL debe decidir (encargo (a), matiz abierto)

El TL pidió «advisory/gate, no sólo shadow». Deep y yo lo interpretamos distinto:

| | Deep (`5a4a806`, integrado) | Buffy (propuesta, `.patch`) |
|---|---|---|
| Cuándo consulta al modelo | asíncrono (dos tiempos), **en la misma petición solo se lanza** | **síncrono**: responde *después* de consultar (deadline `min(max_latency_ms, request_timeout)`) |
| Señal visible para el llamante | **no** — solo `decision_shadow_log` | fila completa al responder + `model_identity` |
| `gate` | idéntico a `advisory` (solo cambia la cobertura de consulta) | **baja `confidence`** con disenso (nunca toca `value`/`probabilities`) — literal ADR-021 §3.3 |
| Cobertura advisory/gate | incluye preguntas **rechazadas/abstencidas** por reglas (`rules_value: Option`) | solo preguntas con **valor** de reglas (abstención de reglas = estado final, sin cambios) |
| Estado | **integrado, verificado por mí** | guardado en `docs/internal/PROPUESTA_advisory_gate_lane_buffy_2026-10-17.patch` (sobre `96faf21`; requiere re-aplicar contra la estructura nueva: `rules_value: Option`) |

Por qué importa (poco hoy, mucho en la adopción §4): con el diseño actual de Deep, un
contrato promovido a `advisory` **no cambia nada observable en la respuesta** respecto a
shadow — la diferencia es solo de auditoría. ADR-021 §3.2 dice que en `advisory` la señal «se
muestra o se añade» y que `gate` puede bajar confianza; eso hoy no existe. Tampoco hay contrato
activo sobre `shadow`, así que **nada está en producción que lo exija ahora**.

**Hueco de cobertura residual (honesto):** el v8 de Deep ejerce el rechazo vía error del
proveedor de reglas (documentado en su IMPL como hallazgo). **Ningún test cubre hoy** el caso
`should_execute = false` (rechazo HITL con *valor*) con el modelo puntuando `true` y
`agreed = 0` — que es exactamente la variante de mi patch (más el cap de `confidence` de
`gate`). Si el TL quiere la semántica literal de §3.2, el patch es el punto de partida; si
prefiere el diseño de Deep (más conservador: respuesta intocable por diseño), el hueco se
puede cerrar con un test de valor de rechazo sobre su mecanismo, sin tocar `service.rs`.

**Recomendación:** cerrar la decisión en Coloquio antes de que un contrato se promueva a
`advisory`/`gate` por config (§4 exige además test pareado). No he tocado `service.rs` ni
`api_decide.rs` en el estado final: no empaqueto un diseño divergente encima del commit de
Deep.

## 5. Límites

- No corrí `guilds_v2_integration` en este base (los 3 fallos de entorno siguen documentados
  en mi revisión de `44d5828` §4: ficheros gitignored ausentes en worktree limpio).
- No he commiteado nada (regla 9); el diff de §3 y este informe quedan sin commitear.

## 6. Impact (ADR-013, para el commit que haga el TL)

- Solo documentación interna (ADR-021 §13, dos docs internos) y un `.patch` de propuesta
  sin aplicar. Cero cambio de código, cero cambio de comportamiento, sin tocar
  `tylluan.toml`, timeouts de guilds, federación ni Coherence Gate.
