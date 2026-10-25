# T-DSH-87 — Mismo agujero que T-DSH-85 en las rutas REST de operador y escritura (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-25
- **Base:** `origin/main` `346cf65`, tras `agent_wt.sh reset dsh`. **Solo lectura**: sin cambios de código; la sonda fue un test temporal, ejecutado y **borrado** (worktree limpio salvo este informe). No se toca MCP/SSE (ya cubierto por T-DSH-86).
- **Encargo (T1766):** ¿aceptan las rutas REST de operador/escritura peticiones de navegador sin `Origin` loopback ni `Content-Type: application/json`? Probar con el router en proceso (`test_state`) `Origin: http://evil.example` + `Content-Type: text/plain` con `dev_mode=true`: tabla ruta | resultado | gravedad (¿ejecuta algo? ¿escribe algo?) | fix de 1-3 líneas. Máx 10 filas.

## 1. Método

Test de integración temporal (ya borrado) con `support::test_state_custom(<tag>, None, true)` (es decir `dev_mode=true`, como la producción actual) y `tylluan_kernel::transport::http::build_router(state)`; `oneshot` con `Origin: http://evil.example` y `Content-Type: text/plain` (petición CORS **simple**, sin preflight) o sin cuerpo. Cada caso con un `HttpState` nuevo. Comando usado:

```
cargo test -p tylluan-kernel --test t_dsh_87_rest_origin_probe -- --nocapture
```

Salida real (recortada):

```
/api/v1/do text/plain             | 200 OK | {"content":["...whoami..."],...}
/maintenance/purge no body        | 200 OK |
/maintenance/vacuum no body       | 200 OK |
/silva/graphrag-trigger no body   | 200 OK | {"elapsed_ms":0,...,"targets_found":0}
/admin/reload no body             | 200 OK | {"attempted":45,"elapsed_ms":17 474,...}
/memory/reindex no body           | 503 Service Unavailable | {"error":"embedding engine not loaded yet"}
/api/v1/config                    | 415 Unsupported Media Type (Content-Type: application/json required)
/api/v1/mcp/external              | 415 Unsupported Media Type
/api/v1/coloquio/channels/general/post | 415 Unsupported Media Type
/api/v1/silva/edge                | 415 Unsupported Media Type
/api/v1/guilds/pi/call/whoami     | 415 Unsupported Media Type
/api/v1/doctor/repair             | 415 Unsupported Media Type
/guilds/no-such-guild/start no body| 500 | {"error":"Unknown guild: 'no-such-guild'"}
/api/v1/do application/json (ctrl)| 200 OK | {"content":["...whoami..."]}
```

## 2. Tabla (8 filas; gravedad y fix)

| # | Ruta | Resultado real (dev_mode=true) | Gravedad (¿ejecuta/escribe?) | Fix (1-3 líneas) |
|---|---|---|---|---|
| 1 | `POST /api/v1/do` (`Content-Type: text/plain`) | **200 OK** y ejecuta la herramienta (`whoami` devuelto; con `intent`/`guild` ejecuta guilds, tools y memoria) | **CRÍTICA** — ejecuta acciones en nombre del kernel; `do_intent_handler` lee `axum::body::Bytes` y parsea JSON **ignorando el Content-Type** (mod.rs 342-355) | Reutilizar el guard de T-DSH-86 extendido a `/api/v1/*` (ver fix común) + usar extractor `Json` en `/api/v1/do` |
| 2 | `POST /api/v1/maintenance/purge` (sin cuerpo) | **200 OK**: `DELETE FROM edges; DELETE FROM nodes;` (purga total de SilvaDB) | **CRÍTICA destructiva** — sin body extractor ni chequeo de Content-Type ni de Origin (api_admin.rs 1662-1678) | Fix común de Origin/Host en rutas de escritura; el handler ya registra warn pero no verifica origen |
| 3 | `POST /api/v1/admin/reload` (sin cuerpo) | **200 OK**: arranca/recarga **45 guilds** (llegó a spawnear procesos Python en el test) | **ALTA** — spawn de procesos y recarga global; solo comprueba `dev_mode` (api_admin.rs 1011-1017) | Fix común de Origin/Host; valorar exigir operador explícito además de `dev_mode` |
| 4 | `POST /api/v1/guilds/{name}/start` (sin cuerpo) | Alcanza el handler: **500 `Unknown guild`** con nombre inexistente (con nombre real arranca el guild) | **ALTA** — con un guild válido arranca un proceso (api_guilds.rs 32); sin Content-Type no hay rechazo | Fix común de Origin/Host |
| 5 | `POST /api/v1/silva/graphrag-trigger` (sin cuerpo) | **200 OK** `{"targets_found":0,...}`: ejecuta una pasada GraphRAG completa | **MEDIA** — coste CPU/escrituras de resúmenes; no valida contenido (api_monitor.rs 738-744) | Fix común de Origin/Host |
| 6 | `POST /api/v1/maintenance/vacuum` (sin cuerpo) | **200 OK**: `VACUUM` real de SQLite | **MEDIA** — bloquea/bloquea E/S de la DB (api_admin.rs 1680-1682) | Fix común de Origin/Host |
| 7 | `POST /api/v1/memory/reindex` (sin cuerpo) | **503** en test (`embedding engine not loaded yet`); el handler **se alcanza** (en producción con ONNX cargado ejecutaría el reindex) | **MEDIA** — reindexado/embeddings (api_memory.rs 98) | Fix común de Origin/Host |
| 8 | `POST /api/v1/config`, `/api/v1/mcp/external`, `/api/v1/coloquio/channels/{id}/post`, `/api/v1/silva/edge`, `/api/v1/guilds/{g}/call/{t}`, `/api/v1/doctor/repair` (`Content-Type: text/plain`) | **415 Unsupported Media Type** en todas (`Expected request with Content-Type: application/json`) | **OK/seguro** — el extractor `Json` de axum rechaza antes del handler (p. ej. api_admin.rs 712, api_mcp.rs 66, api_coloquio.rs 232-236, mod.rs 724, api_guilds.rs 50, mod.rs 590-593) | Ninguno para el vector `text/plain`; aun así aplicar el fix común de Origin/Host como defensa en profundidad (hoy no se valida Origin en `/api/v1/*`) |

Patrón: la línea divisoria es el **extractor**. `Json(...)` → 415 automático (seguro para este vector). `axum::body::Bytes` + parseo manual (`/api/v1/do`) o **sin cuerpo** (maintenance/admin/guilds/start, reindex, graphrag-trigger) → la petición simple del navegador llega al handler y se ejecuta, porque fuera de MCP/SSE **no hay guard de `Origin`** (el middleware de T-DSH-86 filtra por path exacto en auth.rs 672 y solo cubre `/messages`, `/mcp`, `/api/v1/mcp`, `/sse`, `/sse/download`; mod.rs monta la capa en 1522).

**Fix común propuesto (1-3 líneas de cambio efectivo):** aplicar el mismo `validate_mcp_origin_host_*` (mismos helpers de `auth.rs`) a la superficie REST mutante, cambiando la condición del middleware de lista exacta a:

```rust
let guarded = MCP_ORIGIN_GUARD_PATHS.contains(&path.as_str())
    || (path.starts_with("/api/v1/") && !matches!(method, Method::GET | Method::HEAD | Method::OPTIONS));
```

y añadir `method` a la firma del middleware (sin duplicar lógica ni tocar handlers). Alternativa mínima aprobada por superficie: registrar una segunda capa `validate_api_origin_host` reutilizando `origin_is_allowed`/`host_is_allowed` sobre `/api/v1/*` no-GET. Para `/api/v1/do` añadir además el extractor `Json` (una línea) como defensa en profundidad del vector `text/plain`.

## Impact

- **Hallazgo real por la misma clase que T-DSH-85, hoy con `dev_mode=true` (config actual):** una página web abierta en el navegador de José puede resolver un dominio propio a `127.0.0.1` (DNS rebinding) y, con una petición **simple** (sin preflight: `text/plain` o sin cuerpo), **purgar toda la memoria** (`/maintenance/purge`), **ejecutar herramientas** (`/api/v1/do`), **arrancar/reload 45 guilds** (`/admin/reload`), abrir/cerrar guilds, lanzar GraphRAG o vacuum. Sin token y sin leer la respuesta.
- El CORS actual no lo impide (solo bloquea la lectura/preflight JSON); el guard de T-DSH-86 no llega a `/api/v1/*`. Las rutas con extractor `Json` están protegidas de este vector (415), pero tampoco validan `Origin`: con `dev_mode=false` un cliente autenticado de origen arbitrario sigue pasando (defensa en profundidad).
- **No se cambió código**: diagnóstico reproducible, línea divisoria (extractor), rutas exactas y fix de 1-3 líneas reutilizando lo ya integrado en T-DSH-86.

## No verificado

- **Sin kernel real ni navegador**: router en proceso (`test_state`) y `dev_mode=true`; no se ejecutó un DNS-rebinding real ni se comprobó el bloqueo del navegador. `dev_mode=false` no se probó para estas rutas (la expectativa es 401 por falta de token, no auditado aquí).
- **No se probó el spawn real de un guild** (`/guilds/{name}/start` se probó con nombre inexistente → 500): el arranque de procesos de guild queda sin verificar en vivo; en el test `/admin/reload` sí intentó spawnear y falló por entorno (`ModuleNotFoundError: No module named 'guilds'`), lo que confirma el efecto pero no su éxito.
- **`/memory/reindex`** devolvió 503 en test (sin motor de embeddings): en producción con ONNX cargado se ejecutaría, pero no se verificó en vivo.
- **Resto de rutas no listadas** (`/api/v1/memory/write`, blackboard, mailbox, silva/node, save-summary, communities, consolidate, ingest, federation, etc.) no se probaron una a una: `Json` → 415 por patrón (6/6 muestreadas), y las de cuerpo crudo/no-body podrían sumar más filas (la lista del encargo era orientativa, no exhaustiva). `/a2a`, `/gossip` y DELETE/PUT (preflight obligatorio) quedan fuera.
- **Spec**: no se consultó texto externo; el criterio es "el navegador no debe poder ejecutar efectos sin `Origin` loopback válido" (misma doctrina que T-DSH-85/86).
- Gates de documentación: solo el artefacto WSL conocido (`ROADMAP_O3.md`, línea 42); `check_test_count` ya en verde con 1374 (sincronizado por el TL).
