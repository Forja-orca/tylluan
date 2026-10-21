# Las preguntas correctas de Tylluan y el camino a v1.0 — 2026-10-21

**Autor:** Claude (tech lead). **Encargo:** José, 2026-10-21, "qué preguntas debemos hacernos, con paso a paso y respuestas documentadas, como camino a la finalización".
**Estado:** borrador del TL para decisión de José. No vinculante hasta que José lo apruebe punto por punto.

## 0. Límites honestos de este documento

- **No leí "cientos de conversaciones".** No tengo acceso a ellas. Lo que sí leí: la memoria persistente (tesis, visión, decisiones, incidentes), el repo (código, ADR, STATUS, ROADMAP, informes de la flota) y este historial de sesión. Si una conclusión depende de algo dicho y no escrito, no la uso.
- **Niveles de evidencia.** `[REPO]` lo comprobé con un comando en este árbol. `[FUENTE]` lo leí en una fuente primaria durante esta sesión (URL y cita literal). `[MEMORIA]` viene de una nota guardada y no lo revalidé hoy. `[NO VERIFICADO]` lo digo como hipótesis.
- **No existe una respuesta "al 100%".** Lo máximo honesto es: cada pregunta lleva su respuesta provisional, cómo comprobarla con un número y quién decide. Donde la respuesta es "no lo sabemos", lo digo.
- **Los benchmarks de Zep y Mem0 los publican sus propios autores.** Los cito como lo que son: afirmaciones de parte, no replicaciones independientes.
- **No es asesoría legal.** La parte legal es un mapa de preguntas para un jurista, no un dictamen.

## 1. Cómo leo la tesis (para saber qué medir)

Tesis de José `[MEMORIA]`: "Tylluan es la mitad local, persistente, federada y soberana que falta a los modelos LLM para convertirse en agentes completos". Criterio de éxito dicho por él: que un agente pueda decir con verdad "yo continúo" tras reiniciar y reconectar, y que funcione en el caso ancla: un médico, una Raspberry Pi de 16 GB, sin internet fiable.

Traducción operativa: Tylluan **no se mide por cuántas funciones tiene**, sino por tres propiedades comprobables: **continuidad** (el agente retoma su estado), **soberanía** (funciona y se puede abandonar sin perder los datos) y **ligereza** (en hardware modesto). Todas las preguntas de abajo salen de comprobar esas tres.

## 2. Diagnóstico: qué preguntas hemos hecho bien y cuáles no

- **Hemos preguntado mucho "¿funciona el módulo X?"** (1239 tests, 18 ADR en archivo, ciclos de auditoría) y **poco "¿el agente continúa y mejora?"**. La excepción es la nota de julio sobre dogfooding `[MEMORIA]`: el diagnóstico fue "se construye por hitos, no se vive". La cifra que la acompañaba (`recall_feedback` en 0 de 5000 filas) es de julio y no la revalidé.
- **Hemos abierto más ramas de las que hemos cerrado** (Decision Fabric con cuatro variantes, runtimes, mallas). José lo dijo el 2026-10-05 `[MEMORIA]`: un NO-GO con dato es un cierre válido. Este documento aplica esa regla: cada pregunta termina en cierre con número, no en una rama nueva.
- **Hemos medido la corrección interna mejor que el valor externo.** No tenemos todavía una sola medición pública de "agente con Tylluan frente a agente sin Tylluan".

## 3. Las preguntas, en orden de dependencia

### Q1. ¿Qué significa "el agente continúa" y cómo se mide?

- **Por qué importa:** es la tesis. Hoy se afirma, no se mide.
- **Evidencia externa [FUENTE]:** LongMemEval (Wu et al., ICLR 2025, arXiv 2410.10813, https://arxiv.org/abs/2410.10813) define cinco capacidades de memoria a largo plazo (extraer información, razonar entre sesiones, razonar sobre el tiempo, actualizar conocimiento y abstenerse) y reporta que asistentes comerciales y LLM de contexto largo muestran "a 30% accuracy drop on memorizing information across sustained interactions". Generative Agents (Park et al., arXiv 2304.03442, https://arxiv.org/abs/2304.03442): memoria, reflexión y planificación "each contribute critically to the believability of agent behavior" (ablación).
- **Respuesta provisional (confianza media-alta en el método, desconocida en el resultado):** la "continuidad" se descompone en esas cinco capacidades más tres pruebas propias que ningún benchmark cubre: (a) identidad tras reinicio, (b) retomar una tarea abierta, (c) conocer fecha y hora reales. José pidió razonar en "modo frontera": el marco ya existe, falta ejecutarlo.
- **Paso a paso:**
  1. Escribir la especificación del Test de Continuidad (8 sondas, respuesta esperada por sondas, criterio de acierto sin juez LLM cuando sea posible).
  2. Ejecutar LongMemEval en su variante pequeña sobre Tylluan con un modelo local fijado (versión y cuantización registradas).
  3. **Evaluación humana por muestreo**, no solo LLM-como-juez: José prefiere evaluación manual `[MEMORIA]`; el benchmark usa juez automático por defecto, así que se audita a mano una muestra fija.
  4. Publicar números, intervalo de confianza y los fallos, aunque sean malos.
- **Criterio de cierre:** un script reproducible que cualquiera ejecute y que imprima los números; sin él no se afirma continuidad.
- **Decide José:** qué modelo local es el de referencia y quién audita a mano.

### Q2. ¿Funciona de verdad en el caso ancla (Raspberry Pi, sin internet)?

- **Evidencia [REPO]:** el README dice textualmente que se midió en x86 (WSL2) con 34 MB de memoria residente en reposo y que "has not been run on ARM or on physical Raspberry Pi hardware; CI compiles the ARM64 build but does not boot it". Notas antiguas de la memoria decían "validado en Raspberry Pi 4 con 12.000 memorias" `[MEMORIA]`: **no coincide con lo que hoy afirma el propio README**. Hay que creer al README, que es más prudente, hasta que se mida.
- **Evidencia externa [FUENTE]:** Local-first software (Kleppmann et al., 2019, https://www.inkandswitch.com/essay/local-first/) fija siete ideales; los dos que más pesan aquí son "The network is optional" y "You retain ultimate ownership and control".
- **Respuesta provisional:** **hoy esta es la afirmación central sin demostrar.** Puede que se cumpla; lo que no se puede es decirlo.
- **Paso a paso:**
  1. Conseguir una Pi 4/5 de 16 GB (o la placa ARM64 disponible) y registrar el modelo exacto.
  2. Arrancar perfil `portable` y `clinic` con base de 12.000 memorias; medir arranque en frío, memoria residente, latencia de `tylluan_recall` (mediana y p95 en 100 consultas) y que no haya tráfico saliente.
  3. Probar la pila de modelos pequeños (CoherenceGate, etc.) solo en CPU: si no corre, el caso ancla no está cubierto y hay que decirlo.
  4. Mientras no haya hardware: emulación aarch64 sirve para **funcionalidad**, nunca para rendimiento; se marca así.
- **Criterio de cierre:** tabla de la placa real, o retirar la afirmación "Raspberry Pi" de la documentación pública.
- **Decide José:** si dispone del hardware o se compra.

### Q3. ¿La memoria mejora al agente frente a no tenerla?

- **Evidencia [FUENTE]:** Zep (Rasmussen et al., arXiv 2501.13956, https://arxiv.org/abs/2501.13956) afirma mejoras "of up to 18.5%" en LongMemEval y una reducción de latencia del 90 %. Mem0 (Chhikara et al., arXiv 2504.19413, https://arxiv.org/abs/2504.19413) afirma "26% relative improvements" sobre OpenAI en LOCOMO. **Ambas las firman los autores del producto.**
- **Evidencia [MEMORIA]:** la investigación sobre modelos de decisión concluyó que nadie prueba mejora general con diseño pareado; hallazgo coherente con este.
- **Respuesta provisional:** no hay medición propia. Sin ella, cualquier "mejora" es retórica.
- **Paso a paso:** (1) mismo modelo, mismas tareas, con y sin Tylluan, orden aleatorio; (2) n suficiente para un intervalo útil (≥200 ítems); (3) prueba pareada (McNemar) y resultado con p y tamaño del efecto; (4) incluir un caso de **peor resultado**: que Tylluan empeore por ruido de recuperación.
- **Criterio de cierre:** una tabla "con / sin" con su intervalo; si el efecto no es positivo, se publica igualmente y se arregla la recuperación antes de añadir nada.

### Q4. ¿Qué es genuinamente propio y qué es convergencia con el sector?

- **Evidencia [REPO]:** validez temporal existe (`valid_from` y `valid_until` en nodos y aristas; lógica de sustitución en la consolidación nocturna); reranker presente; recuperación híbrida; FSRS para saliencia; CoherenceGate; Signal Loop; procedencia federada; contrato de 6 herramientas.
- **Evidencia [FUENTE]:** MemGPT (Packer et al., arXiv 2310.08560, https://arxiv.org/abs/2310.08560) propone gestión de contexto virtual inspirada en jerarquías de memoria de sistemas operativos; Zep usa un grafo de conocimiento "temporally-aware". Es decir, **el sector converge en lo que Tylluan ya hace**.
- **Respuesta provisional:** la diferencia real no es ningún algoritmo suelto, es la **combinación**: un binario local, soberano, pensado para hardware modesto, con coordinación de flota (Coloquio, contratos de trabajo acotados) y defensa del propio envenenamiento. Un análisis externo reciente afirmó que Tylluan "no tiene validez temporal" y que "perdió el reranker": ambas cosas son **falsas en este árbol**. Por eso los informes externos se verifican antes de actuar.
- **Paso a paso:** (1) una página "qué afirmamos y qué no", con una fila por capacidad y su comando de verificación; (2) retirar de la documentación pública cualquier frase que no tenga fila.
- **Criterio de cierre:** cada afirmación pública enlaza a un test o a una medición.

### Q5. ¿Cómo se defiende la memoria del envenenamiento sin convertirla en una jaula?

- **Evidencia [FUENTE]:** AgentPoison (Chen et al., arXiv 2407.12784, https://arxiv.org/abs/2407.12784) envenena la memoria de largo plazo o la base de conocimiento con una tasa de éxito media superior al 80 % y un impacto menor del 1 % en el rendimiento benigno. PoisonedRAG (Zou et al., USENIX Security 2025, arXiv 2402.07867, https://arxiv.org/abs/2402.07867): "a 90% attack success rate when injecting five malicious texts". Conclusión útil: **no se detecta por degradación de calidad**; un puñado de entradas basta.
- **Evidencia [REPO]:** defensa en capas ya construida (capa 1 de patrones, capa 2 con juez, marca `untrusted_content` con banner en bootstrap, `@inbox` y `tylluan_think`, herencia retroactiva a resúmenes, cuarentena), con matrices adversariales de DSH (T-DSH-10 a T-DSH-18) que dejan abiertos B2, el X1 de identidad declarativa y la decisión de la spec de D-13.
- **Respuesta provisional:** el enfoque "marcar, mostrar y degradar la confianza" encaja con la filosofía de José (fortaleza, no jaula). El hueco estructural **no está en la detección sino en la identidad**: mientras `agent_id` sea un texto declarado, "otro agente" no es una categoría autenticable.
- **Paso a paso:** (1) decidir el binding token a `agent_id` (Q6); (2) aplicarlo a todas las superficies de escritura, también REST (hoy el buzón y Coloquio aceptan autor libre); (3) quitar los `#[ignore]` de X1 y B2; (4) red-team tipo AgentPoison propio: inyectar k entradas envenenadas y medir cuántas llegan al contexto sin banner.
- **Criterio de cierre:** 0 de k entradas sin banner en la matriz repetible y las dos pruebas ignoradas en verde.
- **Decide José:** el binding y su calendario (no se decide sin él).

### Q6. ¿Cómo prueba un agente "quién es" sin una autoridad central?

- **Evidencia [REPO]:** la federación ya firma con Ed25519 por nodo; para agentes locales la identidad se auto-crea al primer contacto y la localidad se decide por "existe una identidad en este kernel" (hallazgo F3 de T-DSH-10); hay además un caso abierto de identidades duplicadas por mayúsculas (`deep` frente a `Deep`).
- **Respuesta provisional:** identidad = clave + contrato (roster), no existencia de nodo. Extender la clave por nodo ya existente a clave por agente, y canonicalizar `agent_id` en un único punto de entrada.
- **Paso a paso:** ADR de identidad (roster más binding, marcar por defecto, no rechazar en el buzón, según la propuesta de DSH en T-DSH-12); canonicalización; migración de las dos identidades duplicadas (**migración de datos: la decide José**).
- **Criterio de cierre:** una prueba que demuestre que registrar una identidad no concede localidad.

### Q7. ¿Qué red necesitamos de verdad?

- **Evidencia [MEMORIA]:** José descartó la malla global el 2026-10-13; el objetivo son redes por equipo con emparejamiento simple y muy seguro (escenarios A, B, C). **[REPO]:** el descubrimiento por DHT mainline es una función opcional apagada por defecto en `tylluan-link`. Un análisis externo lo calificó de "YAGNI"; coincide con la decisión de José, no la cambia.
- **Respuesta provisional:** conservar lo que sirve a redes de equipo (cifrado por par, identidad, despacho); decidir con un ADR el destino del descubrimiento global y del chismorreo (gossip) antes de tocarlos. **Nada se borra sin ese ADR** (regla ya fijada).
- **Paso a paso:** (1) test de extremo a extremo de los tres escenarios; (2) inventario de qué módulos de `tylluan-link` los usan; (3) ADR con tres opciones medidas (mantener, congelar tras una función, eliminar).
- **Criterio de cierre:** los tres escenarios pasan en CI y el ADR está aprobado.
- **Decide José:** el ADR.

### Q8. ¿Qué parte es binario único y qué parte depende de Python?

- **Evidencia [REPO]:** el README dice que el kernel funciona sin Python para memoria, pero que correr las 45 guilds exige Python 3.12 y FastMCP, y que sin ellos "crash-loop" y el doctor reporta `degraded`. Este documento no encuentra incoherencia en eso: es una promesa por niveles. La incoherencia sería **no decirlo**.
- **Evidencia [FUENTE]:** "Small Language Models are the Future of Agentic AI" (Belcak et al., NVIDIA, arXiv 2506.02153, https://arxiv.org/abs/2506.02153) argumenta que los modelos pequeños son "sufficiently powerful, inherently more suitable, and necessarily more economical" para muchas invocaciones agénticas. Es una tesis de posición, no un resultado propio.
- **Respuesta provisional:** formalizar los niveles: **Nivel 0** binario (memoria, 6 herramientas); **Nivel 1** guilds opcionales. Migrar una guild a Rust solo si la telemetría de uso lo justifica, no por pureza.
- **Paso a paso:** (1) el doctor imprime el nivel alcanzado; (2) medir arranque con y sin guilds; (3) contar llamadas reales por guild durante 30 días; (4) priorizar solo las que acumulen uso.
- **Criterio de cierre:** una tabla de uso real; ninguna migración sin esa tabla.

### Q9. ¿La sociedad de modelos pequeños aporta valor medido?

- **Evidencia [REPO]/[MEMORIA]:** la Fase 0 de la sociedad SLM dio NO-GO con datos (n=52: baseline 48,1 %, Self-MoA 50,0 %, deliberativo 48,1 %; diferencia −1,92 puntos frente al umbral de +4). En Decision Fabric: Laya 0,351 frente a 0,531 de las reglas (NO-GO), Clef-Flash NO-GO, Decima-small "GO provisional" (5 aciertos netos en 245 casos, **sin prueba pareada**). STATUS cita arXiv 2606.07810 (SLMJury); **no lo abrí hoy**, queda como `[NO VERIFICADO]`.
- **Respuesta provisional:** hasta ahora **las reglas deterministas ganan o empatan**. La visión de José (modelos pequeños generativos dentro del flujo) sigue siendo legítima, pero lo medido no la apoya todavía.
- **Paso a paso:** (1) prueba pareada (McNemar) de Decima contra las reglas sobre los mismos 245 casos; (2) criterio previo: ≥ +4 puntos con p < 0,05 o NO-GO; (3) cerrar el hilo con ese resultado y no abrir un quinto modelo.
- **Criterio de cierre:** una sola prueba pareada decide; el hilo termina.
- **Decide José:** aceptar el NO-GO si sale negativo.

### Q10. ¿Sobrevive el proyecto a su único humano?

- **Evidencia [FUENTE]:** Avelino et al. (arXiv 1604.06766, https://arxiv.org/abs/1604.06766) definen el factor camión como el mínimo de desarrolladores que, al irse, dejan el proyecto "incapacitated"; hallan que el 65 % de los sistemas estudiados tiene factor ≤ 2.
- **Evidencia [MEMORIA]/[REPO]:** una evaluación externa de octubre puntuó la ausencia de comunidad (1 estrella, 0 forks) como riesgo número uno y "si el desarrollador se retira, el proyecto muere" (cifras de octubre, no revalidadas hoy); el ROADMAP de v0.2 sigue con "First external contributor PR merged" sin marcar.
- **Respuesta provisional:** es el riesgo real más grande y no es técnico. Un matiz honesto: parte del código la escriben agentes, así que el factor camión de **personas** es 1, pero el de **conocimiento escrito** es mejor que el habitual gracias a ADR, informes y tests.
- **Paso a paso:** (1) medir el factor camión con el mismo algoritmo; (2) un `CONTRIBUTING` que lleve a un contribuidor en frío a su primer PR en 30 minutos (el quickstart ya se auditó); (3) 10 issues etiquetados como primeros pasos; (4) documento de continuidad: dónde están las claves, quién firma releases, qué hacer si José no está.
- **Criterio de cierre:** un PR externo fusionado y factor camión medido ≥ 2.
- **Decide José:** gobernanza y firma de releases (política de PR/firma: **suya**).

### Q11. ¿Es legal, ético y honesto lo que hacemos?

Mapa de preguntas para un jurista; no se resuelven aquí.

- **Licencia [REPO]:** el repositorio es **MIT** (archivo LICENSE y `Cargo.toml`). Documentos antiguos del proyecto hermano mencionan AGPL. La pregunta real: ¿qué licencia garantiza "libre para todos" en el sentido de José? MIT permite que un tercero lo cierre y lo venda sin devolver nada; AGPL obliga a ofrecer el código de versiones que se sirvan por red. **Decide José, con consecuencias para contribuidores.**
- **Datos de entrenamiento [FUENTE]:** la tarjeta de `decima-small` publica el modelo bajo Apache-2.0 y declara que se entrenó con datos que incluyen "non-commercial (CC BY-NC 4.0: ANLI, XNLI, multilingual-NLI-26lang)" (https://huggingface.co/amyrmahdy/decima-small/raw/main/README.md). Riesgo para uso comercial; ya documentado.
- **Derecho de supresión [FUENTE]:** RGPD art. 17(1): "The data subject shall have the right to obtain from the controller the erasure of personal data concerning him or her" (https://gdpr.eu/article-17-right-to-be-forgotten/), con excepciones en el 17(3). **[REPO]** hay borrado de nodo y `forget_agent`. Tensión no resuelta: el registro de auditoría encadenado por hash **no se puede editar sin romper la cadena**; la solución habitual es que el audit guarde referencias seudónimas y nunca contenido personal. Verificar que borrar un nodo limpia también el índice de texto y los vectores es una tarea concreta (hoy hay un 55 % de huérfanos en el índice FTS, decisión de reconstrucción de José).
- **Reglamento de IA de la UE [FUENTE]:** art. 2(12): "This Regulation does not apply to AI systems released under free and open-source licences", con tres excepciones: sistemas comercializados como de alto riesgo, prácticas prohibidas (art. 5) y obligaciones de transparencia (art. 50) (https://artificialintelligenceact.eu/article/2/). Consecuencia para el caso ancla: **Tylluan puede quedar exento, pero quien lo despliegue para uso clínico puede convertirlo en parte de un sistema de alto riesgo**. La documentación debe decir que no es un producto sanitario. La normativa de productos sanitarios no la he leído: `[NO VERIFICADO]`.
- **Ley de Ciberresiliencia [FUENTE parcial]:** entró en vigor el 10 de diciembre de 2024; las obligaciones principales se aplican desde el 11 de diciembre de 2027 y la notificación de vulnerabilidades explotadas, desde el 11 de septiembre de 2026 (https://digital-strategy.ec.europa.eu/en/policies/cyber-resilience-act). La página que abrí **no** desarrolla el régimen del software libre ni la figura de "open-source steward": `[NO VERIFICADO]`. Si ya circula la versión 0.17.1, hay que preguntar a un jurista si José es "fabricante" en el sentido de la ley.
- **Honestidad [REPO]:** el proyecto ya publica sus hallazgos negativos (NO-GO, auditorías propias). Esto es una ventaja que no hay que perder: toda cifra pública debe tener comando y fecha.
- **Paso a paso:** (1) José decide licencia; (2) registro legal de una página con las cinco áreas y su estado; (3) una prueba de supresión de extremo a extremo (insertar dato personal ficticio, borrarlo, comprobar nodos, FTS, vectores y audit); (4) aviso "no es producto sanitario" en README y quickstart; (5) consulta a un jurista antes de v1.0.
- **Criterio de cierre:** la prueba de supresión pasa y el registro legal está revisado por una persona cualificada.

### Q12. ¿La eficiencia es real o es una intención?

- **Evidencia [FUENTE]:** Green AI (Schwartz et al., arXiv 1907.10597, https://arxiv.org/abs/1907.10597) propone que la eficiencia sea "an evaluation criterion for research alongside accuracy and related measures" y que se reporte el coste.
- **Evidencia [REPO]:** la afirmación de memoria residente de 34 MB está medida (x86). No hay medición de energía.
- **Respuesta provisional:** la misión "ahorrar recursos" es legítima pero **no se puede afirmar un impacto agregado** (árboles, vatios) sin medir. Sí se puede medir el coste por operación.
- **Paso a paso:** (1) medir Wh por 1000 recuperaciones en cada perfil (portable, clinic, server) con el medidor de energía del sistema; (2) publicar la tabla junto a la exactitud; (3) no comparar con la nube salvo que haya una medida equivalente.
- **Criterio de cierre:** tabla de Wh por perfil con método y máquina.

### Q13. ¿Cuándo está terminado?

Propuesta de definición de v1.0, en la regla de José de cerrar antes de abrir:

1. Test de Continuidad publicado y reproducible (Q1).
2. Caso ancla demostrado en ARM real o afirmación retirada (Q2).
3. Medición con/sin Tylluan publicada (Q3).
4. Identidad con binding y X1/B2 cerrados (Q5, Q6).
5. OAuth fase 2 en producción y `dev_mode=false` (**decisión de José**).
6. Prueba de supresión de extremo a extremo y registro legal revisado (Q11).
7. Un contribuidor externo fusionado; factor camión ≥ 2 (Q10).
8. ADR de la red de equipo aprobado y los tres escenarios en CI (Q7).
9. Congelación de funciones: nada nuevo hasta que 1 a 8 estén cerrados.

## 4. Orden recomendado

1. **Q2 y Q11-supresión** primero: son las dos afirmaciones públicas que pueden ser falsas hoy.
2. **Q1 y Q3** a continuación: sin medición propia no hay argumento.
3. **Q5/Q6** en cuanto José decida el binding.
4. **Q9** se cierra con una sola prueba pareada.
5. **Q7, Q8, Q10, Q12** en paralelo con bajo coste.

## 5. Lo que este documento NO afirma

- No afirma que Tylluan "supere" a ningún competidor.
- No afirma conciencia ni "ser" en sentido filosófico: la tesis de José se trata como un conjunto de propiedades **comprobables** (continuidad, identidad, tiempo).
- No cita nada que no haya leído en esta sesión. Las fuentes que **no** pude leer: la lista oficial OWASP de riesgos agénticos (la página no la mostró), el detalle del régimen del software libre en la Ley de Ciberresiliencia, la normativa de productos sanitarios y el artículo SLMJury citado en STATUS.
