# T-DSH-86 — Middleware `validate_mcp_origin_host` (defensa DNS rebinding en MCP/SSE)

- **Agente:** dsh
- **Fecha:** 2026-10-25
- **Base:** `origin/main` `cb41c03`, tras `agent_wt.sh reset dsh`. **Diff SIN commitear** (3 ficheros, +234/−1, más un test nuevo).
- **Encargo (T1762):** middleware `validate_mcp_origin_host` **solo** para `/messages`, `/mcp`, `/api/v1/mcp`, `/sse`, `/sse/download`; (a) `Origin` presente y no loopback (`127.0.0.1`, `localhost`, `[::1]`, cualquier puerto) ni en `[nexus] allowed_origins` (default vacía) → 403, incluido preflight `OPTIONS` y `Origin: null`; (b) si el kernel escucha en loopback y el `Host` no es loopback ni está en `[nexus] allowed_hosts` → 403; si el host configurado no es loopback, no validar Host y avisar una vez al arrancar; (c) sin `Origin` (Claude Code, mcp-remote, agentes) → pasa. Sin eximir por auth válida. Antes de `CorsLayer` y de la auth. Tests de los 8 casos (403 en 1-7, 200 en 8) y 200 sin `Origin`, con `dev_mode` true y false.

## 1. Cambios

| Fichero | Cambio |
|---|---|
| `src/transport/http/auth.rs` | Nuevo middleware `validate_mcp_origin_host_middleware` (línea 751) y helpers: `MCP_ORIGIN_GUARD_PATHS` (línea 672: exactamente las 5 rutas pedidas), `hostname_from_authority` (soporta la forma IPv6 con corchetes y puerto), `is_loopback_hostname` (`127.0.0.1`/`localhost`/`::1`, case-insensitive), `is_loopback_origin` (http/https + loopback, cualquier puerto), `origin_is_allowed` (línea 708: loopback o `allowed_origins` exacta; **`null`/vacío siempre rechazado, aunque esté en la lista**), `host_is_allowed` (línea 722: loopback, host configurado o `allowed_hosts`), `forbidden_origin_host` (403 JSON `forbidden_origin`/`forbidden_host`). Sin `Origin` → pasa sin tocar la petición. |
| `src/transport/http/mod.rs` | En `build_router`: aviso **una vez al arrancar** si `nexus.host` no es loopback (líneas 1325-1335, "Host validation disabled … Origin validation remains active"). Nueva capa (línea ~1522) añadida **después** de `.layer(cors)` → es la capa más externa de la petición (corre **antes** del `CorsLayer` y de las capas de auth internas de `protected_routes`); el propio middleware filtra por path, así que el resto de la API y federation/p2p no se tocan. |
| `src/config.rs` | `[nexus]` gana `allowed_origins` (línea 618) y `allowed_hosts` (línea 623), `#[serde(default)]` = vacías, inicializadas a `Vec::new()` en `Default` (líneas 654-655). |
| `tests/mcp_origin_guard.rs` (**nuevo**) | 4 tests de integración con `support::test_state_custom` + `build_router` (sin kernel): `rejects_hostile_origin_and_host_in_dev_mode`, `accepts_local_clients_and_loopback_origins`, `rejects_hostile_origin_even_with_valid_bearer`, `configurable_allow_lists_work`. |
| `auth.rs` (`mod mcp_origin_guard_tests`, línea 1401) | 6 unit tests: orígenes loopback (cualquier puerto), orígenes hostiles (`null`, `file://`, `127.0.0.1.evil.example`, `[::2]`), escape hatch de `allowed_origins` (incluye `null` en lista → sigue rechazado), `Host` loopback/configurado/allowed, parseo de authority IPv6, y cobertura exacta de las 5 rutas. |

Decisiones de alcance: `/api/v1/events` (SSE del dashboard) queda **fuera** de la lista pedida y el middleware filtra por path exacto; `sse_routes()` se sigue montando igual pero `/api/v1/events` no se valida (ver "No verificado"). `api_v1/mcp.rs` no se tocó (D-59 de Deep).

## 2. Verificación (salidas reales)

| Comando | Resultado |
|---|---|
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** (50,8 s) |
| `cargo test -p tylluan-kernel --test mcp_origin_guard` | **4 passed; 0 failed** (1,88 s) |
| `cargo test -p tylluan-kernel --lib` | **1283 passed; 0 failed; 4 ignored** (53,9 s). Base **1277** + **6 unit tests nuevos** |
| `cargo test -p tylluan-kernel --test security_audit` | **40 passed; 0 failed** |
| `cargo test -p tylluan-kernel --test federation_audit` | **12 passed; 0 failed** (la federación no se rompe) |
| `git diff --stat` | `auth.rs +201`, `mod.rs +20`, `config.rs +14`; total **+234/−1** + `tests/mcp_origin_guard.rs` nuevo |

Comportamiento cubierto por los tests: 403 con `Origin` hostil en `/messages`, `/mcp`, `/api/v1/mcp` y `/sse`; 403 con `Host` hostil sin `Origin`; **403 en preflight `OPTIONS`** con `Origin` hostil; 403 con `Origin: null`; 403 aunque el Bearer sea válido; **200** con orígenes loopback (incluye `http://[::1]`, `localhost` y `127.0.0.1`, cualquier puerto); **200 sin `Origin`** (con y sin `Host`, dev true y false); **200** con `Origin`/`Host` en las listas configurables; y la ruta no-MCP `/api/v1/coloquio/channels` **sigue devolviendo 200** con `Origin` hostil (el guard no se aplica al resto de la API).

## Impact

- **Cierra el hallazgo de T-DSH-85**: con `dev_mode=true` (config actual) un `Origin` hostil ya no puede ejecutar `tools/list`/`tools/call` ni abrir `/sse`; el 403 ocurre antes del CORS y de la auth, así que también cubre el preflight y las peticiones CORS simples (`text/plain`) que antes se colaban. Sin `Origin`, los clientes no-navegador (Claude Code, mcp-remote, agentes, curl) siguen funcionando exactamente igual; con `dev_mode=false` un Bearer válido ya no exime de la validación de origen.
- **Ruta crítica de auth**: el cambio se limita a las 5 rutas MCP/SSE (filtro por path dentro del middleware); federation/p2p, `/api/v1/*` y el dashboard no cambian (`federation_audit` 12/12 verde y el caso no-MCP del test). La lista de orígenes/hosts es configurable y **vacía por defecto**; no se toca `tylluan.example.toml` (los campos son `#[serde(default)]`, así que cualquier config existente sigue cargando).
- El `Host` solo se valida cuando el kernel está enlazado a loopback; en otro caso se avisa una vez al construir el router y solo queda activa la validación de `Origin`.

## No verificado

- **Sin kernel real ni navegador**: verificación con el router en proceso (`test_state`) y `cargo test`; no se ejecutó un DNS-rebinding real ni se comprobó el navegador (solo los status del middleware).
- **`/api/v1/events`** (SSE del dashboard, misma familia que `/sse` pero fuera de la lista del encargo) **no** está cubierto por el guard; si se considera superficie MCP/SSE habría que añadirlo a `MCP_ORIGIN_GUARD_PATHS` (y decidir si el dashboard debe seguir abriéndolo desde el navegador).
- **Host validation con host configurado no-loopback** no se probó en vivo (solo el caso loopback por defecto); el aviso de arranque está en `build_router` y no se observó en un arranque real.
- **Listas configurables con el túnel WSL** (`[proxy.wsl] fallback_port`) no probadas de extremo a extremo: solo el caso sintético `allowed_origins`/`allowed_hosts` en test.
- **`tylluan.example.toml`/docs** no se actualizaron (fuera de los ficheros permitidos); `check_test_count.sh` esperará ahora **1370** (1283+75+12) frente a 1364 en docs → drift que sincroniza el TL.
- No se probó el resto de rutas API con `Origin` hostil más allá de `/api/v1/coloquio/channels` (el guard no las toca por diseño y el test lo fija).
