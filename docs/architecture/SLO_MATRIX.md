# Matriz SLO (WS6)

> **Propósito:** mapear las señales golden que el kernel ya expone
> (`GET /api/v1/system/signals` y `GET /api/v1/health/golden-signals`,
> también embebidas en `GET /api/v1/dashboard/summary`) a objetivos
> concretos con fuente de medición declarada. Responde a la auditoría
> externa 2026-09-13 (§16-17): un claim global "Tylluan latency = 12.9ms"
> no es un SLO; un SLO es *operación × condición × percentil*.
>
> **Regla de mantenimiento:** igual que
> [MEASUREMENT_DEBT_REGISTER.md](MEASUREMENT_DEBT_REGISTER.md) — cada
> afirmación se re-verifica contra su fuente al escribir. Revisión:
> **2026-09-14**.
>
> **Estados:**
> `MEDIBLE-HOY` — existe fuente continua y consultable (endpoint/DB).
> `MEDIBLE-MANUAL` — existe script/SQL comiteado que la produce, pero no
> corre continuamente ni está expuesto.
> `ASPIRACIONAL` — requiere instrumentación que hoy no existe (enlazada a
> su ítem del registro de deuda de medición).

---

## 1. Inventario honesto de las golden signals expuestas hoy

Fuente verificada en disco 2026-09-14: `api_ops.rs:101-137` (ruteado en
`routes.rs:112,135`) y su duplicado en `api_monitor.rs:147-171`.

| Señal | Campo | Real o sintética | Nota |
|-------|-------|------------------|------|
| Tráfico | `traffic.active_guilds/total_guilds/active_tools` | **Real** — `registry.status_all()` | — |
| Errores | `errors.rate_percent` | **Sintética** — constante por estado de diagnose (healthy→0, degraded→5, resto→20), no cuenta errores | MD-4 |
| Errores | `errors.total_errors` | **Sintética** — hardcodeada a `0` | MD-4 |
| Errores | `errors.critical` | **Real** — `diag.status == "critical"` | — |
| Saturación | `saturation.memory_percent` | **Real** — `diag.system.memory_percent` | — |
| Saturación | `saturation.storage_percent` | **Sintética** — hardcodeada a `0` | MD-4 |
| Saturación | `saturation.node_count/edge_count` | **Real** — Silva | — |
| Uptime | `uptime_seconds` | **Real** — `state.start_time` | — |
| SLO | `slo_target: 99.9` | **Decorativa** — constante sin medición de disponibilidad detrás | MD-4/MD-5 |
| Latencia por request | `GET /api/v1/audit/latency` | **Real** — percentiles nearest-rank sobre `guild_audit_log.latency_ms` (un dispatch = una fila), commit `900816a`. El ring 5s-sampled ya tenía ruta (`/api/v1/metrics/history` — corrije la premisa original del MD-7) pero es media-de-medias, inútil para percentiles | MD-7 (cerrado) |

**Lectura clave:** las señales de *tráfico y saturación* son reales; las de
*errores y disponibilidad* no miden lo que dicen (MD-4). No hay ninguna
señal de latencia expuesta, que es justo la dimensión que la auditoría
exige condicionar (§16-17).

---

## 2. Matriz SLO

Los objetivos ("objetivo propuesto") son propuestas de este documento, no
compromisos firmados — requieren aprobación del Tech Lead para devenir SLO
formal. Las filas de condición vacía = sin condicionar aún.

### 2.1 Latencia

Fuente de evidencia: baseline comiteado
`benchmarks/results/latency_baseline_20260907.json` (kernel `667243f`,
N=36 por operación, generado por `benchmarks/latency_baseline.py` — ambos
comiteados). Valores medidos: **recall** p50=664ms, p95=16.0s, p99=20.0s;
**do** p50=502ms, p95=30.8s, p99=102.8s. El `raw` del JSON muestra el
patrón cold/warm (run 0 dominado por embeddings fríos de 10-20s; run 1
mayormente 74-700ms) pero el `summary` no separa condiciones — ver MD-5.

| Operación | Condición | Percentil | Medido (baseline 09-07) | Objetivo propuesto | Estado |
|-----------|-----------|-----------|------------------------|--------------------|--------|
| `recall` | warm | p50 | ~0.1-0.7s (raw run 1) | < 1.0s | MEDIBLE-MANUAL |
| `recall` | warm | p95 | sin separación en summary | < 3.0s | MEDIBLE-MANUAL |
| `recall` | cold (embeddings fríos) | p99 | ~20s | documentado, sin target hasta separar condiciones | MEDIBLE-MANUAL |
| `do` | warm | p50 | ~0.02-0.7s (raw run 1) | < 1.0s | MEDIBLE-MANUAL |
| `do` | warm | p95 | sin separación en summary | < 5.0s | MEDIBLE-MANUAL |
| `do` | loaded / multi-agent | p99 | sin datos | < 30s | **ASPIRACIONAL** (MD-5: harness sin tags de condición) |
| `do` remote | mesh | p95 | sin datos | < 60s | **ASPIRACIONAL** (sin harness de dispatch remoto) |
| `recall`/`do` | continuo en producción | p50/p95/p99 | `GET /api/v1/audit/latency` (ventana opcional `window_minutes`) | p95 warm < 5s / p99 < 30s | MEDIBLE-HOY (commit `900816a`; ver MD-7 corregido) |

**Cierre de la brecha:** extender `benchmarks/latency_baseline.py` con
columnas de condición (cold/warm, idle/loaded) y separar el summary — el
harness ya existe y está comiteado, es la pieza más barata de cerrar.

### 2.2 Tráfico y saturación

| Señal | Fuente | Objetivo propuesto | Estado |
|-------|--------|--------------------|--------|
| Guilds online / total | golden-signals (`traffic.*`) | online ≥ total − 2 en steady | MEDIBLE-HOY |
| Tools activos | golden-signals (`traffic.active_tools`) | monotonía tras startup | MEDIBLE-HOY |
| Memoria del proceso | golden-signals (`saturation.memory_percent`) | < 85% sostenido 15min | MEDIBLE-HOY |
| Dispatches/min | `guild_audit_log` (SQL directo) | sin objetivo aún | MEDIBLE-MANUAL |
| Confusion matrix del Scheduler | `GET /api/v1/scheduler/confusion` (WS3, commit `8ac01f4`) | ratio `differ` trend antes del cutover | MEDIBLE-HOY |

### 2.3 Errores y disponibilidad

| Señal | Fuente hoy | Objetivo propuesto | Estado |
|-------|-----------|--------------------|--------|
| Tasa de error real | `GET /api/v1/audit/latency` → `error_rate` (status-aware sobre `guild_audit_log`, commit `900816a`); el bloque `errors` de golden-signals sigue sintético (MD-4) | < 1% de dispatches con outcome fallido | MEDIBLE-HOY (percent sobre todas las filas de la ventana; golden-signals pendiente de reconectar a esta fuente) |
| Disponibilidad | **ninguna** (`slo_target: 99.9` decorativo) | 99.9% mensual medido por probe | **ASPIRACIONAL** (requiere probe externo + contador real) |
| Guilds caídas | `registry.status_all()` vía golden-signals | 0 caídas no-intencionales (distinguir FAILURE de INTENTIONAL_STOP — Deep/WS4) | MEDIBLE-HOY (parcial: cuenta caídas, no las clasifica) |

### 2.4 Calidad (fuera del endpoint, con fuente comiteada)

| Métrica | Fuente | Último valor verificado | Estado |
|---------|--------|------------------------|--------|
| Routing accuracy (live matcher) | `benchmarks/benchmark_i7_j13_results.json` (commit `bca9238`, kernel `56b98b3`, N=77) | 36.4% live / 61.0% hybrid+J-13 | MEDIBLE-MANUAL (con la salvedad MD-3: 29/77 `unknown` no son fallos de routing — 17 args faltantes, 11 subtools soberanos, 1 hijack, 1 no-match) |
| Retrieval (LongMemEval) | STATUS.md ciclo 2026-09-03 | Recall@1 46.0%, Recall@5 82.0%, Recall@10 90.0% | MEDIBLE-MANUAL |
| Valor de producto agente+Tylluan | — | **sin medición válida** (MD-1: TEB-Pilot-50 inválido, 3 resultados revocados) | **ASPIRACIONAL** (bloqueado a WS1/Antigravity) |
| Identidad de benchmark | snapshot WS5 en `/health` verbose + filas de `guild_audit_log` (commit `73c3f12`) | presente desde boot `73c3f12` | MEDIBLE-HOY (adelante); histórico pre-`73c3f12` sin identidad (MD-2) |

---

## 3. Qué cerraría la matriz completa

1. ~~**MD-7 (barato):** endpoint read-only sobre `metrics_ring`~~ →
   **HECHO 2026-09-14** (`900816a`): `GET /api/v1/audit/latency` convierte
   las filas de latencia continua y la tasa de error real en MEDIBLE-HOY
   (y corrige la premisa del MD-7 — ver registro).
2. **MD-4 (medio):** contadores reales de error (agregación de
   `guild_audit_log`) + probe de disponibilidad → convierte §2.3.
3. **MD-5 (medio):** tags de condición en `latency_baseline.py` → separa
   cold/warm/loaded en el summary.
4. **WS1 (grande):** agente real en TEB → la única fila de "valor de
   producto" deja de estar vacía.

Hasta que 1-3 existan, cualquier claim de latencia o disponibilidad del
proyecto debe citar la fuente manual concreta (script + commit + fecha),
nunca un número global.
