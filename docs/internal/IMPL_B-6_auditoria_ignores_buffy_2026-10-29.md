# B-6 — Auditoría de `#[ignore]` (G4 / M-BUFFY-12) — Buffy, 2026-10-29

**Base:** `7ed537e` (= `origin/main`; `git rev-parse --short HEAD` confirmado en el primer turno).
**Solo lectura: no se ejecutó ningún binario** (silencio de carga de PUSH202 en vigor).
**COLA:** `E:\tylluan-patches\COLA_buffy_2026-10-29.md`, ítem **B-6**.

## 1. La cifra de la COLA no cuadra, y el motivo importa

| Medición | Comando | Resultado |
|---|---|---|
| Atributos `#[ignore]` reales | `grep -rn --include=*.rs -E '^[[:space:]]*#\[ignore' crates/` | **35** |
| Líneas que **mencionan** `#[ignore` en `.rs` | `grep -rn --include=*.rs -F '#[ignore' crates/` | 74 |
| Menciones en `docs/` | `grep -rn --include=*.md -F '#[ignore' docs/` | 200 |

La COLA pide «tabla completa, **72 filas**» y sugiere `git grep -n "#\[ignore" -- crates`,
que es justo el comando que cuenta comentarios: da 74 a `7ed537e` (72 antes de que entrara mi
fichero de B-1, que cita el atributo en dos comentarios). **Hay 35 pruebas ignoradas, no 72**;
las otras 39 líneas son comentarios que hablan del atributo. La tabla de abajo tiene **35 filas:
todas las que existen**, una por atributo.

Tras PUSH202 serán **33**: el TL quitó los dos de `bwc_agent_binding.rs` al cerrar B-1 (T2119).

> Nota de contabilidad: **mi propio B-9 añade 1** (`bwc_close_binding.rs`, con motivo escrito), así
> que en mi worktree el `grep` da **36** y tras PUSH202 quedará en **34**. El informe cuenta la
> deuda que heredé (35), no la que añado.
>
> Comando sin barras (inmune al escapado de la herramienta que me duplicó `\[` al publicar):
> `grep -rn --include=*.rs -E '^[[:space:]]*#[[]ignore' crates/ | wc -l`
En este árbol (`7ed537e`) siguen puestos y `api_contracts.rs` **no** compara todavía
`agent_id` con `current_bound_agent_id()` — el arreglo de D-69 **no está en `main`** aún, así que
la premisa de la COLA («tus tests pasan sin `#[ignore]`, 3 de 3») no es cierta en el commit que
estoy auditando.

## 2. Tabla completa (35 filas)

Leyenda de **acción**: `MANTENER` (el motivo se sostiene) · `MEDIR` (posible activar, requiere
ejecución) · `REESCRIBIR` (el motivo caducó pero mide algo real) · `REPUNTAR/BORRAR` (ruta o
supuesto muerto) · `MOTIVO` (falta justificación escrita) · `BORRAR` (ya cerrado).

| # | Fichero:línea | Test | Motivo declarado | ¿Válido? | Acción |
|---|---|---|---|---|---|
| 1 | `tylluan-evals/src/tests.rs:616` | `ab_embedding_models_benchmark` | modelos ~2,8 GB, ~49 min CPU | sí | MANTENER |
| 2 | `tylluan-kernel/src/main.rs:2593` | `ort_compatible_with_real_ort` | exige ONNX Runtime real; CI no lo tiene | sí | MANTENER |
| 3 | `src/memory/silva/graph.rs:1097` | `pagerank_warm_start_benchmark` | benchmark manual, imprime tiempos | sí | MANTENER |
| 4 | `src/memory/silva/search.rs:1758` | `t1277f_scale_ranking_and_counters` | corpus 12,5k nodos con HNSW real, minutos | sí | MANTENER |
| 5 | `src/router/embeddings.rs:1632` | `test_real_inference_bge_m3` | descarga 1,2 GB + ~50 s CPU | sí | MANTENER |
| 6 | `src/router/embeddings.rs:1642` | `test_real_inference_minilm` | descarga + ~50 s CPU | sí | MANTENER |
| 7 | `tests/a2a_live.rs:18` | `a2a_client_against_official_sdk` | requiere el agente echo del SDK oficial | sí | MANTENER |
| 8 | `tests/adr024_containment_pending.rs:114` | `a4_external_mcp_add_without_confirmation_is_rejected` | ADR-024 A4 pendiente de aprobación | sí | MANTENER |
| 9 | `tests/adr024_containment_pending.rs:155` | `d1_discover_proposes_without_registering` | ADR-024 D1 pendiente | sí | MANTENER |
| 10 | `tests/adr024_containment_pending.rs:218` | `b1_config_api_rejects_process_execution_keys` | ADR-024 B1 pendiente | sí | MANTENER |
| 11 | `tests/adr024_containment_pending.rs:256` | `a1_shell_metacharacters_in_command_are_rejected` | ADR-024 A1 pendiente | sí | MANTENER |
| 12 | `tests/adr024_containment_pending.rs:295` | `c3_direct_guild_endpoint_is_operator_class_for_sensitive_guilds` | ADR-024 C3 pendiente | sí | MANTENER |
| 13 | `tests/approve_action_intent_adversarial.rs:143` | `d16_intent_route_adversarial_matrix` | X1 abierto: `agent_id` declarativo consume el plan de otro autor (15/16 filas conformes) | sí | MANTENER |
| 14 | `tests/approve_action_self_approval.rs:144` | `d13_self_approval_requires_operator_credential` | «D-13» (sin más detalle) | sí, pero **motivo escueto** | MANTENER |
| 15 | `tests/asi06_judge_hang_health.rs:76` | `hung_judge_times_out_frees_the_slot_and_is_counted_in_health` | ~95 s: espera el timeout real de 90 s | sí | MANTENER |
| 16 | `tests/asi06_layer1_ingest_memory_write.rs:244` | `d17_ingest_stores_injections_marked_and_never_rejects` | verde con `--ignored`; fuera por spawn del guild knowledge (~60 s, Python) | sí | **MEDIR** |
| 17 | `tests/asi06_layer1_ofuscacion_regresion.rs:266` | `limite_conocido_leetspeak_no_se_detecta` | límite conocido D-31 (clase abierta) | sí | MANTENER |
| 18 | `tests/asi06_layer1_ofuscacion_regresion.rs:273` | `limite_conocido_parafrasis_no_se_detecta` | límite conocido D-31 | sí | MANTENER |
| 19 | `tests/asi06_redteam_memory_poisoning.rs:361` | `bypass_05_leetspeak` | residual abierto: leetspeak (medido 2026-10-24) | sí | MANTENER |
| 20 | `tests/asi06_redteam_memory_poisoning.rs:382` | `bypass_07_parafasis_no_esta_en_la_lista` | residual abierto: paráfrasis | sí | MANTENER |
| 21 | `tests/asi06_redteam_memory_poisoning.rs:451` | `bypass_11_matriz_completa_de_variantes` | residual abierto: 5/6 familias cerradas | sí | MANTENER |
| 22 | `tests/asi06_redteam_vias3_6.rs:409` | `procedencia_resumen_herda_si_la_fuente_estaba_marcada` | la herencia no salva una fuente nunca marcada | sí | MANTENER |
| 23 | `tests/bwc_agent_binding.rs:96` | `deliver_rejects_claim_of_another_bound_agent` | «B-1 abierto» | **caducado tras D-69** | BORRAR (con el push) |
| 24 | `tests/bwc_agent_binding.rs:127` | `vote_rejects_claim_of_another_bound_agent` | «B-1 abierto» | **caducado tras D-69** | BORRAR (con el push) |
| 25 | `tests/continuity_probes.rs:373` | `t25_b2_pending_plan_does_not_survive_a_real_restart` | premisa caducada por D-30; el rojo mide otra divergencia | premisa **no**, divergencia **sí** | **REESCRIBIR** |
| 26 | `tests/knowledge_guild_test.rs:10` | `knowledge_guild_handshake_response` | «requires .venv and guilds/core/knowledge.py» | **ruta inexistente** | **REPUNTAR/BORRAR** |
| 27 | `tests/knowledge_guild_test.rs:57` | `knowledge_guild_extract_triples_valid_json` | ídem | **ruta inexistente** | **REPUNTAR/BORRAR** |
| 28 | `tests/project_skills_store_mismatch.rs:125` | `skill_saved_through_mcp_shows_up_in_the_http_list` | M-BUFFY-14: dos almacenes | sí | MANTENER |
| 29 | `tests/project_skills_store_mismatch.rs:155` | `skill_saved_through_http_can_be_opened_by_get_intent` | M-BUFFY-14: dos almacenes | sí | MANTENER |
| 30 | `tests/project_skills_store_mismatch.rs:190` | `deleting_a_listed_skill_removes_it_from_the_list` | M-BUFFY-14: dos almacenes | sí | MANTENER |
| 31 | `tests/project_skills_store_mismatch.rs:221` | `delete_route_for_a_single_skill_is_registered` | `DELETE /api/v1/skills/<name>` sin registrar | sí | MANTENER |
| 32 | `tests/repo_map_real_measure.rs:57` | `repo_map_build_on_real_repository` | recorrido de disco del repo real | sí | MANTENER |
| 33 | `tests/sparse_cascade_bench.rs:67` | `bench_cascade_gate_rate_latency_and_quality_overlap` | **(sin motivo escrito)** | sí por el código: exige BGE-M3 en caché | **MOTIVO** |
| 34 | `tests/sparse_signature_spike.rs:30` | `spike_sparse_signature_overlap_for_near_duplicate_memories` | **(sin motivo escrito)** | sí por el código: descarga BGEM3 | **MOTIVO** |
| 35 | `tests/trust_banners_adversarial.rs:138` | `d18_trust_banners_adversarial_matrix` | 1 vector abierto de 12 (B2) | sí | MANTENER |

**Recuento:** 30 MANTENER · 1 MEDIR · 1 REESCRIBIR · 2 REPUNTAR/BORRAR · 2 MOTIVO · 2 BORRAR (ya
cerrados). Ninguna fila queda sin motivo **declarado salvo dos**, y esas dos sí tienen motivo
**real** (descarga de modelos): el hueco es de documentación, no de deuda oculta.

## 3. Los casos que exigen acción

### 3.1 Sin motivo escrito (33 y 34) — lo más barato de arreglar

`sparse_cascade_bench.rs:67` y `sparse_signature_spike.rs:30` llevan `#[ignore]` pelado. Leídos,
ambos exigen modelos reales: el primero `SparseEngine::try_new` + `bge-m3` en
`~/.fastembed_cache`, el segundo `SparseTextEmbedding::BGEM3` con descarga. El motivo existe en
el código y no en el atributo: quien audite después vuelve a investigarlo. **Un `#[ignore]` sin
razón es exactamente lo que G4 quiere eliminar**, y aquí cuesta dos líneas.

### 3.2 `t25_b2` (fila 25): motivo caducado, hallazgo vivo

El comentario del fichero (líneas 363-370) reconoce que D-30 (`396af2b`) cambió el
comportamiento a propósito: `store_plan` persiste e `init_plan_store` recarga los pendientes, así
que **un plan pendiente ya sobrevive a un reinicio**. Pero el rojo mide algo real: tras el
reinicio in-process, `get_plan` ve el plan recargado y `list_pending_actions` responde «No pending
actions» — dos vistas del mismo store que no coinciden. **Ni activable tal cual ni descartable**:
el nombre y el assert apuntan a la premisa vieja; hay que reescribirlo para afirmar la divergencia
(o corregir la divergencia y borrarlo).

### 3.3 `knowledge_guild_test` (filas 26 y 27): ruta muerta y módulo invisible

El motivo exige `guilds/core/knowledge.py` y **esa ruta no existe** (verificado:
`find guilds -name knowledge.py` → `guilds/scholars/plugins/knowledge.py`). Además todo el módulo
vive tras `#[cfg(feature = "guild_knowledge_test")]`, apagada por defecto: no corre y no aparece
en ningún recuento. O se repunta a `guilds/scholars/plugins/` o se borra el fichero.

### 3.4 `d17` (fila 16): verde, fuera solo por coste

El motivo declara «pasa con `--ignored`» y lo mantiene fuera porque el marcado hace spawn del
guild knowledge (~60 s, entorno Python). **Es decisión de coste, no bug**: es el único candidato
razonable a entrar en una suite nocturna si CI incorpora Python+venv.

## 4. Sobre «activar los que se puedan sin tocar código»

La aceptación de la COLA pide activarlos «en tu diff y pasando». **No he activado ninguno, y no
por prudencia sino por una razón verificable:** sin ejecutar no puedo demostrar que pasan, y
activar un `#[ignore]` a ciegas es cambiar el estado de la suite por una suposición. El único
candidato por motivo es `d17`, y además no es «sin tocar código» (necesita el guild knowledge
levantable, ~60 s y Python). Los otros dos casos accionables (`t25_b2`, `knowledge_guild_test`)
requieren **editar** el test o la ruta, no solo quitar el atributo. En cuanto se levante el
silencio: `--ignored` por fichero y activación con evidencia.

## Impact

Inventario fiable de `#[ignore]`: **35 atributos, no 72** (los 72 son líneas que mencionan el
atributo, comentarios incluidos; a `7ed537e` son 74 y serán 33 tras PUSH202). De los 35, **30 se
mantienen con motivo válido**, **1 es candidato a activar previa medición** (`d17`), **1 mide algo
real bajo un motivo caducado** (`t25_b2`), **2 apuntan a una ruta inexistente y viven tras una
feature apagada** (`knowledge_guild_test`), **2 no tienen motivo escrito** (los dos `sparse_*`) y
**2 ya están cerrados** (B-1). Cero ficheros de código modificados: solo este informe.

## No verificado

- **Ningún test ejecutado** (silencio de carga): «válido» significa «el motivo se sostiene al leer
  el código», no «el rojo es por esa causa». `t25_b2` demuestra que un `ignore` puede tener el
  rojo por una causa **distinta** de la declarada; pueden existir más casos así, y solo una
  corrida con `--ignored` los separa.
- `d17` se acepta como verde **por su propio motivo** («verificado en T-DSH-15»), no por medición
  mía.
- La cuenta 74→72 es aritmética sobre mis propios ficheros, no una medición en el commit donde el
  TL contó las 72.
- Las filas 23 y 24 (B-1) describen el árbol que estoy auditando; el cambio del TL (atributos
  borrados, `agent_id: None`) **no lo he visto**, solo lo he leído en T2119. Si al llegar PUSH202
  el ajuste dejó de probar la suplantación, lo digo en cuanto pueda leerlo.
