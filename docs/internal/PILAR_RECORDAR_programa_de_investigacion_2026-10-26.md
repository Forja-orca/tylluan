# Pilar "Recordar": programa de investigación

Estado: abierto el 2026-10-26 por decisión de José. Es una línea de investigación con método, no una feature. Decisión de convertirla en la columna vertebral de Tylluan: NO tomada, depende de los criterios de la sección 6.

## 1. Tesis de José (literal) y su corrección tras la primera búsqueda

> "Existen cientos de formas de generar memoria para agentes, pero ningún sistema actual ha trabajado recordar; los modelos no saben recordar ni cuándo deben recordar."

La primera búsqueda (sección 3) la matiza. Qué es cierto y qué no, con las fuentes tal como las devolvió la búsqueda (solo extractos; ninguna verificada en el artículo completo):

- Falso como está escrito: "cuándo recuperar" es un campo de investigación activo (FLARE, Self-RAG, Adaptive-RAG; en 2026 ProactRL, MemCon, Remember When It Matters, TriggerBench).
- Falso como está escrito: la inyección automática ya la hacen productos (claude-mem, Supermemory, Mem0, Hindsight, codemem) con los hooks oficiales de cada harness.
- Probablemente cierto y es el hueco: nadie ofrece, de forma neutral al harness, soberana y local, una política que decida CUÁNDO y QUÉ recordar y que demuestre con una medición propia que mejora al agente. Los trabajos académicos evalúan en SciWorld/AlfWorld, no en agentes de código reales; los productos inyectan por heurística (al inicio de sesión o antes de cada prompt), no deciden. Esta frase es una hipótesis (H1), no un hecho.

## 2. Qué medimos nosotros hoy (línea base propia)

- T-DSH-95: 40 preguntas con respuesta conocida. Recall@5 60,0 % por REST, 45,0 % por `tylluan_recall` con el filtro de coherencia. Causas de fallo: 25 % hechos nunca guardados, 15 % ranking, 15 % penalizado fuera del top 5, 7,5 % eliminado por el filtro.
- T-DSH-98: bajar la penalización del filtro a la mitad empeora el Recall@5 (27,5 % a 17,5 % en el subconjunto de prueba). T-DSH-99: ningún umbral absoluto lo mejora. Conclusión: el cuello de botella no es el almacén ni el filtro; es qué se recupera y cuándo.
- Harness de evaluación reproducible: kernel `test-run-*` + 40 preguntas + A/B (informes IMPL_T-DSH-95/98/99).

## 3. Lo que sabemos del estado del arte (primera pasada, 2026-10-26, solo extractos)

| Tema | Hallazgo | Fuente | Verificado |
|---|---|---|---|
| Cuándo recuperar (RAG) | FLARE dispara por baja confianza al generar; Self-RAG decide dentro del modelo pero exige reentrenar y no sirve para modelos cerrados; Adaptive-RAG usa un clasificador externo | https://arxiv.org/pdf/2312.10997 , https://www.arxiv-vanity.com/papers/2305.06983 | no (secundarias) |
| Benchmark de 35 métodos adaptativos (2025) | Los estimadores de incertidumbre rinden igual que pipelines complejos y son más baratos | https://arxiv.org/html/2501.12835v1 | no |
| Recuperación proactiva como política | ProactRL aprende cuándo y qué recuperar; reporta 73,50 % en SciWorld y 71,28 % en AlfWorld con menos recuperaciones | https://arxiv.org/html/2604.20572v1 | no |
| Memoria como proceso controlado | MemCon: MDP con bandido contextual, sin llamada extra a un LLM | https://arxiv.org/html/2607.13591 | no |
| Agente de memoria al lado | "Remember When It Matters": un agente de memoria junto a un agente de acción sin modificar decide inyectar un recordatorio o callar | https://arxiv.org/abs/2607.08716 | no |
| Memoria prospectiva | TriggerBench | https://arxiv.org/pdf/2606.23459 | no |
| Evaluación de memoria | MemoryAgentBench (4 competencias: recuperación exacta, aprendizaje en test, comprensión de largo alcance, olvido selectivo); evalúa la recuperación como capacidad, no el momento | https://arxiv.org/abs/2507.05257v2 | no |
| Hooks de inyección | Claude Code (UserPromptSubmit / additionalContext), Codex (hooks desde ~v0.116; UserPromptSubmit, PostCompact), Gemini CLI (BeforeAgent additionalContext) | https://geminicli.com/docs/hooks/writing-hooks , https://lilting.ch/en/articles/gemini-cli-hooks-research | no (terceros) |
| Riesgo de entrega | Reportes de que additionalContext se descarta en silencio en la extensión de VS Code | https://github.com/anthropics/claude-code/issues/49063 | no |
| Productos existentes | claude-mem (hooks, multi-IDE), Supermemory (Claude Code, Codex, OpenCode, Hermes), Mem0 (plugin vía MCP), Hindsight, codemem | https://supermemory.ai/docs/integrations/claude-code.md , https://docs.mem0.ai/integrations/claude-code , https://docs.claude-mem.ai/gemini-cli/setup | no |

Lagunas de la búsqueda: no se encontró nada sobre inyección por proxy de base URL; no se leyó ningún artículo completo; no se buscó Z Code ni el harness de DeepSeek.

## 4. Preguntas de investigación (RQ) e hipótesis falsables

- RQ1 CUÁNDO recordar. H1: una política que decide cuándo recuperar (disparadores: antes de actuar, laguna detectada, cambio de tarea, tras compactar) iguala o mejora el Recall@5 de recuperar siempre, con menos tokens inyectados. Se refuta si no mejora en 2 de 3 métricas (cobertura, tokens, latencia) frente a "inyectar siempre".
- RQ2 QUÉ traer. H2: ponderar por confianza y por utilidad pasada mejora la precisión de lo inyectado frente a similitud sola. Se refuta si la precisión@3 no sube 5 puntos.
- RQ3 CUÁNDO guardar. H3: guardar al cierre de ciclo (decisiones, correcciones, entregas aceptadas) reduce el 25 % de fallos por "nunca guardado". Prototipo: `tests/cycle_closeout_memory.rs`. Se refuta si el porcentaje no baja a la mitad.
- RQ4 CÓMO ENTRA en cada harness sin hackearlo. H4: los hooks oficiales y el campo `instructions` de MCP bastan para los harnesses con hooks; para el resto hace falta una puerta de entrada (sección 5). Se refuta si en algún harness objetivo no existe ningún canal oficial.
- RQ5 CÓMO MEDIR. H5: se puede medir la utilidad real de lo inyectado (el agente lo usó o lo ignoró). Se refuta si la atribución no es estable entre dos ejecuciones.

## 5. Escalera de integración (sin tocar el IDE del cliente)

- Nivel 0, MCP (hoy): el modelo decide si llama a `tylluan_recall`. Es justo lo que falla: depende de que el modelo sepa cuándo recordar.
- Nivel 1, plugin por harness con los hooks oficiales: Tylluan observa el turno, decide y devuelve contexto. Es el patrón "agente de memoria al lado". Donde el harness lo permite (Claude Code, Codex, Gemini CLI) es el camino principal. El mod `tylluan-map` ya lo hace en esta sesión (`prompt.context`) y demuestra que funciona.
- Nivel 2, puerta de entrada local entre el harness y el modelo (cambio de base URL, configurado por el usuario): sirve a cualquier harness sin hooks; hay que investigarlo (laguna de la sección 3). Encaja con el plan de Inference Gateway (ADR-020/021).
- Descartado: modificar o espiar el IDE del cliente sin que el usuario lo configure. No hace falta, rompe la confianza y contradice la premisa de fortaleza, no de jaula.

## 6. Criterio de decisión (go / no-go) para ser la columna vertebral

GO si, medido en nuestro harness (40 preguntas más una tarea real de agente de código), la política automática mejora el Recall@5 efectivo al menos 15 puntos sobre el estado actual con coste de tokens no mayor, y funciona en al menos 3 harnesses distintos sin tocar su código. NO-GO documentado si no. Un NO-GO con dato es un cierre válido.

## 7. Plan de trabajo (cada ítem: informe con `## Impact` y "no verificado")

1. Revisión bibliográfica profunda: leer completos los artículos de la sección 3 y ampliar (Qwen, búsqueda web profunda, vía José; cola `COLA_*`).
2. Mapa de canales oficiales por harness: Claude Code, Codex, Gemini CLI, OpenCode, Hermes, Z Code, DeepSeek harness (qué hook o campo existe, qué versión, si el contexto realmente llega al modelo).
3. Prototipo de política "agente de memoria al lado" en el kernel, detrás de un flag desactivado por defecto.
4. Extender el harness de evaluación a una tarea real y a "inyectar siempre" como línea de comparación.
5. Medición de utilidad de lo inyectado (RQ5).
