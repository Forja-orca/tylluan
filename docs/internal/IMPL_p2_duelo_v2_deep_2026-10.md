# Duelo P2 sobre la muestra v2 (T1254) — Decima-small vs NLI ONNX, deep

**Fecha:** 2026-10-18 · **Agente:** Deep · **Encargo del TL (T1254):** re-ejecutar el duelo
P2 (T1248/T1249) sobre la muestra v2 de Buffy, con el canal `instructions` ya activo
(`8b7c85c`/T1252), 3 variantes de redacción Decima, NLI fp32 (principal) y quint8
(secundario), baseline trivial v2 = 51.5% (siempre `contradice`), y etiquetar toda cifra
de `contradice` como **SINTÉTICA**. Sin promociones.

**Veredicto en una línea:** de los cinco sistemas medidos, solo **NLI fp32 (66.7%)** y
**Decima `w2_larga` (59.1%)** superan el baseline v2 (51.5%); el canal `instructions`
mejora la calibración de Decima pero no su accuracy; **ningún sistema detecta la clase
`no relacionado`** (Decima R=0 en las tres variantes) — bloqueante para cualquier
promoción a producción. Recomendación: mantener el contrato en sombra en modo
observación (ver §8).

---

## 1. Baselines primero (requisito del TL)

**Baseline trivial v2 = SIEMPRE `contradice`** (fichado por el TL antes de la corrida):

| Métrica | Valor |
|---|---|
| accuracy | **0.5152** (34/66) |
| Brier | 0.9697 |
| ECE | 0.4848 |
| cobertura | 1.0 (0 abstenciones) |

Ganancia de cada sistema sobre el baseline (accuracy):

| Sistema | acc | Δ vs baseline | ¿Supera baseline v2? |
|---|---|---|---|
| **NLI fp32** | **0.6667** (44/66) | **+15.15 pp** | **SÍ** |
| **Decima `w2_larga`** | **0.5909** (39/66) | **+7.57 pp** | **SÍ** |
| Decima `w1_instr` | 0.5152 (34/66) | 0.00 pp | NO (empata) |
| Decima `w1_default` | 0.5152 (34/66) | 0.00 pp | NO (empata) |
| NLI quint8 | 0.5152 (34/66) | 0.00 pp | NO (empata) |

**¿Supera algún sistema lo de la v1?** Comparación SOLO cualitativa — las muestras son
distintas (v1: verdad humana, 0 pares `contradice`; v2: 34 `contradice` **SINTÉTICAS**
con sesgo léxico declarado; **prohibido mezclar cifras**):
- v1: NINGÚN sistema superó su baseline (0.91; el más cercano, Decima W1, 0.88).
- v2: DOS sistemas superan su baseline (51.5%).
- Lectura: en v2 los sistemas aprovechan el sesgo léxico de los edits sintéticos (cambios
  numéricos visibles) — es una victoria sobre **contradicciones sintéticas detectables
  léxicamente**, NO evidencia de utilidad en producción. El discriminante decisivo
  (muestra humana con `contradice` natural ≠ 0) **sigue sin existir** (v1 tuvo 0;
  v2 solo sintéticas).

---

## 2. Setup (todo verificado en vivo)

| Componente | Detalle |
|---|---|
| Kernel de pruebas | `127.0.0.1:47011` (NUNCA 47004; producción intacta), `/health` → `commit 743a706` = `origin/main` |
| Build del kernel | binario `target/debug/tylluan-nexus.exe` reconstruido 2026-10-18 07:03 desde `E:\tylluan-wt-deep` (`743a706` + mi diff T1253 sin commitear — solo endpoints GET/operador añadidos; el camino `/api/v1/decide` NO se toca) |
| Decima serve | `:11436` (proceso python, vivo desde el duelo v1), `model=decima-small` vía `[decision.provider] systemone_http`, `max_inflight=1`, `request_timeout_ms=5000` |
| Muestra v2 | `benchmarks/human_truth_conflict_pairs_v2.json`, sha256 `0f88f03127f8ffd9…`, 66 pares: 34 `contradice` **SINTÉTICA** (synthetic_edit) / 16 consistente / 16 no relacionado, 0 solape con v1 |
| NLI | `cross-encoder/nli-deberta-v3-small` rev. `fa28048…`, ONNX local, intra_op=2 hilos, dirección `mean(ab,ba)`, mapeo `contradiction→contradice · entailment→consistente · neutral→no relacionado` (test de arrastre EN VERDE en ambas corridas); ancla neutral: fp32 `neutral=0.993`, quint8 `neutral=0.336` (cuantización degrada neutrales, igual que en v1) |
| Integridad | Decima: **198/198 HTTP 200**, 0 abstenciones, 0 reintentos fallidos. NLI: 66/66 en fp32 (51.8 s) y 66/66 en quint8 (40.9 s) |

### Variantes Decima (redacción fijada y documentada — estado IDÉNTICO en las tres)

Estado: `{"tarea": "Relación entre las dos afirmaciones del estado.", "afirmacion_a": <texto_a>, "afirmacion_b": <texto_b>}` · pregunta `relacion` (choice).

| Variante | Opciones | `instructions` en el wire | Texto que el modelo recibe |
|---|---|---|---|
| `w1_default` | `consistente \| contradice \| no relacionado` | **ausente** | default del servidor: `Answer the relacion question about this state.` (`resolve_instructions`, `systemone_http.rs:300-304`; ausente ⇒ fallback byte-idéntico al previo a T1252) |
| `w1_instr` | iguales a `w1_default` | explícita, **la MISMA frase que v1 enviaba por wire y el modelo NO recibía**: `Clasifica la relación entre afirmacion_a y afirmacion_b del estado; elige UNA opción.` | esa frase (canal activo desde `8b7c85c`) |
| `w2_larga` | etiquetas descriptivas de Buffy (**byte-idénticas a las de v1**): `las dos afirmaciones son compatibles: pueden ser ciertas a la vez` \| `las dos afirmaciones se contradicen: no pueden ser ciertas a la vez sobre el mismo aspecto` \| `las dos afirmaciones tratan de temas distintos: ninguna afecta a la otra` | ausente | default (`Answer the relacion question about this state.`) — aísla el efecto de las opciones respecto de `w1_default` |

---

## 3. Resultados principales (baseline primero, luego sistemas)

| Sistema | acc | Brier | ECE | abst | lat media (ms) |
|---|---|---|---|---|---|
| BASELINE (siempre `contradice`, **SINTÉTICA**) | 0.5152 | 0.9697 | 0.4848 | 0 | — |
| NLI fp32 | **0.6667** | 0.5223 | 0.1785 | 0 | 389.0† |
| NLI quint8 | 0.5152 | 0.6390 | 0.2733 | 0 | 306.6† |
| Decima `w1_default` | 0.5152 | 0.5780 | 0.1916 | 0 | 705.7 |
| Decima `w1_instr` | 0.5152 | 0.5512 | 0.1101 | 0 | 772.1 |
| Decima `w2_larga` | 0.5909 | **0.4985** | **0.0684** | 0 | 749.1 |

† Latencias NLI **no comparables** con las de v1 (fp32 54.8 ms / quint8 52.4 ms): esta
corrida se ejecutó con el kernel 47011 y el decima serve residentes (carga de CPU
distinta). Las métricas de calidad no dependen de la latencia.

### Acuerdo por clase (P / R / F1; support: consistente 16, contradice 34 **SINTÉTICA**, no relacionado 16)

| Sistema | consistente | contradice (SINT) | no relacionado |
|---|---|---|---|
| NLI fp32 | R=0.500 P=0.533 F1=0.516 | R=0.824 P=0.757 F1=0.789 | R=0.500 P=0.571 F1=0.533 |
| NLI quint8 | R=0.250 P=0.333 F1=0.286 | R=0.735 P=0.625 F1=0.676 | R=0.312 P=0.357 F1=0.333 |
| Decima `w1_default` | R=1.000 P=0.372 F1=0.542 | R=0.500 P=0.773 F1=0.607 | R=0.062 P=1.000 F1=0.118 |
| Decima `w1_instr` | R=1.000 P=0.372 F1=0.542 | R=0.529 P=0.783 F1=0.632 | **R=0.000** F1=0.000 |
| Decima `w2_larga` | R=0.562 P=0.450 F1=0.500 | R=0.882 P=0.652 F1=0.750 | **R=0.000** F1=0.000 |

### Matrices de confusión (filas=verdad, columnas=consistente | contradice | no_rel | ABST)

```
BASELINE (siempre contradice)          NLI fp32
  consistente:      0   16    0   0     consistente:      8    3    5   0
  contradice:       0   34    0   0     contradice:       5   28    1   0
  no relacionado:   0   16    0   0     no relacionado:   2    6    8   0

NLI quint8                             Decima w1_default
  consistente:      4    5    7   0     consistente:     16    0    0   0
  contradice:       7   25    2   0     contradice:      17   17    0   0
  no relacionado:   1   10    5   0     no relacionado:  10    5    1   0

Decima w1_instr                        Decima w2_larga
  consistente:     16    0    0   0     consistente:      9    7    0   0
  contradice:      16   18    0   0     contradice:       4   30    0   0
  no relacionado:  11    5    0   0     no relacionado:   7    9    0   0
```

Distribución predicha (66 pares): baseline `0|66|0`; fp32 `15|37|14`; quint8 `12|40|14`;
`w1_default` `43|22|1`; `w1_instr` `43|23|0`; `w2_larga` `20|46|0`.

---

## 4. Hallazgos

1. **NLI fp32 es el único sistema que claramente supera el baseline v2**
   (44/66 vs 34/66, +15.15 pp) y el único con recall de-mediated en las 3 clases
   (0.50 / 0.82 / 0.50).
2. **Decima `w2_larga` supera el baseline (+7.57 pp) y tiene LA MEJOR calibración de
   todos los sistemas** (Brier 0.4985 y ECE 0.0684 — mejores que incluso NLI fp32):
   pequeño pero consistente. Ojo: R=0 en `no relacionado`.
3. **El canal `instructions` (T1252) mejora la calibración, no la accuracy**
   (`w1_instr` vs `w1_default`: ambas 0.5152, pero ECE 0.1916→0.1101 y Brier
   0.5780→0.5512). Primera evidencia empírica de que el canal nuevo mueve la
   conducta del modelo en la dirección correcta — aunque en esta muestra no alcanza
   para superar el baseline.
4. **La redacción de opciones domina el prior del modelo**: mismo modelo, mismo estado,
   mismas instructions por defecto — `w1_default` predijo `consistente` en 43/66;
   `w2_larga` predijo `contradice` en 46/66, con accuracy +7.6 pp y mejor ECE.
   El contrato de 3 opciones no es estable respecto de la redacción (v1 ya lo
   insinuaba; ahora está medido con el canal instructions aislado).
5. **Bloqueante: cero capacidad sobre `no relacionado`.** Las tres variantes Decima
   predicen esa clase 1, 0 y 0 veces (R=0.062/0.000/0.000); incluso NLI fp32 solo
   alcanza R=0.500. Un contrato en sombra de 3 clases con esta degeneración a
   2 clases NO es promocionable.
6. **La cuantización quint8 se come la victoria**: fp32 0.6667 → quint8 0.5152
   (empata baseline), consistente con el hallazgo de v1 y con la degradación del
   ancla neutral (0.993→0.336). quint8 sigue siendo válido como fila secundaria de
   referencia, nunca como scorer.

---

## 5. Comparación con v1 (solo cualitativa; NO mezclar cifras)

| Eje | v1 (humana, 100 pares, 91/0/9) | v2 (sintética, 66 pares, 16/34*/16) |
|---|---|---|
| ¿Alguien supera su baseline? | NO (mejor: 0.88 vs 0.91) | SÍ, dos (fp32 0.667, w2 0.591 vs 0.515) |
| Decima mejores opciones | W1 corta (0.88) | W2 larga (0.591) — patrón INVERTIDO vs v1 |
| NLI | catastrófico (0.05 — prior de clase en contra) | mejor sistema (0.667) — el sesgo léxico sintético le favorece |
| `no relacionado` | sin dato relevante (9 pares) | sin capacidad en Decima (R≈0) |

\* `contradice` de la v2 = **SINTÉTICA**. El invertirse el ranking Decima (W1→W2) entre
muestras confirma que la sensibilidad a la redacción no es un artefacto de una muestra.

---

## 6. Latencia (informativo; contextos de carga no comparables entre corridas)

Decima v2 (198 llamadas, `max_inflight=1`): media 705–772 ms según variante
(p50 ≈ 710–762 ms; p95 ≈ 1038–1052 ms) — mismo orden que v1 (W1 827.9 / W2 486.8 ms).
NLI v2 (fp32 389.0 / quint8 306.6 ms media) frente a v1 (54.8 / 52.4 ms): Δ de carga
de la máquina (kernel + serve residentes), no del modelo; no usar para comparar runs.

---

## 7. Artefactos y reproducción

Corrida completa y verificada (todos exit 0):

```
# kernel de pruebas (usuario lo arranca): test-run-p2\boot-p2-deep.bat  → :47011
cd E:\tylluan-wt-deep\test-run-p2\harness
python run_decima_v2.py                                          # 3 variantes, 198/198
python run_nli_v2.py                                             # fp32
$env:NLI_RUN_TAG='quint8'; $env:NLI_ONNX='model_quint8_avx2.onnx'; python run_nli_v2.py
python metrics_v2.py                                             # baseline primero
```

Salidas (en `test-run-p2/harness/`, sin commitear): `decima_v2_w1_default.json`,
`decima_v2_w1_instr.json`, `decima_v2_w2_larga.json`, `nli_v2_preds_fp32.json`,
`nli_v2_preds_quint8.json`, `metrics_v2.json` (incluye `labels_note` sintética y el
sha256 de la muestra). Harness nuevo: `run_decima_v2.py`, `run_nli_v2.py`,
`metrics_v2.py`.

---

## 8. Recomendación sobre el contrato en sombra (decisión del TL/José)

**NO promover nada. Mantener el contrato en sombra en modo observación.** Motivos:
1. La mejor accuracy (fp32) y la mejor calibración (w2) están en sistemas distintos y
   ambos fallan la clase `no relacionado` (Decima R=0; fp32 R=0.5).
2. Las victorias de v2 son sobre **contradicciones sintéticas léxicamente marcadas** —
   el caso de uso real (contradicciones naturales) no está representado: v1 tuvo 0
   y v2 solo tiene sintéticas. Falta la muestra decisiva.
3. La dependencia enorme de la redacción (hallazgo 4) hace inestable cualquier
   umbral de acción automática hoy.

Si en el futuro se monta un A/B de sombra, la configuración candidata a probar es:
**Decima `w2_larga`** (mejor calibración) + canal `instructions` con la frase de
`w1_instr`, con **NLI fp32 como stream de referencia paralelo** (mejor accuracy);
quint8 descartado como scorer (empata baseline). Condición previa a cualquier
promoción: cerrar el agujero de `no relacionado` (o redefinir el contrato a 2 clases
para el duelo) y disponer de una **muestra v3 humana con `contradice` natural ≠ 0**
(petición a Buffy/TL: es hoy el cuello de botella del programa entero de decisión).

---

## 9. Limitaciones

- Muestra pequeña (66) y **sintética** en la clase `contradice` (sesgo léxico
  declarado en la ficha de Buffy); las cifras `contradice` son SINTÉTICAS.
- Una sola corrida por variante (sin semillas múltiples); Decima es un modelo
  determinístico vía serve pero el sampling del dialecto no se controló.
- El kernel de pruebas incluye mi diff T1253 sin commitear (endpoints GET de solo
  lectura para operador); el camino `/api/v1/decide` está intacto (verificado en el
  diff: service.rs solo añade `shadow_overview`; api_decide.rs solo añade handler
  GET; auth.rs solo añade entradas de tabla), y el `/health` confirma commit base
  `743a706` = `origin/main`.
- Latencias NLI no comparables con v1 (contexto de carga distinto; §6).
- La mejora de `w1_instr` sobre `w1_default` es un resultado de UNA muestra —
  direccional, no concluyente aisladamente.

---

## Impact

- **Ningún cambio de runtime para producción:** solo artefactos de benchmark y un
  informe (los 3 scripts de harness en `test-run-p2/harness/` + salidas JSON, todo
  fuera de `crates/` y sin commitear). `transport/`, `guilds/`, `integrations/` y
  `tylluan*.toml` NO se tocan.
- **El contrato en sombra NO se promueve** (modo observación, igual que hoy) — la
  decisión final es de TL/José; este informe aporta la evidencia (§8).
- **T1253 (endpoints shadow/conflicted) queda entregado aparte**, sin commitear, con
  su propio informe `IMPL_T1253_shadow_conflicted_deep_2026-10.md`.
- Deja medido por primera vez el efecto del canal `instructions` (T1252) en calidad
  de decisión (§4.3) y deja fijados por escrito los 3 prompts exactos de las
  variantes (§2) para futuras repeticiones.
