# INFORME — Revisión independiente del fix SSRF (`71075b8`) — Buffy (2026-10-20)

**Asignación:** revisión del SSRF pendiente en el resumen del TL (2026-10-20). Contexto: los fallos SSRF los hallé yo en B-2 (`docs/internal/IMPL_b2_parsers_uri_buffy_2026-10.md`); el TL los cerró en `71075b8` (ingest + guard de proveedores externos). Encargo: revisión independiente del fix, buscando bypass. (El TL cita además una revisión de un tercero "sin bypass explotable"; esta es la mía, con verificación en vivo propia.)

**Método:** (1) releído el código post-fix; (2) análisis de clases de bypass (normalización WHATWG de IPv4 decimal/hex/shorthand, IPv6 con v4 embebido —mapped/compatible/NAT64/6to4—, userinfo, DNS rebinding y ventana de segunda resolución, redirects, multi-A, proxy del entorno); (3) **drill en vivo** con binario reconstruido desde `cad1477` (kernel de prueba, puerto y `DATA_DIR` propios, `dev_mode=true`, `[p2p] enabled=false`) — batería de 26 URLs contra `POST /api/v1/ingest` + sonda del guard de proveedores en arranque con config envenenada.

**Resultado central: SIN BYPASS EXOTABLE en ingest.** 23/23 vectores hostiles devolvieron `400` con la razón exacta; la vía permitida llegó a la capa de fetch (verificado: `502` = decisión del guard distinta de la del fetch); el guard de proveedores está cableado en arranque (`validate_security` → `ExternalProvider::validate_all`, config.rs:2201) y removió 2/2 entradas tóxicas con la razón exacta. Cero hallazgo crítico; quedan 6 observaciones de clase defensa-en-profundidad/robustez (H1-H6), ninguna explotable hoy.

## Batería en vivo contra `POST /api/v1/ingest` (kernel drill `cad1477`, puerto propio)

| Vector | HTTP | Respuesta |
|---|---|---|
| `http://169.254.169.254/latest/meta-data/` (metadata) | 400 | `Blocked private IP: 169.254.169.254` |
| `http://2130706433/` (decimal de 127.0.0.1) | 400 | `Blocked private IP: 127.0.0.1` |
| `http://0x7f000001/` (hex de 127.0.0.1) | 400 | `Blocked private IP: 127.0.0.1` |
| `http://127.1/` (shorthand) | 400 | `Blocked private IP: 127.0.0.1` |
| `http://[::ffff:7f00:1]/` (mapped loopback) | 400 | `Blocked private IP: ::ffff:127.0.0.1` |
| `http://[::ffff:169.254.169.254]/` (mapped metadata) | 400 | `Blocked private IP: ::ffff:169.254.169.254` |
| `http://[64:ff9b::a9fe:a9fe]/` (NAT64 → metadata) | 400 | `Blocked private IP: 64:ff9b::a9fe:a9fe` |
| `http://[2002:a9fe:a9fe::1]/` (6to4 → metadata) | 400 | `Blocked private IP: 2002:a9fe:a9fe::1` |
| `http://192.168.1.1:80/`, `http://10.0.0.1/` (RFC1918) | 400 | `Blocked private IP: …` |
| `http://100.64.0.1/` (CGNAT) | 400 | `Blocked private IP: 100.64.0.1` |
| `http://198.18.0.1/` (benchmarking) | 400 | `Blocked private IP: 198.18.0.1` |
| `http://192.0.0.8/` (192.0.0.0/24) | 400 | `Blocked private IP: 192.0.0.8` |
| `http://240.0.0.1/` (240/4 reserved) | 400 | `Blocked private IP: 240.0.0.1` |
| `http://0.0.0.0/` (unspecified) | 400 | `Blocked private IP: 0.0.0.0` |
| `http://[fc00::1]/` (v6 ULA) | 400 | `Blocked private IP: fc00::1` |
| `http://[fe80::1]/` (v6 link-local) | 400 | `Blocked private IP: fe80::1` |
| `http://localhost:<puerto>/health` (resuelto) | 400 | `Blocked private IP: ::1` |
| `http://127.0.0.1:<puerto>/health` (literal) | 400 | `Blocked private IP: 127.0.0.1` |
| `file:///etc/passwd`, `gopher://127.0.0.1/` | 400 | `Blocked scheme '…'` |
| `http://no-such-host.invalid/` (DNS inexistente) | 400 | `Cannot resolve host …` (fail-closed) |
| `http://x@169.254.169.254/` (userinfo + metadata) | 400 | `Blocked private IP: 169.254.169.254` |
| **Control permitido:** `http://93.184.216.34/` (IP pública literal) | **502** | `Failed to fetch URL …` (pasó el guard; 502 = capa fetch sin internet en la caja) |
| Control: `http://93.184.216.34:22/` y `https://93.184.216.34/` | 502 | ídem |

Los `400` del guard y los `502` del fetch son capas distintas: la separación 400/502 es lo que demuestra que los bloqueos de arriba son decisiones del guard, no fallos de red. Crudo en `test-run-ssrf/work/battery_ingest.txt`.

## Sonda del guard de proveedores en arranque (config drill con4 entradas)

| Entrada | base_url | Resultado en arranque |
|---|---|---|
| `evil_metadata` | `http://x@169.254.169.254/` (vector exacto del B-2) | **REMOVIDA** — `credentials (userinfo) in base_url are not allowed` |
| `evil_private` | `http://192.168.1.1:8080` | **REMOVIDA** — `private RFC-1918 IP range` |
| `good_ollama` | `http://127.0.0.1:11434` | conservada (loopback permitido por diseño: Ollama/LM Studio) |
| `evil_nat64` | `http://[64:ff9b::a9fe:a9fe]/` (NAT64 con metadata embebida) | **conservada** — ver H1 |

Logs: `test-run-ssrf/work/kernel_providers.log` y `kernel_providers2.log`.

## Argumentos estáticos para lo que el drill no puede reproducir

- **DNS rebinding / segunda resolución:** no hay ventana — el host de dominio se resuelve UNA vez, se valida (cualquier IP bloqueada de un conjunto multi-A rechaza entero) y la conexión se fija a la IP validada con el override `resolve()` (api_ingest.rs:259). Verificado contra la fuente de reqwest 0.12.28: el override ignora el puerto del `SocketAddr` (`"Set the port to 0 to use the conventional port… Ports in the URL itself will always be used"`, `async_impl/client.rs:2264`), así que el `SocketAddr::new(ip, 0)` es correcto.
- **Redirects:** `redirect::Policy::none()` (api_ingest.rs:256) — un 302 no se sigue y la respuesta no exitosa devuelve `502 URL returned status`. No reproducido con un redirector vivo porque cualquier redirector local es loopback y el guard lo bloquea ANTES del fetch (eso mismo confirma el orden: validar → conectar).
- **Multi-A mixto (pública + privada):** `ips.iter().find(ip_is_blocked)` rechaza si CUALQUIERA es bloqueada (api_ingest.rs:99-101).

## Hallazgos (ninguno crítico; clase defensa-en-profundidad)

- **H1 (informativo): el guard de proveedores no juzga IPv6 con v4 embebido fuera de `::ffff:`.** `check_external_provider` (config.rs:995) usa `to_ipv4_mapped` (solo `::ffff:a.b.c.d`); `64:ff9b::a9fe:a9fe` (NAT64→169.254.169.254) y `2002:a9fe:a9fe::1` (6to4) pasan — verificado en vivo (`evil_nat64` conservada). El guard de ingest SÍ los bloquea (`ip_is_blocked`, api_ingest.rs:21). Explotabilidad práctica casi nula (config de operador + requiere translator NAT64/6to4 en el host). **Recomendación:** que el config guard reutilice `ip_is_blocked` — una función, dos call-sites, desaparece la asimetría.
- **H2 (bajo, disponibilidad): DNS síncrono sin timeout dentro del handler async.** `validate_url_for_ssrf` llama `ToSocketAddrs::to_socket_addrs` (bloqueante, sin timeout) en el thread del runtime; un DNS que cuelgue inmoviliza un worker de tokio por petición (accesible con JWT de agente, clase AGENT_OR_PEER). **Recomendación:** `spawn_blocking` con timeout o el resolver async de reqwest.
- **H3 (bajo, memoria): `resp.text()` sin límite de tamaño** (api_ingest.rs:265). El timeout de 15 s no limita volumen: un host que devuelva gigabytes puede agotar RAM. **Recomendación:** leer por streams con corte a N MB.
- **H4 (informativo): el cliente de ingest no llama `.no_proxy()`.** reqwest 0.12 honra `HTTP_PROXY`/`HTTPS_PROXY` del entorno por defecto (`proxy.rs` lee `NO_PROXY` etc.); con un proxy definido, el salto lo hace el proxy (resuelve él) y la garantía "conexión fijada a la IP validada" queda condicionada a no tener proxy. En el rig actual no hay proxy. **Recomendación:** `.no_proxy()` en el builder de ingest (una línea) para que el pinning sea incondicional.
- **H5 (informativo, cruce con B-4: ASI06): `/api/v1/ingest` y `/api/v1/memory/write` escriben en silva sin la capa L1 de la puerta de escritura.** `matches_injection_pattern` solo se aplica en `tylluan_remember` (handler_remember.rs:63) y en el recall (coherence_gate.rs:316); `memory_write_handler` (api_memory.rs:12) hace `upsert_node_with_validity` directo y el ingest igual. Ambas rutas son clase agente/peer (auth.rs:1009-1010), así que cualquier JWT puede sembrar contenido que la puerta de escritura no ve — y en B-4 medimos que en el recall la L1 catcha 8/40. **Recomendación:** decidir en Coloquio si ambas rutas pasan por la misma L1/L2 (decisión de arquitectura, no parche mío).
- **H6 (informativo): A2A valida solo el esquema y su cliente sigue redirects.** `validate_url` de `api_a2a_agents.rs:52` comprueba el prefijo `http(s)://`; `A2aClient` usa el cliente por defecto de reqwest (redirects activos, hasta 10). Registrando un agente con URL pública que302 a metadata, el kernel la seguiría en `fetch_card`. **Atenuante:** la ruta es clase operador (auth.rs:262) y el operador ya tiene bash→RCE vía `tylluan_do`, así que no añade capacidad real — es inconsistencia de defensa en profundidad frente al guard de providers (que sí bloquea metadata incluso en config de operador).

## Verificaciones (exit capturado)

- `cargo build -p tylluan-kernel --bin tylluan-nexus` → **EXIT 0** (binario del drill = `cad1477`, confirmado por `/health` del drill)
- Batería ingest → 23/23 hostiles `400` + 3/3 controles permitidos `502` (script reproducible en `test-run-ssrf/work/battery_ingest.txt`)
- Sonda providers → 2/2 tóxicos removidos + `evil_nat64` conservada (H1) en dos arranques independientes
- Tras la sonda: drills terminados (`taskkill` de los3 PIDs míos) y **producción `:47004` verificada sana** (`{"status":"ok","version":"0.17.1","commit":"2458b4a"}`), con su proceso intacto (PID identificado por command-line antes de matar nada)
- Tests unitarios del guard:10 tests en `api_ingest.rs` (corren con `--lib`, parte del recuento de CI)

## No verificado / limitaciones

- La caja de drill **no tiene salida a internet** (el resolver no resuelve `example.com`), así que no hubo un fetch positivo con contenido real; la distinción guard/fetch se hizo por código HTTP (400 vs 502) y por el mensaje de error de cada capa.
- Redirects y rebinding no se reprodujeron dinámicamente (imposibles localmente por el propio guard); sus argumentos son estáticos con la fuente de reqwest citada arriba.
- `tylluan-fsrs`/`tylluan-link` y los fetchers de operador (proxy de guilds, `systemone_http`, `a2a_client`, `maintenance.rs` con URLs hardcodeadas de HuggingFace) solo se revisaron estáticamente: sus destinos son config de operador o constantes, no entrada de agentes en tiempo de ejecución.
- No toqué `crates/tylluan-kernel/src/` (exclusión de la cola respetada): H1-H6 son recomendaciones para TL/Deep, no cambios míos.

## Impact

- **ADR-013 / pre-push:** diff de esta revisión = SOLO este informe (los hallazgos H1-H6 no tocan código); el único diff de código en el worktree sigue siendo el de B-5 (tests/). Nada de `transport/`, `guilds/`, `integrations/` ni `tylluan*.toml` modificado.
- **Superficie de producción:** CERO. Drill con puerto y `DATA_DIR` propios en `test-run-ssrf/` (auto-ignorado con su propio `.gitignore`, patrón de `test-run-gate/`); procesos de drill terminados y producción verificada sana después.
- **Valor entregado:** confirmación independiente, en vivo y reproduible, de que los dos hallazgos de B-2 están cerrados; asimetría de defensa en profundidad entre los dos guards (H1); y 5 observaciones de robustez/coherencia (H2-H6), de las cuales H5 es la que merece decisión de equipo (puerta ASI06 no cubre las rutas de escritura de agente).
