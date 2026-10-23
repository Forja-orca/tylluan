# T-DSH-48 — Cómo lanzar el banco `decision_fabric_paired` contra un endpoint SystemOne arbitrario

- **Agente:** dsh
- **Fecha:** 2026-10-23
- **Base:** `E:\tylluan-wt-dsh` en `origin/main` `c3a1066` (push 121; T-DSH-46/47 y contabilidad de tests 1284 integrados). Solo lectura: no se ejecutó nada contra servidores reales ni contra el kernel. El único artefacto es este informe (sin test; ver "Verificación").
- **Encargo (turno 1593):** documentar el comando EXACTO para lanzar `crates/tylluan-kernel/examples/decision_fabric_paired.rs` contra un endpoint arbitrario (flags/variables de entorno, corpus, qué mide, formato de salida, tiempos esperados), con `ruta:línea` de cada flag; comprobar si respeta timeouts y `max_inflight` del modelo local y si imprime una tabla por modelo (aciertos, p50/p95); si falta algo para un banco de N modelos servidos desde una misma URL, proponer el cambio mínimo **sin implementarlo**.

## 0. Aviso de nombre: el ejemplo correcto es `decision_fabric_paired`

Existen **dos** ejemplos casi idénticos en `crates/tylluan-kernel/examples/`:

- `decision_fabric_spike.rs` (104 006 B) y
- `decision_fabric_paired.rs` (104 741 B) = spike **+ 18 líneas** (la salida `DF_PAIRED_OUT` por ítem de la variante D, líneas 2354-2364).

La cabecera documental del propio `decision_fabric_paired.rs` está **desfasada**: sus ejemplos de uso dicen `--example decision_fabric_spike` (`decision_fabric_paired.rs:36-38` y `línea 60-61`), pero el target real de Cargo es el **stem del fichero**, `decision_fabric_paired`. Los comandos de abajo usan ese nombre. (Riesgo de deriva: dos copias del mismo cliente; el TL debería borrar/renombrar la vieja o actualizar su cabecera.)

## 1. Comando exacto

```bash
# build previo opcional (12m 33s en dev en mi máquina, sin features de dashboard):
cargo build -p tylluan-kernel --example decision_fabric_paired

# banco contra un endpoint arbitrario (LM Studio de José una vez tenga la URL):
cargo run --release -p tylluan-kernel --example decision_fabric_paired -- \
  --variant-b \
  --variant-b-endpoint http://127.0.0.1:<PUERTO_LM_STUDIO> \
  --variant-b-model <model-id> \
  --split all \
  --source all \
  --latency-budget-ms 500 \
  [--limit N]
```

Con el binario ya compilado equivale a:

```bash
<E:\tylluan-targets\dsh-t5-1991>\release\examples\decision_fabric_paired.exe --variant-b --variant-b-endpoint http://127.0.0.1:<PUERTO_LM_STUDIO> --variant-b-model <model-id> --split all
```

### Flags y defaults (`main`, líneas 248-300)

| flag | línea | default | qué hace |
|---|---|---|---|
| `--variant-b` | `línea 271-272` | off | activa el cliente HTTP `/v1/systemone` (sin él se ejecuta el extractor de dataset/baseline A) |
| `--variant-b-endpoint URL` | `línea 273-275` | `http://127.0.0.1:<PUERTO>` (el código usa 8080 por defecto, `línea 253`) | base URL; el POST va a `{endpoint}/v1/systemone` (`línea 1530`) y el probe a `{endpoint}/health` (`línea 1691-1693`) |
| `--variant-b-model NAME` | `línea 276-278` | `clef-flash` (`línea 254`) | **model id** que viaja en el cuerpo JSON (`línea 1525`: `{"model": self.model, ...}`) — lo que permite apuntar a un modelo concreto servido por LM Studio |
| `--split test\|all` | `línea 281` | `test` (`línea 256`) | filtra el dataset por split |
| `--source audit\|confusion\|all` | `línea 282` | `all` (`línea 257`) | `guild_audit_log` vs `scheduler_confusion` vs todo |
| `--limit N` | `línea 283-288` | sin límite | corta el número de ítems (útil para smoke test) |
| `--latency-budget-ms MS` | `línea 289-292` | `500` (`línea 258`) | umbral de la cláusula 3 del gate (p99) |
| `--export PATH` | `línea 262-264` | — | **solo sin variante**: escribe el JSONL del dataset (escritura en `línea 442`) |
| `--variant-c DIR` / `--variant-d DIR` / `--golden` | `línea 265-280` | — | otras variantes ONNX; no aplican al banco HTTP |

No hay variables de entorno para URL/modelo: **solo flags**. `DF_PAIRED_OUT` (`línea 2354`) es **exclusivo de la variante D** (escribe por ítem `{id, split, source, label, base, model}`); la variante B **no** escribe fichero, solo stdout.

## 2. Corpus que usa

- Fichero: `benchmarks/decision_fabric_dataset.jsonl` (`const DATASET_PATH` en `línea 565`); **está trackeado** (commit `54aa886`).
- Tamaño real medido: **4 827 ítems** → splits `train` 3 793 / `test` 1 034; fuentes `guild_audit_log` 3 597 / `scheduler_confusion` 1 230.
- `--split test` (default) sin `--limit` ⇒ **1 034 ítems**; con `--split all` ⇒ 4 827.
- Filtrado: `línea 1702-1726` (lee el JSONL, filtra split/source y aplica `--limit`).
- Origen del dataset (cabecera `:1-35`): `data/audit.db::guild_audit_log` (ground truth de `route_guild` = guild que ejecutó de verdad, solo `status='ok'`) + `data/scheduler_confusion.db` (ground truth de `hitl_resolve` = qué pasó sin humano). Baseline A = las reglas vivas ya registradas (`routed_guild`, `scheduler_verdict`).
- Si el fichero faltara: lanzar el ejemplo **sin** flags de variante con `--export benchmarks/decision_fabric_dataset.jsonl` (`línea 36-38`; escritura `línea 442`). Ojo: requiere `data/audit.db` y `data/scheduler_confusion.db` reales.

## 3. Qué mide y formato de salida (variante B)

Flujo: probe `/health` con timeout de 5 s (`línea 1683-1700`) → carga y filtra dataset → **un único POST `{endpoint}/v1/systemone` por ítem**, con `{"model": …, "state": …, "questions": {route_guild, hitl_resolve}}` (`línea 1525-1530`) → evalúa contra las etiquetas y baseline A.

Salida por stdout (secuencia real):

1. `[variant-b] probing SystemOne endpoint {endpoint} (model: {model})…` y aviso si el probe falla (con el recordatorio de `llama-server`/`ollama serve`, `línea 1683-1700`).
2. `[variant-b] items after filters (split=…, source=…, limit=…): N` (`línea 1717-1721`).
3. Por decisión, una tabla de texto:
   - `hitl_resolve` (`línea 1795-1812`): `n`, `accuracy: baseline A → B`, `TP/FP/TN/FN`, `Brier` baseline→B, `ECE(15)` baseline→B, `AUROC: B`, `latency (B, per item incl. HTTP): p50 / p99 / max` (**p50/p99, no p95**).
   - `route_guild` (`línea 1822-1840`): `n`, `accuracy` (baseline sobre ítems con baseline vivo), `Brier multiclass`, `ECE(15) top-class`, `p50/p99/max`.
4. `[variant-b] decide errors: N items skipped` si los hubo (`línea 1845-1847`).
5. Gate ADR-018 §3 (`línea 1850-1888`): cláusulas 1 (accuracy), 2 (calibración Brier+ECE), 3 (p99 ≤ budget) con PASS/FAIL, cláusula 4 abierta (coste) y `VERDICT: GO-PROVISIONAL | NO-GO…`.
6. Bloque `JSON:` final en stdout (`línea 1890-1980`) con claves: `provider` (`systemone-http-v1`), `endpoint`, `model`, `split`, `source`, `items`, `decide_errors`, `hitl{n, acc_variant, acc_baseline, brier_*, ece_*, auroc_variant, tp/fp/tn/fn, latency_p50_ms, latency_p99_ms}`, `route{n, acc_variant, acc_baseline, brier_*, latency_p50_ms, latency_p99_ms}`, `gate_verdict`. **No hay p95** en ninguna parte.

## 4. Timeouts y `max_inflight` — qué respeta y qué no

- **Timeout por petición: fijo, hardcodeado a 120 s** en el cliente reqwest del provider (`línea 1487-1490`). No hay flag ni env para cambiarlo y **no se lee el timeout configurado en el modelo servidor**. Si el modelo tarda >120 s por ítem, el `decide` falla y el ítem se salta (se cuenta en `decide_errors`, no aborta la corrida).
- **Probe `/health`: fijo 5 s** (`línea 1688-1690`); si falla solo avisa (WARNING) y sigue.
- **`max_inflight`: no existe como concepto.** El bucle de ítems es secuencial (`for it in &items { … provider.decide(&req).await … }`, `línea 1726-1778`), así que el inflight efectivo es **1** por construcción. No hay concurrencia ni flag para regularla, y **no se consulta ni se respeta un `max_inflight` declarado por el modelo local** (si LM Studio permitiera más, el banco no lo aprovecha; si exigiera menos, 1 cumple).
- Consecuencia práctica: la latencia impresa es "por ítem incluido HTTP" (`línea 1814`, `línea 1840`) y es comparable entre modelos **siempre que se corra secuencialmente**; no es un banco de throughput.

## 5. ¿Tabla por modelo? No

Cada invocación corre **un solo modelo** (`--variant-b-model`); imprime tablas por **tipo de decisión** para ese modelo y un JSON con el campo `model` escalar. No hay tabla multi-modelo ni comparación entre modelos.

## 6. Lo que falta para un banco de N modelos desde la MISMA URL (propuesta mínima, sin implementar)

LM Studio sirve varios modelos por el mismo base URL; el id viaja en el cuerpo (`línea 1525`), así que el cliente ya es agnóstico. Falta barrer la lista:

1. **Flag de lista**: hacer `--variant-b-model` repetible o añadir `--variant-b-models "id1,id2,id3"` (parseo en `main`, `línea 253-254` y `línea 276-278`). Mantener `--variant-b-model` como atajo de un elemento.
2. **Bucle por modelo**: cargar dataset **una vez** (`línea 1702-1711`), y para cada model id ejecutar el cuerpo de `run_variant_b_inner` (`línea 1674-1983`). Para no duplicar impresión, factorizar el cálculo de stats (bloques `hitl`/`route`) a una función que devuelva una `struct ModelSummary { model, items, decide_errors, hitl{n, acc, acc_base, brier, ece, auroc, p50, p99}, route{…}, verdict }`; la versión actual imprime directamente.
3. **Tabla resumen final** (una línea por modelo):
   `model | items | n_hitl | acc_hitl (base→B) | ECE_hitl | p50 | p99 | n_route | acc_route | p50 | p99 | errs | verdict`
   Mantener las tablas detalladas por modelo solo con `--verbose` o dejarlas tal cual por modelo.
4. **Salida a fichero**: `--variant-b-json <path>` que escriba un **array** de los JSON actuales (reutilizar el patrón de escritura de la variante D, `línea 2354-2364`; no reutilizar `DF_PAIRED_OUT`, que es por-ítem de D).
5. **Opcional, recomendado para el banco real**: `--variant-b-timeout-ms` (usar en el constructor `línea 1487-1490`) y `--variant-b-concurrency N` (default 1, semáforo tokio en el bucle `línea 1726`). Aviso: con N>1 los p50/p99 miden latencia bajo carga (throughput+cola), no latencia de ítem único; el JSON debería registrar `concurrency` para no comparar peras con manzanas. Y, si se quiere respetar el límite del servidor, documentar `max_inflight` en la config del banco (hoy no existe) — el cliente no puede adivinarlo.
6. Coste estimado del cambio: ~80-100 líneas en el propio ejemplo, sin dependencias nuevas (tokio/semáforo ya disponibles), sin tocar `src/`.

## 7. Tiempos esperados

Fórmula: `t ≈ N_ítems × latencia_media_por_ítem` (secuencial, 1 request por ítem que tenga al menos una pregunta válida; los ítems sin preguntas se saltan antes del POST, `línea 1740-1744`).
- Smoke test: `--limit 10` ⇒ segundos/minutos según modelo.
- `--split test` (1 034 ítems): con un modelo decisional local de ~0,3-1 s/petición ⇒ **~5-17 min**; con 2 s/petición ⇒ ~35 min.
- `--split all` (4 827 ítems) ⇒ **~25-80 min** en el rango anterior.
- Medición real de referencia del repo: la variante C (ONNX en CPU) reporta su propio p50/p99 por ítem; la B añade el coste HTTP+inferencia del servidor (etiqueta "incl. HTTP"). No hay tiempos publicados de LM Studio + d1-3B en el repo; los anteriores son **estimaciones**, no medidas.

## 8. Verificación

| comprobación | resultado |
|---|---|
| target de Cargo existe y compila | `cargo build -p tylluan-kernel --example decision_fabric_paired` → `Finished dev profile … in 12m 33s`; binario en `debug/examples/decision_fabric_paired.exe` (20 560 384 B) |
| dataset y cifras | `benchmarks/decision_fabric_dataset.jsonl` trackeado; 4 827 líneas medidas → train 3 793 / test 1 034; audit 3 597 / confusion 1 230 |
| flags y líneas | leídos y citados sobre `decision_fabric_paired.rs` (todas las `ruta:línea` de arriba) |
| ejecución contra servidor real | **no** (José aún no dio URL; prohibido lanzar nada) |
| diff | solo este informe; sin tocar `src/`, guilds, ni config; sin commitear |
| `check_test_count.sh` / `check_doc_links.py` / `check_docs_reality.sh` | 1284 ✅ / exit 0 / solo el artefacto preexistente `ROADMAP_O3.md:42` (WSL) |

## Impact

- **Solo informe** (no test): el encargo era documentar un ejemplo; no se ha modificado código ni se ha ejecutado contra modelos reales. Cero cambios en `src/`, guilds, configuración o corpus.
- **Desbloquea el banco que quiere José**: deja el comando exacto y los flags para apuntar LM Studio (`--variant-b-endpoint` + `--variant-b-model`) sin montar backend ni motor, y deja explícito que hoy **no** hay barrido multi-modelo, ni tabla por modelo, ni timeout/concurrencia configurables: los cuatro cambios mínimos (lista de modelos, factorizar resumen, tabla, JSON a fichero, timeout/inflight) están propuestos sin implementar.
- **Aviso de duplicado**: `decision_fabric_spike.rs` y `decision_fabric_paired.rs` son casi iguales y la cabecera del paired cita el nombre equivocado; conviene consolidar antes de que el banco crezca.

## No verificado

- **Nada ejecutado contra un servidor SystemOne real** (ni LM Studio ni llama.cpp/Ollama): tiempos, latencia real, comportamiento del probe y del timeout de 120 s no medidos. Los tiempos esperados son fórmula/estimación.
- **`max_inflight` del modelo local**: no existe en el cliente; no se comprobó si LM Studio expone/limita concurrencia. El "1 por secuencialidad" sale de leer el bucle, no de una prueba.
- **Compatibilidad LM Studio `/v1/systemone`**: el ejemplo está pensado para el contrato Jev de llama.cpp/Ollama; que la build 0.4.26 de LM Studio acepte exactamente ese body (`model`/`state`/`questions` con `instructions` sintetizadas, `línea 1600-1645`) no se ha probado (lo hará el TL con su URL).
- **DF_PAIRED_OUT**: confirmado por lectura que solo lo usa la variante D (`línea 2354`); no se ejecutó D.
- **Dataset**: no se regeneró; se asume el trackeado (`54aa886`). Si `data/audit.db` cambia, las etiquetas envejecen.
- **Duplicado de ejemplos**: no se verificó si `decision_fabric_spike.rs` se compila/usa en algún sitio (CI/script) ni cuál es el canónico para el TL.
- **Sin test añadido**: el encargo lo permitía opcionalmente ("si añades un test…"); un test con servidor falso en `127.0.0.1:0` requeriría lanzar el binario del ejemplo (compilación de ejemplo en test → coste alto) y solo pinaría strings; se prioriza el informe verificado por compilación + lectura. Si el TL lo quiere, se puede añadir como test de integración que arranque un fake `/v1/systemone` y ejecute el binario con `--limit 1` (puerto efímero, sin red externa).
