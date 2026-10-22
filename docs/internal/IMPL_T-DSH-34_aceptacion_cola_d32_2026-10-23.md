# T-DSH-34 — Aceptación ejecutable de la cola D-32 (V3..V7) + test vivo de V1/V2

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`f824436`, push 106); limpio antes de empezar.
- **Encargo (turno 1551):** crear `tests/cola_d32_acceptance.rs` con **un test ignorado (`#[ignore = "D-32"]`) por vía V3..V7** que escriba el veneno en español por la vía real (reutilizando el harness de `asi06_redteam_vias3_6.rs`) y afirme el comportamiento **DESEADO** (nodo marcado `untrusted_content` o rechazado); hoy deben fallar por la razón correcta. Más un test **vivo** (no ignorado) que fije que V1 y V2 siguen defendidas con un veneno en inglés de la lista de la capa 1. Criterio: `cargo test --test cola_d32_acceptance` pasa; con `--ignored` fallan 5 por la razón correcta; clippy `-D warnings` exit 0; sin `cfg(unix)/cfg(windows)`; no tocar `src`; informe con `## Impact` y no verificado; diff sin commitear.

## Qué contiene el fichero

`crates/tylluan-kernel/tests/cola_d32_acceptance.rs` (nuevo, `src/` intacto). Reutiliza `mod support;` (`test_state`, `build_test_app`, `build_router`) y los helpers del red-team (`untrusted_flag_of_node_containing`, `call_tool`, `post_json`), con el **mismo literal** de veneno español que Buffy (`VENENO_ES`, `asi06_redteam_vias3_6.rs` línea 40-41). Sin `cfg(unix)/cfg(windows)` en ninguna función.

| test | vía | comportamiento DESEADO (lo que afirma) | estado hoy |
|---|---|---|---|
| `v3_silva_node_debe_marcar_veneno_es` | `POST /api/v1/silva/node` | 201 con nodo marcado `untrusted_content` **o** rechazo 403 | **falla**: 201 y `flag=Some(false)` |
| `v4_coloquio_index_debe_marcar_veneno_es` | Coloquio → `record_memory` (réplica del worker) | nodo `agent_memory` marcado | **falla**: `Some(false)` |
| `v5_grafo_debe_marcar_veneno_es` | `tylluan_graph(add_triple)` | nodo concepto (`subject`) marcado | **falla**: `Some(false)` |
| `v6_skill_save_debe_marcar_veneno_es` | `@skill:save` vía `tylluan_do` | nodo `project_skill` marcado | **falla**: `Some(false)` |
| `v7_cluster_summary_debe_marcar_veneno_es` | `POST /api/v1/silva/save-cluster-summary` | nodo `graphrag_summary` marcado **o** rechazo | **falla**: 200 y `Some(false)` |
| `v1_v2_siguen_defendidas_con_veneno_en` | `memory/write` + `ingest` | V1: 403; V2: 200 + marcado | **pasa hoy** (test vivo) |

Los 5 ignorados fallan **por la razón correcta**: la escritura ocurre y el nodo queda **sin** `untrusted_content`; no fallan por un error del harness. Cuando D-32 (y D-31 para el veneno en español) estén implementados, el TL quita el `#[ignore = "D-32"]` de cada vía cerrada **sin tocar el test**: el comportamiento deseado es el que ya está asertado.

## Salidas pegadas

Corrida normal (solo el test vivo; los 5 rojos quedan fuera de la suite):

```text
test v6_skill_save_debe_marcar_veneno_es ... ignored, D-32
test v7_cluster_summary_debe_marcar_veneno_es ... ignored, D-32
test v1_v2_siguen_defendidas_con_veneno_en ... ok

test result: ok. 1 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 3.61s
```

Corrida `-- --ignored` (los 5 fallan por la razón correcta):

```text
running 5 tests
test v7_cluster_summary_debe_marcar_veneno_es ... FAILED
test v4_coloquio_index_debe_marcar_veneno_es ... FAILED
test v5_grafo_debe_marcar_veneno_es ... FAILED
test v6_skill_save_debe_marcar_veneno_es ... FAILED
test v3_silva_node_debe_marcar_veneno_es ... FAILED

V7 (D-32 pendiente, enmienda T-DSH-33): el nodo graphrag_summary debe quedar marcado
   untrusted_content o rechazarse; hoy status=200 OK flag=Some(false) body={"status":"ok"}
V4 (D-32 pendiente): record_memory debe marcar untrusted_content el nodo del Coloquio
V5 (D-32 pendiente): el nodo concepto (subject) debe quedar marcado untrusted_content
V6 (D-32 pendiente): el nodo project_skill debe quedar marcado untrusted_content
V3 (D-32 pendiente): el veneno ES debe quedar marcado untrusted_content o rechazarse;
   hoy status=201 Created flag=Some(false) body={"ok":true,"id":"document__..."}

test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 1 filtered out; finished in 5.90s
```

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test cola_d32_acceptance` | **1 passed / 0 failed / 5 ignored** (3,61 s) |
| `cargo test --test cola_d32_acceptance -- --ignored` | **5 failed**, los 5 por la razón correcta (salida arriba) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** `?? crates/tylluan-kernel/tests/cola_d32_acceptance.rs` (`src/` intacto) |
| `check_test_count.sh` | exit 0 (1264 sin cambios: test de integración en `tests/`) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

## Impact

- **Solo test, fichero nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no se arrancó el kernel ni el puerto 47004; no se tocó `E:\tylluan`.
- **Para el cierre de D-32**: la lista de aceptación queda **ejecutable**: 5 criterios rojos hoy (V3-V7, con V7 como enmienda de T-DSH-33) y 1 test vivo que impide que V1/V2 se degraden sin que nadie lo note. El mensaje de fallo de cada ignorado dice el estado exacto (`status`, `flag`) y qué se espera, así que el TL no tiene que interpretar nada al quitar el `ignore`.
- **Nota para D-32**: los tests asertan la **marca** (`untrusted_content`), que es la semántica que D-32 pide; no asertan el juez de capa 2 (fire-and-forget) porque su resultado no es determinista en un test sin backend.

## No verificado

- **Con D-32 implementado**: no existe todavía (rama `f824436` sin defensas en V3-V7), así que no se ha comprobado que los 5 tests pasen al quitar el `ignore`; solo que hoy fallan por la razón correcta.
- **V4 con el worker real**: el test llama a `record_memory` directamente (el job `episodic_index` corre en `main.rs`, fuera del router de tests), igual que hace el red-team de Buffy; no se ejecutó el worker end-to-end.
- **V6 `record_experience`**: no es medible con `test_state` (responde "Agent memory subsystem not available"), así que el test vivo/ignorado cubre la mitad medible (`@skill:save`); `record_experience` queda como matiz documentado en T-DSH-33, no como aserción aquí.
- **Juez de capa 2 / cuarentena**: no asertado (solo la marca de capa 1), y el juez puede marcar `quarantined` de forma asíncrona sin afectar a estas aserciones.
- **Semántica "o rechazado"**: V3 y V7 aceptan marca o 403; no se decidió cuál debe ser la definitiva (es decisión de producto de D-32/TL según la fricción descrita en la cola).
- **CI real**: no ejecutado `verify.sh`; local Windows con `CARGO_TARGET_DIR` propio.
