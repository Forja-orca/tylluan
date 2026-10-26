# Pilar "Recordar": síntesis de los tres informes de investigación profunda (2026-10-26)

Entradas: informe A y informe B (pegados por José en el chat, agente de investigación externo) e informe C (fichero `GO CONDICIONADO_...md` en Descargas). Los tres concluyen GO CONDICIONADO. Esta nota separa lo que el TL verificó abriendo la fuente de lo que sigue sin verificar. Sin cambios de código.

## 1. Verificado por el TL (fuente abierta hoy)

| Afirmación de los informes | Resultado | Fuente |
|---|---|---|
| "Remember When It Matters": agente de memoria al lado, inyecta o calla; +8,3 puntos en Terminal-Bench 2.0 y +6,8 en tau2-Bench; supera a inyección siempre activa, banco pasivo, solo consejos y recuperación general. | CONFIRMADO en el resumen. Autores: Wu, Zhang, Zhou, Wang, Peng, Li, Fan, Zhao; enviado el 9 julio 2026. Que sea "de Meta" NO está en la página: no verificado. Además entrena un modelo abierto de 27B con SFT y GRPO. | https://arxiv.org/abs/2607.08716 |
| REALM: reconsolidación disparada por la recuperación; 75,97 por ciento LoCoMo (+7,17) y 65,11 por ciento LongMemEval (+1,31). | CONFIRMADO en el resumen; solo QA conversacional, no código; mecanismo exacto no leído. | https://arxiv.org/abs/2609.16053 |
| InjecMEM: una sola interacción, sin acceso de lectura ni escritura al almacén; COLM 2026. | CONFIRMADO. Las cifras RSR 46,5 y ASR 76,6 del informe A NO aparecen en el resumen: no verificadas. | https://arxiv.org/abs/2608.23471 |
| Copilot descarta `additionalContext` en `userPromptSubmitted` y `postToolUse`. | PARCIALMENTE OBSOLETO: el informe B lo da como riesgo alto, pero el problema fue cerrado el 16 sep 2026 como resuelto desde la v1.0.85 de Copilot CLI. Hay que reverificar en esa versión. | https://github.com/github/copilot-cli/issues/2652 |

## 2. Dónde coinciden los tres informes (alta confianza de dirección, no de cifras)

1. GO condicionado a un prototipo; NO a "columna vertebral" hasta medirlo.
2. El criterio de 15 puntos de Recall@5 efectivo es frágil. Las tres versiones lo sustituyen por un criterio compuesto: mejora, tokens, latencia, precisión de lo inyectado, daño por error y número de harnesses.
3. Arquitectura: núcleo local de decisión más adaptador delgado por harness.
4. La seguridad es requisito de partida (procedencia, cuarentena, tratar lo inyectado como datos, registro de auditoría).
5. El hueco se ha estrechado: hay trabajo cercano (side-agent selectivo, políticas aprendidas, reconsolidación, SWE-MeM); lo que sigue sin verse unido es política externa, local, neutral al harness, con reconsolidación y medida en agentes de código.

## 3. Lo más valioso que añaden (a incorporar)

- Informe B: la CAPTURA (25 por ciento de hechos nunca guardados, dato propio T-DSH-95) hay que medirla aparte; si no, el techo del recall es 75 por ciento y se atribuiría a la política lo que es captura. Criterio propuesto: tasa de captura mínima del 85 por ciento, ΔRecall sobre hechos guardados, y ablación obligatoria de la reconsolidación (si no aporta 3 puntos, se retira de v1).
- Informe B: el canal prioritario son Claude Code, Codex CLI y Gemini CLI; Cursor y Copilot con salvedad. Informe C prioriza Claude Code, Cursor y Copilot y trata Codex y Gemini solo como MCP (sin eventos): es menos preciso (otras fuentes hablan de hooks oficiales en ambos).
- Informe C: usar benchmarks de código (ContextBench, ReviewBench) y un estudio de memoria en agentes de código donde la memoria redujo exploración y coste más que mejorar la calidad. Todo sin verificar.
- Empezar simple: estimadores de incertidumbre clásicos rinden como pipelines complejos (benchmark de 35 métodos).

## 4. Debilidades a tener en cuenta

- El informe C cita fuentes secundarias (blogs, Medium, resúmenes) para McDaniel, Einstein, Nader y Schiller, y deja sin eventos los hooks de Gemini y Codex. Menos fiable que A y B.
- Los informes A y B reutilizan citas "tomadas del informe previo" para la biología (Tulving, Guzmán, Nader, Anderson, Hart) sin reabrir la fuente primaria. Pendiente.
- Ninguno ha medido nada en Tylluan. Todas las cifras de mejora son de terceros y en otros entornos.
- Matiz de "Recall@5 efectivo": el informe C lo define como parche correcto, lo que mezcla recuperación con capacidad del modelo. Definirlo bien antes de medir.

## 5. Decisión que corresponde a José

Aprobar o no un prototipo acotado (no la columna vertebral): núcleo de decisión local detrás de un flag desactivado por defecto, adaptadores para Claude Code, Codex y Gemini, y el experimento con tres condiciones (sin memoria, inyectar siempre, solo si el modelo llama a la herramienta, política selectiva) más la ablación de reconsolidación. Antes, dos pasos baratos que ya son nuestros: medir la tasa de captura con `cycle_closeout_memory` y reverificar los hooks de cada harness en documentación oficial.

No verificado: todo lo no listado en la sección 1.
