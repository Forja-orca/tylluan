# D-32 — Defensa de escritura en las vías que no tenían ninguna (V3..V7)

- **Agente:** Deep (OpenCode)
- **Fecha:** 2026-10-23
- **Base:** `origin/main` = `decc315`
- **Patch:** `E:\tylluan-patches\deep\D-32.patch` — 43 079 B, **11 ficheros**, **+553 / −57**
- **Estado:** implementado y verificado en local; aplicado en limpio sobre `decc315` (`git apply` + `cargo check --tests` aislado). Sin commitear.
- **Dependencia:** D-31 ya está en `main` (`8b858af` + `6f5051d`), así que los venenos en español de los tests casan. El patch **no** incluye D-31.

## Impact

**Qué cambia para un usuario.** Cinco vías de escritura que no tenían defensa alguna
(`POST /api/v1/silva/node` —la ruta de la subida de ficheros y de las guilds de visión—,
`record_memory` del indexado de Coloquio, `tylluan_graph(add_triple)`, `@skill:save` y
`POST /api/v1/silva/save-cluster-summary`) aplican ahora **la misma semántica que `ingest`**:
si el contenido casa con la capa 1, el nodo se guarda **marcado** `untrusted_content` (no se
rechaza: son documentos/mensajes legítimos que pueden hablar de seguridad) y el juez de
capa 2 corre después, fire-and-forget, sobre el nodo ya escrito. `POST /api/v1/silva/node`
deja además procedencia en `metadata.source` (la que declare la guild, o `http_api`).

**Qué NO cambia.** Ningún umbral de gate, ningún valor por defecto de comportamiento
existente, ningún token, `dev_mode` ni datos reales. La única palanca nueva
(`[security] mark_uploads_untrusted`) está **apagada por defecto** y la decisión de
encenderla es de José/TL.

**Riesgo asumido (declarado).** El juez pasa a lanzarse en vías de más volumen
(cada `add_triple` escribe dos nodos → dos juicios). Hoy es barato porque el juez está
roto de facto (D-33: 400 sin `model`, error tragado en `debug!`); **cuando D-33 lo
arregle, este coste se vuelve real** y conviene un límite de tasa o juzgar solo nodos
nuevos. No lo implementé aquí para no cambiar comportamiento por mi cuenta.

**No depredación.** Solo `crates/tylluan-kernel/src/**` y tres ficheros de test. No se
tocó `E:\tylluan`, ni `guilds/`, ni `transport/` fuera de los handlers citados, ni
configuración de nadie. Los tests de Buffy 3–6 se **invierten** (así lo pide el criterio
de D-32) y su test de procedencia queda como estaba.

---

## 1. Diseño (corto, como pedía el encargo)

1. **Una sola regla, un solo sitio.** La marca vivía *inline* en `api_ingest.rs`
   (T-DSH-33, matiz 6: el "ayudante" no existía). Se extrae a
   `security::trust::mark_if_layer1_flagged(meta, content, origin) -> bool`, junto al
   contrato `untrusted_content` (D-18/D-27). Devuelve si marcó, para que el llamante no
   repita el `matches_injection_pattern` (que ya pagó una normalización de D-31).
   Reutiliza `trust::mark_untrusted`, así que el motivo entra en el banner que ve el
   modelo (`origin_of` → `[CONTENIDO NO CONFIABLE - origen: …]`).
2. **Marcar, no rechazar** (V3, V4, V5, V6, V7). Rechazar la subida de un PDF que explica
   inyección de prompts sería un falso positivo de producto; la decisión de D-17 se
   mantiene y ahora es la decisión de todas las vías.
3. **Condicional a la capa 1 en las vías que también guardan texto legítimo**
   (`record_memory`, `record_experience`): la marca no es "todo lo que escribe un agente".
4. **Procedencia en `silva/node`.** `metadata.source` se respeta si la guild la declara
   (`guild:vision`), y se rellena con `http_api` si no. Si el `metadata` no es un objeto
   JSON, se guarda tal cual (no se inventa estructura) y el nodo no se marca.
5. **Capa 2 después de escribir**, en las cinco vías, con el mismo `spawn_write_gate_judge`
   de D-17 (`JUDGE_SPAWNS` cuenta, `in_flight` sube, nunca bloquea la escritura).
6. **Palanca de producto, apagada.** `[security] mark_uploads_untrusted = false`.
   Con `true`, todo lo que entre por `silva/node` queda marcado por procedencia
   (`untrusted_reason = "upload_provenance (…)"`) aunque la capa 1 no detecte nada.

### Fricción de encender `mark_uploads_untrusted` (dato para José/TL, no decisión mía)

- Todo nodo marcado se **penaliza ×0.1** en el recall (CoherenceGate capa 2) y recibe
  **banner** en bootstrap/`@inbox`/resumen de sesión.
- En una instalación normal, la mayoría de nodos creados por `silva/node` **son** subidas
  del usuario (guilds `ingest`, `vision`). Encenderlo equivale a decir "ningún documento
  subido participa al 100 % en la recuperación": un usuario que sube sus propias notas
  vería el banner en cada respuesta que las cite.
- Por eso el defecto es `false`: sin capa 1 no hay marca, y la decisión de tratar la
  procedencia como desconfianza es de producto, no mía.

## 2. Vías: antes → después

| Vía | Sitio | Antes | Ahora |
|---|---|---|---|
| **V3** `POST /api/v1/silva/node` | `api_v1/mod.rs` `silva_create_node_handler` | upsert + embedding, sin capa 1, sin juez, sin procedencia | marca condicional (origen de la guild o `http_api`) + `source` + juez |
| **V3b** `vision_description` | `api_ingest.rs` (descripción de visión) | nodo sin capa 1 ni juez | marca + juez |
| **V4** Coloquio → `record_memory` | `memory/agent_memory.rs` | metadata sin marca (el handler de Coloquio era el sitio equivocado: el nodo lo crea `record_memory`) | marca condicional + juez |
| **V4b** `record_experience` | `memory/agent_memory.rs` | sin capa 1 | marca condicional + juez |
| **V5** `tylluan_graph(add_triple)` | `handler_graph.rs` | dos nodos `concept`, sin capa 1 | marca **en ambos** (subject y object, cada uno con su texto) + juez por nodo |
| **V6** `@skill:save` | `handler_do/skill.rs` | `project_skill` con texto arbitrario | marca + juez |
| **V7** `POST /api/v1/silva/save-cluster-summary` | `memory/graph_rag.rs` `save_summary` (enmienda T-DSH-33) | INSERT directo sin capa 1 | marca en el metadata del nodo + juez tras el commit |
| **V2** `ingest` | `api_ingest.rs` | marca inline | misma marca, ahora vía el helper (sin cambio de comportamiento observable salvo el texto del motivo, que añade `(ingest)`) |

## 3. Aceptación ejecutable

- `tests/cola_d32_acceptance.rs` (DSH): **las 5 vías pierden el `#[ignore]`** y pasan
  (`6 passed / 0 failed`). No hubo que tocar aserciones: afirmaban el comportamiento
  deseado.
- `tests/asi06_redteam_vias3_6.rs` (Buffy): las 4 aserciones de bypass se **invierten**
  (`Some(false)` → `Some(true)`, con mensaje nuevo) y pierden el `#[ignore]`;
  `4 passed / 1 ignored` (el de procedencia sigue ignorado, clase abierta y suya).
- `tests/d32_vias_escritura_regresion.rs` (nuevo, mío): el otro lado del contrato —
  4 tests, todos vivos:
  1. contenido benigno por las 5 vías **no** se marca (incluida una asignación de rol
     en español, la clase que D-31 descartó);
  2. `silva/node` rellena `source: http_api` y **respeta** `guild:vision`;
  3. `mark_uploads_untrusted` es opt-in: apagada no marca, encendida marca con
     `upload_provenance` en el motivo;
  4. la regla de escritura marca **0** de los 340 benignos (40 EN + 300 ES de
     `benchmarks/`).

## 4. Trampa de test encontrada (y documentada en el código)

`test_state` monta `TylluanConfig::load_cached()`: **la config es global al proceso del
binario de test**. El primer intento produjo un fallo intermitente (V3 benigno marcado
como `upload_provenance`) porque otro test encendía la bandera mientras corría. Se
resolvió con un `tokio::sync::Mutex` estático en el fichero de test, que serializa los
tests que tocan la config, y restaurando el valor al final. Queda comentado en el
fichero para el siguiente que añada un test de configuración.

## 5. Verificación

```
cargo check -p tylluan-kernel                                              → exit 0
cargo clippy -p tylluan-kernel --lib -- -D warnings                        → exit 0 (35,93 s)
cargo clippy ... --test cola_d32_acceptance --test asi06_redteam_vias3_6
             --test d32_vias_escritura_regresion --test d17_layer1_write_paths
             --test asi06_layer1_ingest_memory_write -- -D warnings        → exit 0 (16,83 s)
cargo test -p tylluan-kernel --lib                                          → 1188 (29,24 s)
cargo test -p tylluan-kernel --lib                                          → 1188 (30,95 s)  [segunda corrida]
cargo test --test cola_d32_acceptance                                       → 6 passed / 0 failed
cargo test --test asi06_redteam_vias3_6                                     → 4 passed / 1 ignored
cargo test --test d32_vias_escritura_regresion                              → 4 passed (x3 corridas)
cargo test --test d17_layer1_write_paths                                    → 1 passed (56,10 s)
cargo test --test asi06_layer1_ingest_memory_write                           → 1 passed / 1 ignored
cargo test --test asi06_judge_hang_writes                                    → 1 passed
cargo test --test pipeline_tests --test sovereign_tools_e2e                  → 12 + 31 passed
```

Aplicación en limpio (`E:\tylluan-patch-wt32` en `origin/main` = `decc315`):

```
git apply --check D-32.patch → 0 ; git apply → 0
CARGO_TARGET_DIR=… cargo check -p tylluan-kernel --tests → 0 (2m16s)
```

Base de lib: `decc315` tiene **1187**; con el test nuevo de `trust.rs` → **1188**
(los tres ficheros de test de integración no suman al contador de lib).

## 6. Pendiente / decisiones

1. **Producto (José/TL):** ¿se enciende `mark_uploads_untrusted`? Coste descrito arriba;
   yo no lo enciendo.
2. **Volumen del juez:** con D-33 arreglado, estas cinco vías pasan a generar juicios
   reales (cada `add_triple`, dos). Recomiendo decidir un tope o juzgar solo nodos
   nuevos antes de encender D-33 en producción.
3. **Fuera de alcance (T-DSH-33, nota de alcance):** `POST /api/v1/skills`
   (`api_admin.rs`) escribe un `.md` con contenido arbitrario sin capa 1. Es clase
   operador y no está en las 7 vías; queda como hallazgo abierto, no como deuda de D-32.
4. **D-35:** `origin/main` ya trae `4fdfb2e` (parser multilínea + enforce ignora
   `command` en herramientas no-exec) con aceptación `d56f8b8`. **No lo reimplementé**;
   si queda trabajo de D-35, hace falta decir cuál.

## No verificado

- Sin kernel arrancado: los tests usan `test_state` (in-memory) y el router de test; no se
  llamó al puerto 47004 ni a la DB real.
- El juez de capa 2 se **lanza** (contador), pero no se verificó su veredicto: depende del
  backend, que en tests normalmente no existe y en producción está roto hasta D-33.
- La guild `ingest` real (`guilds/core/silva_utils.py`) no se ejecutó; se verificó su
  destino (`silva/node`) con el mismo payload que envía.
- `record_experience` por la ruta MCP: `test_state` no monta el subsistema de agente
  ("Agent memory subsystem not available"), igual que documenta T-DSH-34; la marca se
  verificó en la función (`record_memory`/`record_experience`), no end-to-end.
- No se ejecutó `scripts/verify.sh` (10 min, TL).
