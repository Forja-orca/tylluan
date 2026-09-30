# 📊 BENCHMARK MD-3: Accuracy DESCOMPUESTA del router (routing / args / validez de ítem)

> **Protocolo:** cierre de MD-3 (`docs/architecture/MEASUREMENT_DEBT_REGISTER.md`).
> Ejecución 100% real contra **kernel de test aislado** (`:47007`, patrón T4/MD-5 —
> NUNCA producción `:47004`), dataset `dataset_i7_routing_curated.json` held-out
> (N=77), una llamada por ítem: `POST /api/v1/do {"intent":…, "plan":true}` (M31-P2:
> plan NO ejecuta la tool). Clasificación por los **prefijos de error estables de
> Stage-1** (`handler_do::error_prefixes`, commit `00a93af`) + validez intrínseca
> de ítem 3-way. Harness: `benchmarks/benchmark_md3_decomposed_accuracy.py`.

---

## 0. Sello de identidad de la corrida (MD-2)

| Campo | Valor |
| :--- | :--- |
| Timestamp | 2026-09-30 (corrida final) |
| Kernel de test | `tylluan-nexus` 0.17.0, commit `844c105` (incluye `00a93af` prefijos + `f00d293` fixes de registro) |
| Endpoint | `http://127.0.0.1:47007` (test aislado; config `test-run-md3/tylluan-md3.toml`) |
| git HEAD de la corrida | `f00d293` (lag kernel-vs-HEAD = 0) |
| Dataset | held-out N=77 (de 191 totales; filtro NON_ROUTABLE igual que la v3) |
| Artefactos | `benchmark_md3_results.json` (agregado) + `benchmark_md3_items.jsonl` (per-item, incremental) |
| Outcomes dinámicos | `plan_ok=46, args_missing=23, fractal_candidates=8` — **cero fallos de infra, cero unparsed** |

**Diferencias de entorno vs la v3 (bca9238, kernel `56b98b3`, producción `:47004`):**
kernel de test con SilvaDB recién creada (sin lesson priors ni routing anchors
acumulados) y registry limpio. Las cifras NO son comparables item a item con la
v3; se citan para continuidad histórica, no como delta.

---

## 1. Las 4 cifras separadas (lo que MD-3 exigía)

| # | Cifra | Valor | Denominador y regla |
| :--- | :--- | :---: | :--- |
| **1** | **Accuracy de ROUTING pura** | **31/68 = 45.59%** | Solo ítems válidos de routing (68), excluye 8 subtools soberanas + 1 hijack (no miden routing). Guild visible en plan o en error con prefijo; `fractal_candidates` cuenta como acierto solo si el target está entre los candidatos (0/8 lo estuvo). |
| **2** | **Completitud de ARGS (guilds resueltos)** | **40/60 = 66.67%** | Sobre routing cases con guild resuelto visible (60): 40 completaron args (plan_ok), 20 cayeron en `MISSING_ARGS:` (prefijo nuevo). |
| **3** | **Validez de ítem (intrínseca, 3-way)** | **68 routing / 8 subtool / 1 hijack** | El 11.7% del held-out (9/77) son ítems que NO miden routing. Desglose dinámico cruzado en §3. |
| **4** | **End-to-end (continuidad v3)** | **32/73 = 43.84%** | Mismo criterio de denominador evaluable que la v3 (excluye non-routable). Referencia v3: live 36.36% / hybrid+J-13 61.04%. |

**Por tipo de ambigüedad (routing cases):** `clear_keyword` 13/25 = 52.0% ·
`historical_real` 14/27 = 51.9% · `semantic_paraphrase` 2/5 = 40.0% ·
`cross_guild_ambiguity` 2/11 = 18.2% (sigue siendo la categoría más débil).

---

## 2. Qué muestra la descomposición que la cifra única ocultaba

### 2.1 El triaje manual v3 era OPTIMISTA en su suposición central

La v3 clasificó 17 de los 29 "unknown" como *"guild correcto pero args
obligatorios ausentes"*. Con los prefijos (`MISSING_ARGS:`) y la extracción del
guild del error, esa suposición es ahora **medible**: de los 20 `args_missing`
en routing cases, solo **9/20 (45%) tenían el guild correcto**; los otros 11
tenían un guild equivocado (pdf, audio_tools, code, coloquio_digest×2,
screenshot_tools, browser, database, code_graph, coloquio, ingest).

La narrativa "el fallo atribuible al routing puro es ~1 de 29" **no sobrevive
la medición descompuesta**: en esta corrida, 18 plan_ok con guild equivocado +
11 args_missing con guild equivocado + 8 fractal sin el target = **37/68
routing cases con decisión de guild incorrecta** (54.4%). Parte del gap v3 era
artefacto de medición (subtools no-routing, args ocultando el guild), pero el
routing puro también tiene fallos genuinos que la cifra única diluía.

### 2.2 El fractal gate (M23) intercepta 8 ítems sin resolver

8 intents no llegan a tylluan_do: `status:"ambiguous"` con 4 candidatos. En
ninguno de los 8 estaba el target → si el agente cliente no elige candidato,
son 8 fallos de routing encubiertos como "pide más especificidad". El eval v3
los clasificaba como `unknown` sin distinguirlos.

### 2.3 El coordinator-hijack está CERRADO en vivo (WS2 confirmado)

El ítem hijack (`create a new branch called feature/vector-tiering`, target
git) ya NO va a coordinator: fue a `code_graph` con `MISSING_ARGS:` (gate de
delegación WS2, `9fc8fbf`, verificado end-to-end). Sigue siendo un fallo de
routing (guild equivocado), pero la familia hijack está muerta.

### 2.4 Los 2 subtools `tylluan_remember:*` fallan por args, no por routing

Los 6 subtool-dispatches legítimos (`doctor_diagnose`, `docker_status`,
`explore *`, `query_model`) resuelven plan_ok. Los 2 `tylluan_remember:*`
caen en `MISSING_ARGS:` — coherente con el gate de 2 capas de ASI06 (exigir
contenido explícito), no son fallos de routing.

---

## 3. Cross-tab validez intrínseca × outcome dinámico

| Validez \ Outcome | plan_ok | args_missing | fractal | Total |
| :--- | :---: | :---: | :---: | :---: |
| routing_case | 40 (22 guild correcto) | 20 (9 guild correcto) | 8 (0 con target) | **68** |
| sovereign_subtool | 6 | 2 | 0 | **8** |
| coordinator_hijack | 0 | 1 (code_graph, no coordinator) | 0 | **1** |
| **Total** | **46** | **23** | **8** | **77** |

---

## 4. Bugs reales de registro descubiertos por el harness (fix `f00d293`)

Correr el protocolo contra un kernel de test (CWD ≠ raíz del workspace) destapó
2 bugs que en producción estaban enmascarados por el `registry.json` persistido:

1. **`LAZY_GUILDS` con rutas de módulo obsoletas** (`guilds.core.<name>` para 7
   guilds migrados a los gremios): el registro pre-v2 ganaba la carrera de
   "primera registración gana" sobre `[guilds.v2]` → todo spawn perezoso moría
   con `MCP handshake FAILED`. Fix: el loop LAZY resuelve el module path desde
   el catálogo (deriva del disco real en cada arranque) + test estructural
   `test_lazy_guilds_module_paths_resolve_to_real_files`.
2. **Registro v2 dependiente del CWD**: la comprobación de existencia de
   plugins de gremio usaba la ruta relativa del TOML contra el CWD del proceso
   → kernels lanzados desde otro directorio descartaban TODOS los plugins v2
   ("declared in TOML but missing on disk"). Fix: anclar al directorio que
   contiene el árbol `guilds/`.

Ambos verificados en vivo: 0 "missing on disk" en el arranque del kernel de
test y spawn real de guilds de gremio (`search → search_and_remember`).

---

## 5. Conclusión para MD-3

El protocolo de 3 ejes está **operativo y automatizado** (prefijos estables +
harness + artefactos per-item). A partir de ahora, cualquier cita de accuracy
del router DEBE acompañarse de las 4 cifras de §1 — nunca "accuracy" a secas.
La cifra única histórica (live 36.36%) mezclaba medición contaminada (ítems no
routing, args ocultando el guild) con fallos reales de routing (54% de las
decisiones de guild en routing cases en este kernel de test, con la salvedad de
memoría fría del kernel aislado). La referencia v3 (36.36/61.04) queda citada
como histórica; el baseline descompuesto de este documento es el nuevo punto de
partida para medir mejoras del matcher.
