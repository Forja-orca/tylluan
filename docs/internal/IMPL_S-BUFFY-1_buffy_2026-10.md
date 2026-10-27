# IMPL S-BUFFY-1 — ADR-024 listo para activar: 5 tests rojos de contención

**Autora:** Buffy · **Fecha:** 2026-10-27 · **Base:** `origin/main` = `HEAD` = `d79990a`.
**Encargo:** decisión de José (2026-10-26): la seguridad **no** está congelada. Escribir el test de regresión que **hoy falla** para cada control A4, D1, B1, A1 y C3 de ADR-024, marcado `#[ignore]`, de modo que aprobar sea **solo activar**. Sin tocar código de producción.

**Entregable:** `crates/tylluan-kernel/tests/adr024_containment_pending.rs` (nuevo, sin commitear).
**Estado:** los **5 fallan hoy por la razón correcta** (`test result: FAILED. 0 passed; 5 failed; 0 ignored`, exit 101). En la suite normal quedan invisibles y en verde (`0 passed; 5 ignored`, exit 0).
**Log de la corrida roja:** `E:\tylluan-patches\buffy\adr024_red.log`.

```bash
CARGO_TARGET_DIR=/e/cargo-target/buffy cargo test -p tylluan-kernel \
  --test adr024_containment_pending -- --ignored --nocapture --test-threads=1
```

---

## Tabla ítem → test → salida roja → arreglo esperado

| Ítem (ADR-024 §4) | Test | Salida de hoy (rojo) | Arreglo esperado (fichero:línea) |
|---|---|---|---|
| **A4** confirmación humana + auditoría del diff en alta/edición de MCP externo | `a4_external_mcp_add_without_confirmation_is_rejected` | `assertion left == right failed: ADR-024 A4: sin confirmación explícita del comando el alta debe ser 400; hoy responde 201 Created y el cuerpo es {"name":"adr024-a4","status":"registered"}` | `api_v1/api_mcp.rs:66` (`add_mcp_server_handler`, rechazo antes de registrar) y `api_v1/api_mcp.rs:175` (`update_mcp_server_handler`); auditoría con `log_audit_entry`/`audit_db_path` (`transport/server/handler_do/audit.rs`) |
| **D1** descubrimiento como propuesta, sin registro automático | `d1_discover_proposes_without_registering` | `ADR-024 D1: discover no debe registrar nada sin aprobación; hoy discovered = Some(Number(1)) (servers = Some(Array [String("adr024-ghost")]))` | `api_v1/api_mcp.rs:288` (`discover_mcp_servers_handler`): dejar de hacer `config.external_mcp.extend(...)` + `persist_external_mcp_config` y devolver la lista de propuestas |
| **B1** allowlist de claves escribibles en `POST /api/v1/config` | `b1_config_api_rejects_process_execution_keys` | `ADR-024 B1: sembrar [[external_mcp]].command por la API debe ser 400; hoy 200 OK (cuerpo: {"status":"saved"})` | `api_v1/api_admin.rs:712` (`save_config_handler`), tras el parseo TOML y antes de `fs::write` |
| **A1** sin `cmd /c` para rutas absolutas verificadas; metacaracteres rechazados | `a1_shell_metacharacters_in_command_are_rejected` | `ADR-024 A1: un command con metacaracteres de shell debe rechazarse al alta; hoy 201 Created (cuerpo: {"name":"adr024-a1","status":"registered"})` | `api_v1/api_mcp.rs:66` (validación del `command` en el alta) + `registry/guild_process.rs:649` (`Command::new("cmd")`: spawn directo si la ruta absoluta es un ejecutable, wrapper solo para extensiones permitidas) |
| **C3** endpoint directo de guild operator-class para `bash`/`filesystem` | `c3_direct_guild_endpoint_is_operator_class_for_sensitive_guilds` | `ADR-024 C3: 'bash' por endpoint directo debe exigir token de operador; POST /api/v1/guilds/bash/call/execute es hoy agent-class` | `transport/http/auth.rs:258` (`operator_class_required`) para las dos rutas, y/o `api_v1/api_guilds.rs:50` (`guild_tool_call_handler`) con el chequeo por guild; rutas en `api_v1/routes.rs:32` (`/tools/`) y `:139` (`/call/`) |

### Qué hace cada test exactamente

- **A4:** `POST /api/v1/mcp/external` con `command` de stdio y **sin** campo de confirmación → exige `400` y un cuerpo que mencione la confirmación. Hoy registra el server, intenta arrancarlo y persiste config (201).
- **D1:** siembra un perfil sintético de terceros (`{"mcpServers": {"adr024-ghost": {...}}}`) en un `APPDATA` **temporal** y llama a `POST /api/v1/mcp/external/discover` → exige `discovered == 0` y propuestas (o lista vacía de registrados). Hoy importa el server (`discovered = 1`) y lo persiste.
- **B1:** `POST /api/v1/config` con un TOML **válido** que contiene `[[external_mcp]].command` → exige `400` y que el fichero **no** se reescriba. Hoy devuelve `200 {"status":"saved"}` y deja la clave sembrada para el siguiente arranque. (El TOML parsea: el 200 es del guardado, no un falso positivo por parseo.)
- **A1:** alta con `command` que contiene metacaracteres (`| rem ...`) → exige `400`. Hoy acepta y lo manda a `cmd /c` (la corrida imprime «El sistema no puede encontrar la ruta especificada», que es el propio `cmd` intentando ejecutarlo).
- **C3:** matriz de clase de ruta: `bash` y `filesystem` por `/call/` y `/tools/` deben ser operator-class, y `vision`/`llama_backend` por `/call/` deben seguir siendo agent-class (T-DSH-71: dashboard de visión y juez). Hoy las cuatro primeras son agent-class.

---

## Aislamiento (nada toca producción ni el repo)

- La config se desvía con `TylluanConfig::set_config_path_override(<temp>/tylluan.toml)`: **A4, A1, B1 y D1 escriben ahí, nunca en el `tylluan.toml` del repo**. Verificado: el `tylluan.toml` del worktree no aparece en `git status`.
- `APPDATA` y `USERPROFILE` se desvían a temporales solo dentro del test de D1 (proceso de test, serializado con un `tokio::sync::Mutex`; `set_var` va en `unsafe` con su comentario, como el resto de la suite).
- No se arranca ningún kernel, ni guild, ni se abre ningún puerto: los handlers se conducen in-process con `api_v1_routes()` sobre el `HttpState` del harness compartido (`tests/support/mod.rs`).
- Los 5 son `#[ignore = "ADR-024 <ítem>: pendiente de aprobación"]`, así que **CI y la suite normal no cambian** (verificado: `0 passed; 5 ignored`, exit 0).

---

## Límites declarados de esta entrega

1. **C3 se comprueba por clase de ruta, no por el 403 real.** Es donde el ADR ubica su test (`auth.rs`) y es estable sin montar middleware. La matriz de comportamiento (token de agente → 403; operador → 200; guild no sensible → 200) exige sembrar `acl.roles` y el contexto de auth en el router autenticado: queda como paso siguiente al aprobar, no simulada aquí para no fingir cobertura.
2. **A1 se comprueba por el rechazo en el alta**, no inspeccionando el argv del spawn (la función de spawn es privada y no hay constructor de argv público). Con el arreglo, el test sigue siendo válido: el alta debe rechazar el metacaracter antes de llegar al spawn.
3. **D1 depende de `APPDATA`/`USERPROFILE`** porque `discover` compone sus rutas con ellos; por eso el test es determinista solo si esas variables se fijan **antes** de llamar al handler (lo hace).
4. **No hay implementación**: nada de `src/` se ha tocado. Los cinco siguen rojos hasta que José apruebe y alguien implemente; entonces basta quitar cada `#[ignore]`.

---

## Verificación ejecutada

| Comprobación | Resultado |
|---|---|
| `cargo test -p tylluan-kernel --test adr024_containment_pending --no-run` | compila, exit 0 |
| Igual con `-- --ignored --nocapture --test-threads=1` | **5 failed**, exit 101 — los 5 rojos citados arriba |
| Igual sin `--ignored` (suite normal) | `0 passed; 5 ignored`, exit 0 |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` (incluye el fichero nuevo) | **exit 0** |
| `git status` del worktree | el `tylluan.toml` del repo **no** aparece modificado |

## Impact

- **Producción:** cero cambios. Fichero de test nuevo, todos sus casos `#[ignore]`; ni `src/`, ni workflows, ni timeouts, ni config versionada.
- **CI:** no se altera el número de tests ejecutados (los 5 se declaran ignorados); futuro trabajo del TL: al aprobar cada control, quitar su `#[ignore]` y el test entra en la suite normal sin más.
- **Riesgo:** bajo. El único efecto lateral es que, al ejecutarse con `--ignored`, dos tests intentan un spawn fallido contra rutas inexistentes (`C:\tylluan-no-existe\...`) y uno escribe en un `tylluan.toml` temporal; nada sale de `%TEMP%`.
- **ADR-013:** no se borra ni reescribe trabajo ajeno; el ADR se cita, no se modifica.

## No verificado

- **No** he implementado ningún control: no hay prueba de que los tests pasen con el arreglo (solo de que fallan hoy por la condición que el ADR describe).
- **No** he verificado el efecto de A4/D1 sobre el dashboard (`/api/v1/mcp/external` tendría que enviar la confirmación): el ADR lo advierte y esta entrega no toca el dashboard.
- **No** he probado C1 (ACL fail-closed) ni C2/A2/A3/E: están fuera del encargo S-BUFFY-1 (A4, D1, B1, A1, C3).
- **No** he medido la fricción real de los cambios propuestos con agentes vivos; las cifras de consumidores son las de T-DSH-71, no una medición nueva.
- **No** he comprobado el comportamiento en Linux/macOS de A1 (wrapper `.cmd`): solo aplica a Windows y aquí no se ejercita.
