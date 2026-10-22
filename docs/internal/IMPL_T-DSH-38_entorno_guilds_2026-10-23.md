# T-DSH-38 — Auditoría del entorno que recibe una guild al arrancar (prepara D-34)

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`7ae896e`, push 110); limpio antes de empezar.
- **Encargo (turno 1562):** solo lectura sobre src + un test nuevo. Mapa de qué entorno recibe cada guild al arrancar: leer `guild_process.rs` (~478-491, inyección de `TYLLUAN_TOKEN`, y todo lo que construye el `Command`); crear `tests/guild_env_audit.rs` que arranque una guild de prueba mínima (script python de una línea en directorio temporal) que vuelque sus variables de entorno, y verifique (a) qué `TYLLUAN_*` recibe, (b) si recibe el token maestro, (c) si hereda secretos del padre (TOKEN/KEY/SECRET/PASSWORD). Fijar el comportamiento actual con un test que pase hoy y **marcar `#[ignore = "D-34"]` el que afirma lo deseado** (sin token maestro). No arrancar el kernel ni tocar puertos; sin `cfg(unix)/cfg(windows)`; no tocar `src`; clippy exit 0.

## Cómo se construye el entorno (lectura de `guild_process.rs`)

- El `Command` se construye en el `match &self.launcher` (líneas 405-475): `Python` (con `PYTHONPATH`/`PYTHONUNBUFFERED` y, si el perfil manda, `docker run`), `External` (comando + args + `cwd` + `envs()` opcionales; en Windows envuelto con `cmd /c`), `Http`/`Sse` (sin subproceso).
- **No se llama a `env_clear()` en ningún punto**: `std::process::Command` hereda por defecto **todo** el entorno del kernel.
- Justo después del `match`, si `resolve_self_auth_token()` devuelve `Some`, se hace `command.env("TYLLUAN_TOKEN", token)` (**línea 491**): el token que acepta el kernel, es decir, el maestro (env → `.tylluan-token`).
- En la rama sandbox Docker (líneas 426-438) el `Command` es `docker run …`; la inyección de `TYLLUAN_TOKEN` va al **cliente docker**, y el contenedor solo recibe `-e PYTHONPATH` y `-e PYTHONUNBUFFERED` (el token no entra por entorno en el contenedor).

## Test y medición

`crates/tylluan-kernel/tests/guild_env_audit.rs` (nuevo; `src/` intacto):

- crea un directorio temporal con un **servidor MCP stdio mínimo** en Python que, antes del handshake, escribe `os.environ` a un JSON; después responde `initialize` y `tools/list` (así la guild arranca por el camino REAL: `GuildProcess::spawn` → `McpProxy::spawn` → `list_tools`);
- como "kernel" (proceso de test) fija `TYLLUAN_TOKEN` a un centinela y cuatro secretos `D34_PARENT_*` con TOKEN/KEY/SECRET/PASSWORD en el nombre;
- arranca la guild con `GuildLauncher::External` (mismo código de inyección de token que las Python), lee el volcado y lo verifica.

**Comportamiento ACTUAL (test que pasa hoy, `current_guild_env_receives_master_token_and_parent_secrets`):**

| variable | ¿recibida? | valor (enmascarado) | motivo |
|---|---|---|---|
| `TYLLUAN_TOKEN` | **SÍ** | `d34-…` (25 bytes) = token maestro centinela | **inyectada** por `guild_process.rs` línea 491 |
| `TYLLUAN_GIT_COMMIT` | **SÍ** | `7ae8…` (7 bytes) | **heredada** del entorno del kernel (no la inyecta el spawn) |
| `PYTHONPATH` / `PYTHONUNBUFFERED` | no en `External` (sí en launcher `Python`, líneas 446-447) | — | set por el launcher Python; en `External` solo si el padre las tiene |
| `D34_PARENT_TOKEN_KEY` | **SÍ** | `pare…` (19 bytes) | **heredada sin filtrar** (Command sin `env_clear`) |
| `D34_PARENT_API_KEY` | **SÍ** | `pare…` (17 bytes) | heredada sin filtrar |
| `D34_PARENT_PASSWORD` | **SÍ** | `pare…` (22 bytes) | heredada sin filtrar |
| `D34_PARENT_CREDENTIAL_SECRET` | **SÍ** | `pare…` (19 bytes) | heredada sin filtrar |

En palabras: **cualquier variable del entorno del kernel llega a la guild**, y además `TYLLUAN_TOKEN` se inyecta explícitamente con el valor del token maestro. La guild puede leer también `.tylluan-token` si corre con el workspace montado (caso sandbox).

**Comportamiento DESEADO (`desired_guild_env_does_not_receive_master_token_or_parent_secrets`, `#[ignore = "D-34"]`):** afirma que la guild **no** recibe el token maestro ni los secretos del padre. Hoy **falla por la razón correcta**:

```text
thread 'desired_guild_env_does_not_receive_master_token_or_parent_secrets' panicked:
D-34 pendiente: la guild sigue recibiendo el token maestro como TYLLUAN_TOKEN
test result: FAILED. 0 passed; 1 failed; 0 ignored
```

## Lectura para D-34

1. **La credencial de clase agente de D-34 es necesaria pero no suficiente**: aunque se sustituya `TYLLUAN_TOKEN` por una credencial de guild, **el resto del entorno del kernel sigue pasando** (claves de API de embeddings/inferencia, tokens de servicios, rutas). D-34 debería incluir también **`env_clear()` + lista blanca explícita** (`PATH`, `PYTHONPATH`, `PYTHONUNBUFFERED`, `HOME`/`USERPROFILE` si hace falta, `TYLLUAN_TOKEN` nuevo) para no heredar secretos del operador.
2. **Hoy la lección es doble**: la guild recibe el maestro (H6, ya en la cola) y hereda secretos arbitrarios del entorno del kernel (nuevo dato medido). La segunda parte no estaba en el ADR-022.
3. **Sandbox**: en la rama Docker el token no entra por entorno en el contenedor (solo al CLI docker), pero el workspace `:ro` deja leer `.tylluan-token` si existe — otro motivo para no dejar el maestro en un fichero montado cuando D-34 exista.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test guild_env_audit -- --nocapture` | **1 passed / 0 failed / 1 ignored** (0,7 s); tabla impresa |
| `cargo test --test guild_env_audit -- --ignored` | **1 failed**, por la razón correcta (D-34 pendiente) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** el test nuevo + este informe; `src/` intacto |
| `check_test_count.sh` | exit 0 (1272; test de integración) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función; no se arrancó el kernel ni se tocaron puertos.

## Impact

- **Solo lectura sobre `src` + test nuevo**: cero cambios en código de producción, guilds, configuración o corpus; el test arranca **un** proceso MCP mínimo en un temporal y lo mata al terminar; no toca `E:\tylluan` ni el kernel `:47004`. Coste: ~0,7 s.
- **Para D-34/D-35**: deja medido y reproducible el mapa de entorno de guilds: token maestro inyectado + herencia completa del entorno del kernel. Añade a D-34 un requisito que no estaba explícito (limpiar/filtrar el entorno) y deja el test deseado listo para des-ignorar cuando D-34 cierre.

## No verificado

- **Launcher `Python` real**: el test usa `GuildLauncher::External` (mismo bloque de inyección de token, líneas 478-491) para poder apuntar a un script temporal; no se ejecutó el launcher Python de una guild real (necesitaría un módulo en `guilds/`). Las variables `PYTHONPATH`/`PYTHONUNBUFFERED` que añade ese launcher se leen del código (líneas 446-447), no se midieron en el hijo.
- **Rama sandbox Docker**: no ejecutada (Docker Desktop estaba con el motor apagado, T-DSH-35); la ausencia de `TYLLUAN_TOKEN` en el entorno del contenedor se deduce de los `-e` de las líneas 426-438.
- **`TYLLUAN_GIT_COMMIT`**: aparece heredada del entorno del proceso de test (mi shell/build); no verifiqué quién la exporta en producción (no está en `guild_process.rs`).
- **Volcado completo del entorno**: el test guarda el JSON completo en un temporal y lo borra, pero **solo imprime** `TYLLUAN_*` y nombres con TOKEN/KEY/SECRET/PASSWORD (valores enmascarados); no revisé a mano todas las variables restantes para no volcar secretos reales a los logs.
- **Otros launchers** (`Http`/`Sse`): no tienen subproceso, no aplican; no medidos.
- **CI**: medido en mi Windows; el temporizador del handshake MCP podría necesitar más margen en un runner lento (hoy 0,7 s con 20 s de timeout de guild).
