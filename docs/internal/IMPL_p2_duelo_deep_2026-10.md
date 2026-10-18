# IMPL — Duelo P2 "puntos neuronales": Decima-small vs NLI local ONNX (Deep, 2026-10-18)

- **Tarea**: P2-DUELO, Coloquio T1248 (GO del TL) + T1249 (push `9e43e2b`, muestra
  enriquecida = decisión del TL, no mía).
- **Base**: `9e43e2b` (worktree `E:\tylluan-wt-deep`, diff sin commitear; solo Claude commitea).
- **Verdad de campo**: `benchmarks/human_truth_conflict_pairs_v1.json` (Buffy, 100 pares;
  91 consistente / 9 no relacionado / **0 contradice**; único anotador agente, sin kappa —
  NO es verdad humana de José, declarado en toda cifra de este informe).

## 0. Baselines triviales PRIMERO (requisito del TL)

| Sistema trivial | accuracy | Brier | ECE |
|---|---|---|---|
| **Siempre "consistente"** | **0.91** (91/100) | **0.18** | **0.09** |
| Siempre "no relacionado" | 0.09 | 1.82 | 0.82 |

Ningún sistema neural de este duelo supera al baseline trivial en accuracy, Brier ni ECE.

## 1. Tabla de resultados (100 pares; abstención = incorrecto)

| Sistema | accuracy | Brier | ECE | latencia media/par | p50 | p95 |
|---|---|---|---|---|---|---|
| **Baseline trivial** | **0.91** | **0.18** | **0.09** | — | — | — |
| Decima W1 (etiquetas cortas) | 0.88 (88/100) | 0.456 | 0.400 | 827.9 ms (HTTP) | 626.2 | 2759.5 |
| Decima W2 (etiquetas descriptivas) | 0.65 (65/100) | 0.555 | 0.220 | 486.8 ms (HTTP) | 420.8 | 1184.9 |
| NLI fp32 (ONNX, 2 hilos, media 2 direcciones) | 0.05 (5/100) | 1.675 | 0.847 | 54.8 ms (2 llamadas) | 41.3 | 193.0 |
| NLI quint8_avx2 (secundario) | 0.04 (4/100) | 1.587 | 0.796 | 52.4 ms (2 llamadas) | 38.1 | 188.8 |

Cobertura 1.00 en los 4 (0 abstenciones, 0 `answer_out_of_options`, 0 errores HTTP).

### Por clase — recall sobre soporte real

| Sistema | R consistente (91) | R no relacionado (9) | R contradice (0) |
|---|---|---|---|
| Baseline trivial | 1.000 | 0.000 | n/a |
| Decima W1 | 0.934 | **0.333** (3/9) | no evaluable |
| Decima W2 | 0.714 | **0.000** (0/9) | no evaluable |
| NLI fp32 | 0.022 | **0.333** (3/9) | no evaluable |
| NLI quint8 | 0.022 | 0.222 (2/9) | no evaluable |

**La clase "contradice" NO es evaluable**: soporte 0 en la muestra (decisión de muestra = TL, T1249).
Los "contradice" predichos (Decima W1: 6, W2: 30, NLI: 82/74) no pueden puntuarse aquí.

### Matrices de confusión crudas (filas = verdad, columnas = predicción; ABST = abstención)

```
                    consistente  contradice  no_relacionado  ABST
Baseline consist.         91          0            0          0
NLI nr(9):                 1          5            3          0      (NLI fp32)
NLI consistente(91):       2         77           12          0

Decima W1 consistente:    85          4            2          0
Decima W1 no_relacionado:  4          2            3          0
Decima W1 nr→(fila):       (4 contradice, 3 aciertos, 2 consistente... fila completa: 4 consistente, 2 contradice, 3 no_relacionado)

Decima W2 consistente:    65         26            0          0
Decima W2 no_relacionado:  5          4            0          0

NLI fp32 consistente:      2         77           12          0
NLI fp32 no_relacionado:   1          5            3          0
```
(Completos y exactos en `test-run-p2/harness/metrics.json` — fuente de todas las cifras.)

## 2. Hallazgos

1. **Ningún sistema le gana al baseline trivial del 91%.** Decima W1 llega a 0.88;
   el NLI colapsa a 0.05. Brier y ECE: el baseline trivial también gana (0.18/0.09).
   **No promuevo nada** (T1248): el contrato en sombra de P2 lo decide el TL con estas cifras.
2. **El cross-encoder NLI tiene un sesgo catastrófico de "contradice" sobre este dominio**:
   predice contradice en 82/100 (fp32) — incluidos 77 de los 91 pares `consistente`
   (p. ej. cp-001, dos funciones del mismo archivo: modelo C=0.961 en ONNX y torch).
   Es un dominio OOD (microtexto técnico/identificadores), no un fallo de pipelina:
   mapeo de etiquetas validado por anclas (E→índice1, C→índice0, N→índice2 por
   eliminación + ancla "capital of France"→neutral 0.993), y **ONNX fp32 == torch
   exactamente** en anclas y en 3 reales (spot-check cp-001/cp-046/cp-089).
3. **La cuantización quint8 degrada además** (hallazgo del paso 0): ancla neutral
   "capital of France" fp32 0.993 vs quint8 0.336 (contradiction 0.660); accuracy
   0.05→0.04. El duelo se ejecutó en **fp32 ONNX** (desviación mínima y justificada
   respecto al "172 MB cuantizado" del T1248) con el quint8 como fila secundaria.
4. **Efecto de redacción confirmado, pero EN EL SENTIDO INVERSO al anecdótico previo**:
   etiquetas cortas 0.88 vs etiquetas descriptivas largas 0.65 (−23 pp), y el recall
   de "no relacionado" cae 0.333→0.000 con las largas (W2 predice "contradice" 30 veces
   vs 6 de W1). Ambas redacciones exactas, en §3.
5. **Las `instructions` del wire NO llegan al modelo**: `question_spec()` genera su propio
   texto en `systemone_http.rs:287` ("Answer the {name} question about this state.") —
   solo llegan `state` + nombres de opciones + nombre de pregunta. Por eso W1/W2 cambian
   SOLO las opciones (estado idéntico → variable aislada). Dato a considerar si P2
   avanza: hoy el canal de redacción del contrato está cortado en el provider.
6. Latencia: NLI 54.8 ms/par (2 direcciones, 2 hilos) ≈ **15× más rápido** que Decima W1
   (827.9 ms HTTP, servicio compartido). W2 corrió DESPUÉS que W1 (orden fijo) — su menor
   media puede ser efecto cálido del servidor; crudo en §1, sin interpretar.

## 3. Protocolo exacto (reproducible)

**Lado Decima** (`test-run-p2/harness/run_decima.py`, exit 0, 100/100 HTTP 200 en ambas):
- Kernel de PRUEBAS aislado: puerto **47011**, `TYLLUAN_DATA_DIR=E:\tylluan-wt-deep\test-run-p2\data`,
  `boot-p2-deep.bat` (patrón test-run-*/boot-*.bat). **Nunca** el 47004 de producción
  (verificado: `host=127.0.0.1`, otro puerto, otro data-dir; producción sin tocar).
- `[decision.provider] kind=systemone_http endpoint=http://127.0.0.1:11436 model=decima-small
  max_inflight=1` (Decima compartido: 1 petición en vuelo, secuencial).
- Consulta libre (sin contrato), `max_latency_ms=10000` (deadline real = min(10000, 5000)).
- `state` idéntico en ambas redacciones: `{tarea, afirmacion_a, afirmacion_b}` (textos del
  dataset, saneados ≤300 chars, en el orden A,B del registro).
- **W1 (cortas)** — opciones: `consistente | contradice | no relacionado`.
- **W2 (descriptivas)** — opciones:
  `las dos afirmaciones son compatibles: pueden ser ciertas a la vez |
   las dos afirmaciones se contradicen: no pueden ser ciertas a la vez sobre el mismo aspecto |
   las dos afirmaciones tratan de temas distintos: ninguna afecta a la otra`
- Probabilidades W2 llegaron con las claves literalmente intactas (0 `answer_out_of_options`).

**Lado NLI** (`run_nli.py`, exits 0): `cross-encoder/nli-deberta-v3-small`, revisión
`fa2804872c3b4bd748f38c0185cc85775361e735`, ONNX `model.onnx` (fp32, sha256 verificado
`59fd8dd7…30897`) y `model_quint8_avx2.onnx` (sha256 `03c22213…9b6dae`), ORT
`intra_op=2 / inter_op=1` (2 hilos máx.), 256 tokens, ambos sentidos (A|B y B|A) con
media de probabilidades, mapeo `contradiction→contradice, entailment→consistente,
neutral→no relacionado` leído de `config.id2label`.

**Licencia verificada ANTES de descargar** (T1248 regla 1) — evidencia de la API oficial
de HuggingFace (JSON completo en `test-run-p2/models/nli-deberta-v3-small/hf_card_evidence.json`):
```json
{"id":"cross-encoder/nli-deberta-v3-small","sha":"fa2804872c3b4bd748f38c0185cc85775361e735",
 "gated":false,"private":false,"lastModified":"2025-04-15T08:18:25.000Z",
 "license_tag":["license:apache-2.0"],
 "cardData":{"language":"en","pipeline_tag":"zero-shot-classification",
   "datasets":["nyu-mll/multi_nli","stanfordnlp/snli"],"metrics":["accuracy"],
   "license":"apache-2.0","base_model":["microsoft/deberta-v3-small"],
   "library_name":"sentence-transformers"}}
```
`cardData.license = "apache-2.0"` — uso permitido; revisión de la API fijada y cotejada
con la revisión descargada (script aborta si difieren).

**Códigos de salida preservados**: build 0 · download_nli 0 · diag_labels 0 · diag_torch 0 ·
NLI fp32 0 · NLI quint8 0 · run_decima 0 (W1 100/100, W2 100/100) · metrics 0.

## 4. Límites declarados

- Muestra solo evalúa **consistente vs no relacionado** (0 contradicciones): "contradice" no evaluable.
- Anotador único = agente (Buffy), sin acuerdo inter-anotador (sin kappa): no es verdad humana.
- Textos saneados/truncados a 300 chars del dataset (unidad del benchmark); Decima siempre
  con el orden A→B (el NLI promedia ambos sentidos).
- Un solo modelo NLI (elegido por MELD §7.8); la muestra enriquecida con contradicciones
  reales = decisión del TL (T1249), no tomada aquí.
- Servicio Decima compartido: corridas W1→W2 sin enfriar (orden fijo, pos efecto cálido).

## 5. Artefactos (untracked, en el worktree)

`test-run-p2/`: `tylluan.toml` + `boot-p2-deep.bat` (kernel aislado), `models/` (ONNX+pesos
+ `hf_card_evidence.json`), `harness/` (`download_nli.py`, `diag_labels.py`,
`diag_labels_torch.py`, `run_nli.py`, `run_decima.py`, `metrics.py`,
`nli_preds_fp32.json`, `nli_preds_quint8.json`, `decima_w1_corta.json`,
`decima_w2_larga.json`, `metrics.json`). Ningún `.rs` modificado.

## Impact

- **Producción intacta**: cero cambios de código (ni `crates/`, ni `guilds/`, ni `tylluan*.toml`
  trackeados); el único cambio trackeable es este informe en `docs/internal/`. El kernel de
  producción 47004 no recibió una sola llamada (todo contra 47011); Decima :11436 solo con
  `max_inflight=1` secuencial.
- **Para P2 (decisión del TL, sin promoción mía)**: con esta verdad de campo, ningún modelo
  supera al trivial 91%; el NLI además colapsa en este dominio y su cuantizado empeora. Si
  P2 merece contrato en sombra, el salto de valor está en la clase `contradice` — que exige
  la muestra enriquecida (decisión T1249) antes de cualquier cifra de "detección de
  contradicciones".
- **Señal para ADR-020**: las `instructions` del wire no llegan al modelo (systemone_http.rs:287);
  hoy la redacción útil se hace por `state` + nombres de opciones. Si se quiere el canal de
  redacción completo, es un cambio de diseño, no de prompt.
