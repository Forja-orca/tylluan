# Registro de Deuda de Medición (WS6)

> **Propósito:** inventario vivo de la brecha entre lo que cada métrica del
> proyecto *dice* medir y lo que *realmente* mide. Nacido del principio de la
> auditoría externa de 2026-09-13: "la propia metodología está descubriendo
> dónde sus mediciones no corresponden con lo que pretenden medir" — eso es
> más valioso que otra feature, pero solo si se registra.
>
> **Regla de mantenimiento:** cada número de este documento se re-verifica
> contra su fuente (commit, JSON comiteado, código en disco o post de
> Coloquio) **en el momento de escribir/actualizar**. Nada entra por memoria.
> Fecha de esta revisión: **2026-09-14**.
>
> **Estados:** `ABIERTO` · `PARCIAL` (una parte cerrada, otra no) · `CERRADO`
> (con la evidencia del cierre).

---

## MD-1 · TEB-Pilot-50 mide al arnés, no a un agente — `CERRADO` (2026-09-28, `877ad5e`)

**Qué dice medir:** la mejora que Tylluan aporta a un agente real
(condición C0 baseline vs C1 Tylluan, protocolo pareado).

**Qué mide realmente:** la capacidad del propio orquestador de diferenciar
dos pipelines sintéticos. La respuesta de C1 se construye insertando
literalmente `task['ground_truth']` en una plantilla:
`benchmarks/teb/teb_orchestrator.py:211-213` — verificado en disco
2026-09-14, las líneas siguen ahí. No hay agente externo en ninguna de las
dos condiciones.

**Los 3 resultados publicados son inválidos** (conciliación
2026-09-13 en `STATUS.md`, entrada "Ciclo 2026-09-13 — reconciliación
TEB-Pilot-50"): (1) TSR C0=38.0%/C1=92.0% (+54.0pp, `4773501`); (2)
C0=40.0%/C1=89.33% (+49.33pp, `b16f4f4`); (3) C0=20.0%/C1=100.0% (+80.0pp,
`e08ecf8`, el que estaba en disco). Ninguno debe citarse como evidencia de
valor de producto.

**Cierre real (2026-09-28, `f395c7f` Antigravity → merge `877ad5e`):**
`evaluate_task_c0()`/`evaluate_task_c1()` reemplazados por inferencia real
(0 ocurrencias de `ground_truth` verificadas en el harness), +202 líneas de
test nuevo (`tests/python/test_teb_orchestrator.py`, 5/5 en verde). Verificado
por el tech lead antes de mergear: diff línea a línea, sin regresión en 898
kernel tests. El piloto de 50 tareas sigue pendiente de re-ejecutarse desde
cero con el harness ya corregido antes de escalar a N=200-500 — el fix del
arnés está cerrado, la re-ejecución del piloto es trabajo nuevo, sin dueño
todavía.

---

## MD-2 · Ambigüedad de identidad de benchmarks — `PARCIAL` (cierre hacia adelante por WS5)

**Qué pasaba:** los artefactos de benchmark no llevaban identidad
machine-checkable del sistema que los produjo. Caso real comiteado: la
re-ejecución J-13 de 2026-09-13 (`bca9238`) grabó en
`benchmarks/benchmark_i7_j13_results.json` →
`live_kernel.commit: "56b98b3"`, `git_head: "c7abb00…"`,
`commit_lag: "8"` — el propio artefacto documenta 8 commits de desfase
entre el binario medido y el HEAD del repo. Los 3 resultados TEB
coexistentes (MD-1) son el mismo problema sin campo ninguno.

**Lo que cierra WS5** (commit `73c3f12`, 2026-09-13; 5 archivos, +360/−7):
`router/system_snapshot.rs` calcula al boot una identidad canónica
(`commit`, `version`, `config_hash`, `catalog_hash` sobre la superficie de
guilds+tools, `model_hash`, `schema_version`, `computed_at`) desde
superficies vivas, expuesta en `/health` (verbose) y estampada en cada fila
nueva de `guild_audit_log` (columna `system_snapshot`, fuera del chain
hash) y en cada fila del store de confusión WS3 (`8ac01f4`).

**Lo que sigue abierto:**
1. **Retroactivo:** los artefactos existentes (J-13/I-7, LongMemEval,
   baseline de latencia) solo llevan strings de commit — sin hash de
   config ni de catálogo, esas corridas no son reproducibles al 100%.
   Marcarlos `snapshot: null` honesto si se migran.
2. **Harness Python de TEB:** no incrusta el snapshot; podrá leerlo de
   `/health` verbose solo cuando el kernel en ejecución se reconstruya a
   ≥ `73c3f12` (a 2026-09-14 corre `56b98b3`, anterior a WS5 — verificado
   por `/health` durante la sesión del 2026-09-13).
3. **Modelo:** `model_hash` es el identificador del modelo de config, no
   el hash del fichero (documentado en `system_snapshot.rs`) — el estado
   real del binario del modelo sigue siendo externo al snapshot.

---

## MD-3 · "Accuracy del matcher en vivo" mezcla routing con validación de argumentos — `ABIERTO`

**Qué dice medir:** la calidad del enrutamiento de producción
(live matcher) frente al híbrido offline.

**Qué mide realmente:** accuracy end-to-end de `tylluan_do`, que mezcla
routing, validación de args de Stage 1 y composición del dataset. La
re-ejecución comiteada (`bca9238`, 2026-09-13, N=77, kernel `56b98b3`):
live 36.36% vs hybrid+J-13 61.04%. La corrida RRF previa comiteada
(`benchmarks/benchmark_i7_j13_results_after_rrf.json`, 2026-08-23, N=73,
kernel `be69f11`): live 49.32% vs J-13 64.38%. La brecha es estructural
(13.0pp y 24.7pp), no un episodio.

**Trazado manual de los 29 unknown** (`benchmarks/BENCHMARK_I7_J13.md`,
sección 0, comiteado en `bca9238`): **17** = guild correcto pero args
obligatorios ausentes (validación de Stage 1, `handler_do/mod.rs:469-475`,
error antes de que exista `result`), **11** = dispatch legítimo de subtools
soberanos (ítems del dataset que no son casos de routing), **1** =
coordinator-hijack de la cascada proactiva (esa familia la cierra WS2,
commit `9fc8fbf`→`9fc8bfb` con gate de delegación + regresión test),
**1** = genuine no-match. Es decir: el fallo atribuible al *routing* en
sí es ~1 de 29, y el dataset arrastra 11 ítems que no miden routing.

**Cierre:** un protocolo de evaluación que separe (a) decisión de guild,
(b) completitud de args, (c) validez del ítem como caso de routing. Mientras
tanto, citar siempre las 4 cifras por separado (live / hybrid / triage de
unknowns / composición del dataset), nunca "accuracy" a secas.

---

## MD-4 · El bloque `errors` de golden-signals es sintético — `ABIERTO`

**Qué dice medir:** tasa de errores del sistema (señal dorada "errors").

**Qué mide realmente:** constantes. `api_ops.rs:116-117` (verificado en
disco 2026-09-14): `rate_percent` = 0/5/20 según `diag.status`
(healthy/degraded/otro), `total_errors: 0` hardcodeado. Además el payload
declara `slo_target: 99.9` sin ninguna medición de disponibilidad detrás.
Estos campos parecen métricas y son placeholders — exactamente el patrón
"datos simulados presentados como reales" que la flota ya sufrió en
dashboard (incidente documentado en `AGENTS.md`).

**Cierre:** o se cablean contadores reales (la tabla `guild_audit_log` ya
tiene `status` por llamada y es la fuente natural), o el payload se etiqueta
explícitamente como placeholder. Decisión pendiente de asignación — no
asignado a nadie aún (2026-09-14).

---

## MD-5 · Claims globales de latencia vs cola pesada condicionada — `CERRADO` (2026-09-29, T4)

**Qué decía medir:** "la latencia de Tylluan" como cifra única.

**Qué medía realmente:** una distribución con cola pesada fuertemente
condicionada. Baseline comiteado (`benchmarks/results/
latency_baseline_20260907.json`, kernel `667243f`, CPU, n=36 por op):
recall p50=664ms pero p95=16.0s/p99=20.0s; do p50=502ms pero p95=30.8s/
p99=102.8s. Cita honesta: citar p50 sin p95/p99 y sin condición es
presentar la cola como si no existiera.

**Cierre real (2026-09-28/29, Deep, T4 del plan T464):** el harness de
latencia que faltaba ahora existe comiteado con atribución
(`benchmarks/latency/cpu_baseline.py`, Antigravity, merge `eeeed9d`) y el
primer baseline post-fixes está medido y comiteado con dos condiciones del
flag `embed_batching_enabled`. Kernel `0d46c4d` (contiene T842-i
`da0f34d`, literales `c73dd0f`, checkpoint bajo budget `7118fe7`), test
aislado `:47005`, `mxbai-embed-large` CPU, n=50 por celda por op,
`system_snapshot` estampado. Artefactos:
`benchmarks/latency/results_concurrent_20260928_224924.json` (flag OFF,
default producción) y `results_concurrent_20260928_233858.json` (flag ON,
experimental).

**Cifras por condición (nunca una sola):**

Flag OFF (baseline de producción):

| Op | C=1 p50/p95/p99 | C=4 p50/p95/p99 | C=8 p50/p95/p99 |
|----|-----------------|-----------------|-----------------|
| recall | 5.2s / 18.3s / 18.9s | 43.3s / 61.5s / 63.7s | 84.2s / 110.4s / 115.5s |
| do | 121ms / 391ms / 724ms | 272ms / 1.07s / 1.28s | 739ms / 10.9s / 13.2s |

Flag ON (experimental): recall C=4 p50 baja a 25.0s (el coalescer agrupa
bien a concurrencia moderada) pero C=1 p50 sube a 12.6s, C=8 p99 sube a
236s y `do` se degrada en las 3 condiciones (121ms→1.3s, 272ms→3.2s,
739ms→6.9s de p50) — el batcher global único crea head-of-line blocking
cruzado entre recall y el embed de routing de `do`.

**Hallazgos honestos de la medición (registrados, no ocultados):**
1. El suelo de recall con mxbai-large en CPU y queries distintas (cache
   fría) es ~5s por embed; el techo C=8 es la cola del Mutex del modelo.
   El viejo p50=664ms no es comparable (modelo/cache distintos).
2. Flag ON generó 31 nodos stale en mitad de la corrida C=8 (reindexer:
   "Found 31 stale" a las 23:26:03 UTC, 0/1 en el resto) — hipótesis
   mecanística: la cola acotada del batcher rechaza con Busy bajo C≥4 y
   esos embeds caídos dejan nodos sin embedding. Refuerza el NO-GO de
   `e70bc42`: el flag sigue sin ser adoptable; queda OFF en producción.
3. Cadencia de producción intacta: 1 checkpoint TRUNCATE por celda
   (correlación de ventana verificada contra el log) — el stall periódico
   de mantenimiento sigue presente por diseño; su gate de config es la
   pieza pequeña pendiente (Buffy T844.3), no bloqueante para este cierre.

**Recomendación registrada:** mantener `embed_batching_enabled=false`.
Si se revisita el batching, necesita colas por clase de call-site (recall
vs routing) o integración con la LRU cache — un solo batcher global
acoplado a la cola acotada actual degrada más de lo que salva.

---

## MD-6 · Drift narrativo de cifras canónicas — `ABIERTO`, con instancia viva hoy

**Qué pasa:** las cifras que citan los docs (conteo de tests, HEAD) se
desincronizan de la realidad y terceros las citan como estado verificado.
Precedentes: `STATUS.md:36` documenta la recurrencia 2026-09-13 (línea
"839/758" stale citada textualmente por un auditor externo) y declara que
los 4 gates mecánicos (`check_head_sync`, `check_test_count`,
`check_docs_reality`, `check_no_predation`) no cubren la línea narrativa.

**Instancia viva, re-verificada 2026-09-14:** `README.md:172` y
`STATUS.md:36` dicen **861** tests; la realidad tras el commit WS3
(`8ac01f4`, validación propia en worktree aislado, reportada en Coloquio
2026-09-14) es **867** (786 kernel + 69 link + 12 fsrs). El gate de conteo
compara README contra realidad y fallaría — pero solo corre en push/CI,
no al commitear. Este registro lo documenta; corregir README/STATUS es del
propietario del próximo pase de docs-sync (no se toca aquí para mantener el
diff de este ciclo en archivos nuevos).

**Cierre:** sentinel de frescura en `check_docs_reality.sh` (WS9,
pendiente) + disciplina de actualizar conteo en el mismo commit que añade
tests.

---

## MD-7 · Telemetría de latencia por-request sin fuente consultable — `CERRADO` (2026-09-14, commit `900816a`)

**Qué decía el ítem:** `metrics_ring` se muestrea en proceso pero ninguna
ruta HTTP la lee.

**Corrección del registro (2026-09-14, al re-verificar para cerrar):** la
premisa era falsa. `GET /api/v1/metrics/history` existe y lee el ring
(`metrics_history_handler` en `api_monitor.rs`, ruta en `routes.rs:174`)
desde la era v0.1.0 — presente en el split `8008c50` del 2026-08-13 y antes
en el `api_v1.rs` monolítico. La verificación original falló porque el grep
se ancló en el nombre del módulo (`metrics_ring`), y ni la cadena de ruta
(`metrics/history`) ni la del handler (`metrics_history_handler`) lo
contienen. Lección registrada: verificar por handler/ruta, no por nombre de
módulo.

**El gap real, que sí existía:** ni el ring (muestreo de 5s, latencia media
de medias por guild — inútil para percentiles) ni ningún otro endpoint
agregaba la latencia **por request** que `guild_audit_log.latency_ms` ya
registraba en cada dispatch de `tylluan_do`.

**Cierre:** `GET /api/v1/audit/latency` (commit `900816a`) — agregación
read-only sobre `guild_audit_log` con conexión estrictamente
SQLITE_OPEN_READ_ONLY (helper `audit_open_readonly`: una escritura
accidental fallaría a nivel de driver, no por convención).

**Campos:** `latency_ms.{p50,p95,p99,max,sampled,zero_latency_excluded}`
(percentil nearest-rank), `error_rate.{total,errors,percent}` (status-aware
sobre TODAS las filas de la ventana), `available`, `window_minutes`,
`source`, `note`, `generated`. Parámetro opcional `window_minutes`; sin
él, historial completo.

**Qué es una fila:** un dispatch completado de `tylluan_do`. Las filas del
do-path principal llevan la duración real del ciclo intent→resultado; los
caminos secundarios escriben 0 y se excluyen de los percentiles
reportándose aparte (`zero_latency_excluded`) — ni inventados ni
descartados en silencio. DB ausente → `available:false` con 200, nunca
500 (convención non-fatal de golden-signals). `TYLLUAN_AUDIT_DB` reubica
el store (seam de test, patrón `TYLLUAN_CONFUSION_DB`).

**Verificación:** 796/796 lib (786 + 10 nuevos), test de integración a
través del router real con matemática exacta de percentiles
(`tests/audit_latency_endpoint_test.rs`), clippy --all-targets -D warnings
limpio.

---

## MD-8 · Precisión@5 de LongMemEval sin contexto matemático — `CERRADO` (2026-09-03)

Ejemplo del ciclo completo de un ítem de este registro: el "gap" entre
Recall@5 82.0% y Precision@5 16.4% se auditó matemáticamente
(`STATUS.md`, ciclo 2026-09-03): para consultas single-needle (R=1), el
máximo teórico de Precision@5 sin normalizar es exactamente 20.0% y
0.82×0.20=16.4% — el número estaba en el valor esperado al dígito. No era
ruido de retrieval; era una métrica inapropiada para la tarea. Métricas
correctas adoptadas y documentadas: Recall@1 46.0%, Recall@5 82.0%,
Recall@10 90.0%, MRR/R-Precision 46.0%. Se mantiene en el registro como
ejemplo de qué significa "cerrar" un ítem: corrección matemática + métrica
correcta adoptada, no borrado del número.

---

## MD-9 · Gates de verificación existentes pero no invocados — `CERRADO` (2026-09-28)

**Qué dice medir:** la batería de scripts `scripts/check_*` / `verify_*` presenta al proyecto como cubierto frente a deriva de docs, de contratos, de kernel vivo, de portabilidad y de tests sobre código muerto.

**Qué medía realmente (antes del fix):** solo 8 de los 13 scripts corrían en algún punto. `check_docs_reality.sh`, `check_contracts.sh`, `check_dead_code_tests.sh`, `check_live_kernel_drift.sh`, `verify_contracts.py` y `no-absolute-paths.sh` no se invocaban de forma completa desde `.github/workflows/*` ni desde `scripts/verify.sh`. Además `check_docs_reality.sh` era inejecutable en clon limpio (exit 2 en línea 47).

**Origen:** quinta ronda de auditoría externa (Claude Opus 5, 2026-09-27) — ver `docs/roadmap/ROADMAP_O3.md`.

**Cierre (2026-09-27 Claude Code; 2026-09-28 Antigravity):**
1. **Pase 1 (Claude Code):** línea 47 de `check_docs_reality.sh` arreglada; 4 gates (`check_docs_reality.sh`, `check_contracts.sh`, `check_dead_code_tests.sh`, `check_no_predation.sh`) cableados a `scripts/verify.sh` y a `.github/workflows/ci.yml`. `check_live_kernel_drift.sh` cableado a `verify.sh` (local-only, documentado).
2. **Pase 2 y Cierre Total (Antigravity):**
   - Implementado el **meta-test de cobertura** `T6` y su **control negativo** `T7` en `scripts/test_verify_semantics.sh`: inspecciona dinámicamente todo script de verificación (`check_*.sh`, `check_*.py`, `verify_*.py`, `verify_*.sh`, `no-absolute-paths.sh`) en disco y valida mecánicamente que esté invocado en `scripts/verify.sh`, en `.github/workflows/*.yml`, o como sub-helper explícito de otro gate (ej. `verify_contracts.py` invocado por `check_contracts.sh`).
   - `T7` (control negativo hermético): un gate huérfano introducido en sandbox hace fallar el test inmediatamente, demostrando que la detección es load-bearing y previene regresiones en futuras auditorías.
   - Cableados a `scripts/verify.sh --docs` los gates restantes: `check_dead_config.sh` (report-only) y `no-absolute-paths.sh` (portabilidad). Añadidos a CI los jobs `mojibake-check` y `no-absolute-paths-check`.
   - Verificación: 7/7 tests semánticos pasan en `test_verify_semantics.sh` (T1-T7), `verify.sh --docs` limpio, cero scripts huérfanos en disco.

---

## Resumen

| ID | Ítem | Estado | Dueño del cierre |
|----|------|--------|------------------|
| MD-1 | TEB mide al arnés (ground_truth inyectado) | CERRADO | cerrado 2026-09-28, `877ad5e` — re-ejecución del piloto sigue pendiente, sin dueño |
| MD-2 | Identidad de benchmarks | PARCIAL | WS5 cerró adelante; retroactivo pendiente |
| MD-3 | Accuracy live mezcla routing+args+dataset | ABIERTO | sin asignar (eval protocol) |
| MD-4 | Golden-signals errors sintético | ABIERTO | sin asignar |
| MD-5 | Claims globales de latencia vs cola condicionada | CERRADO | cerrado 2026-09-29 (Deep, T4) — baseline post-fixes comiteado con 2 condiciones del flag de batching; flag ON NO-GO re-confirmado con datos |
| MD-6 | Drift narrativo de cifras (instancia viva: 861 vs 867) | ABIERTO | WS9 / docs-sync |
| MD-7 | Latencia por-request sin fuente consultable | CERRADO | cerrado 2026-09-14, `900816a` (con corrección de premisa) |
| MD-8 | Precision@5 sin contexto (LongMemEval) | CERRADO | cerrado 2026-09-03 |
| MD-9 | Gates de verificación existentes pero no invocados | CERRADO | cerrado 2026-09-28 (Claude Code cablería inicial + Antigravity meta-test T6/T7 y cobertura 100%) |
