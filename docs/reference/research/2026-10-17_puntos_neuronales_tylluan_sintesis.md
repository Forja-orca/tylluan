# Puntos neuronales de Tylluan: literatura, simulación de encaje y síntesis (2026-10-17)

**Autor:** Claude Code (TL). **Encargo de José:** investigar como ingeniero de sistemas y científico qué tipo de modelo y en qué puntos de Tylluan mejoraría el sistema; al menos 3 propuestas por punto; simular cómo encajan en las costuras reales; sintetizar lo aprendido en ciclos de investigación y práctica.

**Etiquetas de evidencia** (cada afirmación lleva una):
- **[CÓDIGO]** leído hoy en el repositorio.
- **[MEDIDO]** medido hoy sobre el kernel o los datasets del repositorio.
- **[FUENTE]** leído en la fuente primaria, pero **solo el resumen (abstract)** salvo que se diga otra cosa; los PDF completos no se han leído.
- **[SECUNDARIO]** resultado de buscador, blog o resumen de terceros.
- **[HIPÓTESIS]** razonamiento propio sin medir.

## 1. Cómo preparan y documentan sus proyectos los equipos de referencia

| Práctica | Qué es | Evidencia | Qué tomamos |
|---|---|---|---|
| Model Cards (Mitchell et al., 2019) | Ficha por modelo: uso previsto y fuera de alcance, datos de evaluación, métricas, rendimiento por condiciones, consideraciones éticas | [SECUNDARIO] | Una "Component Card" por componente aprendido de Tylluan |
| Datasheets for Datasets (Gebru et al.) y Data Cards (Google) | Ficha por dataset: origen, composición, sesgos, usos | [SECUNDARIO] | Ficha del dataset de evaluación, con su verdad de campo y cómo se etiquetó |
| RouteLLM (Ong et al., ICLR 2025) | Router aprendido con datos de preferencia entre un LLM fuerte y uno débil; define métricas propias, CPT y APGR, para medir calidad frente a coste | [FUENTE] + [SECUNDARIO] | Medir siempre calidad frente a coste, con una métrica de "calidad recuperada" |
| "When Does Memory Help?" (arXiv 2609.05441) | Evalúa estrategias de memoria con contabilidad completa de tokens y dólares sobre 23.440 episodios | [FUENTE] | Contabilidad de coste en cada medición; el valor de la memoria se mide en acciones, no solo en recall |
| MemConflict (arXiv 2605.20926) | Taxonomía de conflictos de memoria: **dinámicos** (validez temporal), **estáticos** (corrección de hechos) y **condicionales** (aplicabilidad al contexto) | [FUENTE] | Clasificar el conflicto antes de decidir quién lo resuelve |
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
| **2A. Verificador NLI sobre los candidatos estructurales** | La regla actual propone pares; un clasificador de contradicción/implicación/neutro los verifica (promediando ambos órdenes) y **solo puede podar falsos positivos** | Tamaño ~100M a 400M de parámetros; 368 candidatos a la hora; no hay presión de latencia. Encaja en `flag_contradiction_nodes` | [FUENTE] MELD (arXiv 2608.16357) usa una señal NLI junto a identidad de clave de afirmación y similitud de embedding; reporta AUC 0,968 y tasa de fusión falsa 0,013 sobre pares adjudicados. No se leyó el PDF: modelo, umbrales y límites sin verificar. [FUENTE] ECon (arXiv 2410.04068): promediar ambos órdenes porque el NLI depende del orden |
| **2B. Modelo System One como clasificador de relación** | Pregunta `choice` con tres opciones (contradice, consistente, no relacionado) usando el servicio que ya tenemos | Reutiliza todo ADR-021. **Hipótesis a medir:** Decima-small se entrenó con ANLI, XNLI y MNLI, es decir, su distribución de entrenamiento **es** NLI | [CÓDIGO]/[FUENTE] licencias y datos en la investigación verificada de 2026-10-17; [HIPÓTESIS] que rinda como un NLI dedicado |
| **2C. Clasificar el tipo de conflicto antes de resolver** | Taxonomía dinámico, estático o condicional; los dinámicos se resuelven con `valid_from` (ya existe), sin modelo | Cero modelos; extiende la supersesión temporal M35 | [FUENTE] MemConflict |
| **2D. Síntesis generativa con compuerta de coherencia** | Qwen3-0.6B o similar reescribe la reconciliación; la compuerta BGE (umbral 0,85) descarta alucinaciones | 570 MB, no instalado; mejora de calidad, no corrección de un fallo | [CÓDIGO] ADR-010 §7.3 |

Simulación de encaje: `flag_contradiction_nodes` sigue generando candidatos. Para cada par nuevo se invoca `decide(contract="contradiction_verify", question relation:choice[contradicts, consistent, unrelated])` en sombra. Con confianza alta en `consistent`, el verificador **sugiere** quitar la marca (nunca la pone). Tras la medición pareada contra una muestra etiquetada a mano, se promueve a `advisory` y después a `gate` solo para podar. El invariante v8 se cumple: el modelo nunca anula un rechazo determinista ni crea conflictos.

**Recomendación:** 2C ahora (coste cero), 2B y 2A como duelo medido sobre las 368 marcas actuales, con etiquetado humano de una muestra de 100. El resultado decide qué proveedor entra.

### P3. Escritura de memoria

| Opción | Descripción | Evidencia |
|---|---|---|
| **3A. Reconciliación al escribir** (supersedir en vez de acumular) | Al guardar un hecho nuevo, detectar el hecho anterior sobre la misma clave y cerrar su `valid_until` | [FUENTE] "When Does Memory Help?": en hechos actualizados, la recuperación por embeddings es inestable (0,30 a 0,95 según modelo) mientras que las memorias que actualizan al escribir se mantienen en 0,70 a 1,00 |
| **3B. Puerta de importancia aprendida** | Cabeza pequeña sobre señales ya registradas (acceso, feedback de recall, saliencia) para decidir qué consolidar | [SECUNDARIO] Hindsight: la consolidación tiene cuatro palancas (importancia, fusión, decaimiento, expulsión) y casi nadie documenta su política |
| **3C. Pregunta booleana "merece guardarse" a un modelo System One** | Reutiliza el servicio | [HIPÓTESIS]; sin verdad de campo ni caso de uso medido |

Encaje: 3A reutiliza el verificador de P2 en el camino de escritura. 3B se entrena de noche (NightConsolidation) con los pares que ya produce el Signal Loop. **Recomendación:** 3A primero porque ataca el modo de fallo que la literatura señala; 3C se descarta hasta tener verdad.

### P4. Contenido no confiable

| Opción | Descripción | Evidencia |
|---|---|---|
| **4A. Estructural: procedencia y taint** | El contenido de herramientas y de otros agentes lleva su procedencia; mientras esté "tainted", no puede disparar herramientas privilegiadas. Se apoya en `CallerTrustTier` y `PrivacyLevel` de TaskContext | [SECUNDARIO] CaMeL (arXiv 2503.18813): garantías por diseño, resuelve el 67 % de las tareas de AgentDojo (resumen de terceros; otro resumen habla de 77 % frente a 84 % sin defensa), con coste en utilidad |
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

## 7. Qué no está verificado

- Los PDF completos de MELD, MemConflict, "When Does Memory Help?" y CaMeL; solo se leyeron resúmenes y reseñas de terceros. Modelo NLI, umbrales y límites de MELD sin confirmar.
- Las cifras de CaMeL (67 % y 77 % frente a 84 %) proceden de resúmenes secundarios.
- Que Decima-small rinda en NLI: hipótesis a medir.
- La licencia efectiva de los pesos tras entrenar con datos NC (riesgo residual, ver la investigación verificada del 2026-10-17).
