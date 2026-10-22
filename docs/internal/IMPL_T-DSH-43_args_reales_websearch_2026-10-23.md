# T-DSH-43 — ¿Qué argumentos recibe de verdad `web_search` por el router?

- **Agente:** dsh
- **Fecha:** 2026-10-23
- **Base:** `E:\tylluan-wt-dsh` (HEAD `d56f8b8`), con el diff de T-DSH-42 **intacto y separado** en el worktree. Encargo: solo lectura + un test; sin tocar `src`.
- **Encargo (turno 1575):** medir qué argumentos recibe `web_search` cuando se llama por el router, partiendo de la semilla de args con `"command": intent` (`prepare.rs`, ~línea 126) y de si se poda contra el esquema de la herramienta. Tests: (1) ¿el JSON final trae `command` y con qué valor? (2) lo mismo para `vision_ocr`/`vision_analyze`; (3) lo mismo para `bash`; fijar el comportamiento de hoy en un test vivo. Si la función no es alcanzable desde `tests/` sin tocar `src`, decirlo y pedir la visibilidad mínima. Conclusión: si `command` llega siempre, proponer el criterio exacto para que `enforce_capabilities` lo ignore en herramientas que no son de ejecución (sin implementarlo); si no llega, el falso bloqueo no existe y T-DSH-42 se integra tal cual.

## Alcanzabilidad — la función real NO es alcanzable (y por qué)

- `resolve_and_prepare_tool_call` es **`pub(super)`** (`crates/tylluan-kernel/src/transport/server/handler_do/prepare.rs:33`) y el módulo que la contiene es **privado**: `mod prepare;` (`handler_do/mod.rs:42`). `handler_do` sí es `pub`, pero `prepare` no, así que un test de integración no puede nombrar ni llamar la función.
- **Visibilidad mínima que pido (no la he cambiado):** extraer la semilla literal de `prepare.rs:125-131` a un helper público puro, p. ej.
  `pub fn seed_tool_args(intent: &str, url_hint: &str) -> serde_json::Value` (mismo contenido, sin cambio de comportamiento) y que `resolve_and_prepare_tool_call` lo use; el test lo llama directamente. Alternativa más amplia: `mod prepare;` → `pub mod prepare;` y `fn` → `pub fn`, pero eso expone toda la resolución (y su dependencia de `TylluanServer`), así que recomiendo el helper puro.
- Mientras no exista, el test mide por dos vías que sí son verificables **sin kernel y sin tocar `src`**:
  1. **réplica fiel del seed** (`prepare.rs:125-131`, copiado literal; para este intent no hay `path` ni `agent_id`, así que el mapa es exactamente el original),
  2. **la función real de contención** `enforce_capabilities` (pública) con las CAPABILITIES declaradas por los `.py` (parser local), para medir la consecuencia exacta.

## Evidencia estática: no hay poda contra el esquema

- `prepare.rs:125-131` siembra **para toda guild/toda herramienta**:
  `{"command": intent, "intent": intent, "task": intent, "query": intent, "text": intent, "content": intent, "prompt": intent, "message": intent, "input": intent, "server_url": url_hint, "url": url_hint, "timeout_secs": 30, "language": "", "depth": 2, "max_results": 50}`.
- `prepare.rs:335-338` construye `CallToolRequestParam { arguments: Some(tool_args.as_object()...) }` **directamente con ese mapa**: no hay llamada a esquema, ni `retain`/`filter` por `properties`/`inputSchema` en todo `prepare.rs` (búsqueda en el fichero: 0 referencias a `schema|prune|retain|input_schema`).
- Ese `params` es exactamente el que recibe la contención: `GuildProcess::check_capabilities(&params)` (`guild_process.rs:540`, invocada en 649/662/680) y `enforce_capabilities(&self.name, caps, params)` (576/592).
- Bloqueo actual para `process_execution=false`: `is_exec_like_tool(params)` **o** `has_command_arg` (args contienen `command` string no vacío) → `guild_process.rs` (~226-241).

## Tabla: herramienta × argumentos finales (intent `"busca en la web rust async"`)

Del test `router_seed_carries_command_for_web_search_vision_and_bash` (réplica del seed, `--nocapture`):

| herramienta | `command` en args finales | valor | otras claves relevantes | nota |
|---|---|---|---|---|
| `web_search` | **sí** | `busca en la web rust async` | `query`, `intent`, `task`, `text`, … también = intent | 15 claves del seed; `websearch.py` acepta `command` y usa `q = query or intent or command` |
| `vision_ocr` | **sí** | `busca en la web rust async` | ídem | seed idéntico; sin bloque específico de guild |
| `vision_analyze` | **sí** | `busca en la web rust async` | ídem | ídem |
| `bash_execute` | **sí** | intent crudo en el seed; luego **sobrescrito** por `extract_command_from_intent(intent)` en `prepare.rs:154-175` | ídem | el valor exacto no se puede medir (extractor privado), pero es no vacío y la conclusión no cambia |

Y se cumple lo esperado de "sin poda": `claves del seed == claves enviadas` en `CallToolRequestParam` (aserción del test).

## Consecuencia medida con `enforce_capabilities` (función real) + CAPABILITIES reales

Con las CAPABILITIES declaradas en los `.py` (`vision` y `websearch`: `process_execution=false`) y el override de **Strict**, el test `strict_enforcement_blocks_vision_and_websearch_because_of_router_seed` da:

```text
herramienta      caps       args                      process_execution=false
web_search       parsed     seed con command=intent   BLOQUEADA (falso bloqueo)
web_search       parsed     sin command               permitida
vision_ocr       parsed     seed con command=intent   BLOQUEADA (falso bloqueo)
vision_ocr       parsed     sin command               permitida
vision_analyze   parsed     seed con command=intent   BLOQUEADA (falso bloqueo)
vision_analyze   parsed     sin command               permitida
bash_execute con caps declaradas (process_execution=true): None
```

## Conclusión

**`command` llega siempre a `web_search`** (y a `vision_ocr`, `vision_analyze` y `bash`): lo siembra el router para toda petición y no se poda contra el esquema. Por tanto **el falso bloqueo es real**, y peor de lo medido en T-DSH-41: no es solo `web_search`, **también `vision_ocr` y `vision_analyze`** quedarían bloqueadas bajo Strict en cuanto las CAPABILITIES se parseen (T-DSH-42). El bloqueo desaparece si se quitan del mapa los args `command` (fila "sin command") ⇒ el disparador es el seed, no el usuario.

**Consecuencia operativa: T-DSH-42 NO debe integrarse solo.** Si se integra el parser sin tocar la contención, cualquier usuario en perfil Strict (o con override de agente) pierde websearch y visión — justo el caso "permitido → roto" que señaló el TL.

### Criterio exacto propuesto para `enforce_capabilities` (D-35b, NO implementado aquí)

La semántica de ejecución debe salir de la **herramienta**, nunca de la mera presencia de un argumento `command`, porque `prepare.rs:126` lo siembra siempre con el intent:

1. `is_exec_like_tool(params)` se queda como único criterio por nombre (contiene `execute|run|exec|spawn|shell|command`).
2. En la rama `process_execution=false`: **eliminar el término `has_command_arg`**; bloquear solo si `is_exec_like_tool(params)`.
3. En la rama de allowlist no vacía: extraer y verificar `command` **solo si** `is_exec_like_tool(params)` es verdadero; para herramientas no-ejecutoras, ignorar `command` por completo (hoy produciría "command 'busca' not allowed" en cualquier guild con allowlist).
4. El resto (`filesystem_scope`, allowlist de binarios para herramientas exec-like) queda igual.
5. Si en el futuro se quiere política basada en argumentos, hace falta **metadato explícito** (campo en `CAPABILITIES` o en el `inputSchema` de la herramienta, p. ej. `command_arg: "cmd"`), no la heurística de "existe la clave command" — con el seed actual es indistinguible del intent del router. Fuera de alcance de D-35b.

Riesgo residual declarado: una herramienta peligrosa con nombre no exec-like y argumento `command` dejaría de bloquearse; con el seed actual el heurístico **tampoco la bloquearía de forma fiable** (bloquearía también todo lo legítimo), así que no se pierde una defensa real, se elimina un falso positivo estructural.

No recomiendo tocar el seed del router en esta ronda: cambiar `prepare.rs:126` afecta a todas las guilds (algunas usan `command` como fallback legítimo; `web_search` ya tiene `query`/`intent`) y es un cambio de comportamiento más amplio; el criterio 2-3 es mínimo y deja el seed intacto.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test router_websearch_args -- --nocapture` | **2 passed / 0 failed** (tablas arriba) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `check_test_count.sh` | exit 0 (1273; el test nuevo es de integración y no cuenta) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente `ROADMAP_O3.md:42` (artefacto WSL) |
| sin `cfg(unix)`/`cfg(windows)` en ninguna función | ok |
| diff de T-DSH-42 | **intacto y separado**; T-DSH-43 solo añade `tests/router_websearch_args.rs` + este informe |

## Impact

- **Solo lectura + un test nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no arranca el kernel ni toca puertos; sin red (las CAPABILITIES se leen de los `.py` y `enforce_capabilities` es pura).
- **Decisión desbloqueada:** confirma que el falso bloqueo existe y es más amplio de lo creído (visión incluida) y da el criterio exacto para arreglarlo en `enforce_capabilities` sin tocar guilds ni router. Con eso, T-DSH-42 puede integrarse **junto con** el arreglo de contención (o después de él), pero no solo.
- **Para el TL:** la visibilidad pedida (helper puro `pub fn seed_tool_args`) es de una función y sin cambio de comportamiento; el test actual seguirá siendo válido cuando exista (se sustituye la réplica por la llamada real).

## No verificado

- **Función real no ejecutada**: `resolve_and_prepare_tool_call` es `pub(super)`; la tabla sale de la réplica literal del seed + lectura del código, no de invocarla. Cuando exista el helper público, hay que re-ejecutar para confirmar (el test ya deja la base).
- **Sin kernel en vivo**: no se llamó `tylluan_do` ni MCP; no se midió el path Balanced+flag (`capabilities_enforce` apagada por defecto) — solo el caso Strict/caps con `process_execution=false`, que es el que cambia.
- **`bash`**: el valor exacto de `command` tras `extract_command_from_intent` (privado) no se mide; solo que es no vacío y que sus caps (`process_execution=true`) no bloquean.
- **Filtrado del lado guild**: si FastMCP descarta kwargs desconocidos, la guild no vería `command`; la contención del kernel **sí** lo ve (ocurre antes del dispatch), que es lo que importa para el falso bloqueo; no se verificó qué hace cada guild con args extra.
- **CAPABILITIES parseadas**: el test parsea las declaraciones del `.py` por su cuenta (réplica), precisamente para no depender de que T-DSH-42 esté integrado; no se comprobó el catálogo real en `origin/main` (allí sigue `None`).
- **El arreglo propuesto** (D-35b) no está implementado ni probado; solo el criterio.

## Cierre del TL (2026-10-23)

- Integrado junto con T-DSH-42 (parser) y **D-35b** implementado por el TL en `registry/guild_process.rs`: en `enforce_capabilities`, rama `process_execution=false`, el bloqueo depende solo de `is_exec_like_tool` (se quita `has_command_arg`); en la rama de allowlist, `command` solo se verifica si la herramienta es exec-like.
- Tests ajustados: `capabilities_enforce_matrix` (0 falsos bloqueos; 7 maliciosas pasan: la 7ª es `vision_analyze` con `command` inyectado, **residual declarado**), `cola_d35_acceptance` (el ignorado `D-35` pasa a vivo), `router_websearch_args` (ahora afirma que NO se bloquea) y 2 unit tests nuevos en `guild_process.rs`.
- Residual y decisión: `capabilities_enforce` sigue apagada por defecto (decisión de José). Una herramienta de nombre no exec-like que ejecute su argumento `command` dejaría de bloquearse bajo Strict; con el seed del router el heurístico anterior tampoco lo distinguía del intent. Defensa futura: metadato explícito en CAPABILITIES.
- Verificación TL: lib 1187 (4 ignorados) en verde, clippy `-D warnings` exit 0, 3 tests de integración verdes.
