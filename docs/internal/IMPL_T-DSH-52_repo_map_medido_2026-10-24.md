# T-DSH-52 — `RepoMap::build` medido sobre el repo real con D-39a

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `243882d` (push 126: D-39a + T-DSH-50 integrados).
- **Encargo (T1606):** medir de verdad la proyección de T-DSH-51 — test `#[ignore = "mide el repo real"]` que llame a `RepoMap::build` sobre la raíz real, imprima con `--nocapture` segundos/ficheros/directorios/líneas, compare con los 3 arranques del log y —si sigue >10 s— tabla de directorios que aún pesan. Sin arrancar kernel ni tocar puertos; solo lectura de disco; ejecutar UNA vez y anotar la carga de la máquina.
- **Entregable:** `crates/tylluan-kernel/tests/repo_map_real_measure.rs` + este informe. Sin tocar `src/`, guilds ni config.

## 1. Test

`crates/tylluan-kernel/tests/repo_map_real_measure.rs`:

- `#[ignore = "mide el repo real: recorrido de disco completo (no arranca kernel ni toca puertos)"]`; CI no lo ejecuta.
- `RepoMap::build` ya es **público** (`pub mod repo_map` en `lib.rs:34`, `pub fn build` en `repo_map.rs:78`) y los campos de `RepoMap` son públicos: **no hizo falta cambiar visibilidad**.
- Raíz: `TYLLUAN_REPO_ROOT` si está; si no, sube desde `CARGO_MANIFEST_DIR` hasta el primer ancestro con `.git` (en un worktree mediría el worktree, que NO tiene los artefactos del repo del operador — por eso se pasa explícita).
- Imprime: raíz, segundos, ficheros, directorios, líneas, `build_duration_ms`, comparación con los 3 arranques del log (factor), top-15 directorios del mapa y top-10 lenguajes; aviso si >10 s.

Comando exacto (una vez):

```
TYLLUAN_REPO_ROOT=E:\tylluan rustup run 1.99.1 cargo test \
  -p tylluan-kernel --test repo_map_real_measure -- --ignored --nocapture
```

## 2. Resultado medido

```
[medición] raíz: E:\tylluan
[medición] RepoMap::build: 21.777 s | 16348 ficheros | 2055 directorios | 3887269 líneas | build_duration_ms=21777
```

**Carga de la máquina: ~99-100% de CPU antes y después** (hay agentes usando CPU/motores, como avisó el TL). Es una medición en carga: el tiempo real en una máquina ociosa sería menor, pero no puedo dar esa cifra sin repetir en reposo (no lo hice: se pidió una vez).

| métrica | log pre-D-39a (3 arranques) | medido ahora | delta |
|---|---|---|---|
| segundos | 979,059 / 945,990 / 1 531,144 | **21,777** | **45× / 43× / 70× más rápido** |
| ficheros | 24 880 / 24 878 / 24 884 | **16 348** | −34% |
| líneas | 17 898 448 / 17 900 941 / 17 904 534 | **3 887 269** | **−78%** |
| directorios | 3 404 | 2 055 | −40% |

Verificación cruzada: una reproducción independiente de las reglas D-39a en Python (mismos `SKIP_DIRS`+`SKIP_TOP_LEVEL_DIRS`+prefijos+cota 2 MiB, sin seguir junctions) da exactamente **16 348 ficheros / 3 887 373 líneas** (104 líneas de diferencia, 0,003%) y 232,1 MB leídos. Confirma que el test mide el árbol que D-39a realmente lee.

## 3. Sigue por encima de 10 s: directorios que aún pesan

Reproducción de reglas D-39a, reparto por bytes de ficheros ≤2 MiB (los que se leen para contar líneas) y estimación de segundos proporcional al total medido (21,777 s):

| directorio | ficheros | leído (≤2 MiB) | líneas | % del leído | ~s (est.) | qué es |
|---|---:|---:|---:|---:|---:|---|
| **guilds-python** | 14 220 | 148 547 345 | 3 121 084 | **64,0%** | **~13,9** | runtimes Python vendorizados: `windows-x86_64` (7 006 f.) + `linux-x86_64` (7 214 f.), no es código de Tylluan |
| **docs-site** | 391 | 35 720 236 | 206 074 | **15,4%** | **~3,4** | casi todo `docs-site/.next` (361 ficheros, salida de build de Next) |
| crates | 529 | 20 055 019 | 150 390 | 8,6% | ~1,9 | código real (mayoría `crates/tylluan-kernel`) |
| benchmarks | 188 | 7 135 537 | 199 542 | 3,1% | ~0,7 | datasets; el `.db` de 99 MB queda fuera de la lectura por la cota de 2 MiB |
| dashboard | 200 | 5 766 476 | 52 141 | 2,5% | ~0,5 | código real |
| desktop | 70 | 4 901 618 | 20 209 | 2,1% | ~0,5 | código real |
| docs | 290 | 4 879 514 | 68 048 | 2,1% | ~0,5 | docs |
| assets | 10 | 1 767 859 | 0 | 0,8% | ~0,2 | binarios (0 líneas) |
| resto (guilds, packages, scripts, raíz, tests, herramientas…) | 450 | 7 354 511 | 68 902 | 3,2% | ~0,7 | código/config real |

**El 79,4% de la lectura restante está en dos directorios que no son código de Tylluan**: `guilds-python` (runtime vendorizado) y `docs-site/.next` (build de Next). En un checkout limpio esos directorios probablemente ni existan; en el del operador están.

## 4. Propuesta mínima (SIN implementar; hay que medirla)

Para bajar de 10 s en este árbol:

1. Añadir `guilds-python` a `SKIP_TOP_LEVEL_DIRS` (junto a `models`, `logs`, `data`): fuera 148,5 MB / 3,12 M líneas / 64% de la lectura. Estimación lineal: **~7,8 s**.
2. Añadir `docs-site` a `SKIP_TOP_LEVEL_DIRS` (o `.next` a `SKIP_DIRS`, porque es salida de build en cualquier ruta): otros 35,7 MB / 15,4%. Estimación acumulada: **~4,5-5 s**.
3. Opcional: `assets` (1,8 MB, 0 líneas) y `benchmarks` (7,1 MB legibles) son pequeños; no merecen exclusión.

Las dos exclusiones son del mismo tipo que D-39a (`models`/`logs`/`data`) y no tocan código: el mapa seguiría teniendo `crates/tylluan-kernel` (el 8,6% de lectura, 150 390 líneas). **No las he aplicado** (el encargo era medir y proponer). Alternativa más robusta que excluir: construir el repo map **después del bind** (es informativo para `/api/v1/repo-map`; no bloquea ninguna ruta crítica) — eso elimina el problema para cualquier árbol, no solo el del operador.

## 5. Verificación

| comprobación | resultado |
|---|---|
| test compila y mide | `cargo test -p tylluan-kernel --test repo_map_real_measure -- --ignored --nocapture` → ok, 21,777 s |
| coherencia interna | `build_duration_ms=21777` ≈ 21,777 s medidos |
| reproducción independiente de reglas D-39a | 16 348 ficheros / 3 887 373 líneas / 232,1 MB (coincide con el test; 0,003% de líneas de diferencia) |
| junctions | 1 026 reparse points bajo `guilds-python`; `walkdir(follow_links=false)` no los sigue y el test tampoco (la primera reproducción los seguía e inflaba a 24 377 ficheros / 5,5 M líneas; corregido) |
| gates | `check_doc_links.py` exit 0; `check_docs_reality.sh` solo el artefacto preexistente `ROADMAP_O3.md` (línea 42, WSL); `check_test_count.sh` ✅ 1288 (el test es `#[ignore]`, no suma al conteo) |
| `cfg(unix)`/`cfg(windows)` en funciones enteras | no hay |
| diff | 2 ficheros nuevos sin commitear: el test y este informe. Sin tocar `src/`, `repo_map.rs`, guilds ni config |

## Impact

- **Cierra la proyección de T-DSH-51 con una medición real**: D-39a pasa de ~16-26 min a **21,8 s en carga** (43-70×), pero **no cumple <10 s** en el árbol del operador.
- **Señala exactamente qué sobra**: `guilds-python` (runtime vendorizado, 64% de la lectura) y `docs-site/.next` (build de Next, 15%). Dos entradas más en `SKIP_TOP_LEVEL_DIRS` deberían dejar el arranque en ~5 s; la alternativa robusta es construir el mapa después del bind.
- **Cero cambios de código**: test ignorado + informe; el mapa real no se altera.

## No verificado

- **Dos ejecuciones, no una**: la primera invocación perdió la salida porque el directorio de logs no existía (error de redirección de mi shell, no del test) y no capturó nada; repetí una vez para tener evidencia. No es una segunda medición independiente.
- **Medición en carga (~100% CPU por otros agentes)**: 21,777 s es una cota pesimista; en reposo bajaría, pero no lo medí.
- **Estimaciones de la sección 4**: proporcionales a bytes de lectura, no medidas con las exclusiones aplicadas; son orientativas.
- **Semántica del mapa con las exclusiones**: no verifiqué qué consumidores de `RepoMap` (endpoint `/api/v1/repo-map`, routing) necesitan ver `guilds-python` o `docs-site`; excluirlos reduce `total_files`/`languages` a código real, que es lo deseable para un mapa de repo, pero es decisión del TL.
- **Un solo árbol/disco/momento**: los 16 348 ficheros son del checkout de José hoy; en CI (repo limpio) el mapa será mucho menor.
- **Junctions**: confirmé que walkdir no los sigue (los números lo demuestran), pero no auditó qué hay al otro lado de los 1 026 reparse points de `guilds-python`.
- **Duplicados de `Cargo.toml` anidados**: el test no los toca; `extract_rust_identifiers` se lee solo para `.rs`, correcto.

## Verificacion del TL (2026-10-24)
Ejecute el mismo test sobre la raiz real con D-39a (21,777 s segun DSH) y tras aplicar D-39a2 (`guilds-python` en `SKIP_TOP_LEVEL_DIRS`, `.next` en `SKIP_DIRS`; ambos no trackeados y en `.gitignore`): `RepoMap::build` 1,235 s, 1769 ficheros, 571 117 lineas. Tests de `repo_map` 5/5 y clippy `-D warnings` sin avisos. Medido con la maquina cargada.

## Impact
Cambia `repo_map.rs` (solo dos listas de exclusion): el arranque del kernel deja de leer los runtimes Python vendorizados y la salida de build de Next. Efecto real tras reiniciar produccion (decision de Jose).
