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

## MD-1 · TEB-Pilot-50 mide al arnés, no a un agente — `ABIERTO`

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

**Cierre:** reemplazar `evaluate_task_c0()`/`evaluate_task_c1()` por
inferencia real del mismo agente externo en ambas condiciones, juez sin
acceso al repo (sandbox separado) y mapping A/B aleatorizado por tarea.
Asignado: WS1 (Antigravity, handoff publicado en Coloquio 2026-09-13).
El piloto de 50 tareas se repite desde cero antes de escalar a N=200-500.

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

## MD-5 · Claims globales de latencia vs cola pesada condicionada — `ABIERTO`

**Qué dice medir:** "la latencia de Tylluan" como cifra única.

**Qué mide realmente:** una distribución con cola pesada fuertemente
condicionada. Baseline comiteado (`benchmarks/results/
latency_baseline_20260907.json`, kernel `667243f`, CPU, n=36 por op):
recall p50=664ms pero p95=16.0s/p99=20.0s; do p50=502ms pero p95=30.8s/
p99=102.8s. Cita honesta: citar p50 sin p95/p99 y sin condición es
presentar la cola como si no existiera.

**Matiz verificado:** la causa dominante del tail de arranque fue
identificada y corregida DESPUÉS del baseline (primer tick inmediato del
Agnostic Reindexer compitiendo por el modelo ONNX — ciclo 2026-09-07/08 de
`STATUS.md`, fix `2890d35`, primer disparo retrasado 600s). **No existe
baseline post-fix comiteado** — el tamaño real del tail hoy es desconocido,
y esa es la deuda, no el número viejo.

**Cierre:** la matriz SLO (`docs/architecture/SLO_MATRIX.md`, mismo ciclo)
define objetivo por operación × condición; falta el harness que produzca
esas corridas condicionadas de forma reproducible (el WIP de
`benchmarks/latency/` en el working tree, de otro agente, apunta ahí — no
se construye sobre él hasta que lo comitee con su atribución).

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

## MD-7 · `metrics_ring` se muestrea pero no se expone — `ABIERTO`

**Qué pasa:** el kernel recolecta telemetría en proceso
(`metrics_ring::MetricsRingBuffer`, creado y muestreado en
`transport/http/mod.rs:430` y `:840` — verificado 2026-09-14) pero
**ninguna ruta HTTP la lee** (no aparece en `api_v1/routes.rs`). Es el caso
opuesto al MD-4: medición real, invisible. Para la matriz SLO es la fuente
natural de percentiles por-request sin instrumentación nueva — falta
exponerla (endpoint read-only trivial, fuera del alcance docs-only de este
ciclo).

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

## Resumen

| ID | Ítem | Estado | Dueño del cierre |
|----|------|--------|------------------|
| MD-1 | TEB mide al arnés (ground_truth inyectado) | ABIERTO | WS1 / Antigravity |
| MD-2 | Identidad de benchmarks | PARCIAL | WS5 cerró adelante; retroactivo pendiente |
| MD-3 | Accuracy live mezcla routing+args+dataset | ABIERTO | sin asignar (eval protocol) |
| MD-4 | Golden-signals errors sintético | ABIERTO | sin asignar |
| MD-5 | Claims globales de latencia vs cola condicionada | ABIERTO | matriz SLO + harness de latencia |
| MD-6 | Drift narrativo de cifras (instancia viva: 861 vs 867) | ABIERTO | WS9 / docs-sync |
| MD-7 | metrics_ring invisible | ABIERTO | sin asignar (endpoint read-only) |
| MD-8 | Precision@5 sin contexto (LongMemEval) | CERRADO | cerrado 2026-09-03 |
