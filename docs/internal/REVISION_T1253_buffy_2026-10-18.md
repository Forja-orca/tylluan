# REVISIÓN ADVERSARIAL T1253 (Buffy) — endpoints `decide/shadow` y `silva/conflicted`

- **Fecha:** 2026-10-18 · **Revisor:** Buffy (turno 1268 del TL, aviso T1269).
- **Objeto:** diff SIN commitear de Deep en `E:\tylluan-wt-deep` sobre `743a706`
  (= `origin/main`): 5 ficheros modificados + `tests/decide_admin_read_api.rs`
  + su informe `IMPL_T1253_shadow_conflicted_deep_2026-10.md`.
- **Método:** el diff se copió (git patch + fichero de test) a MI worktree
  (`E:\tylluan-wt-buffy`, realineado a `743a706`) y se ejecutó TODO aquí.
  El worktree de Deep no se tocó. Añadí una suite adversarial propia
  (`tests/t1253_buffy_adversarial.rs`, 6 tests) para los casos que el test
  de Deep no cubre.

## VEREDICTO: ✅ APTO PARA COMMIT — sin hallazgos bloqueantes

Los cuatro puntos del encargo (T1268) verificados con test propio donde fue
posible. Tres notas cosméticas no bloqueantes al final.

## 1. No se filtra `state` ni texto libre en shadow — VERIFICADO

- El handler serializa **a mano** los 11 campos congelados
  (`api_decide.rs`, mapeo `rows` del `decide_shadow_handler`); ni
  `model_probs_json` ni `model_latency_ms` viajan, aunque `read_shadow_log`
  los SELECTe (`service.rs:721-723`) — se quedan en el struct interno.
- `request_hash` = SHA-256 de `state || 0x1f || pregunta`
  (`service.rs:51-59`): irreversible, solo hash.
- Test de Deep `shadow_response_carries_no_state_or_question_text`:
  marcador `T1253_LEAK_MARKER_XYZ` en el `state` de un `decide()` real →
  ausente de la respuesta; claves por fila == exactamente las 11.
- Test propio `shadow_no_question_text_and_summary_coherent` amplía la
  prohibición a las claves `state`, `question`, `instructions`,
  `model_probs_json`, `model_latency_ms` en TODO el cuerpo: ausentes.
- `rules_value`/`model_value` sí viajan, pero son los VALORES de respuesta
  del contrato (opciones fijas del contract), no texto de preguntas — es
  la forma congelada sincronizada con Antigravity (T1258/T1260).

## 2. Acceso: 403/401/query-token/dev_mode — VERIFICADO

- Ambas rutas en la tabla única `operator_class_required`
  (`auth.rs:275-276`); gate por `path ==` exacto **sin condición de
  método**, así que HEAD/POST a esas rutas también son clase operador (con
  maestro → 405 de axum; sin él → 403; el handler no se ejecuta nunca).
- `operator_gate_middleware` usa `request.uri().path()` (`auth.rs:337`) —
  el query-string no altera la clasificación. El token maestro por query
  produce `MasterTokenQuery` que `operator_gate_allows` rechaza
  (`auth.rs:326-331`) → **403**.
- Test de Deep: JWT OAuth → 403, sin credenciales → 401, maestro por
  cabecera → 200, sobre el router de PRODUCCIÓN (`build_router`).
- Tests propios:
  - `query_string_master_token_is_rejected_on_operator_reads`:
    `?token=<master>` y `?Authorization=<master>` → **403** en ambos
    endpoints (el hueco que el test de Deep no cubría).
  - `dev_mode_bypass_is_accepted_by_design`: dev_mode=true sin token →
    200 vía `DevBypass` — mismo trato que el resto de rutas operador
    (`auth.rs:329`); fijado con test para que un cambio futuro de política
    lo note.
  - `trailing_slash_and_case_variants_do_not_bypass`: `/shadow/`,
    `/conflicted/`, `/Decide/shadow`, `/decide/Shadow` → **404** (ni el
    gate ni el handler se eluden; sin datos).

## 3. Límites y coherencia total/summary — VERIFICADO

- `limit` ausente o `0` → 50; tope duro `n.min(500)` (shadow) /
  `n.min(200)` (conflicted). Test de Deep cubre 0/9999/default.
- Test propio `degenerate_limits_reject_cleanly`: `-1`, `abc`, `1e3`,
  `500.5` y desbordamiento de u64 (`18446744073709551616`) → **400** del
  extractor `Query` (consistente con el resto de GETs con `Query` del
  código), sin panic y sin 500.
- Coherencia (test propio): `sum(n del summary) == len(rows)`;
  `n == n_agreed + n_disagreed + n_null` por contrato; `total` (COUNT(*) de
  la tabla, `service.rs` `shadow_overview`) >= ventana. El resumen SOBRE
  la ventana devuelta es la forma congelada acordada (T1260).

## 4. `content_excerpt`: 300 chars Unicode + saneado + edge cases — VERIFICADO

- `sanitize_excerpt` (`api_v1/mod.rs`): `char::is_control()` → espacio y
  luego `chars().take(300)` — chars Unicode, **no bytes**.
- Test propio `conflicted_excerpt_unicode_crlf_empty`:
  - 400× U+1F44D (4 bytes/carácter): excerpt = exactamente **300
    caracteres**, primer emoji intacto (un tope en bytes habría cortado
    distinto).
  - CRLF + BEL (`\r\n`, `\u{0007}`) → espacios; ningún `is_control()`
    sobrevive.
  - Contenido vacío → excerpt `""`, sin panic.
  - `total_conflicted` coherente; nodos no marcados no aparecen (test de
    Deep).
- Test de Deep `conflicted_limit_and_excerpt_cap` cubre el tope en ASCII,
  límites y exclusiones.

## Verificación ejecutada (en MI worktree con el diff de Deep)

| Comando | Resultado |
|---|---|
| `cargo test --test decide_admin_read_api` (Deep) | **5/5** |
| `cargo test --test t1253_buffy_adversarial` (mío) | **6/6** |
| `cargo test --test decide_systemone_http` | **20/20** |
| `cargo test --test decide_adversarial` | **13/13** |
| `cargo test --test decide_parity_e2e` | **4/4** |
| `cargo test --lib -- auth::` | **30/30** |
| `cargo test --test oauth_operator_gate` | **6/6** |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **0 avisos** |
| `cargo test -p tylluan-kernel --lib` | **1053** (sin cambios: los tests nuevos son de integración) |
| `scripts/check_test_count.sh` | **exit 0** (1139) |

## Notas NO bloqueantes (para el TL, no impiden commit)

1. **Caracteres de formato (Cf) sobreviven al saneado:** U+200B (zero-width
   space) y similares no son `is_control()` (categoría Cc) y pasan al
   excerpt. Inofensivo (no filtra nada), pero un panel podría mostrar
   "texto invisible"; si se quiere, añadir un paso Cf→'' en
   `sanitize_excerpt`. Decisión cosmética.
2. **`get_all_conflicted` carga TODO antes del `take(limit)`:** lee todos
   los nodos conflitados con contenido completo y luego trunca en el
   handler. A escala actual (375 nodos) irrelevante; si la tabla crece
   miles, mover el `LIMIT` al SQL.
3. **`total` con `unwrap_or(0)`:** si `decision_shadow_log` faltase, el
   total silenciaría 0 en vez de error. Aceptable para un panel de
   diagnóstico; el test siembra filas reales y lo cubre bien.

## Impacto de ESTA revisión

- Aditivo en mi worktree, sin commitear: `tests/t1253_buffy_adversarial.rs`
  (nuevo, mío) + este informe. El diff de Deep se copió aquí SOLO para
  ejecutar la verificación (mismo parche byte a byte); no lo modifiqué.
- No toco código de producción, config, ni la BD. El TL decide si el test
  adversarial viaja en el mismo commit que el diff de Deep o se descarta.
