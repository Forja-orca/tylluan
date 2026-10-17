# Puntos neuronales de Tylluan: literatura, simulación de encaje y síntesis (2026-10-17)

**Autor:** Claude Code (TL). **Encargo de José:** investigar como ingeniero de sistemas y científico qué tipo de modelo y en qué puntos de Tylluan mejoraría el sistema; al menos 3 propuestas por punto; simular cómo encajan en las costuras reales; sintetizar lo aprendido en ciclos de investigación y práctica.

**Etiquetas de evidencia** (cada afirmación lleva una):
- **[CÓDIGO]** leído hoy en el repositorio.
- **[MEDIDO]** medido hoy sobre el kernel o los datasets del repositorio.
- **[FUENTE]** leído en la fuente primaria (con **[VERIFICADO]** cuando se ha leído el PDF completo y contrastado sección/tabla/página; solo abstract si no se indica otra cosa).
- **[SECUNDARIO]** resultado de buscador, blog o resumen de terceros.
- **[HIPÓTESIS]** razonamiento propio sin medir.

## 1. Cómo preparan y documentan sus proyectos los equipos de referencia

| Práctica | Qué es | Evidencia | Qué tomamos |
|---|---|---|---|
| Model Cards (Mitchell et al., 2019) | Ficha por modelo: uso previsto y fuera de alcance, datos de evaluación, métricas, rendimiento por condiciones, consideraciones éticas | [SECUNDARIO] | Una "Component Card" por componente aprendido de Tylluan |
| Datasheets for Datasets (Gebru et al.) y Data Cards (Google) | Ficha por dataset: origen, composición, sesgos, usos | [SECUNDARIO] | Ficha del dataset de evaluación, con su verdad de campo y cómo se etiquetó |
| RouteLLM (Ong et al., ICLR 2025) | Router aprendido con datos de preferencia entre un LLM fuerte y uno débil; define métricas propias, CPT y APGR, para medir calidad frente a coste | [FUENTE] + [SECUNDARIO] | Medir siempre calidad frente a coste, con una métrica de "calidad recuperada" |
| "When Does Memory Help?" / MERIT (arXiv 2609.05441v1) | Evalúa 6 arquitecturas de memoria (C0–C5) con contabilidad completa de tokens y dólares sobre **23.440 episodios puntuados (\$42,57)**: piloto de 2 generaciones en `gpt-4.1-mini` (9.940 ep., \$9,12) + rejilla prerregistrada 3 modelos × 3 semillas (13.500 ep., \$33,45) y métrica **CAMU** (*puntos de utilidad ganados por dólar sobre C0*) | [FUENTE — VERIFICADO, PDF §1, §4.4, §4.5, Tabla 2] | Contabilidad de coste en cada medición; el valor de la memoria se mide en acciones (y tasa de ignorancia, *Ignore Rate* 45 %–53 %), no solo en recall |
| MemConflict (arXiv 2605.20926v1) | Taxonomía diagnóstica de conflictos de memoria: **dinámicos** (validez temporal ante actualizaciones reales), **estáticos** (corrección factual de atributos invariantes frente a menciones falsas posteriores) y **condicionales** (aplicabilidad contextual de pares condición–valor); evalúa 6 sistemas (MemOS, Letta, A-Mem, Memobase, Mem0, LangMem) en caja negra (AA, UOCS, CRS) y caja blanca (SEH@K, SRS, EUG) sobre 12 instancias (52,33 sesiones y 203.910,83 tokens de media) | [FUENTE — VERIFICADO, PDF §1, §3.1, §3.6, §3.7, Tablas 2–4 y 7] | Clasificar el conflicto antes de decidir quién lo resuelve y separar fallo de recuperación (*Retrieval Failure*) de fallo de utilización (*Evidence Utilization Gap*) |
| ADRs + notas de investigación con etiquetas de evidencia | Práctica propia de Tylluan (ADR-010 a ADR-021) | [CÓDIGO] | Mantener; añadir la ficha de componente |

Hueco ya anotado el 2026-10-15: la literatura no documenta la **evaluación en sombra de clasificadores de riesgo**. Nuestro arnés (`decision_shadow_log` más la suite pareada) es contribución propia.

## 2. Mapa verificado de los puntos de decisión y modelos que ya existen

[CÓDIGO] salvo indicación. ADR-018 ya contó **9 mecanismos de decisión dispersos** (scheduler, Coherence Gate, calor estigmérgico, aprobación BWC, marcado de contradicciones de GraphRAG, fases de NightConsolidation, necesidad de reranker, disparadores HITL, selección de proveedor).

| # | Punto | Qué hay hoy | Dolor conocido |
|---|---|---|---|
| P1 | Enrutado de `tylluan_do` (matcher y complejidad) | Matcher por palabras clave con modo semántico opcional; `complexity.rs` heurístico con cascada Direct/Reactive/Proactive | Spike ADR-010 §7.6: DistilBERT subió la decisión de 77,27 % a 86,36 % sobre 44 intents etiquetados a mano, pero con p50 de 38 ms frente al límite de 20 ms. NO-GO por latencia |
| P2 | Contradicciones y consenso | `flag_contradiction_nodes` marca `conflicted` cuando una fuente y un predicado tienen más de un destino distinto; `memory::consensus::ConsensusEngine` resuelve por fórmula de peso y confianza | Incidente 2026-09-26: la regla puramente estructural marcó todo el fan-out de GraphRAG y causó ~2000 % de CPU. Hoy hay 368 nodos `conflicted` de 9.896 [MEDIDO] |
| P3 | Escritura de memoria (`tylluan_remember`) | Hash de contenido, auto_link, extractor determinista de tripletas, salience y FSRS | No hay filtro de "merece guardarse" ni reconciliación semántica al escribir |
| P4 | Contenido no confiable (herramientas, A2A, Coloquio) | Coherence Gate, `dispatch_allow`, TaskContext con CallerTrustTier y PrivacyLevel | Backlog abierto: `untrusted_content` en REA; sin detector semántico |
| P5 | Ranking de recall | Búsqueda híbrida RRF, reranker jina turbo, LightReranker FFN 4-16-1 entrenado con la señal de feedback (ADR-011) | Ruido de medición; mejoras sin probar con rigor |
| P6 | Servicio común de decisión | `tylluan_decide` (ADR-021): reglas, proveedor HTTP System One, modo sombra, registro con hash | Reglas solo leen `risk`; la verdad de campo humana no existe |

## 3. Lo medido hoy con Decima-small (resumen)

[MEDIDO], kernel real, proveedor `systemone_http` contra Decima-small int8 en modo libre.

- Enrutado `scheduler_routing` (30 casos sintéticos con política monótona): modelo 20 % frente a reglas 77 %.
- `hitl_resolve` (245 casos de test): P(true) entre 0,30 y 0,75, media 0,495; 45 % de acierto frente a "siempre true" 99,6 % y reglas 53 %. La etiqueta es lo que hizo el sistema, no un humano.
- Enrutado de guild (789 casos de test): con el nombre de la opción como etiqueta, 2,7 %; con etiquetas descriptivas, 19,9 %. Referencia mayoritaria 51,6 %; regla trivial "contiene coloquio" 92 %.
- Latencia de Decima por consulta: ~180 ms de media en CPU con 2 hilos.

Lectura: **la herramienta funciona de punta a punta, pero hemos probado al modelo fuera de su terreno** (estado numérico, preguntas de seguridad, etiquetas opacas).

## 4. Propuestas por punto (mínimo tres cada una) y simulación de encaje

### P1. Enrutado de `tylluan_do`

| Opción | Descripción | Coste y encaje | Evidencia |
|---|---|---|---|
| **1A. Cabeza aprendida ligera con abstención** | Regresión logística o MLP pequeño sobre los embeddings que el kernel ya calcula (o TF-IDF de caracteres). Si la confianza supera un umbral, decide; si no, cae al matcher actual | ~0,01 ms por consulta; sin modelo nuevo; nuevo tipo de proveedor local en ADR-020. Encaja en `blend_with_mlp`, que ya existe | [MEDIDO] TF-IDF más regresión logística: 96,1 % de acuerdo con el enrutador actual en 789 casos de test, 99,5 % con confianza ≥0,7 y 93 % de cobertura. **Con dos salvedades graves:** el 83,4 % de los intents de test aparecen en el entrenamiento, y las etiquetas son la salida del propio enrutador. En el subconjunto no visto (130 intents únicos, 91 % coloquio) llega a 96,2 % frente a 90,8 % de la clase mayoritaria: no concluyente |
| **1B. Codificador afinado en el cuello de botella** | DistilBERT o T5-small cuantizado como señal adicional del blend | 5 a 38 ms. Ya hay NO-GO por latencia con DistilBERT; T5-small mide 5,4 ms pero no se evaluó la precisión | [CÓDIGO] ADR-010 §7.6 |
| **1C. Modelo System One como proveedor de enrutado** | Decima, Laya o Kev con pregunta `choice` | 50 a 300 ms; ya conectado | [MEDIDO] zero-shot 2,7 a 19,9 %. Solo tendría sentido tras afinar con nuestros datos, y entonces compite con 1A siendo más caro |
| **1D. Cascada por coste (estilo RouteLLM)** | Router barato, y el coordinador solo si la confianza es baja | Se superpone con la cascada Direct/Reactive/Proactive existente | [FUENTE] RouteLLM (reducción de coste de más de 2x declarada) |

Simulación de encaje: la consulta entra en `tylluan_do`; el matcher y la cabeza 1A puntúan en paralelo; la cabeza solo anula al matcher cuando supera τ y ambos coinciden en dominio; en cualquier otro caso manda el matcher. Se registra en sombra bajo un contrato `guild_routing` con ambas respuestas, y el desacuerdo se revisa por muestreo.

**Recomendación:** 1A en sombra primero. **Prerrequisito:** un conjunto de evaluación etiquetado por humanos y sin fugas (ver §6). Sin él, cualquier cifra de acierto es circular.

### P2. Contradicciones y consenso

| Opción | Descripción | Coste y encaje | Evidencia |
|---|---|---|---|
| **2A. Verificador NLI sobre los candidatos estructurales** | La regla actual propone pares; un clasificador de contradicción/implicación/neutro los verifica (promediando ambos órdenes) y **solo puede podar falsos positivos** | Tamaño ~100M a 400M de parámetros; 368 candidatos a la hora; no hay presión de latencia (en MELD cuesta 23,5 ms/delta en CPU edge: 4,0 ms embed + 19,4 ms NLI). Encaja en `flag_contradiction_nodes` | [FUENTE — VERIFICADO, PDF §4.1–4.2, §6.5, §7.2, §7.8, §9.1] **MELD** (`arXiv:2608.16357v1`) combina tres señales: identidad de clave $\kappa$, similitud coseno $\sigma$ con `all-MiniLM-L6-v2` y cross-encoder NLI $\chi$ con **`cross-encoder/nli-deberta-v3-small`** (evaluado también contra `RoBERTa-MNLI` y encoders `all-MiniLM-L12-v2` / `all-mpnet-base-v2`). Punto operativo calibrado sobre un dev-set de 9 pares: **$\theta_{\text{merge}} = 0{,}90$**, **$\sigma_{\text{lo}} = 0{,}12$**, piso de retención **$R_{\min} = 0{,}10$** (la compuerta de conflicto se evalúa desde $\sigma_{\text{lo}} = 0{,}12$ antes del merge, subiendo el recall de conflicto de 0,23 a 0,96). En su gold diagnóstico de 104 pares (26 semillas × 4 clases) obtiene **macro-F1 0,845** (IC 95 % `[0,766, 0,912]`), **tasa de fusión falsa 0,013** (`1/78` pares no fusionables) y **ROC AUC 0,968** (`merge`-vs-`rest`). Declara **5 limitaciones explícitas (§9.1)**: (1) evaluación frente a base centralizada sin demostrar optimalidad del transporte, (2) el CRDT de estado converge estatus (`active ⊑ deprecated ⊑ overruled ⊑ revoked`) pero **no adjudica verdad**, (3) **el grafo de fusión es sensible al orden de entrega** (Apéndice M: solo 91/200 = 45,5 % de reordenamientos reproducen el grafo del ejemplo guía y 177/200 huellas distintas en 16 claims), (4) premisa empírica abierta sobre el valor de enlaces tipados ($H(X_L \mid X_N) > 0$), y (5) **umbrales calibrados empíricamente que no transfieren entre encoders** (fallan al pasar de MiniLM a `mpnet`). [FUENTE] ECon (arXiv 2410.04068): promediar ambos órdenes porque el NLI depende del orden |
| **2B. Modelo System One como clasificador de relación** | Pregunta `choice` con tres opciones (contradice, consistente, no relacionado) usando el servicio que ya tenemos | Reutiliza todo ADR-021. **Hipótesis a medir:** Decima-small se entrenó con ANLI, XNLI y MNLI, es decir, su distribución de entrenamiento **es** NLI | [CÓDIGO]/[FUENTE] licencias y datos en la investigación verificada de 2026-10-17; [HIPÓTESIS] que rinda como un NLI dedicado |
| **2C. Clasificar el tipo de conflicto antes de resolver** | Taxonomía **dinámico** (actualización temporal legítima $\Delta_t$), **estático** (mención contradictoria falsa $P_q = \{(b, v^-)\}$ sobre atributo invariante $b \in \mathcal{A}_I$ que **no** debe sobrescribir $v^*$) o **condicional** (pares condición–valor $(c_\ell, v_\ell)$ simultáneamente válidos en contextos distintos); los dinámicos se resuelven con `valid_from` (ya existe), sin modelo | Cero modelos; extiende la supersesión temporal M35 | [FUENTE — VERIFICADO, PDF §3.1, §3.3.2–3.3.4, §4.3, Tablas 3–4 y 7] **MemConflict** (`arXiv:2605.20926v1`) demuestra que ningún sistema domina los tres tipos: los orientados a actualización (`LangMem`, mejor en dinámicos con AA 0,4966 y SEH@3 0,7842) colapsan en estáticos (AA 0,1944) y condicionales (AA 0,1556) al sobrescribir hechos estables o perder la condición; los conflictos estáticos son los más difíciles en todos los sistemas (mejor AA 0,4375 en `MemOS`, y reconocimiento explícito de contradicción `CRS ≤ 0,2501`), además de sufrir brecha de utilización (`EUG` medio de 0,0769 a 0,1527) |
| **2D. Síntesis generativa con compuerta de coherencia** | Qwen3-0.6B o similar reescribe la reconciliación; la compuerta BGE (umbral 0,85) descarta alucinaciones | 570 MB, no instalado; mejora de calidad, no corrección de un fallo | [CÓDIGO] ADR-010 §7.3 |

Simulación de encaje: `flag_contradiction_nodes` sigue generando candidatos. Para cada par nuevo se invoca `decide(contract="contradiction_verify", question relation:choice[contradicts, consistent, unrelated])` en sombra. Con confianza alta en `consistent`, el verificador **sugiere** quitar la marca (nunca la pone). Tras la medición pareada contra una muestra etiquetada a mano, se promueve a `advisory` y después a `gate` solo para podar. El invariante v8 se cumple: el modelo nunca anula un rechazo determinista ni crea conflictos.

**Recomendación:** 2C ahora (coste cero), 2B y 2A como duelo medido sobre las 368 marcas actuales, con etiquetado humano de una muestra de 100. El resultado decide qué proveedor entra.

### P3. Escritura de memoria

| Opción | Descripción | Evidencia |
|---|---|---|
| **3A. Reconciliación al escribir** (supersedir en vez de acumular) | Al guardar un hecho nuevo, detectar el hecho anterior sobre la misma clave y cerrar su `valid_until` | [FUENTE — VERIFICADO, PDF §1, §5.4, §5.7, Tabla 2] **"When Does Memory Help?" / MERIT** (`arXiv:2609.05441v1`): en el nivel `hard` (hechos actualizados con contradicciones entre sesiones), la recuperación por embeddings (`C2`, `text-embedding-3-small`) colapsa de forma impredecible entre modelos y semillas (**0,30 a 0,95**: Haiku 4.5 da 0,90/0,95/0,30 en D1/D2/D3; GPT-4.1 da 0,70/0,45/0,60; `gpt-4.1-mini` da 0,70±0,23 / 0,38±0,08 / 0,37±0,03 con brecha entre semillas de hasta 0,45), mientras que las memorias que actualizan al escribir (`C4` fact store estructurado y `C3` resumen LLM) se mantienen en **0,70 a 1,00** (0,80–1,00 en la rejilla multmodelo y 0,70–1,00 incluyendo el piloto). Además, el híbrido (`C5 = C4 + C2`) **empeora a `C4` solo** en `hard` (0,50–0,80 frente a 0,75–1,00) porque la mitad vectorial reimporta el hecho obsoleto, y aun cuando `C2` recupera el hecho actualizado en el bloque de memoria (55 episodios `hard`), el agente lo ignora el **45 %** de las veces (*Ignore Rate* = 0,45 en generación real, 0,53 en starter) |
| **3B. Puerta de importancia aprendida** | Cabeza pequeña sobre señales ya registradas (acceso, feedback de recall, saliencia) para decidir qué consolidar | [SECUNDARIO] Hindsight: la consolidación tiene cuatro palancas (importancia, fusión, decaimiento, expulsión) y casi nadie documenta su política |
| **3C. Pregunta booleana "merece guardarse" a un modelo System One** | Reutiliza el servicio | [HIPÓTESIS]; sin verdad de campo ni caso de uso medido |

Encaje: 3A reutiliza el verificador de P2 en el camino de escritura. 3B se entrena de noche (NightConsolidation) con los pares que ya produce el Signal Loop. **Recomendación:** 3A primero porque ataca el modo de fallo que la literatura señala; 3C se descarta hasta tener verdad.

### P4. Contenido no confiable

| Opción | Descripción | Evidencia |
|---|---|---|
| **4A. Estructural: procedencia y taint** | El contenido de herramientas y de otros agentes lleva su procedencia; mientras esté "tainted", no puede disparar herramientas privilegiadas. Se apoya en `CallerTrustTier` y `PrivacyLevel` de TaskContext | [FUENTE — VERIFICADO, PDF Abstract, §5, §6.1–6.5, Tablas 2, 3, 5, 6, 7 y 10–13] **CaMeL** (`arXiv:2503.18813v2`, Google / DeepMind / ETH Zurich): separa **Privileged LLM** (genera plan en subconjunto de Python sin ver datos no confiables) y **Quarantined LLM** (parsea datos no confiables a esquemas Pydantic sin acceso a herramientas, más booleano `have_enough_information`), junto a un intérprete con grafo de flujo de datos (`NORMAL` y `STRICT`), capacidades (`sources`, `readers`) y políticas deterministas en Python. **Ambas cifras de los resúmenes secundarios existen en el PDF pero corresponden a modelos y tablas distintas:** (1) en el titular con **`o3 High`** (Abstract y Tabla 2, benigno) resuelve el **77 %** (`77,3 % ± 8,3`) de AgentDojo frente al **84 %** (`84,5 % ± 7,2`) sin defensa (y `79,8 %` vs `79,0 %` bajo ataque, Tabla 3); (2) en la comparativa de defensas con **Claude 3.5 Sonnet** (Tablas 5, 6 y 7), CaMeL obtiene **63,92 %** de utilidad benigna (vs **90,72 %** sin defensa) y **66,70 % (~67 %)** de utilidad bajo ataque (vs **86,83 %** sin defensa), reduciendo los ataques exitosos con políticas de `44` (o `297` en Gemini 2.5 Flash y `163` en Gemini 2.5 Pro, Tabla 4) a **0 de 949** dentro de su modelo de amenaza, con un sobrecoste mediano de **2,73× tokens de entrada y 2,82× de salida** en benigno (2,02× / 2,64× bajo ataque) |
| **4B. Clasificador de inyección como prefiltro** | PromptGuard 2 o PIGuard | [SECUNDARIO] baratos, con huecos conocidos |
| **4C. Bandera aditiva con System One** | Pregunta `noul`: "¿contiene instrucciones dirigidas a una IA?"; solo marca, no bloquea | [HIPÓTESIS]; Decima no sirve de puerta de seguridad (su propia tarjeta) |

Encaje: 4A es la columna vertebral; 4B y 4C son sensores en sombra que alimentan métricas. **Recomendación:** 4A como diseño (ADR nuevo), 4B/4C solo como medición. Nunca un modelo como único guardia.

### P5. Ranking de recall (ya hay un modelo)

| Opción | Descripción |
|---|---|
| **5A. Mantener** el LightReranker 4-16-1 y medirlo con rigor | [CÓDIGO] ya existe; falta A/B limpio tras el arreglo de contención (T770) |
| **5B. Aprendizaje a ranking** (gradient boosting) con la señal de feedback | [HIPÓTESIS] |
| **5C. Cross-encoder presupuestado** solo cuando el score de RRF sea ambiguo | Encaja con la heurística de necesidad de reranker ya existente |

**Recomendación:** 5A primero (cerrar la deuda de medición), el resto después.

### P6. El servicio de decisión como espina común

Qué cambia en ADR-021 si adoptamos lo anterior:
1. **Nuevos tipos de proveedor**, además de `rules` y `systemone_http`: `linear_head` (P1) y `relation_verifier` (P2). Mismo trait `DecisionProvider`.
2. **Contratos nuevos**: `guild_routing`, `contradiction_verify`. Cada contrato fija sus preguntas, sus opciones con **etiquetas descriptivas** y su umbral por contexto.
3. **Tabla de adjudicación humana**: cada decisión del panel HITL se guarda como verdad de campo ligada al `request_hash`. Es lo que hoy falta.
4. **Regla aditiva** sin excepciones: los modelos podan, marcan o bajan confianza; nunca resucitan lo rechazado ni son puerta única.

## 5. Síntesis de este ciclo (lo aprendido)

1. **El nombre de la opción es parte del modelo.** Pasar de nombres sueltos a descripciones multiplicó el acuerdo por 7 (2,7 % a 19,9 %). Cada contrato debe versionar sus textos.
2. **Una etiqueta que sale del propio sistema es circular.** El registro de auditoría tiene 0 intervenciones humanas, y las etiquetas de enrutado son la salida del enrutador. Hace falta verdad de campo humana.
3. **Siempre las líneas base triviales.** "Siempre true" saca 99,6 % en `hitl_resolve`; "contiene coloquio" saca 92 % en enrutado. Una métrica sin su línea base miente.
4. **Fugas entre entrenamiento y test.** El 83,4 % de los intents de test ya estaban en el entrenamiento: los datasets de auditoría son repetitivos y exigen deduplicar.
5. **Probar el modelo en su distribución.** Decima-small se entrenó con NLI; la prueba natural es relación entre dos enunciados (P2), no estado numérico ni seguridad.
6. **Las heurísticas estructurales fallan por forma.** El incidente de GraphRAG marcó fan-out estructural como contradicción. La verificación semántica debe ser aditiva sobre la estructura, no sustituirla.
7. **El activo es el aparato de medida**, no un modelo: el carril de sombra, la bitácora con hash, el arnés pareado y la telemetría ya funcionan de punta a punta.

## 6. Enfoque a perseguir

**Columna vertebral determinista, adjuntos aprendidos medidos en sombra, promoción solo por prueba pareada.** Orden:

1. **Verdad de campo antes que modelos.** Dos conjuntos etiquetados por humanos y deduplicados: 100+ intents diversos para enrutado y 100 pares de las 368 marcas de conflicto. Mientras no existan, ninguna cifra de acierto es concluyente.
2. **P2 como primer duelo real**, porque es donde el modelo tiene terreno propio (NLI) y el dolor está probado (incidente, 368 marcas vivas).
3. **P1 con la cabeza ligera** en sombra, midiendo contra el conjunto del punto 1.
4. **P4 como diseño estructural** (ADR), con sensores de modelo solo para medir.
5. **Ficha de componente** para todo lo que entre: tarjeta de modelo, ficha de datos, contrato, resultados de sombra y criterio de promoción.

## 7. Estado de verificación de fuentes primarias y qué sigue sin verificar

### 7.1. Verificado en PDF completo (2026-10-17)

- **`[VERIFICADO]` MELD (`arXiv:2608.16357v1`, Lovén et al., 41 págs.):**
  - **Señales y modelos exactos (§4.1, §6.5, §7.2, Apéndice L):** identidad de clave $\kappa$, similitud densa $\sigma$ con `all-MiniLM-L6-v2` y veredicto NLI $\chi$ con el cross-encoder **`cross-encoder/nli-deberta-v3-small`** (entrenado en MNLI; barrido adicional con `all-MiniLM-L12-v2`, `all-mpnet-base-v2` y `RoBERTa-MNLI`).
  - **Umbrales calibrados (§4.2, §6.5, §7.2):** $\theta_{\text{merge}} = 0{,}90$, $\sigma_{\text{lo}} = 0{,}12$ (calibrados sobre 9 pares de desarrollo) y piso de retención $R_{\min} = 0{,}10$. Evaluar contradicción en $\sigma \ge \sigma_{\text{lo}}$ antes de las reglas de fusión sube el recall de conflicto de `0,23` a `0,96` sin alterar la tasa de fusión falsa.
  - **Métricas y coste (§7.2, §7.8):** macro-F1 `0,845` (IC 95 % `[0,766, 0,912]`), tasa de fusión falsa `0,013` (`1/78` pares no fusionables) y ROC AUC `0,968` sobre el conjunto diagnóstico de 104 pares; `0/8` contradicciones omitidas en muestra naturalista de HotpotQA (precisión `0,381` por sobremarcado seguro de neutros); coste de admisión por delta de `23,5 ms` en CPU edge (`4,0 ms` embed + `19,4 ms` NLI; `9,8 ms` en GPU V100) y rodilla de saturación de `~54 merges/s`.
  - **Las 5 limitaciones explícitas (§9.1, Apéndice M):** (1) evaluación relativa a línea base centralizada sin probar optimalidad del transporte; (2) la convergencia de estatus CRDT no adjudica la verdad; (3) el grafo de fusión es sensible al orden de entrega (`91/200 = 45,5 %` en el ejemplo guía; `177/200` huellas distintas en 16 claims); (4) premisa empírica abierta sobre si los enlaces tipados aportan información más allá de los nodos ($H(X_L \mid X_N) > 0$); (5) los umbrales son calibrados empíricamente y no transfieren entre encoders (fallan al cambiar a `mpnet`).
- **`[VERIFICADO]` MemConflict (`arXiv:2605.20926v1`, Tao et al., 33 págs.):**
  - **Taxonomía y protocolo (§3.1, §3.3, §3.6, §3.7):** conflictos **dinámicos** (actualización temporal real), **estáticos** (mención falsa posterior sobre atributo invariante que no debe sobrescribir el valor estable) y **condicionales** (pares condición–valor dependientes del contexto), con inyección de distractores de entidades relacionadas sobre 12 instancias (media de `52,33` sesiones, `2.349,17` turnos, `203.910,83` tokens y `124,33` preguntas por instancia).
  - **Hallazgos (§4.3–4.5, Tablas 3, 4 y 7):** ningún sistema entre los 6 evaluados (`A-Mem`, `LangMem`, `Letta`, `MemOS`, `Mem0`, `Memobase`) domina los tres tipos; los conflictos estáticos son los más difíciles (mejor AA `0,4375` en `MemOS`, y reconocimiento explícito de contradicción `CRS ≤ 0,2501`), y existe una brecha medible entre recuperar el recuerdo válido (`SEH@3`) y usarlo en la respuesta (`EUG` medio entre `0,0769` y `0,1527`).
- **`[VERIFICADO]` "When Does Memory Help?" / MERIT (`arXiv:2609.05441v1`, Mishra & Mishra, 14 págs.):**
  - **Volumen y contabilidad (§1, §4.4, §4.5):** `23.440` episodios puntuados por `\$42,57` en total (`9.940` ep. / `\$9,12` en el piloto de dos generaciones sobre `gpt-4.1-mini` + `13.500` ep. / `\$33,45` en la rejilla prerregistrada de 3 modelos × 3 semillas con `gpt-4.1-mini`, `GPT-4.1` y `Claude Haiku 4.5`).
  - **Colapso en hechos actualizados (§1, §5.4, §5.7, Tabla 2):** en el nivel `hard` (preferencias y hechos actualizados entre sesiones), la recuperación vectorial (`C2`, `text-embedding-3-small`) oscila entre **`0,30` y `0,95`** según modelo y semilla (brecha entre semillas de hasta `0,45`), mientras que los almacenes que actualizan al escribir (`C4` fact store estructurado y `C3` resumen LLM) se mantienen en **`0,70` a `1,00`**. La combinación híbrida (`C5 = C4 + C2`) rinde peor que `C4` solo (`0,50–0,80` vs `0,75–1,00` en el piloto) porque la búsqueda vectorial reintroduce hechos obsoletos, y cuando `C2` sí recupera el hecho actualizado, el agente lo ignora el **45 %** de las veces (*Ignore Rate* = `0,45` en generación real, `0,53` en starter).
- **`[VERIFICADO]` CaMeL (`arXiv:2503.18813v2`, Debenedetti et al., 125 págs.):**
  - **Aclaración de las dos cifras (`67 %` vs `77 % frente a 84 %`):** ambas son reales en el PDF primario, pero miden modelos y tablas distintas en AgentDojo. Con **`o3 High`** (Abstract y Tabla 2, utilidad benigna), CaMeL resuelve el **77 % (`77,3 % ± 8,3`)** frente al **84 % (`84,5 % ± 7,2`)** del sistema sin defensa (y `79,8 %` vs `79,0 %` bajo ataque, Tabla 3). Con **Claude 3.5 Sonnet** en la comparativa frente a otras defensas (Tablas 5 y 6), CaMeL obtiene **63,92 %** de utilidad benigna (frente a `90,72 %` sin defensa) y **66,70 % (~67 %)** de utilidad bajo ataque (frente a `86,83 %` sin defensa).
  - **Garantías y límites (§3.1, §6.2–6.5, §7, Tablas 4, 7 y 10–13):** con políticas de seguridad activas, CaMeL reduce a **0 de 949** los ataques exitosos dentro de su modelo de amenaza (las únicas excepciones en Tabla 4 son 1 tarea en `banking` cuando el propio documento pide ejecutar instrucciones cuyos datos requieren acción, y 1 tarea en `travel` donde `print()` muestra una reseña inyectada sin alterar flujo de control ni de datos, ambas declaradas fuera de alcance en §3.1 y §6.2.1), con un coste mediano de `2,73×` tokens de entrada y `2,82×` de salida en benigno (`2,02×` / `2,64×` bajo ataque) y vulnerabilidad residual a canales laterales de excepción/tiempo mitigada parcialmente por el modo `STRICT` (§7).

### 7.2. Qué sigue sin verificar (`[NO]` / pendiente)

- **`[NO]`** Los PDF completos de **RouteLLM** (`arXiv:2406.18665`), **ECon** (`arXiv:2410.04068`), **Hindsight**, **Model Cards** (Mitchell et al., 2019) y **Datasheets for Datasets** (Gebru et al.): en este documento siguen citados por abstract o fuente secundaria.
- **`[NO]`** Que **Decima-small** rinda como verificador NLI sobre nuestras 368 marcas de conflicto de SilvaDB: sigue siendo **[HIPÓTESIS]** a medir contra la muestra humana de 100 pares (§4 P2, §6).
- **`[NO]`** La licencia efectiva de los pesos de Decima-small tras haberse entrenado con datasets no comerciales (`ANLI` y `XNLI` bajo `CC-BY-NC-4.0`): riesgo legal residual abierto (ver `docs/internal/INVESTIGACION_decima_clef_hermes.md`).
