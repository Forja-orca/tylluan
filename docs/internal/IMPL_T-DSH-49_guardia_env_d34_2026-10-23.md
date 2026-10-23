# T-DSH-49 — Guardia: variables de entorno que las guilds leen y D-34 no hereda

- **Agente:** dsh
- **Fecha:** 2026-10-23
- **Base:** `origin/main` `8fb9a26` — **D-34 ya integrado** (`guild_inherited_env` + `guild_may_inherit` en `crates/tylluan-kernel/src/registry/guild_process.rs`, con las 14 variables que añadió el TL y el grant de `WATCHER_LLM_KEY`).
- **Encargo (COLA del TL, `docs/internal/COLA_dsh_2026-10-24.md`, ya en origin):** barrido COMPLETO de `guilds/`, tabla `variable -> fichero:línea -> default -> ¿secreto? -> ¿cubierta?`, test nuevo `tests/guild_env_reads_are_covered.rs` que **lea `guild_process.rs` con regex** y falle si una guild lee una variable fuera de la lista blanca / `guild_may_inherit` / excepciones documentadas, e informe con hallazgos reales.
- **Entregable:** `crates/tylluan-kernel/tests/guild_env_reads_are_covered.rs` + este informe. Sin tocar `src/` ni ningún `.py`.
- **Resultado:** **guardia en verde sobre `origin/main` `8fb9a26`** (0 lecturas no cubiertas; 58 lecturas catalogadas de 23 guilds). Criterio del TL cumplido.

## 1. Cómo funciona la guardia

1. Lee `crates/tylluan-kernel/src/registry/guild_process.rs` y extrae, con regex y parseo de corchetes (saltando el tipo `&[&str]`), el `const EXACT` y `const PREFIXES` de `guild_inherited_env`, y las armas `guild => condición` de `guild_may_inherit` (sufijos `ends_with("…")` y nombres `== "…"`). **No duplica la lista blanca**: si D-34 cambia, el test cambia con él.
2. Barre `guilds/**/*.py` COMPLETO (`builders/`, `scholars/`, `wardens/`, `watchers/`, `core/`, `plugins/` y subcarpetas) y clasifica cada lectura:
   - `os.environ["X"]`, `os.environ.get("X")`, `os.getenv("X")`, `"X" in os.environ` → lectura literal;
   - clave dinámica (`os.getenv(api_key_env)`): intenta resolverla contra constantes literales del mismo fichero (`NAME = "X"`); si no se resuelve, solo se acepta en guilds con comodín por sufijo en `guild_may_inherit` (hoy `llama_backend` con `*_API_KEY`/`*_API_TOKEN`), porque ese comodín es el contrato explícito;
   - f-string (`os.getenv(f"...")`) → misma regla;
   - `os.environ.setdefault(...)`, `os.environ.copy()`, `{**os.environ}`, `dict(os.environ)` e iteraciones bare **no** son lecturas del kernel (setdefault escribe en el entorno local del guild; copy/forward reenvía el entorno YA filtrado por D-34): se catalogan y se marcan cubiertas.
3. Excepciones documentadas dentro del test (con justificación): `kernel_auth` puede leer `TYLLUAN_TOKEN` (fallback de arranque manual del diseño D-34; el maestro no se inyecta y con `TYLLUAN_GUILD_TOKEN` no hay fallback). `TYLLUAN_GUILD_TOKEN` está permitida (la inyecta D-34).
4. Falla listando las lecturas no cubiertas. Si `guild_inherited_env` no existe todavía en el árbol, declara «D-34 no integrado» y pasa sin evaluar (criterio del TL: verde una vez empujado D-34).

## 2. Tabla completa de lecturas (`guilds/` completo, 58 filas)

Calculada contra el contrato **real** de `origin/main` `8fb9a26`: 75 nombres exactos, 6 prefijos y 5 guilds con grant. Columnas: variable, fichero:línea, default efectivo, ¿nombre de secreto?, ¿cubierta por D-34?

| variable | fichero:línea | default | ¿secreto? | ¿cubierta? |
|---|---|---|---|---|
| `COMFYUI_URL` | `guilds/builders/plugins/audio_tools.py:13    or "` | `http://localhost (puerto 8188)"` | ).rstrip | ("/" no       sí |
| `TYLLUAN_ROOT` | `guilds/builders/plugins/bash.py:240          os.g` | `etcwd(         no` | sí | - |
| `FFMPEG_PATH` | `guilds/builders/plugins/ffmpeg_tools.py:13   or "` | `ffmpeg"        no` | sí | - |
| `FFPROBE_PATH` | `guilds/builders/plugins/ffmpeg_tools.py:14   or "` | `ffprobe"       no` | sí | - |
| `TYLLUAN_ROOT` | `guilds/builders/plugins/filesystem.py:23     os.g` | `etcwd(         no` | sí | - |
| `LOCAL_LLM_URL` | `guilds/builders/plugins/local_llm_proxy.py:12 or` | `"http://localhost (puerto 1143)` | 4").rstr | ip("/" no       sí |
| `LOCAL_LLM_PROVIDER` | `guilds/builders/plugins/local_llm_proxy.py:13 or` | `"ollama"        no` | sí | - |
| `SCREENSHOTS_DIR` | `guilds/builders/plugins/screenshot_tools.py:12 or` | `"data/screenshots" no` | s | í |
| `KERNEL_BASE` | `guilds/core/benchmark_routing.py:23          -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/benchmark_routing.py:24          -` | `no` | sí | - |
| `TYLLUAN_AGENT_ID` | `guilds/core/check_coloquio.py:47             "").` | `strip(         no` | sí | - |
| `KERNEL_BASE` | `guilds/core/check_coloquio.py:86             -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/check_coloquio.py:87             -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/coloquio.py:22                   -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/coloquio.py:23                   -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/coloquio_digest.py:39            -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/coloquio_digest.py:40            -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/coloquio_watcher.py:35           "htt` | `p://127.0.0.1 (puerto 47004)" n` | o | sí |
| `TYLLUAN_INBOX` | `guilds/core/coloquio_watcher.py:37           str(` | `Path.home() / ".tyllua` | n" / "in | box" no       sí |
| `TYLLUAN_PENDING` | `guilds/core/coloquio_watcher.py:99           str(` | `Path.home() / ".tyllua` | n" / "pe | nding_actions" no       sí |
| `COMFY_BASE_URL` | `guilds/core/comfy_ui.py:34                   "htt` | `p://127.0.0.1 (puerto 8188)").r` | strip("/ | " no       sí |
| `KERNEL_BASE` | `guilds/core/coordinator.py:37                -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/coordinator.py:38                -` | `no` | sí | - |
| `TYLLUAN_GUILD_TOKEN` | `guilds/core/kernel_auth.py:41                "").` | `strip(         SÍ` | sí | - |
| `TYLLUAN_TOKEN` | `guilds/core/kernel_auth.py:44                "").` | `strip(         SÍ` | sí | - |
| `<dinamico: api_key_env>` | `guilds/core/llama_backend.py:162             "")` | `if api_key_env else ""` | SÍ | sí |
| `<dinamico: env_var>` | `guilds/core/llama_backend.py:199             -` | `no` | sí | - |
| `PATH` | `guilds/core/mcp_bridge.py:69                 ""),` | `no` | sí | - |
| `HOME` | `guilds/core/mcp_bridge.py:70                 ""),` | `no` | sí | - |
| `USERPROFILE` | `guilds/core/mcp_bridge.py:71                 ""),` | `no` | sí | - |
| `APPDATA` | `guilds/core/mcp_bridge.py:72                 ""),` | `no` | sí | - |
| `TERM` | `guilds/core/mcp_bridge.py:73                 "xte` | `rm-256color"), no` | sí | - |
| `N8N_BASE_URL` | `guilds/core/n8n_bridge.py:28                 "htt` | `p://localhost (puerto 5678)").r` | strip("/ | " no       sí |
| `N8N_API_KEY` | `guilds/core/n8n_bridge.py:29                 ""` | `SÍ` | sí | - |
| `N8N_WEBHOOK_BASE` | `guilds/core/n8n_bridge.py:30                 _BAS` | `E              no` | sí | - |
| `KERNEL_BASE` | `guilds/core/night_reasoner.py:346            -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/night_reasoner.py:347            -` | `no` | sí | - |
| `SCHEDULER_DB` | `guilds/core/scheduler.py:37                  -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/scheduler.py:42                  -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/scheduler.py:43                  -` | `no` | sí | - |
| `SCHEDULER_POLL_SECS` | `guilds/core/scheduler.py:54                  "30"` | `no` | sí | - |
| `DEEPEVAL_TIMEOUT_SECS` | `guilds/core/scheduler.py:184                 "900` | `")),           no` | sí | - |
| `KERNEL_BASE` | `guilds/core/seed_tools.py:30                 -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/seed_tools.py:31                 -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/silva_utils.py:38                -` | `no` | sí | - |
| `KERNEL_BASE` | `guilds/core/silva_utils.py:39                -` | `no` | sí | - |
| `TYLLUAN_KERNEL_URL` | `guilds/scholars/plugins/memory_bridge.py:25  -` | `no` | sí | - |
| `TYLLUAN_KERNEL_URL` | `guilds/scholars/plugins/memory_bridge.py:26  -` | `no` | sí | - |
| `GEMINI_API_KEY` | `guilds/watchers/antigravity_watcher.py:29    -` | `SÍ` | sí | - |
| `ANTHROPIC_API_KEY` | `guilds/watchers/claude_watcher.py:29         -` | `SÍ` | sí | - |
| `FORJA_URL` | `guilds/watchers/forja_watcher.py:17          "htt` | `p://127.0.0.1 (puerto 3033)" no` | s | í |
| `LM_STUDIO_URL` | `guilds/watchers/lmstudio_watcher.py:19       "htt` | `p://127.0.0.1 (puerto 1234)" no` | s | í |
| `LM_STUDIO_MODEL` | `guilds/watchers/lmstudio_watcher.py:20       "goo` | `gle/gemma-4-12b" no` | sí | - |
| `WATCHER_LLM_URL` | `guilds/watchers/openai_compat_watcher.py:47  "htt` | `p://127.0.0.1 (puerto 1234)" no` | s | í |
| `WATCHER_LLM_MODEL` | `guilds/watchers/openai_compat_watcher.py:48  "goo` | `gle/gemma-4-12b" no` | sí | - |
| `WATCHER_LLM_KEY` | `guilds/watchers/openai_compat_watcher.py:49  "lm-` | `studio"        SÍ` | sí | - |
| `WATCHER_AGENT_ID` | `guilds/watchers/openai_compat_watcher.py:100 "ass` | `istant"        no` | sí | - |
| `CRON_DIR` | `guilds/watchers/plugins/cron_scheduler.py:14 or "` | `data/cron"     no` | sí | - |

Todas cubiertas. Las fuentes de cobertura son: allowlist directa (`KERNEL_BASE`, `TYLLUAN_*`, `N8N_BASE_URL`…), grants por guild (`N8N_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `WATCHER_LLM_KEY`), dinámicas de `llama_backend` (comodín por sufijo), `TYLLUAN_GUILD_TOKEN` y la excepción documentada de `kernel_auth`.

## 3. Hallazgo real: lo que la guardia cazó ANTES de las 14 del TL

Con el `D-34` original de Deep (61 exactas, sin las 14 que añadió el TL) la guardia fallaba con **15 lecturas sin cubrir**. Ninguna revienta: todas tienen default, así que con `env_clear()` **el override del operador dejaba de aplicarse en silencio**:

| # | variable | fichero:línea | default que se imponía | ¿secreto? | efecto si el usuario la fijaba |
|---|---|---|---|---|---|
| 1 | `COMFYUI_URL` | `guilds/builders/plugins/audio_tools.py:13` | `http://localhost` (puerto 8188) | no | ComfyUI de otro host/puerto ignorado |
| 2 | `FFMPEG_PATH` | `guilds/builders/plugins/ffmpeg_tools.py:13` | `ffmpeg` (PATH) | no | binario ffmpeg custom ignorado |
| 3 | `FFPROBE_PATH` | `guilds/builders/plugins/ffmpeg_tools.py:14` | `ffprobe` (PATH) | no | ídem ffprobe |
| 4 | `LOCAL_LLM_URL` | `guilds/builders/plugins/local_llm_proxy.py:12` | `http://localhost` (puerto 11434) | no | proxy LLM local ignorado |
| 5 | `LOCAL_LLM_PROVIDER` | `guilds/builders/plugins/local_llm_proxy.py:13` | `ollama` | no | proveedor forzado a ollama |
| 6 | `SCREENSHOTS_DIR` | `guilds/builders/plugins/screenshot_tools.py:12` | `data/screenshots` | no | capturas en el directorio por defecto |
| 7 | `DEEPEVAL_TIMEOUT_SECS` | `guilds/core/scheduler.py:184` | `900` | no | timeout de deepeval no configurable |
| 8 | `WATCHER_LLM_URL` | `guilds/watchers/openai_compat_watcher.py:47` | `http://127.0.0.1` (puerto 1234) | no | watcher apunta al default |
| 9 | `WATCHER_LLM_MODEL` | `guilds/watchers/openai_compat_watcher.py:48` | `google/gemma-4-12b` | no | modelo ignorado |
| 10 | `WATCHER_LLM_KEY` | `guilds/watchers/openai_compat_watcher.py:49` | `lm-studio` | **SÍ** | **clave real sustituida por el literal `lm-studio`** |
| 11 | `WATCHER_AGENT_ID` | `guilds/watchers/openai_compat_watcher.py:100` | `assistant` | no | identidad del watcher ignorada |
| 12 | `FORJA_URL` | `guilds/watchers/forja_watcher.py:17` | `http://127.0.0.1` (puerto 3033) | no | watcher de Forja apunta al default |
| 13 | `LM_STUDIO_URL` | `guilds/watchers/lmstudio_watcher.py:19` | `http://127.0.0.1` (puerto 1234) | no | ídem LM Studio |
| 14 | `LM_STUDIO_MODEL` | `guilds/watchers/lmstudio_watcher.py:20` | `google/gemma-4-12b` | no | modelo ignorado |
| 15 | `CRON_DIR` | `guilds/watchers/plugins/cron_scheduler.py:14` | `data/cron` | no | cron escribe/lee en el directorio por defecto |

**Estado actual:** en `origin/main` `8fb9a26` el TL añadió las 14 de configuración a `guild_inherited_env` y dejó `WATCHER_LLM_KEY` donde corresponde: **grant acotado** `"openai_compat_watcher" => upper == "WATCHER_LLM_KEY"` (no en la lista global, así las demás guilds no ven esa credencial). Resultado: **0 no cubiertas**, la guardia queda verde y el riesgo de «secreto a todas las guilds» no existe.

## 4. Verificación

| comprobación | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test guild_env_reads_are_covered` sobre `origin/main` **`8fb9a26` (D-34 real)** | **ok** (1 passed, 2.09 s); contrato extraído del fuente: 75 exactas, 6 prefijos, 5 grants; 0 no cubiertas |
| mismo test con el **D-34 de Deep sin las 14** (`git apply D-34.patch` sobre `c3a1066`) | **FAILED con 15 no cubiertas** (log `guard_patch_final.log`): la guardia caza el hueco que luego rellenó el TL |
| mismo test con el contrato **simulado** (14 exactas + grant), sobre el parche | **ok** (contrato 75 exactas, 5 grants) |
| sin D-34 en el árbol (`c3a1066`) | ok declarando «D-34 no integrado» (se activa solo cuando llega el push, criterio del TL) |
| `cargo clippy -p tylluan-kernel --test guild_env_reads_are_covered -j 3 -- -D warnings` | exit 0 |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` sobre `8fb9a26` | exit 0 (log `clippy_alltargets_8fb9a26.log`) |
| `bash scripts/check_test_count.sh` | ✅ (conteo de AGENTS/README tras el push de D-34) |
| `python scripts/check_doc_links.py` | exit 0 |
| `bash scripts/check_docs_reality.sh` | solo el artefacto preexistente `ROADMAP_O3.md` (línea 42, WSL) |
| `cfg(unix)` / `cfg(windows)` en funciones enteras | no hay (el test solo lee ficheros) |
| diff | 2 ficheros nuevos sin commitear: test + informe. Sin `src/`, sin guilds, sin kernel |

## Impact

- **Ningún cambio de comportamiento**: no toca `src/`, ni guilds, ni configuración; solo un test de integración que lee `.py` y el fuente Rust.
- **Guardia permanente y auto-actualizada**: la lista blanca se extrae del código real de D-34 en cada ejecución; si mañana alguien añade `os.getenv("NUEVO_SECRETO")` en cualquier guild, el CI falla con fichero:línea y variable. Si D-34 amplía/reduce la lista, la guardia se adapta sin editar el test.
- **Evidencia del valor**: con el D-34 de Deep, 15 overrides legítimos (incluida una credencial, `WATCHER_LLM_KEY`) se habrían perdido en silencio; la guardia los señaló antes de integrarse y el TL los cerró con 14 entradas + un grant acotado.

## No verificado

- **Comportamiento en vivo de las guilds bajo `env_clear()`**: no ejecuté ninguna guild; «se usa el default» es lectura de los `or`/defaults citados. (El TL sí verificó en vivo la guilda `bash`: sin `TYLLUAN_TOKEN`, con `TYLLUAN_GUILD_TOKEN` y sin claves de API — T1599; no es mi medición.)
- **Parser propio, no `ast`**: cubre las formas usadas hoy en `guilds/`; `getattr(os, "environ")`, `importlib`, o un subproceso con `env=` custom no se modelan.
- **Dinámicas de `llama_backend`**: se aceptan por el comodín de sufijo del contrato; no se puede verificar estáticamente que el nombre real acabe en `_API_KEY`/`_API_TOKEN` (el contrato lo autoriza, la guardia no lo demuestra).
- **`WATCHER_LLM_KEY`**: la guardia acepta el grant sin juzgar si esa credencial debería existir; es decisión de D-34. Verificado que está como grant y no en la lista global.
- **Guilds no registradas**: se escanean todos los `.py` de `guilds/`, estén o no en `LAZY_GUILDS`/`always_on`; también quedarían cubiertas.

## Nota del TL (2026-10-24)
Verificado por el TL sobre `8fb9a26`: `cargo test -p tylluan-kernel --test guild_env_reads_are_covered` verde (1 passed), clippy `-D warnings` exit 0, y la unica aparicion de `cfg(unix)`/`cfg(windows)` es un comentario. Defecto menor del informe: las columnas de la tabla de la seccion 2 salieron desalineadas (el parseo de las filas se mezclo con los `or "..."` del codigo fuente), asi que la tabla no es fiable para leerla; el contrato real vive en el test. Pendiente de una pasada futura: regenerar la tabla desde el propio test (`--nocapture`).
