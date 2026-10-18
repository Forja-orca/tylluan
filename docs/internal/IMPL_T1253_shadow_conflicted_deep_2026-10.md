# IMPL T1253 — Endpoint de lectura del `decision_shadow_log` + nodos conflicted (Deep)

**Fecha:** 2026-10-18 · **Base:** `e191cbe` (= `origin/main` al empezar) · **Diff sin commitear** (solo Claude Code commitea).
**Tarea:** T1253 del TL (GO de José y TL), bloquea D1 de Antigravity; ampliada en el sync T1255 con `GET /api/v1/silva/conflicted`.

## Qué se implementó

### 1. `GET /api/v1/decide/shadow?limit=N` (clase OPERADOR)

`crates/tylluan-kernel/src/transport/http/api_v1/api_decide.rs` → `decide_shadow_handler`.

- **Shape CONGELADO** (sync T1255): `{"rows":[...], "summary":[{contract,n,n_agreed,n_disagreed,n_null}], "limit":N, "total":N}` — sin alias (`entries`/`items`/`by_contract`), exactamente la forma única que consume el panel de decisión que construye Antigravity (D1).
- **Rows:** los 11 campos públicos de `decision_shadow_log`: `id, ts, contract, request_hash, rules_value, model_value, provider, latency_ms, agreed, mode, model_identity`. **No** viajan `model_probs_json`, `model_latency_ms`, `state` ni texto de preguntas (solo su `request_hash`, ya hash al llegar aquí).
- **`summary`** se calcula SOBRE la ventana devuelta (las `limit` filas más recientes): `n_agreed` = `agreed=1`, `n_disagreed` = `agreed=0`, `n_null` = `agreed IS NULL` (sin valor de reglas o sin respuesta de modelo aún). Orden por contrato alfabético (BTreeMap, determinista).
- **`total`** = `COUNT(*)` de la tabla entera, independiente del límite.
- **`limit`:** ausente o `0` → 50 (defecto), tope duro **500** (`n.min(500)`).
- Cálculo nuevo en `service.rs`: `DecisionService::shadow_overview(limit)` → `ShadowOverview { rows, summary, total }` (lee con `read_shadow_log` + un `COUNT(*)`; solo lectura, sin abrir ninguna puerta de escritura).

### 2. `GET /api/v1/silva/conflicted?limit=N` (clase OPERADOR)

`crates/tylluan-kernel/src/transport/http/api_v1/mod.rs` → `silva_conflicted_handler` (junto al `list_contradictions_handler` preexistente).

- **Shape CONGELADO:** `{"nodes":[{id,node_type,content_excerpt,conflicted_since}], "total_conflicted":N, "limit":N}`.
- Fuente: `SilvaDB::get_all_conflicted()` (`WHERE conflicted = 1`, `nodes.rs:630`) — la lista REAL de conflictos, a diferencia de `/api/v1/silva/contradictions` que devuelve `deprecated_nodes` (otra cosa; hallazgo Regla 6 verificado por el TL en `api_v1/mod.rs:610`).
- **`content_excerpt`:** caracteres de control sustituidos por espacio + acotado a **300 chars Unicode** (chars, no bytes).
- **`conflicted_since` ≈ `updated_at` del nodo** — aproximación declarada: el esquema no guarda cuándo se marcó el flag.
- **`limit`:** ausente o `0` → 50, tope duro **200**. `total_conflicted` = nodos conflitados totales (independiente del límite).

### 3. Gate de clase operador

`auth.rs::operator_class_required` — las dos rutas en el bloque de *Exact endpoints* (path-level, como `/api/v1/bash`): exigen el token maestro **por cabecera** (`operator_gate_allows`: `MasterToken` o `DevBypass` únicamente → JWT OAuth, query-string y bypass de par = **403**; sin credenciales = **401**). El middleware `operator_gate_middleware` ya estaba en la cadena de `protected_routes`, así que el gate aplica sin nuevo cableado.

### 4. Gemelo MCP NO requerido (justificación, por encargo)

`/api/v1/decide/shadow` es **admin/diagnóstico**: alimenta el panel del operador en el dashboard vía REST con el token maestro. La superficie MCP expone las 6 herramientas soberanas a agentes (clase Agente) — añadir un gemelo MCP daría a cualquier llamante MCP una ventana de auditoría que la política de clase operador deliberadamente restringe, sin ningún consumidor que lo pida. Mismo criterio que el resto de lecturas de diagnóstico (no hay twin MCP de `/api/v1/audit/latency`, `/api/v1/doctor`, etc.).

## Tests (5 nuevos: `tests/decide_admin_read_api.rs`)

| Test | Cubre |
|------|-------|
| `shadow_requires_operator_master_token` | Router de PRODUCCIÓN (`build_router`): JWT→403, sin creds→401, maestro→200 |
| `shadow_limit_respected_and_summary_correct` | 5 filas sembradas → `limit=2` respeta, `total=5`, resumen `{c_b,n=2,n_agreed=1,n_null=1}` sobre la ventana, defecto 50, `0→50`, `9999→500` |
| `shadow_response_carries_no_state_or_question_text` | `decide()` real con state marcado (`T1253_LEAK_MARKER_XYZ`) → la respuesta no contiene el marcador; sin clave `model_probs_json`; cada fila tiene EXACTAMENTE los 11 campos |
| `conflicted_requires_operator_master_token` | Mismo trío 403/401/200 sobre `/api/v1/silva/conflicted` |
| `conflicted_limit_and_excerpt_cap` | 3 conflitados + 1 limpio (limpio no aparece), `total_conflicted=3`, excerpt = 300 chars, `limit=2`, defecto 50, tope 200 |

Aserciones nuevas en `auth.rs` (unit): las dos rutas clasificadas como operador; el inventario estructural `route_inventory_every_mutating_route_is_classified` sigue en verde (ambas rutas son GET, el parser solo clasifica mutantes, pero quedan documentadas en la tabla unitaria).

## Verificación

- `cargo check -p tylluan-kernel --all-targets` → OK.
- `cargo test -p tylluan-kernel` **COMPLETA** → **lib 1053/1053 + TODAS las integraciones, 0 failed** (incluidos los 5 nuevos, `decide_systemone_http` 20, `decide_adversarial` 13, `decide_parity_e2e` 4, `oauth_operator_gate` y auth unit 30).
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → **Finished, 0 errores** (un `doc_lazy_continuation` en el doc del test corregido antes de dar por bueno).
- `scripts/check_test_count.sh` → verde (1139: los tests nuevos son de integración, el recuento de lib no cambia).

## Notas y límites

- `agreed` llega como JSON `null` cuando no es calculable (esperado por el panel).
- Un `?limit=0` degenerado cae al defecto 50 en vez de devolver vacío (documentado en el handler).
- El endpoint no escribe nada y lee solo `decision_shadow_log`/`nodes`; solo lectura por construcción (sin rutas de escritura nuevas).
- `conflicted_since` es `updated_at` (mejor dato disponible; el esquema v29 no tiene columna de marca de conflicto).

## Impact

Toca camino crítico de auth: `transport/http/auth.rs` (tabla única `operator_class_required`, fuente de verdad del gate de clase operador) y `transport/http/api_v1/routes.rs` (tabla de rutas CONTRACT-05, dos rutas NUEVAS añadidas al final del bloque, ninguna existente movida). Efecto sobre la superficie de red: **dos lecturas nuevas protegidas con la clase más restrictiva** (token maestro por cabecera) — ningún rol Agente/Par/OAuth gana acceso (los tests 403/401 lo fijan), ninguna ruta existente cambia de clase, ni `POST /api/v1/decide` (sigue clase Agente, aserción `decide_parity_e2e` intacta). Escritura: cero — solo SELECTs. Privacidad: sin fugas de `state` ni texto de preguntas (test con marcador). Los paneles del dashboard (D1/D2 de Antigravity) consumen shapes ya congelados; si la rama de Antigravity asume campos distintos, el contrato vive en este informe y en los tests.
