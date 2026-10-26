# IMPL_M-BUFFY-12 — Caza de fallos: suite repetida 10×, tests `#[ignore]` y soak (Buffy, 2026-10-26)

Orden de José (vía cola `E:\tylluan-patches\cola\COLA_buffy_2026-10-26b.md`, M-BUFFY-12): orientar el
trabajo a "Tylluan funciona al 100 %, sin un solo fallo" — CAZAR fallos reales con repetición y carga.
Sin diff de código: solo informe.

## Qué se midió (método, reproducible)

Todo en `E:\tylluan-patches\buffy\mbuffy12\`, contra el worktree `E:\tylluan-wt-buffy` (HEAD del día),
con `CARGO_TARGET_DIR=/e/cargo-target/buffy` propio:

| Parte | Script | Salida |
|---|---|---|
| 1 — repetición | `run_suite.sh` (espejo de `E:\tylluan-patches\tl_t92.sh`) | `run1..10.log`, `status.log` |
| 2 — `#[ignore]` | `run_ignored.sh` (`-- --ignored` por target) | `ignored/<target>.log`, `ignored_status.log` |
| 3 — soak | `soak_runner.sh` + `soak_load.py`, kernel en **47012** con `DATA_DIR` propio (`soak-run/`) | `soak_status.log`, `samples.csv`, `metrics.json` |

El kernel de producción (47004) no se tocó en ningún momento.

## Parte 1 — 10 repeticiones seguidas de la suite de CI

Cada run = `cargo test -p tylluan-kernel --lib` + `cargo test -p tylluan-cli` + los 14 `--test` de
`.github/workflows/ci.yml` = **16 ejecuciones × 10 runs = 160**.

| Run | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|
| duración (s) | 3254 | 1690 | 2507 | 1806 | 1392 | 1587 | 1570 | 1586 | 1816 | 1842 |
| fallos | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

- **160/160 `EXIT=0`**, 160/160 `test result: ok`, 0 `FAILED`/`panicked` en los 10 logs
  (verificado con `grep -c "^EXIT_"`, `grep -v "=0$"`, `grep -c "test result: ok"` y
  `grep -lE "FAILED|panicked"`).
- Run 1 pagó la compilación en frío (3254 s); el resto 1392–2507 s.
- **Conclusión: 0 tests flaky detectados en 160 ejecuciones.**

## Parte 2 — los tests `#[ignore]` con `-- --ignored`

`ALL_DONE 2026-10-26T10:59:38` — 26 targets ejecutados (arranque ~07:52, ≈3 h por la compilación
en frío de cada binario aislado + `tylluan_evals` a 49 min). Desglose: **6 targets fallan hoy,
10 pasan de verdad, 9 ya no tienen ningún test ignorado, 1 especial (feature-gated)**.

`grep -rn '#\[ignore' crates` da 68 coincidencias: **24 son atributos reales** (la tabla de
resultados); las otras 44 son menciones en doc/comentarios (`//!`, `///`, cabeceras obsoletas).

### A. Resultado por target

| Target | EXIT | ¿qué corrió? | Resultado |
|---|---|---|---|
| kernel_lib (`--lib --ignored`) | 0 | 4 tests, 267 s | **PASA** todo |
| kernel_bin_nexus | 0 | 1 test (`ort_compatible_with_real_ort`, 0.2 s) | **PASA** |
| tylluan_evals | 0 | `ab_embedding_models_benchmark` | **PASA** en 2932 s (≈49 min) |
| a2a_live | 101 | 1 test | **FALLA** — sin SDK externo |
| approve_action_intent_adversarial | 101 | 1 matriz (16 filas) | **FALLA** — X1 (1 fila) |
| approve_action_self_approval | 101 | 1 test (4 casos) | **FALLA** — D-13 casos 2 y 4 |
| asi06_judge_hang_health | 0 | 1 test, 95 s | **PASA** |
| asi06_layer1_ingest_memory_write | 0 | 1 test, 377 s | **PASA** |
| asi06_layer1_ofuscacion_regresion | 0 | 2 tests | **PASA** (×2 = huecos abiertos) |
| asi06_redteam_memory_poisoning | 0 | 3 tests | **PASA** (×3 = residuos abiertos) |
| asi06_redteam_vias3_6 | 101 | 1 test | **FALLA INVERSO** — el bypass ya no existe |
| cola_d29_d30_acceptance | 0 | 0 tests (4 filtrados) | **VACÍO** — ignores retirados |
| cola_d32_acceptance | 0 | 0 tests (6 filtrados) | **VACÍO** |
| cola_d35_acceptance | 0 | 0 tests (3 filtrados) | **VACÍO** |
| continuity_probes | 101 | 1 test | **FALLA** — plan pendiente no sobrevive reinicio |
| d32_vias_escritura_regresion | 0 | 0 tests (4 filtrados) | **VACÍO** |
| decide_adversarial | 0 | 0 tests (13 filtrados) | **VACÍO** — endpoint ya aterrizó |
| knowledge_guild_test | 0 | **0 tests compilados** | **ESPECIAL** — tras `#[cfg(feature = "guild_knowledge_test")]` |
| oauth_f2_adversarial_spec | 0 | 0 tests (8 filtrados) | **VACÍO** — ignore retirado en `1a2b6a0` |
| oauth_f2_consent_adversarial | 0 | 0 tests (7 filtrados) | **VACÍO** |
| regression_d55_d56_outside_crate | 0 | 0 tests (2 filtrados) | **VACÍO** — fix ya aterrizó |
| regression_origin_host_outside_crate | 0 | 0 tests (8 filtrados) | **VACÍO** — `346cf65` |
| repo_map_real_measure | 0 | 1 test, 78.5 s | **PASA** |
| sparse_cascade_bench | 0 | 1 test, 1006 s | **PASA** |
| sparse_signature_spike | 0 | 1 test, 32.8 s | **PASA** |
| trust_banners_adversarial | 101 | 1 matriz | **FALLA** — queda 1 caso (B2) |

### B. Tabla granulada: fichero:línea | motivo del ignore | resultado hoy | propuesta

#### `src/` (unit) — 6 atributos, todos PASAN

| fichero:línea | motivo del ignore | resultado hoy | propuesta |
|---|---|---|---|
| `tylluan-evals/src/tests.rs:615` | descarga ~2.8 GB y "minutes on CPU" | **PASA**, pero 2932 s (≈49 min) reales | mantener; corregir motivo ("hasta ~1 h en esta máquina") |
| `tylluan-kernel/src/main.rs:2550` | necesita ONNX Runtime real (CI no lo tiene) | **PASA** (0.2 s) | mantener (correcto para CI) |
| `memory/silva/graph.rs:1097` | benchmark manual warm-start | **PASA** (en los 267 s de lib) | mantener (benchmark manual) |
| `memory/silva/search.rs:1758` | T1277-F: HNSW con 12.5k nodos, minutos | **PASA** (mismo bloque de 267 s) | mantener |
| `router/embeddings.rs:1632` | inferencia real bge-m3, auto-descarga 1.2 GB ~50 s | **PASA** | mantener |
| `router/embeddings.rs:1642` | inferencia real minilm, ~50 s CPU | **PASA** | mantener |

(`search.rs:1704` y `embeddings.rs:1624` del inventario son líneas de comentario, no atributos.)

#### `tests/` — 18 atributos

| fichero:línea | motivo del ignore | resultado hoy | propuesta |
|---|---|---|---|
| `a2a_live.rs:18` | requiere el agente echo oficial a2a-sdk en local | **FALLA**: `127.0.0.1<puerto>` connection refused (os error 10061) | dejar ignore (justificación correcta): sin el SDK externo el test no puede pasar |
| `approve_action_intent_adversarial.rs:143` | T-DSH-9 X1 abierto: `agent_id` declarativo permite aprobar; requiere binding token→agent_id (decisión de José). "F1,F3 ya corregidos" | **FALLA**: 15/16 filas conformes, solo X1 no conforme | mantener hasta la decisión; **actualizar comentario L140-141** (dice "9 filas no conformes", hoy es 1) |
| `approve_action_self_approval.rs:144` | D-13 | **FALLA**: CASO 2 el plan se consume sin credencial de operador (esperado `operator_required`); CASO 4 filas de audit con `human_intervention` incorrectas | mantener (D-13 abierto, misma decisión de binding); informar de que el consumo ocurre también en la ruta de error |
| `asi06_judge_hang_health.rs:76` | lento por diseño (~95 s: espera el `JUDGE_TIMEOUT` real) | **PASA** (95.0 s exactos) | mantener; candidato a nightly |
| `asi06_layer1_ingest_memory_write.rs:244` | spawn del guild `knowledge` (~60 s, dependiente de Python); verificado en T-DSH-15 | **PASA** (377 s) | mantener (dependencia de entorno) |
| `asi06_layer1_ofuscacion_regresion.rs:266` | límite D-31: leetspeak sin normalizar (clase abierta) | **PASA** ⇒ el hueco **sigue abierto** | mantener (tripwire de clase abierta) |
| `asi06_layer1_ofuscacion_regresion.rs:273` | límite D-31: paráfrasis no cubierta por lista cerrada | **PASA** ⇒ hueco **sigue abierto** | mantener |
| `asi06_redteam_memory_poisoning.rs:361` | ASI06: leetspeak sin normalizar (medido 2026-10-24) | **PASA** ⇒ residuo **sigue abierto** | mantener |
| `asi06_redteam_memory_poisoning.rs:382` | ASI06: clase de paráfrasis infinita; BASE_PARA sin detectar | **PASA** ⇒ residuo **sigue abierto** | mantener |
| `asi06_redteam_memory_poisoning.rs:451` | ASI06: matriz 5/6 familias cerradas, `v5_leetspeak` evade | **PASA** ⇒ hoy sigue evadiendo solo `v5_leetspeak` | mantener |
| `asi06_redteam_vias3_6.rs:409` | ASI06: procedencia — "la herencia no salva a una fuente nunca marcada" | **FALLA INVERSO**: `CASO B: el resumen de fuentes nunca marcadas salió marcado — el bypass ya no existe. Actualiza el informe.` (CASO A, herencia, pasa) | verificar si es cierre real (D-18) o sobre-marcado; si es real: invertir el test, quitar `#[ignore]`, actualizar el informe |
| `continuity_probes.rs:370` | `PLAN_STORE` es `OnceLock` en proceso: los planes pendientes NO persisten entre procesos; in-process no es medible | **FALLA**: `t25_b2` hace reinicio REAL y confirma que el plan pendiente **no sobrevive** | mantener; decisión abierta: persistir planes pendientes en DB (Deep) o aceptarlo y borrar el test |
| `knowledge_guild_test.rs:10` y `:57` | requiere `.venv` y `guilds/core/knowledge.py` | **NO EJECUTADOS**: todo el módulo está tras `#[cfg(feature = "guild_knowledge_test")]` ⇒ `-- --ignored` compila 0 tests | correr con `--features guild_knowledge_test` (No verificado hoy) o retirar cfg+ignore |
| `repo_map_real_measure.rs:57` | mide el repo real: recorrido de disco completo | **PASA** (78.5 s) | mantener |
| `sparse_cascade_bench.rs:67` | `#[ignore]` **sin motivo** (bench con modelos reales) | **PASA** (1006 s) | mantener; añadir motivo al atributo |
| `sparse_signature_spike.rs:30` | `#[ignore]` **sin motivo** (spike de solapamiento) | **PASA** (32.8 s) | mantener; añadir motivo al atributo |
| `trust_banners_adversarial.rs:137` | T-DSH-10: 3 vectores abiertos de D-18 (banner en think/@inbox legacy, localidad por identidad auto-creada, herencia no retroactiva) | **FALLA** pero con **1 solo** caso: `B2 mailbox_send: externo que se auto-registra identidad pasa por local → identidad_creada=true banner=false` | mantener; **actualizar el motivo** (de 3 vectores a 1: B2) |

#### Menciones `#[ignore]` en doc con el atributo YA retirado (9 targets, 0 tests ignorados hoy)

`cola_d29_d30_acceptance.rs:18` · `cola_d32_acceptance.rs:7` · `cola_d35_acceptance.rs:19,21` ·
`d32_vias_escritura_regresion.rs:4` · `decide_adversarial.rs:13,18-25` ·
`oauth_f2_adversarial_spec.rs:251` · `oauth_f2_consent_adversarial.rs:15` ·
`regression_d55_d56_outside_crate.rs:164` · `regression_origin_host_outside_crate.rs:16`

Todos sus tests corren ya en la suite normal (filtrados al pasar `--ignored`). Propuesta: limpiar
las cabeceras/documentación que siguen describiéndolos como ignorados
(`decide_adversarial.rs` L13-25 es una tabla entera obsoleta; `cola_d29_d30` L16-18 sigue diciendo
que los tests deseados están `#[ignore]`).

### C. Los 6 fallos, por severidad

1. **`asi06_redteam_vias3_6` (FALLA INVERSO — acción de documentación)**: el test se queja de que el
   bypass de procedencia **ya no existe**. Indica que la clase se cerró (o que hay sobre-marcado);
   hay que verificar, invertir el test y actualizar informe.
2. **`approve_action_self_approval` (D-13 abierto)**: un agente no operador **consume** el plan
   (HTTP 200 en vez de `operator_required`) y la auditoría HITL no registra bien
   (`human_intervention`). Misma familia que X1: falta el binding token→agent_id.
3. **`approve_action_intent_adversarial` (X1 abierto)**: spoof de `agent_id` declarativo consume el
   plan — 1 fila de 16 (F1/F3 ya cerrados desde el comentario original).
4. **`trust_banners_adversarial` (T-DSH-10, 1 de 3 vectores)**: identidad auto-registrada por un
   externo pasa por "local" ⇒ sin banner.
5. **`continuity_probes` t25_b2**: los planes pendientes no sobreviven un reinicio real (arquitectura:
   `PLAN_STORE` en memoria).
6. **`a2a_live`**: dependencia de entorno (SDK oficial en 8901), no es un defecto.

## Parte 3 — soak de 30 min (kernel de prueba, puerto 47012)

**Ejecución válida**: `attempt6b`, 2026-10-26 19:07:36 → 19:37:28 (30 min exactos de carga,
parada limpia por PID propio a las 19:37:46, `SOAK6B_ALL_DONE`).

### Método (reproducible)

- Kernel de prueba: binario `debug` del worktree (v0.17.1, commit `5d21cc7`),
  `CARGO_TARGET_DIR=/e/cargo-target/buffy`.
- Config: `tylluan.toml` de `E:\tylluan-patches\buffy\mbuffy12\soak-run` → `127.0.0.1<puerto>`,
  `dev_mode=false`, token propio. **DATA_DIR aislado (CWD = soak-run)**: verificado por
  `soak-run/data/active_port.json` y `*.db` frescos a las 18:57; la base de producción
  (puerto 47004) no recibió escrituras nuestras.
- Arranque **desprendido** (`Start-Process`, ventana oculta, sin árbol colgado de comandos
  expirados del harness — ver tabla de intentos).
- Carga `soak_load.py`, 1800 s, 3 hilos: `POST /api/v1/memory/write` 1/s,
  `GET /api/v1/memory/search` 1/s, ciclo MCP (`initialize` + `notifications/initialized` +
  `tools/list` + `tylluan_remember` + `tylluan_recall`) 1 ciclo/3 s.
  Media total ≈205 req/min, **por debajo del rate limiter (300/min/IP) → 0 × 429**.
- Muestreo cada 15 s de WS/hilos/handles vía PowerShell sobre el WINPID real →
  `samples.csv` (97 muestras, 19:07:36 → 19:37:28).
- Parada: `Stop-Process -Id <WINPID>` (sólo el PID propio, jamás por nombre de imagen).

### Historial de intentos (hallazgo de proceso)

| intento | ventana | resultado | causa raíz |
|---|---|---|---|
| run0 | 18:21–18:24 | 866 × `429 rate_limit` | tasas ~500/min > 300/min/IP: el limiter hizo su trabajo; tasas ajustadas |
| attempt5 | 18:28–18:35:57 | kernel muere a los 7 min, **sin panic y sin evento 1000 de Windows** | su wrapper bash (árbol de un comando mío que **expiró** en el harness) murió a la vez → muerte silenciosa del árbol entero. Corroborado: wrapper muerto, Defender sólo eventos rutinarios (sin 1116/1117), 227 GB de RAM (OOM descartado). Artefacto del método de arranque, no defecto del kernel |
| attempt6a | 18:57 | arrancó **sin `-ArgumentList` ni logs** | backticks de continuación de línea comidos por bash en mi script; relanzado en una línea con rutas `cygpath` |
| attempt6b | 19:06–19:37 | **30 min completos ✓** | éste |

### Resultados (attempt6b)

| endpoint | ok | err | nota |
|---|---:|---:|---|
| `POST /memory/write` | 1757 | 0 | 1/s sostenido |
| `GET /memory/search` | 1681 | 2 | `status=-1` (timeout >10 s) en los primeros 60 s |
| MCP `initialize` | 544 | 0 | |
| MCP `tools/list` | 544 | 0 | |
| MCP `tylluan_remember` | 544 | 0 | |
| MCP `tylluan_recall` | 535 | 9 | mismos `status=-1` de calentamiento |
| **total contado** | **5605** | **11 (0,2 %)** | todos en calentamiento; 0 después; 0 × 429 |

| recurso (97 muestras / 15 s) | inicio | pico | fin | tendencia |
|---|---:|---:|---:|---|
| WS MB | 489,4 | 3420,8 | 1088,3 | sube con el calentamiento y luego **baja**: sin fuga (Δ pico→fin = −2076 MB) |
| hilos | 252 | 252 | 248 | plano (rango 247–252) |
| handles | 1046 | 1353 | 1090 | Δ neto +44 en 30 min, sin crecimiento sostenido |

Log del kernel (30 min): **576 ERROR, 1462 WARN, 0 panicked, 0 RATE_LIMIT**.

- **100 % de ERROR/WARN = familia spawn de guilds** (`bash`, `memory`, `filesystem`):
  `MCP handshake failed … expect initialize response`, raíz
  `ModuleNotFoundError: No module named 'guilds'` (el CWD de prueba no contiene el paquete
  Python). Limitación del entorno de medición, no defecto del kernel. Fuera de esta familia:
  0 errores.
- **Hallazgo anidado (higiene)**: el supervisor anuncia `failed 3 times — marking as
  degraded, no further restarts` y sin embargo lo repite **528 veces** en 30 min (loop
  degrade↔watchdog cada 10 s). El mensaje "no further restarts" no se cumple y satura el log.
- **Arranque**: con la huella del modelo cacheada, `/health` OK a los **44 s** (huella servida
  en 12 s). En frío, el hash de 1,25 GB no cabe en los 120 s que usan los scripts de
  verificación (attempt1 observado >109 s dentro del hash; attempt2 murió dentro de ese hash
  con el método de arranque defectuoso) — por eso los intentos en frío fallaban antes de servir.

### Salud del soak

30 min de carga continua: **el kernel nunca cayó** (`/health` OK en cada punto de control),
sin panic, sin fugas de memoria/hilos/handles, sin 429, y con escritura+recuperación
verificadas de extremo a extremo (el primer sanity-check previo devolvió el nodo escrito).

## Impact

- **Suite normal sólida**: 0 fallos y 0 flaky en 160 ejecuciones de la lista de CI.
- **Los `#[ignore]` de seguridad están justificados**: los 6 que documentan huecos abiertos
  (leetspeak, paráfrasis, matriz `v5`, X1, D-13, T-DSH-10 B2, t25_b2) siguen fallando/passando
  exactamente como declaran — son tripwires, no tests rotos.
- **3 ignorados decaídos en documentación**: motivos con recuentos viejos (X1: "9 filas"→1;
  T-DSH-10: "3 vectores"→1; evals: "minutes"→49 min) y cabeceras de 9 ficheros que siguen
  hablando de ignores ya retirados.
- **1 test invisible**: `knowledge_guild_test` no corre ni con `--ignored` sin la feature.
- **Soak de 30 min superado (attempt6b)**: 5605 peticiones contadas OK + 11 timeouts de
  calentamiento (0,2 %), 0 caídas, 0 panic, 0 fugas (WS/hilos/handles planos o a la baja tras
  el pico de 3,4 GB), 0 × 429 con tasas bajo el límite.
- **3 muertes "del kernel" eran del método de arranque**: los intentos que murieron en
  silencio compartían arranque bajo un comando expirado del harness; con arranque desprendido,
  1/1 soak completo. El rate limiter por defecto (300/min/IP) corta cargas ingenuas con 429
  (comportamiento correcto, ahora documentado con números).
- **Arranque en frío > 120 s** (hash del modelo de 1,25 GB) frente a 44 s con caché: los
  health-timeouts de 120 s de los scripts no cubren el primer arranque.
- **Higiene del supervisor**: "degraded, no further restarts" repetido 528 veces en 30 min
  (loop watchdog cada 10 s contra guilds que ya no van a volver).

## No verificado

- No repetí `cargo clippy -D warnings` 10 veces (solo tests); ni Docker smoke, install smoke, el
  check de ARM ni el dashboard (`pnpm test`).
- `tylluan-evals` solo se ejecutó su target `--ignored` completo una vez (49 min), no 10 veces.
- `knowledge_guild_test` no se ejecutó con `--features guild_knowledge_test`.
- `a2a_live` no se ejecutó contra el SDK oficial (no instalado aquí).
- No investigué **por qué** `vias3_6` CASO B ahora sale marcado (cierre real vs sobre-marcado).
- No corrí los tests ignorados con `--nocapture` ni con hilos limitados; sin medición de CPU.
- El soak corrió **sin guilds reales** (el CWD de prueba no importa el paquete `guilds`):
  no medí carga con jueces/guilds vivos ni el caso en que un guild de verdad cae en bucle.
- **Una sola ejecución válida** de 30 min: es una muestra, no una estadística; la estabilidad
  observada no descarta intermitencias a horas de carga.
- No investigué por qué el WS **baja** tras el pico de 3,4 GB (¿trimming de Windows?
  ¿liberación ONNX?): sólo constato que no sube.
- Los eventos `LiveKernelEvent 141` (watchdog de GPU, 18:39:42) y `RADAR_PRE_LEAK` de
  explorer (18:45:13) del sistema ocurrieron 4–9 min **después** de la muerte del attempt5 y
  con el proceso ya caído: correlación temporal, no causalidad acreditada; no reproducido.
- Los 11 `status=-1` no distinguen timeout de cliente vs corte de conexión (el cliente no
  conserva el cuerpo del error).
- No medí CPU/temperatura de la GPU durante el soak, ni repetí el soak en modo CPU
  (`--device cpu`) para aislar DirectML.
