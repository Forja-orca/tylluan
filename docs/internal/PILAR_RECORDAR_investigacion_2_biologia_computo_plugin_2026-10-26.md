# Pilar "Recordar": segunda pasada (biología, cómputo y plugin universal)

Estado: 2026-10-26. Complementa `PILAR_RECORDAR_programa_de_investigacion_2026-10-26.md`. Todo son extractos de búsqueda; salvo la arquitectura del harness de DeepSeek (página abierta), nada está verificado en el texto completo. Las analogías biológicas son metáforas de diseño, no modelos validados.

## A. Qué es "recordar": biología

| Hallazgo | Fuente | Qué sugiere para Tylluan |
|---|---|---|
| Memoria prospectiva con dos rutas: monitorización estratégica (prefrontal, cuesta atención) y recuperación espontánea disparada por una pista (hipocampo). Con pistas muy salientes domina la automática; con pistas sutiles hace falta vigilancia. | https://www.nature.com/articles/srep25983 , https://www.ncbi.nlm.nih.gov/pmc/articles/PMC4500919/ | Hoy el modelo debe vigilar él mismo (llamar a recall): la ruta cara. Falta la ruta espontánea: Tylluan detecta la pista en el turno. |
| Completado de patrones en CA3: una pista parcial reconstruye el patrón entero. | https://www.nature.com/articles/ncomms8462 , https://www.ncbi.nlm.nih.gov/pmc/articles/PMC6786823/ | Recordar es reconstruir desde pistas parciales, no buscar por similitud con la pregunta. |
| Reconsolidación: reactivar una memoria la hace temporalmente modificable; el error de predicción parece necesario para actualizarla. Debate abierto: ¿es la recuperación o la nueva información el disparador? | https://research.birmingham.ac.uk/files/40975303/LeeNaderSchiller_TICS_JLedits_ds_jl.pdf , https://www.frontiersin.org/journals/systems-neuroscience/articles/10.3389/fnsys.2023.1103770/pdf | Recuperar es también la ocasión de corregir lo guardado (relacionado con la fricción de DCR que fusionaba sin guardar lo nuevo). |
| Metamemoria: el sentimiento de saber (FOK) estima si algo se reconocerá aunque no se evoque; guía el control (qué reestudiar, curiosidad). | https://link.springer.com/article/10.3758/BF03213977 , https://www.tandfonline.com/doi/abs/10.1080/09658211.2020.1867746 | Saber que se sabe sin traerlo todo: señal barata de confianza antes de inyectar. |
| El hipocampo sirve a la vez a la memoria del pasado y a la predicción de lo próximo. | https://www.mrcbndu.ox.ac.uk/publications/prediction-and-memory-predictive-coding-account | Recordar es predecir lo que se va a necesitar; ver B. |

## B. Qué es "recordar": cómputo

| Hallazgo | Fuente | Qué sugiere |
|---|---|---|
| ACT-R: la activación de un recuerdo = uso base (frecuencia y recencia) + activación por el contexto actual; solo se recupera lo que supera un umbral. | http://people.ucsc.edu/~abrsvn/ACT-R_subsymbolic_3.pdf (diapositivas antiguas) | Política con umbral y pistas de contexto, no solo similitud. No se confirmó el análisis racional de Anderson y Schooler (probabilidad de necesidad). |
| Hopfield modernas igual a atención: la recuperación es completar una pista parcial. | https://arxiv.org/abs/2008.02217 | Mismo principio que CA3 en cómputo. |
| Prefetching de caché: predecir qué se va a necesitar; riesgo de contaminar la caché. | https://arxiv.org/pdf/2606.09955 | Inyectar de más "contamina" el contexto: el coste de equivocarse es real. |
| "Remember When It Matters": un agente de memoria al lado interviene selectivamente; reporta superar la inyección siempre activa y la recuperación general (+8,3 puntos en Terminal-Bench, +6,8 en tau2-Bench). | https://arxiv.org/abs/2607.08716 | Apoya H1 con una evidencia ajena (no verificada, solo resumen). Falta replicar en un agente de código real. |
| Memoria asociativa predictiva: recuperación más allá de la similitud usando coocurrencia temporal. | https://arxiv.org/pdf/2602.11322 | Candidata para "qué traer" (H2). No leído. |
| Riesgo: un ataque de inyección de memoria (InjecMEM) demuestra que una sola interacción puede dirigir respuestas posteriores. | https://arxiv.org/pdf/2608.23471 | Una política proactiva amplía la superficie de ataque; la procedencia del recuerdo debe acompañar a lo inyectado. |

Síntesis provisional (hipótesis, no hecho): recordar no es buscar; es (1) detectar una pista en el contexto, (2) completar el patrón desde recuerdos con activación suficiente, (3) estimar la confianza antes de actuar, (4) aprovechar para actualizar lo guardado. Las ofertas actuales hacen sobre todo (2) con similitud. Nuestro hueco sería (1), (3) y (4) de forma neutral al harness.

## C. Tylluan como plugin universal

| Hallazgo | Fuente |
|---|---|
| Claude Code: plugin con `.claude-plugin/plugin.json`, hooks, skills, `.mcp.json`, y catálogo `marketplace.json`. | https://code.claude.com/docs/en/plugins-reference , https://code.claude.com/docs/en/plugins/marketplace-reference |
| Codex: plugins desde marzo 2026 (skills, conectores, MCP); hooks de plugin no administrados hasta que el usuario los revisa y confía; define variables de entorno con alias compatibles con Claude. | https://developers.openai.com/codex/plugins/build |
| Agent Skills (`SKILL.md`): formato que leen decenas de herramientas (Codex, Gemini CLI, Cursor, OpenCode, Copilot...) en rutas como `.agents/skills/`. Las fuentes son blogs, no la especificación. Un plugin de Cursor solo sirve en Cursor. | https://www.cubic.dev/blog/agent-skills |
| Harness de DeepSeek: eventos de plugin `agent/pre-step` (reescribir mensajes) y `agent.inject()` ("lands in the next admitted request"); todo lo que ve el modelo se registra como evento. | https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md (página abierta) |
| ZCode (Z.ai): según una fuente de proveedor, incrusta el runtime de Claude Code y tiene hooks nativos (recuperar antes de cada prompt, retener tras cada turno). Fuentes discrepan sobre si es código abierto. | https://hindsight.vectorize.io/blog/2026/07/20/zcode-persistent-memory (no oficial) |
| Productos multi-harness: un servidor de memoria local compartido más un adaptador de hooks por herramienta (agentmemory, ai-memory, Supermemory, ECC con asistente multi-harness). | https://github.com/akitaonrails/ai-memory , https://github.com/rohitg00/agentmemory |
| Sin código (claude.ai, ChatGPT): conectores remotos MCP, por conversación, no localhost, la memoria nativa compite; el modelo decide llamar; no se vio inyección automática. | https://docs.mem0.ai/integrations/claude-ai (vendor) |

Conclusión provisional: no existe un único empaquetado universal. El patrón viable es un núcleo común (servidor local Tylluan + skill `SKILL.md` + MCP) con un adaptador delgado por harness (Claude Code, Codex, DeepSeek harness, ZCode, Gemini CLI, OpenCode). Para clientes sin código, solo L0 (MCP) y por tanto el problema de "cuándo" queda del lado del modelo, salvo una puerta de entrada que el usuario configure.

## D. Lo que falta (la siguiente pasada)

1. Leer completos: Remember When It Matters, ProactRL, MemCon, AgeMem, Predictive Associative Memory; Anderson y Schooler; Tulving (especificidad de codificación).
2. Verificar en fuente oficial: especificación de Agent Skills, hooks de Codex, ZCode, Gemini CLI, OpenCode, Cursor.
3. Retrieval-induced forgetting, olvido y sueño en agentes: no se encontró nada.
4. Medir en nuestro harness (cuando José lo decida): inyectar siempre frente a intervención selectiva.

No verificado: todo lo anterior salvo lo indicado.
