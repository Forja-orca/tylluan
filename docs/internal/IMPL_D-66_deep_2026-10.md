# IMPL D-66 — Aviso de `expose_guild_tools` al arrancar

**Agente:** Deep
**Base:** `origin/main` = `d4f2537` (fast-forward desde `1a24e82`, con `REVIEW.md` ya leído).
**Tipo:** diff sin commitear. Solo el TL comitea y empuja.

## Cambio

En `crates/tylluan-kernel/src/main.rs`, dentro del arranque de guilds `always_on`, se lee `config.nexus.expose_guild_tools` y, si es `true`, se calcula el número de tools que devuelve `all_tools()` y se emite un `warn!`:

```text
[SECURITY] nexus.expose_guild_tools=true: exposing N MCP tools instead of the 6 CONTRACT-01 sovereign tools. This is an opt-in exception; keep it false in production.
```

No se cambia el valor por defecto, el flag ni el comportamiento de `all_tools()`.

Referencias: `main.rs`, líneas 1454-1472; `tools.rs`, líneas 16-44 (modo normal y expandido); `tools.rs`, línea 49 (`sovereign_tool_names`).

## Tests

En `crates/tylluan-kernel/src/transport/server/tools.rs`:

- `default_all_tools_are_exactly_six_in_sovereign_order`: servidor de test con memoria en proceso; `all_tools()` devuelve exactamente los seis nombres de `sovereign_tool_names()`, en ese orden.
- `expose_guild_tools_still_contains_all_six_sovereign_tools`: con `expose_guild_tools = true`, las seis tools siguen presentes y el total no es menor que seis.

Ambos usan `#[tokio::test(flavor = "multi_thread")]` porque `SilvaDB::in_memory()` usa `block_in_place` y el runtime de un solo hilo lo rechaza.

## Verificación

```text
cargo test -p tylluan-kernel --lib -j 2        -> 1403 passed, 0 failed, 4 ignored
cargo clippy -p tylluan-kernel --all-targets -j 2 -- -D warnings -> exit 0
```

La base tenía 1401 lib tests; los dos tests nuevos dejan 1403.

## Impact

- **Observabilidad:** el operador ve en el arranque que el flag opt-in está activo y cuántas tools expone; antes solo se descubría consultando `tools/list`.
- **Seguridad:** no cambia la superficie ni el default. El flag sigue siendo la excepción documentada de CONTRACT-01.
- **Riesgo:** el número se calcula cuando terminan de arrancar las guilds `always_on`; guilds perezosas o warm pool pueden añadir más tools después, y el aviso no se repite. El mensaje dice "exposing N" en ese instante, no un total futuro.
- **No depredación:** solo `main.rs` y `tools.rs`; no se tocó ningún guild, configuración ni kernel vivo.

## No verificado

- No arranqué un kernel real con `expose_guild_tools = true` para leer el `warn!` en el log; los tests cubren la lista, no la captura del log.
- No medí el número de tools con una configuración de producción que tenga guilds externas activas.
- El recuento es el del servidor en el momento del aviso, no el de guilds que arranquen después.
