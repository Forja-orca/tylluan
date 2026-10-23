# T-DSH-50 — Por qué el repo-map escanea 31 GB y tarda ~16 min antes de abrir el puerto

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `06bfcc1` (T-DSH-49 ya integrada en push 124).
- **Encargo (T1601):** (1) leer `repo_map.rs` y citar las reglas reales de exclusión; (2) reproducir el recorrido fuera del kernel con las MISMAS reglas y entregar la tabla «directorio de primer nivel → ficheros → líneas → % del total»; (3) proponer, SIN implementar, la exclusión mínima que baje el recorrido a segundos y decir cuántos ficheros/líneas quedan.
- **Entregable:** `scripts/measure_repo_map_scan.py` (nuevo, solo lectura; sin commitear) + este informe. No se modificó `repo_map.rs`, ni guilds, ni config; no se lanzó nada contra el kernel, LM Studio, Strata ni FreeLLMAPI.

## 1. Las reglas reales de `repo_map.rs` (`crates/tylluan-kernel/src/repo_map.rs`)

- **Exclusiones de directorio:** `const SKIP_DIRS` (línea 43) = `.git`, `node_modules`, `target`, `__pycache__`, `.venv`, `.mypy_cache`, `.pytest_cache`, `.ruff_cache`, `.hypothesis`. Se aplican con `WalkDir::filter_entry` comparando **el nombre exacto** del directorio (líneas 72-77): omite el subárbol en cualquier profundidad, pero **no hay comodines** (un `target-codex-baseline` o `test-run-p1` no coincide).
- **No hay filtro por extensión para contar:** todo fichero que sobrevive se cuenta en `total_files` y se lee entero (línea 132 `count_lines`). La extensión solo se usa para clasificar lenguaje (líneas 112-130).
- **Sin tope de tamaño ni detección de binarios:** `count_lines` (líneas 186-191) hace `std::fs::read_to_string(path)` y, si el fichero no es UTF-8 válido, devuelve 0 líneas — pero **YA ha leído todos los bytes**. Un `.onnx` de 2 GB, un `.safetensors` de 3 GB o un `.log` de 1,3 GB se leen completos para acabar contando 0 (o contando líneas, en el log).
- **`.rs` se lee dos veces:** `count_lines` (línea 132) + `extract_rust_identifiers` (línea 140) releen el fichero.
- **Cuándo se ejecuta:** `RepoMap::build` se llama en el armado del estado HTTP (`crates/tylluan-kernel/src/transport/http/mod.rs` líneas 460-466, `spawn_blocking`) y el log sale en las líneas 583-584. El `TcpListener::bind` está en otra función (línea 317) que recibe el estado ya construido: **el puerto se abre después del recorrido** (confirmado por el orden del código, no solo por el log).

**Prueba real del arranque** (`E:\tylluan\logs\kernel.log`, tres arranques):

```
2026-10-22T23:04:43  Repo map built: 24880 files, 3404 dirs, 17898448 lines (979059ms)
2026-10-22T23:53:47  Repo map built: 24878 files, 3404 dirs, 17900941 lines (945990ms)
2026-10-23T01:13:40  Repo map built: 24884 files, 3404 dirs, 17904534 lines (1531144ms)
```

## 2. Reproducción fuera del kernel (mismas reglas)

`scripts/measure_repo_map_scan.py --root E:/tylluan` replica exactamente: `SKIP_DIRS` por nombre de directorio con semántica `filter_entry`, `total_files` sin filtrar extensión, y `count_lines` = `read_to_string` + `str::lines().count()` (no-UTF8 ⇒ 0 líneas).

**Medición de hoy** (el repo ha crecido desde el arranque: hay `target-codex-baseline`, `test-run-*` y logs nuevos):

- **32 924 ficheros**, **4 318 directorios**, **31 128 942 752 bytes (31,1 GB)**, **19 576 531 líneas** (6 865 ficheros no-UTF8).
- Tiempo: 1,2 s de recorrido + **719 s de lectura/recuento = 720 s** (≈ 43 MB/s de lectura efectiva en Python). El kernel tarda 946-1531 s por el mismo motivo: **leer 31 GB de bytes**, no por `walkdir`.

### Tabla por directorio de primer nivel (medida exacta)

`%líneas` sobre 19 576 531; `%bytes` sobre 31,1 GB.

| directorio | ficheros | bytes | líneas | %líneas | %bytes | no-UTF8 |
|---|---:|---:|---:|---:|---:|---:|
| target-codex-baseline | 8 169 | 9 682 139 923 | 59 416 | 0,30% | 31,11% | 4 018 |
| models | 14 | 5 037 856 159 | 1 099 369 | 5,62% | 16,18% | 7 |
| test-run-p1 | 94 | 3 939 554 054 | 1 319 897 | 6,74% | 12,66% | 43 |
| test-run-md3 | 70 | 3 789 671 820 | 1 185 947 | 6,06% | 12,17% | 23 |
| .fastembed_cache | 37 | 3 775 610 146 | 1 131 168 | 5,78% | 12,13% | 4 |
| crates | 555 | 2 407 793 293 | 1 181 423 | 6,03% | 7,73% | 166 |
| logs | 4 | 1 356 329 263 | **8 158 077** | **41,67%** | 4,36% | 0 |
| data | 115 | 456 542 416 | 53 623 | 0,27% | 1,47% | 91 |
| guilds-python | 22 249 | 392 746 742 | **4 764 046** | **24,34%** | 1,26% | 2 354 |
| docs-site | 391 | 116 863 960 | 206 080 | 1,05% | 0,38% | 67 |
| benchmarks | 188 | 110 153 999 | 204 369 | 1,04% | 0,35% | 3 |
| target-test | 1 | 33 626 112 | 0 | 0,00% | 0,11% | 1 |
| .freebuff | 6 | 8 210 697 | 42 | 0,00% | 0,03% | 1 |
| dashboard | 200 | 5 766 476 | 52 141 | 0,27% | 0,02% | 13 |
| desktop | 70 | 4 901 618 | 20 209 | 0,10% | 0,02% | 50 |
| docs | 287 | 4 846 966 | 67 750 | 0,35% | 0,02% | 1 |
| assets | 10 | 1 767 859 | 0 | 0,00% | 0,01% | 10 |
| <raíz> (72 ficheros) | 72 | 1 265 056 | 20 646 | 0,11% | 0,00% | 0 |
| test-run-v27 | 22 | 1 223 123 | 3 464 | 0,02% | 0,00% | 13 |
| guilds | 89 | 679 360 | 17 631 | 0,09% | 0,00% | 0 |
| packages / scripts / tests / resto | 436 | 2 318 900 | 53 616 | 0,27% | 0,01% | 0 |

**Los excluidos por `SKIP_DIRS` ni siquiera aparecen** (no se recorren): `.git` (22 809 ficheros), `target` (90 949), `.venv` (18 596), `node_modules` (18). Correcto: no son el problema.

### Dónde está el coste

- **96,3% de los bytes** está en 7 directorios que NO están en `SKIP_DIRS`: `target-codex-baseline` (9,7 GB), `models` (5,0 GB), `test-run-p1` (3,9 GB), `test-run-md3` (3,8 GB), `.fastembed_cache` (3,8 GB), `crates` (2,4 GB, casi todo `crates/tylluan-kernel/.fastembed_cache`), `logs` (1,4 GB). Total ≈ 30,0 GB de 31,1 GB.
- **96,2% de las líneas** sale de: `logs` (8,16 M líneas, casi todas de `logs/kernel.log.1`, 1,3 GB), `guilds-python` (4,76 M líneas: 22 249 ficheros de runtimes Python vendorizados en `linux-x86_64/` y `windows-x86_64/`), y los `test-run-*`/`.fastembed_cache`/`models`/`crates` (≈ 5,9 M líneas, en buena parte `tokenizer.json` de 17 MB pretty-printed).
- **6 865 ficheros no-UTF8** (binarios: `.safetensors`, `.onnx`, `.rlib`, `.db`) se leen enteros y se descartan al fallar el decode: son ~28 GB de lectura tirada.
- `crates` parece pequeño en ficheros (555) pero pesa 2,4 GB: dentro hay un `.fastembed_cache` anidado (2,27 GB) que la lista no excluye.

## 3. Propuesta de exclusión mínima (SIN implementar)

### Opción A — solo ampliar `SKIP_DIRS` (cambio mínimo, ~12 nombres)

Añadir a `const SKIP_DIRS` (`repo_map.rs` línea 43): `target-codex-baseline`, `target-test`, `test-run-p1`, `test-run-md3`, `test-run-v27`, `.fastembed_cache`, `logs`, `data`, `models`, `docs-site`, `guilds-python`, `scratch`.

**Medido con el script** (`--skip-extra` con esos 12): **1 677 ficheros, 241 directorios, 158 493 249 bytes, 561 918 líneas, 2,3 s** (0,1 s recorrido + 2,2 s lectura). Retiene el **5,1% de los ficheros y el 2,9% de las líneas**, y el repo-map conserva lo que de verdad describe el código: `crates`, `dashboard`, `desktop`, `guilds`, `docs`, `benchmarks`, `scripts`, `tests`, `packages`, `tools`… (perdería solo artefactos de build/runtime y datos).

### Opción B — recomendada (A + guarda por fichero, ~15 líneas)

Además de A, en el bucle de ficheros (antes de `count_lines`, línea 132):
1. no leer si `size > 2 MiB` (o un umbral configurable): esos ficheros no aportan identificadores y casi siempre son binarios;
2. no leer si la extensión no es de texto (lista blanca de las que ya clasifica: `rs,py,ts,tsx,js,jsx,md,json,toml,yaml,yml,html,css,sql,sh,bat,ps1,proto,vue,svelte`), y contar 0 líneas sin abrir;
3. opcional: si se quiere ser futuro-proof, sustituir la comparación exacta por `name.starts_with("target")` para cubrir cualquier `target-*` nuevo.

Con A ya se baja a segundos; B evita que un artefacto nuevo fuera de la lista vuelva a costar minutos (p. ej. un `.onnx` suelto en `assets/`). Efecto esperado: mismo `total_files` de A, menos lecturas inútiles (en la simulación A quedan 231 ficheros no-UTF8 pequeños, `assets/` y similares; B los salta).

**Nota sobre el repo-map resultante:** con A/B `total_files`/`total_lines`/`top_level_dirs` dejan de incluir artefactos (hoy dominan el 96%). Los `languages` se vuelven representativos del código real (Rust/Python/TS/Markdown) en vez de basura de build; `identifiers` solo se extrae de `.rs`, así que no cambia. No he medido el `RepoMap` resultante con exclusiones (sería el siguiente paso si el TL lo quiere).

## 4. Verificación

| comprobación | resultado |
|---|---|
| reproducción exacta de reglas | `scripts/measure_repo_map_scan.py`, MISMO `SKIP_DIRS`, `total_files` sin filtro, `read_to_string`+`lines().count()`, no-UTF8 ⇒ 0 |
| números del kernel (log real) | 24 878 ficheros / 3 404 dirs / 17 900 941 líneas / 945 990 ms (y hasta 1 531 144 ms) |
| números de hoy (repo crecido) | 32 924 ficheros / 4 318 dirs / 31,13 GB / 19 576 531 líneas / 720 s |
| simulación exclusión mínima | 1 677 ficheros / 561 918 líneas / 2,3 s |
| gates de documentación | `check_doc_links.py` exit 0; `check_docs_reality.sh` solo el artefacto preexistente `ROADMAP_O3.md` (línea 42, WSL); `check_test_count.sh` ✅ (sin tests nuevos: el entregable es script) |
| sin `cfg(unix)`/`cfg(windows)` | n/a (Python), no aplica |
| diff | 2 ficheros nuevos sin commitear: `scripts/measure_repo_map_scan.py` + este informe. Sin tocar `repo_map.rs` ni `src/` |

## Impact

- **Solo lectura**: no se modificó `repo_map.rs` ni nada del kernel; el script no escribe y acepta `--root`, sin rutas de máquina hardcodeadas. Deep puede usar la tabla para D-39 tal cual.
- **Causa raíz medida, no supuesta**: el arranque lento no es `walkdir` ni el conteo de líneas en sí, es **leer 31 GB** (96% artefactos de build/runtime: 30 GB en 7 directorios fuera de `SKIP_DIRS`) + leer binarios enteros para descartarlos + doble lectura de cada `.rs`.
- **Ahorro propuesto enorme y barato**: añadir 12 nombres a `SKIP_DIRS` baja de 946-1531 s a ~2 s (medido) reteniendo 1 677 ficheros / 561 918 líneas; una guarda de tamaño/extensión evita que el problema vuelva con artefactos nuevos.

## No verificado

- **No he modificado `repo_map.rs`** (el encargo era proponer, no implementar): los 2,3 s son de mi reproducción Python con las exclusiones simuladas, no del kernel real con el parche. La cifra exacta del kernel habría que medirla tras implementar.
- **Números absolutos distintos de los del log**: mi escaneo es de hoy y el repo creció (`target-codex-baseline`, `test-run-*`, `logs/kernel.log.1`); el log es de los arranques del 22-23 oct. La composición (dominio de artefactos) es la misma; no puedo reconstruir el árbol exacto de aquel instante.
- **Python no es Rust**: el tiempo de lectura/decodificación no es extrapolable 1:1; sirve para orden de magnitud y para atribuir bytes/líneas, que es el objetivo.
- **Contenido del `RepoMap` tras excluir**: no medí cuántos lenguajes/top_level_dirs quedarían ni si algún consumidor del mapa espera `data/` o `benchmarks/silva_reference_anon.db` (este último sí lo propongo conservar: `benchmarks` no está en la lista A). Revisar antes de implementar.
- **Causa del porqué de cada directorio**: `test-run-*` y `target-codex-baseline` parecen artefactos de sesiones anteriores, y `guilds-python` runtimes vendorizados; no verifiqué con el TL si alguno debe conservarse a propósito (p. ej. para onboarding).
- **Script**: es un reproductor de medida, no un test de CI; no lo he añadido a `scripts/verify.sh` (no lo pedía el encargo).

## Nota del TL (2026-10-24)
Verificado por el TL contra `logs/kernel.log` (tres arranques: 979 s, 946 s y 1531 s), `repo_map.rs` (SKIP_DIRS en la linea 43; `count_lines` lee el fichero entero) y los bytes de `models` (14 ficheros, 5 037 856 159 B), `.fastembed_cache` (37, 3 775 610 146 B) y `test-run-p1` (94, 3 939 554 054 B): coinciden al byte. No se repitio la medicion completa de 12 min. **El TL aplico el arreglo minimo (D-39a)** en `repo_map.rs`: omite en la raiz `models`, `logs`, `data` y los prefijos `test-run-`, `target-`, `boot-`; omite `.fastembed_cache` en cualquier nivel; y no lee para contar lineas ficheros de mas de 2 MB. Queda para Deep (D-39b) construir el mapa en segundo plano y abrir el puerto antes.
