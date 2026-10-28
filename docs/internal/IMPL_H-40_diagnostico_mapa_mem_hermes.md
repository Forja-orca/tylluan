# IMPL_H-40 — Diagnostico mapa MEM: save_embedding y recall (hermes, solo lectura)

**Turno asignacion:** T1949 (TL claude-code). **Reentrega H-40b ordenada en T1959** (TL leyo el informe pidio: base origin/main, restaurar nodes.rs, clippy `--all-targets -D warnings`=0, tests con cifras, LF). **Base actual:** `c005979` (origin/main tras PUSH178; diff rebaseado sin conflicto — el unico cambio upstream en mis ficheros son 5 `#[allow(dead_code)]` de T-DSH-115 en zonas alejadas). Base de la primera pasada: `399db1f`; respaldos en `E:\tylluan-patches\hermes\_saved\20261028-064419\` y `...\20261028-093722\` (pre-rebase). **Tipo:** diagnostico solo lectura + diff propuesto SIN commitear. Fecha: 2026-10-28.

**Nota de transparencia (estado intermedio que vio el TL en T1959):** entre la primera lectura del TL y esta reentrega, el worktree se rebaseo externamente a 399db1f a mitad de sesion y un `git checkout -- nodes.rs` propio (para limpiar un diff inflado por EOL) dej6 transitoriamente nodes.rs sin su parte de B1 mientras coherence_gate ya la llamaba — el "no compila" que observo el TL fue real en esa ventana y ya no existe: nodes.rs esta reaplicado (+22) y todo compila y pasa sobre 399db1f primero y sobre c005979 ahora (cifras abajo). Los ficheros llegan en LF puro (`git diff | grep -c CR` = 0, `git ls-files --eol` = i/lf w/lf x3).

El mapa (`docs/internal/mapa_bloques.json:27`, bloque MEM, escrito en `79f88c6` 2026-10-26) afirma: "save_embedding ignorado en remember; recall filtra 30 de 33 resultados". Verificadas ambas contra el codigo y los datos vivos.

## A) "save_embedding ignorado en remember" — NO se reproduce (arreglado upstream)

- Ruta actual: `handler_remember.rs:512` → `persist_memory_embedding` (`handler_remember.rs:58-76`) → `SilvaDB::save_embedding` (`memory/silva/nodes.rs:463`). El fallo ya NO se descarta: avisa con `node_id`/`agent_id` (warn, `handler_remember.rs:47-53`) y suma el contador visible `EMBEDDING_SAVE_FAILURES` (`:35`).
- Arreglado en `79f88c6` (T-DSH-94, 2026-10-26) — el MISMO commit que escribio la afirmacion del mapa: el mapa quedo desactualizado respecto a su propio fix.
- Prueba (tests existentes en el repo, ejecutados esta sesion): `cargo test -p tylluan-kernel --lib embedding_save_failure` → `embedding_save_failure_warns_and_counts` y `embedding_save_failure_keeps_the_node_stored` en verde. Salida real pegada abajo (seccion Verificacion).
- Matiz honesto, NO es la ruta `tylluan_remember`: 5 sitios fuera del handler si mantienen el `let _ =` (fallo silencioso): `main.rs:1031` (embeddings de propuestas), `main.rs:1643` (job coloquio→memoria), `api_v1/api_memory.rs:144`, `api_v1/mod.rs:745` y `:908`. Queda marcado como candidato, sin tocar (la queja del mapa es "en remember").
- Segundo matiz: si `matcher.engine()` es `None` (sin motor de embeddings), remember omite el embedding SIN warn (rutas `handler_remember.rs:218` y `:507`). El contador T-DSH-94 solo cuenta fallos de persistencia, no ausencia de motor. Tampoco lo toco; queda flag.

## B) "recall filtra 30 de 33 resultados" — REAL, pero el mecanismo es entierro x0.1, no eliminacion

**Que pasa de verdad:** el Coherence Gate penaliza (score x0.1) a TODO candidato cuyo coseno(query, nodo) < 0.85. En produccion eso es casi todo: medido en `E:\tylluan\logs\kernel.log*` (lectura solo lectura): **1698 lineas del gate, 63.711 candidatos evaluados, 62.955 penalizados (98,8 %), 59 eliminados (0,1 %)**. Muestras literales: `Coherence Gate: 0 eliminated, 50 penalized of 50 candidates` (x640), `0 eliminated, 30 penalized of 30 candidates` (x753+308), `29 penalized of 30` (x90). El "30 de 33" del mapa es un muestreo de esta misma linea con 33 candidatos.

**Ruta exacta (fichero:linea en 399db1f):**
1. `transport/server/handler_recall.rs:638` y `:874` — `CoherenceGate::filter_with_scope(...)` en ambas rutas (cache-hit y cache-miss).
2. `security/coherence_gate.rs:863-909` — bucle por candidato; sin patron inyectable (no elimina), despues capa semantica `:892-902`: `score *= SEMANTIC_PENALTY` (0.1, `:32`) si `cosim < COHERENCE_THRESHOLD` (0.85, `:30`).
3. Alcance por defecto `"all"`: `security/coherence_gate.rs:785-793` (`semantic_scope_penalizes`), config default `config.rs:1535` (`coherence_semantic_scope`), cableado `main.rs:1359`. `E:\tylluan\tylluan.toml` tiene `[security]` (`:220`) SIN la clave → corre "all".
4. Por que 0.85 es inalcanzable: en space de embeddings, pares query~recuerdo relevantes tipicos quedan en 0.5-0.8; 0.85 solo se acerca a duplicados casi exactos. Con scope "all", el gate trata como "sospechoso" el 100 % del recall normal.
5. **Agrava: el gate compara contra embeddings de modelos DISTINTOS sin filtrar.** `memory/silva/nodes.rs:576` — `SELECT node_id, embedding FROM node_embeddings WHERE node_id IN (...)`, sin `model_name`. Medido en copia readonly de `E:\tylluan\data\silva.db`: 5.821 embeddings → `mxbai-embed-large-v2-onnx` 3.943, `bge-m3-v2-onnx` 1.087, `distill-v1` 785, `bge-m3` 6. El 32 % (1.878) son de modelos anteriores; su coseno contra querys mxbai es ruido sistematicamente bajo → penalizacion garantizada a priori.
6. Efecto aguas abajo (por que se "filtra" visualmente): los supervivientes conservan el score ya x0.1; `handler_recall.rs:934` y `:949` reordenan POR ESE score; LightReranker recibe `score_rrf = score penalizado` (`:994`); y `scored.truncate(limit)` con limit default 5 (`:159-160`, línea 1011). Un penalizado (0.09) jamas gana a un no-penalizado (0.95): queda fuera del top y desaparece del resultado. En los tests: `survivors.len()==33` (nadie se elimina) pero 30 enterrados.

**Reproduccion (test unitario SIN tocar produccion):** `h40_default_scope_penalizes_realistic_recall_pairs` en `security/coherence_gate.rs` (diff sin commitear): 33 nodos con `SilvaDB::in_memory()`, query [1,0,0]; 3 alineados (cosim 1.0) y 30 legitimos con cosim 0.75..0.68. Afirma `penalized==30 of 33, eliminated==0` y los x0.1. Ejecucion real y salida abajo.

## Causa

1. Umbral `COHERENCE_THRESHOLD=0.85` calibrado como "coherente" (casi-duplicado) pero usado con scope `all` como filtro de riesgo: penaliza recall legitimo por defecto.
2. Comparacion de embeddings cross-model sin filtro de `model_name` en la lectura batch del gate.
3. El castigo x0.1 se propaga a reordenados y al reranker, y `truncate(limit)` remata: el usuario ve "desaparecen resultados".

## Arreglo propuesto (diff SIN commitear; decision del TL)

- **B1 (APLICADO como diff local SIN commitear en este worktree):** el gate solo compara embeddings del MISMO modelo que el de la query. Ficheros tocados (números de línea del working copy con el diff): `memory/silva/nodes.rs:570-605` (`get_node_embeddings_batch` gana `model_name: Option<&str>`; NULL = legacy; `model_clause` numerada `:583-587`; params encadenados `:596-598`), `security/coherence_gate.rs` (`filter_with_scope` gana `query_model_id: Option<&str>` en `:834`; wrapper `filter` pasa None = comportamiento intacto; call-site Layer 3 `:861`; Layer 4/juez híbrido pasa None con flag explícito en línea 1006 — es opt-in y apagado por defecto), `transport/server/handler_recall.rs:638` y `:875` (pasan el `engine_id()` del motor que embeddeó la query). Sin embedding o modelo distinto → no penaliza (misma semantica que "sin embedding", probada por `candidates_without_embedding_are_not_penalized`). Test propio: `h40_gate_never_compares_cross_model_embeddings` (cross-model no penaliza; legacy sigue comparando todo).
- **B2 (propuesta de 1 linea SIN aplicar — politica de seguridad, es del TL):** `tylluan.toml` `[security]` → `coherence_semantic_scope = "conflicting_top"` (via ya existente desde T-DSH-96: solo penaliza fuertes-en-desacuerdo, `coherence_gate.rs:790`). Reduce el castigo masivo sin tocar el umbral. **Postura registrada por el TL en T1959: "recomiendo SI; decision para Jose".** No lo aplico: es postura de seguridad del producto y la decide Jose.
- **B3 (no es diff, es operacion):** re-embed de los 1.878 nodos con `model_name` obsoleto (bge-m3-v2-onnx 1.087, distill-v1 785, bge-m3 6). Con B1, esos nodos dejan de penalizarse YA; el re-embed les devuelve busqueda vectorial correcta. Lo decide el TL (carga ONNX en CPU).
- **B4 (no aplicado, flag):** umbral 0.85 y `SEMANTIC_PENALTY=0.1` seguiran aplastando el 90 %+ del recall mxbai-vs-mxbai legitimo. Bajando el umbral (o haciendo configurable 0.85) se arregla el fondo, pero cambia la postura de seguridad ADR-011: requiere decision explicita del TL+Jose, no la propongo como diff para no invadir ese diseno.

## ## Impact

- B1 toca `memory/silva/nodes.rs` (helper de lectura) y `security/coherence_gate.rs` + `handler_recall.rs` (cableado). Si falla: el gate vuelve a "no penalizar por semantica" (degradation suave: sin security false-positives; recall mas ruidoso, no roto). No toca transporte ni `tylluan*.toml`. Tests del gate (15+) cubren el path.
- B2 cambia la postura del gate en produccion: menos penalizaciones (solo top en conflicto). Riesgo si se activa sin mirar: candidatos medios mal alineados dejan de bajar de puesto.
- Sin B1/B2: el recall del producto sigue enterrando el ~99 % de sus propios candidatos; el pilar Recordar queda degradado silenciosamente en cada `tylluan_recall`.

## No verificado / limites

- [no verificado] El valor EXACTO "33" del mapa: no hay ninguna linea de log con "of 33 candidates" (contado en los 3 rollados); el mapa posiblemente midio un run no persistido o redondeo de "30 penalized of 30". El fenomeno esta si medido (98,8 %).
- [no verificado] Que la caida del 90 %+ de coseno mxbai-vs-mxbai legitimo sea el rango universal: la franja 0.68-0.75 del test es una eleccion razonable (distancias tipicas de mxbai), no una medicion distribucional nuestra. Una medicion real de cosenos query→nodo en silva.db lo cerraria (requiere correr el motor ONNX completo: no lo hice para no cargar la maquina).
- [no verificado] antiguedad real de `distill-v1` (no aparece en config actual; lo encontro solo la DB).
- No toque produccion: analisis de silva.db sobre URI `mode=ro&immutable=1` (sin escrituras, sin checkpoints forzados). No reinicie nada. No commitee nada.

## Senderos abiertos (esta entrega)

| cosa | estado | que falta | esfuerzo |
|---|---|---|---|
| B1 cross-model gate fix | diff local sin commitear, tests del gate en curso | TL revisa e integra (el commitea) | S para el TL |
| B2 scope `conflicting_top` en tylluan.toml | NO aplicado (postura de seguridad) | decision TL+Jose | XS |
| B3 re-embed 1.878 nodos legacy | NO hecho (carga ONNX CPU) | decision TL | M |
| B4 umbral 0.85 / penalizacion 0.1 | flag, sin diff | rediseno ADR-011 deliberado | M-L |
| `let _ =` en 5 call sites de embedding fuera de remember | flag, sin tocar | nueva H si el TL quiere | S |
| `AUTOMATIONS.md` en E:\tylluan sin trackear | NO es de hermes; TL confirmado en T1959 ("no es tuyo, gracias") | queda en manos de Antigravity/TL | — |
| H-39 | entregada T1927+T1930 | en cola de revision del TL (no re-pedir) | — |

## Verificacion (comandos y salida real, 2026-10-28, worktree en 399db1f)

1. Afirmacion A NO se reproduce — tests T-DSH-94 en verde sobre la base actual 399db1f (compilada esta sesion con el diff B1 aplicado):
```
$ FASTEMBED_CACHE_DIR=E:/cache/fastembed cargo test -p tylluan-kernel --lib -- embedding_save_failure
test transport::server::handler_remember::embedding_failure_tests::embedding_save_failure_warns_and_counts ... ok
test transport::server::handler_remember::embedding_failure_tests::embedding_save_failure_keeps_the_node_stored ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 1343 filtered out; finished in 0.05s
```
2. Afirmacion B reproducida (test unitario nuevo, SIN kernel de pruebas ni produccion):
```
$ cargo test -p tylluan-kernel --lib -- h40_default_scope_penalizes_realistic_recall_pairs
test security::coherence_gate::tests::h40_default_scope_penalizes_realistic_recall_pairs ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1343 filtered out; finished in 0.08s
```
3. Produccion (solo lectura, sin tocar nada): 1698 lineas "Coherence Gate: X eliminated, Y penalized of Z candidates" en `E:\tylluan\logs\kernel.log*` → 62.955/63.711 penalizados (98,8 %). Distribucion `node_embeddings` de copia readonly de silva.db: mxbai 3.943 / bge-m3-v2 1.087 / distill-v1 785 / bge-m3 6 (total 5.821).
4. Suite completa del gate con B1 aplicado (umbral 52 tests = 50 previos + 2 nuevos), worktree con diff local:
```
$ FASTEMBED_CACHE_DIR=E:/cache/fastembed cargo test -p tylluan-kernel --lib -- coherence_gate
test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 1293 filtered out; finished in 1.45s
(tests previos en verde con B1: candidates_without_embedding_are_not_penalized, gate_fetches_embeddings_in_one_batch_round_trip, semantic_scope_conflicting_top..., layer3_penalizes_semantic_drift_from_query, hybrid_classify_is_inert_in_test_builds)

$ cargo test -p tylluan-kernel --lib -- cross_model
test security::coherence_gate::tests::h40_gate_never_compares_cross_model_embeddings ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1344 filtered out
```
5. `python scripts/check_doc_links.py` → exit 0 (184 relative links, 451 ficheros).
6. **Reentrega H-40b sobre c005979** (diff rebaseado via stash pop sin conflicto; solo 5 `#[allow(dead_code)]` upstream en la zona):
```
$ git diff --stat          # 3 ficheros, +126/-7; CRLF en diff = 0; ls-files --eol = i/lf w/lf x3
$ cargo clippy -p tylluan-kernel --all-targets -- -D warnings
   (unica linea de salida: "warning: tylluan-kernel@0.17.2: bundled-dashboard feature not enabled" — aviso del build script)
   CLIPPY_EXIT=0
$ cargo test -p tylluan-kernel --lib
test result: ok. 1346 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 41.84s
   (= 1344 upstream en c005979 + 2 tests propios h40)
```
