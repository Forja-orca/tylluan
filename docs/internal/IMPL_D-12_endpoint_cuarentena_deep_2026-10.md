# D-12 — Endpoint de cuarentena ASI06 (lista + liberación)

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` @ `8001d23`
**Parche:** `E:\\tylluan-patches\\deep\\D-12.patch` (23.232 B, 5 archivos, +481 líneas, 0 borradas)
**Estado:** implementado y verificado en local. **Sin commitear** (solo Claude Code comitea).

---

## Qué se entregó

| Archivo | Cambio |
|---|---|
| `crates/tylluan-kernel/src/memory/silva/nodes.rs` | `QuarantinedNodeRow` + `list_quarantined(limit, offset)` + `release_quarantine(id)` |
| `.../transport/http/api_v1/api_memory.rs` | `quarantine_list_handler`, `quarantine_release_handler` |
| `.../transport/http/api_v1/routes.rs` | 2 rutas nuevas, **al final** del bloque (no se reordena nada) |
| `.../transport/http/auth.rs` | prefijo en `operator_class_required` + 2 asserts en el test de inventario |
| `crates/tylluan-kernel/tests/quarantine_endpoint_test.rs` | 6 tests sobre el router real |

Contrato respetado **literalmente** (nombres y tipos fijados con Antigravity, lote AE):

```
GET  /api/v1/memory/quarantine?limit=50&offset=0
 -> {"total":N,"nodes":[{"id":str,"content_preview":str(<=200),"quarantine_reason":str,
                         "provenance":str,"agent_id":str|null,"created_at":int}]}
POST /api/v1/memory/quarantine/{id}/release
 -> {"released":true,"id":str}    |    404 {"error":"not_found"}
```

Detalles que el contrato no fijaba y que decidí (declarado, no silencioso):

- `limit` **se recorta** a 200, no devuelve 400. Un panel que pide 500 recibe 200 y no
  tiene que manejar un caso de error extra. `limit=0` o negativo también se recorta a 1.
- `created_at` se castea en SQL (`CAST(strftime('%s', created_at) AS INTEGER)`) porque la
  columna es `DATETIME` TEXT y el contrato pide entero. La conversión vive en la capa de
  datos: un solo sitio que mantener si el contrato cambia.
- `agent_id` se lee del JSON de `metadata`; metadata corrupta o ausente → `null`, la página
  no falla entera por un nodo mal formado.
- `quarantine_reason` y `provenance` vacíos nunca ocurren en la práctica (NOT NULL con
  default), pero se mapean a `""` en vez de romper la fila.
- La liberación **borra** `quarantine_reason` (`= NULL`), no lo deja obsoleto: una razón
  vieja sobre un nodo ya liberado confunde al siguiente que lea la tabla.

## Auditoría de la liberación

`log_audit_entry(...)` (`handler_do/audit.rs:25`) con:

- `intent = "quarantine:release"`, `guild = "memory"`, `tool = "quarantine_release"`
- `agent_id` = `current_bound_agent_id()` → `operator:<rol>` → `"operator"` (en ese orden)
- `human_intervention` = `true` **solo** si `current_auth_kind()` es `MasterToken`. Es la
  misma honestidad que pide D-13 para `approve_action`: el campo no debe afirmar que hubo
  un humano cuando la credencial no lo era.
- Se lanza en `spawn_blocking` **sin `await`**: `log_audit_entry` abre un segundo fichero
  SQLite (`./data/audit.db`, o `TYLLUAN_AUDIT_DB`) y no debe estar en el camino de la
  respuesta. (Un `await` ahí cuelga el runtime de los tests al hacer shutdown: los
  `spawn_blocking` vivos se esperan. Lo vi en la primera versión de este mismo parche.)

## Verificación

```
cargo test -p tylluan-kernel --test quarantine_endpoint_test
  -> test result: ok. 6 passed; 0 failed; 0 ignored   (14,69 s)

cargo test -p tylluan-kernel --lib
  -> test result: ok. 1110 passed; 0 failed; 4 ignored   (38,05 s)

cargo clippy -p tylluan-kernel --all-targets -- -D warnings
  -> Finished `dev` profile ... EXIT=0

bash scripts/check_test_count.sh
  -> kernel=1110 link=75 fsrs=12 total=1197 · README y AGENTS coinciden
```

Los 6 tests:

1. `list_returns_contract_shape_and_only_quarantined_nodes` — forma exacta del contrato,
   preview cortada a 200 caracteres (marcador sembrado en el 261 para demostrar que no se
   filtra), `agent_id` desde metadata, `created_at` entero, y un nodo limpio que **no**
   aparece.
2. `list_is_ordered_created_at_desc_and_paginates` — `created_at DESC`, `total` es el
   universo (3) y no la página, `offset` correcto, `limit=999` recortado sin error.
3. `release_existing_node_clears_flag_and_reason` — 200 + `{"released":true}`, y en la DB
   `quarantined=0` con `quarantine_reason` **NULL**; desaparece del listado.
4. `release_missing_or_already_released_is_404` — id inexistente y nodo existente pero
   nunca cuarentenado dan el mismo `404 {"error":"not_found"}`.
5. `oauth_jwt_with_mcp_scope_gets_insufficient_scope_on_both_routes` — JWT `scope=mcp` →
   403 con `WWW-Authenticate: ... insufficient_scope` en GET y POST, y el nodo sigue en
   cuarentena (el 403 no ejecutó nada).
6. `master_token_in_query_string_does_not_satisfy_the_operator_class` — `?token=` no
   satisface la clase operador (coherente con E1).

## Ejemplo `curl` real

**No lo ejecuté** (restricción de entorno: no arranco procesos; el kernel de pruebas
necesita `llama-server` y un `DATA_DIR` propio). Comando exacto para el TL:

```bash
# kernel de prueba aislado (puerto y DATA_DIR propios)
DATA_DIR=test-run-d12 tylluan-cli start --port 47112 &

# lista (token maestro por CABECERA, nunca por query)
curl.exe -s -H "Authorization: Bearer $(cat .tylluan-token)" \
  "http://127.0.0.1:<puerto-de-prueba>/api/v1/memory/quarantine?limit=2&offset=0"

# liberar
curl.exe -s -X POST -H "Authorization: Bearer $(cat .tylluan-token)" \
  "http://127.0.0.1:<puerto-de-prueba>/api/v1/memory/quarantine/<id>/release"

# sin operador -> 403 (JWT OAuth scope=mcp, o token en query)
curl.exe -s -o NUL -w "%{http_code}\n" "http://127.0.0.1:<puerto-de-prueba>/api/v1/memory/quarantine?token=$(cat .tylluan-token)"
```

Equivalente verificado en proceso (lo que sí corrí): los 6 tests de arriba levantan
`build_router(state)` real y hacen `oneshot` sobre las rutas exactas — misma pila de
middleware (auth, clase operador, rate limit) que un `curl`, sin socket.

## Impact

- **Contrato público nuevo:** 2 rutas REST bajo `/api/v1`. Añadidas al final del bloque,
  sin reordenar; cualquier consumidor que enumere rutas debe tolerarlas.
- **Superficie de seguridad:** el GET devuelve hasta 200 caracteres del texto que el juez
  marcó como hostil. Por eso va en clase operador (token maestro por cabecera) y no en
  agente: no es un endpoint más de lectura de memoria.
- **Escritura:** la liberación es la única forma soportada de des-cuarentenar un nodo. No
  toca umbrales de gates ni `dev_mode`.
- **Auditoría:** +1 fila en `guild_audit_log` por liberación, con cadena de hash intacta
  (`log_audit_entry` ya la mantiene).
- **No-depredación (ADR-013):** no modifica ficheros de otros agentes; `nodes.rs` sólo
  añade métodos; `auth.rs` sólo añade un prefijo y sus asserts.

## No verificado (declarado)

- El `curl` contra kernel real de prueba: no arranco procesos en esta máquina.
- Que el panel de Antigravity renderice con estos datos reales: el contrato es el suyo y
  los tests fijan la forma, pero el render es su lote.
- Rendimiento con una cuarentena grande (miles de filas): `COUNT(*)` + `LIMIT/OFFSET`
  simples sobre `idx_nodes_quarantined`; no medí con volumen.
