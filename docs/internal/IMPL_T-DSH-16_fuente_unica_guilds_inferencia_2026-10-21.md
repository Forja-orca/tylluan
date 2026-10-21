# T-DSH-16 — Fuente única de verdad para las guilds de inferencia CPU

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `54dad51` == origin/main (push87; T-DSH-15 integrada, lib 1150).
- **Encargo (turno 1483):** cerrar la deuda declarada por Deep en D-19: `UNLIMITED_PATIENCE_GUILDS` (`registry/guild_process.rs`) y `cpu_inference_guilds` (`main.rs`) se mantenían a mano. Diff sin commitear con **una sola fuente de verdad** + test que falle si alguien añade una guild de inferencia en una y no en la otra; sin cambiar el contenido de la lista ni ningún timeout. Archivos: `guild_process.rs`, `main.rs` (solo esas líneas) y el test en el mismo módulo. Criterios: clippy `--all-targets -D warnings` exit 0, lib 1150 +1 test nuevo, `effective_tool_call_secs` idéntico por guild (tabla).

## Solución

**`crates/tylluan-kernel/src/registry/guild_process.rs`**

- Nueva **const de módulo** `pub(crate) const CPU_INFERENCE_GUILDS: &[&str]` con los mismos 6 miembros, en el mismo orden: `vision`, `deep_analysis`, `knowledge`, `comfy_ui`, `n8n_bridge`, `llama_backend`. Su doc conserva la historia que justifica cada miembro (10+ minutos en CPU; el caso `llama_backend` de D-19/B-6).
- Se **elimina** la const asociada `UNLIMITED_PATIENCE_GUILDS` (era la copia del registry) y `effective_tool_call_secs` pasa a usar `CPU_INFERENCE_GUILDS`.
- Nuevo acceso `pub fn cpu_inference_guilds() -> &'static [&'static str]` que devuelve **esa misma** const.

**`crates/tylluan-kernel/src/main.rs`** (solo las líneas de la lista)

- La lista local `let cpu_inference_guilds = [...]` desaparece y se sustituye por el acceso: `let cpu_inference_guilds = tylluan_kernel::registry::guild_process::cpu_inference_guilds();`. El resto del bloque (`g.tool_timeout = None` para always_on) queda igual.

**Por qué accessor y no `pub(crate)` a secas para ambos sitios:** `src/main.rs` es **otro crate target** del mismo package (binario), así que no puede ver items `pub(crate)` del crate lib. La const queda `pub(crate)` (uso interno del lib) y el binario la consume por una función pública de solo lectura; sigue habiendo **una única lista**.

**Test anti-deriva** (mismo módulo, `guild_process.rs`): `cpu_inference_guilds_list_has_a_single_source_of_truth` falla si:
1. `main.rs` vuelve a declarar su propia lista (`let cpu_inference_guilds = ["…`), o contiene un array literal de guilds de inferencia aunque use otro nombre de variable (`["vision", "deep_analysis"`);
2. `main.rs` deja de consumir `cpu_inference_guilds()`;
3. el acceso público devuelve algo distinto de `CPU_INFERENCE_GUILDS`;
4. algún nombre de la const no existe en el catálogo real (typo o guild retirada).

## `effective_tool_call_secs` antes/después — idéntico (45 guilds del catálogo; `docker` se midió dos veces por el fixture)

Medición: test temporal en el mismo módulo (fixture `system 15 s / analysis 60 s / heavy 180 s`, `tool_call_secs` plano 3600), ejecutado **antes** y **después** del refactor; las 46 líneas salieron idénticas (`Compare-Object` sin diferencias). Se eliminó el test temporal tras medir.

| guild | seg | categoría |
|---|---|---|
| ast_surgeon | 60 | Analysis |
| audio_tools | 180 | Heavy |
| audit | 60 | Analysis |
| bash | 15 | System |
| biome_warden | 60 | Analysis |
| browser | 60 | Analysis |
| clipboard_tools | 60 | Analysis |
| code | 60 | Analysis |
| code_analysis | 60 | Analysis |
| code_graph | 60 | Analysis |
| code_reviewer | 60 | Analysis |
| coloquio | 60 | Analysis |
| coloquio_digest | 60 | Analysis |
| comfy_ui | **3600** | Heavy |
| coordinator | 60 | Analysis |
| cron_scheduler | 60 | Analysis |
| data_tools | 60 | Analysis |
| database | 180 | Heavy |
| deep_analysis | **3600** | Heavy |
| deep_web_research | 180 | Heavy |
| docker | 180 | Heavy |
| ffmpeg_tools | 180 | Heavy |
| filesystem | 15 | System |
| formatter | 60 | Analysis |
| git | 15 | System |
| ingest | 60 | Analysis |
| knowledge | **3600** | Heavy |
| llama_backend | **3600** | Analysis |
| local_llm_proxy | 60 | Analysis |
| mcp_bridge | 60 | Analysis |
| memory | 60 | Analysis |
| monitor | 15 | System |
| n8n_bridge | **3600** | Heavy |
| night_reasoner | 60 | Analysis |
| pdf | 180 | Heavy |
| sandbox | 60 | Analysis |
| scheduler | 60 | Analysis |
| scrapling | 60 | Analysis |
| screenshot_tools | 60 | Analysis |
| search | 60 | Analysis |
| seed_tools | 60 | Analysis |
| sequential_thinking | 60 | Analysis |
| system_metrics | 60 | Analysis |
| vision | **3600** | Heavy |
| websearch | 60 | Analysis |

(En negrita las 6 guilds de inferencia CPU: mantienen 3600 s plano pese a su categoría, que es exactamente el contrato que protegen los otros dos tests del módulo.)

## Verificación de criterios

| criterio | resultado |
|---|---|
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** (comprobado tras la ÚLTIMA edición, incluidos los doc comments — lección del turno 1483: en T-DSH-15 mi clippy no cubrió el comentario final y el del TL cazó `doc_lazy_continuation`) |
| `cargo test -p tylluan-kernel --lib` | **1151 passed / 0 failed / 4 ignored** (1150 + 1 test nuevo) |
| contenido de la lista | idéntico (6 guilds, mismo orden) |
| timeouts | idénticos (tabla anterior) |
| archivos tocados | solo `guild_process.rs` y `main.rs`; el test vive en el módulo de tests de `guild_process.rs` |

## Impact

- **Diff sin commitear: 2 ficheros de `src`** (`guild_process.rs`: const de módulo + accessor − const asociada, y el test nuevo en su módulo; `main.rs`: 1 línea reemplazada + comentario). Ningún otro archivo, ninguna dependencia nueva.
- **Cero cambios de comportamiento**: la lista y todos los timeouts efectivos son idénticos (tabla); el único cambio es de estructura y la existencia del test guardián.
- Cero escrituras en producción; sin secretos.

## No verificado

- **Que no exista otra copia de la lista fuera de los dos ficheros**: el test guardián solo vigila `main.rs` (el `include_str!` del binario) y el catálogo; no escanea todo el repo por literales equivalentes en tests o docs. En `src/` el grep de `cpu_inference_guilds|UNLIMITED_PATIENCE_GUILDS` no dejó copias.
- **Efecto en el arranque real del kernel**: el cambio es de tipos (una línea de `main.rs`), pero no se arrancó un kernel (regla de flota); verificado por compilación y tests.
- **Comportamiento con `guild_timeouts` sin configurar**: la rama de fallback (devuelve el plano) es la primera y no cambió; la tabla se midió con `guild_timeouts` configurado, que es el caso que distingue categorías.
