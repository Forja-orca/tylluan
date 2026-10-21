# T-DSH-23 — Q9: prueba pareada Decima-small vs reglas (McNemar exacto, 245 casos)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** worktree `E:\tylluan-wt-dsh` en `--detach origin/main` (`3dd9dc6`); worktree limpio antes de empezar.
- **Encargo (turno 1500):** Q9 de `docs/reference/research/2026-10-21_preguntas_correctas_camino_a_v1.md`: prueba **pareada** de Decima-small contra las reglas sobre los mismos 245 casos del dataset de Decision Fabric (`benchmarks/decision_fabric_dataset.jsonl`): tabla 2×2 de aciertos, McNemar exacto, diferencia en puntos y p. **Criterio previo (no cambiado después): GO solo si ≥ +4 puntos con p < 0,05; si no, NO-GO.** Solo lectura + tests; usar el harness `examples/decision_fabric_spike.rs` o replicar su evaluación. Informe con `## Impact` y «no verificado».

## Respuesta corta

**NO-GO.** Decima-small acierta 135/245 (55,10 %) frente a 130/245 de las reglas (53,06 %): **+2,04 puntos**, con 73 pares discordantes (39 gana solo el modelo, 34 ganan solo las reglas) y **McNemar exacto bilateral p = 0,640**. El criterio pre-registrado exigía ≥ +4 puntos **y** p < 0,05; falla en los dos. El «+5 aciertos netos» que citaba Q9 es real (39 − 34 = 5) pero estadísticamente indistinguible de azar con n=245.

## Método

1. **Casos (245).** `benchmarks/decision_fabric_dataset.jsonl` tiene 4.827 ítems; el filtro `split == "test"` y `source == "scheduler_confusion"` da exactamente **245**, y los 245 tienen pregunta `hitl_resolve` (comprobado con un script de conteo, no a ojo). Es el mismo corte que la cifra del ROADMAP (split test, confusion, n=245 hitl).
2. **Brazo «reglas» = `baseline_A.hitl_resolve` del dataset.** Es la predicción que el motor de reglas vivo tomó en producción para cada caso, exportada por el harness (T912); no se re-ejecuta ninguna regla, se evalúa la misma decisión registrada. Predicción positiva = `"true"`.
3. **Brazo «Decima-small»** = `DecimaOnnxDecisionProvider` (`decima-onnx-v1`, int8, 122M, dual-encoder encoder.onnx + scorer.onnx, temperatura 0.9355568358032639 del `decima.json` sin redondear), réplica fiel del código de la variante D del harness. Predicción positiva = `p(true) >= 0,5`. Se emiten los 245 pares (id, etiqueta, predicción reglas, predicción Decima).
4. **Paridad del modelo (golden).** `--golden` reproduce el ejemplo publicado del model card con deltas máx 0,0068 (billing 0,0089 vs 0,0040; technical 0,0316 vs 0,0310; sales 0,0023 vs 0,0010; account security 0,9572 vs 0,9640) — los mismos deltas que documenta el ROADMAP para esta variante, así que el modelo evaluado es el correcto.
5. **Estadística.** McNemar exacto bilateral sobre los pares discordantes: p = 2·Σ_{k≤min(b,c)} C(n,k)·0,5^n con n = b + c (b = acierta solo el modelo, c = aciertan solo las reglas), recortado a 1. Diferencia en puntos = (b − c)/n_casos · 100.

## Tabla 2×2 (245 casos pareados)

|                       | reglas acierta | reglas falla |
|-----------------------|----------------|--------------|
| **Decima acierta**    | 96 (ambos)     | **39** (solo modelo) |
| **Decima falla**      | **34** (solo reglas) | 76 (ambos fallan) |

- n = 245; ambos aciertan 96; ambos fallan 76; discordantes totales **73**.
- Aciertos reglas = 96 + 34 = **130/245 → 53,06 %**.
- Aciertos Decima = 96 + 39 = **135/245 → 55,10 %**.
- Diferencia = **+2,04 puntos** (b − c = +5 aciertos netos).
- **McNemar exacto bilateral: p = 0,640** (b=39, c=34, n=73).

## Criterio pre-registrado y veredicto

> GO solo si ≥ +4 puntos con p < 0,05; si no, NO-GO.

- Diferencia +2,04 puntos: **< +4** → falla.
- p = 0,640: **> 0,05** → falla.
- **VEREDICTO: NO-GO.** No procede abrir un quinto modelo (criterio de cierre de Q9: «una sola prueba pareada decide; el hilo termina»).

## Por qué no contradice al ROADMAP (que dice GO-PROVISIONAL)

El ROADMAP (variante D, cláusula 1 de §3) usa un criterio **no pareado y de una sola dirección**: «accuracy Decima > accuracy reglas» → 0,5510 > 0,5306, PASS. Q9 impone un criterio **más estricto y pre-registrado** (margen de +4 puntos y significancia con McNemar). Ambos son correctos en su marco: la variante D pasó su gate §3 por un margen no significativo, y la prueba pareada que Q9 exige demuestra que ese margen es compatible con el azar. **La decisión que cierra Q9 es la de esta prueba: NO-GO.** (El «+5 aciertos netos» del ROADMAP es exactamente b − c = 39 − 34 = 5; no había contradicción de cifras, sí de interpretación.)

## Reproducibilidad

- Dos ejecuciones independientes (sin y con volcado pareado): misma accuracy en ambos brazos (0,5510 y 0,5306), mismos TP/FP/TN/FN de la variante D (TP 134 / FP 0 / TN 1 / FN 110). El modelo es determinista (sin sampling); solo varió la latencia p99 entre corridas (111/143/202 ms, irrelevante para accuracy).
- Comandos exactos (toolchain 1.99.1, `CARGO_TARGET_DIR` propio; bundle leído en solo-lectura desde `E:\tylluan\models\decima`, que está gitignored y no existe en el worktree):

```text
cargo run -q -p tylluan-kernel --example decision_fabric_paired -- --variant-d E:/tylluan/models/decima --golden
cargo run -q -p tylluan-kernel --example decision_fabric_paired -- --variant-d E:/tylluan/models/decima --split test --source confusion
```

con `DF_PAIRED_OUT=<ruta>/paired_test_confusion.jsonl`; el JSON con el resultado está en `E:\tylluan-targets\dsh-t7-data\t23\mcnemar_result.json` (evidencia fuera del repo).

## Ficheros del diff

- **Nuevo**: `crates/tylluan-kernel/examples/decision_fabric_paired.rs` — copia del harness con dos añadidos (~15 líneas): acumula el par `(id, etiqueta, predicción reglas, predicción Decima)` en la rama `hitl_resolve` de la variante D y lo vuelca a la ruta de `DF_PAIRED_OUT`. No cambia ninguna lógica de decisión, métricas ni el gate §3 del harness original.
- **Nuevo**: este informe.
- **Cero cambios en `src/`**; no se tocó ningún test existente ni el harness original.

## Impact

- Solo **medición y evidencia**: no hay cambio de comportamiento del producto, ni de src, ni de configuración. El ejemplo nuevo es una réplica de evaluación (duplica código del harness a propósito, para no modificar el harness original que es también la referencia de la variante C/B); si el TL prefiere no acumular 104 KB duplicados, el volcado pareado se puede reducir a un parche de ~15 líneas sobre `decision_fabric_spike.rs` y borrar la copia — la decisión NO-GO no depende de qué fichero se conserve.
- Coste: ~245 inferencias CPU por corrida (≈30-40 s en debug con p50 ≈110 ms/ítem); cero llamadas de red; el bundle del modelo se leyó del checkout compartido en solo-lectura (no se escribió nada en `E:\tylluan`).
- Consecuencia de producto: **Q9 queda cerrada con NO-GO**. No se abre quinto modelo; el hilo de la «sociedad de modelos pequeños» para decisión se cierra con el dato. José decide si acepta el NO-GO, como pide Q9.
- No se arrancó ni tocó el kernel `:47004`. No se hizo commit ni push (regla de flota: solo Claude Code).

## No verificado

- **Runtime ONNX**: se usó el `onnxruntime.dll` que ya apunta `ORT_DYLIB_PATH` (1.22-directml en la máquina); no se validó que sea la misma versión exacta con la que se generaron las cifras del ROADMAP, aunque la paridad golden (deltas ≤0,0068) y la accuracy 0,5510 coinciden con lo publicado, lo que indica que el cómputo es el mismo.
- **Build en release**: la corrida fue en debug; la latencia p99 no es comparable con la del ROADMAP (202 ms vs 147 ms de referencia) y por eso el informe no la usa para el veredicto. La accuracy es independiente del perfil de compilación (mismo grafo, misma temperatura), pero no se ejecutó en release.
- **Reglas**: se evalúa el `baseline_A` exportado (decisión real registrada) — no se re-ejecutó el motor de reglas vivo contra los 245 estados hoy. Es la misma base que usó el ROADMAP.
- **Procedencia del split**: `split`/`source` se toman del dataset tal cual (generado por el harness en T908/T914); no se re-auditó que no haya fuga train/test más allá de campo declarado.
- **Una sola corrida determinista**: no se repitió con distintas semillas (el modelo no muestrea); no se calculó intervalo de confianza del +2,04 pts (el p exacto de McNemar ya decide).
- **Sin corrección por comparaciones múltiples**: es una única prueba pre-registrada, no una batería; no aplica, pero se declara.
- **CI**: el ejemplo nuevo no se añade a ningún workflow (no se pidió); no se ejecutó `verify.sh` completo (solo clippy `--all-targets` del crate, exit 0).
