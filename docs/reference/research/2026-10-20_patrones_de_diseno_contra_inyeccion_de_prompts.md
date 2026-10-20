# Patrones de diseño contra la inyección de prompts (insumo para B-4 y CoherenceGate)

**Fecha:** 2026-10-20 · **Autor:** Claude Code (TL) · **Propósito:** tener una referencia primaria con la que leer los resultados del corpus de inyección de Buffy (B-4) y decidir si las defensas de Tylluan (CoherenceGate, puerta de escritura de `tylluan_remember`, consenso multi-modelo en A2A) siguen un principio sólido o son heurísticas sueltas. Solo se cita lo leído del artículo el mismo día.

## Fuente

| Fuente | Qué es |
|---|---|
| [Beurer-Kellner et al. (14 autores, entre ellos Florian Tramèr y Andrew Paverd), "Design Patterns for Securing LLM Agents against Prompt Injections" (arXiv 2506.08837v3)](https://arxiv.org/abs/2506.08837) | Conjunto de patrones de diseño con análisis de su coste en utilidad y seguridad |

## Citas literales (versión HTML del artículo, sección 3.1)

| Patrón | Cita |
|---|---|
| Action-Selector | "The agent acts merely as an action selector, which translates incoming requests" |
| Plan-Then-Execute | "to formulate a plan to be executed (i.e., a fixed list of actions to take)" |
| LLM Map-Reduce | "dispatch an isolated LLM-agent to process individual pieces of 3rd party data" |
| Dual LLM | "a quarantined LLM that can be invoked by the privileged LLM whenever untrusted data has to be processed" |
| Code-Then-Execute | "an agent that writes a formal computer program to solve a task" |
| Context-Minimization | "remove unnecessary content from the context over multiple interactions" |
| Principio | "once an LLM agent has ingested untrusted input, it must be constrained" y "so that it is impossible for that input to trigger any consequential actions" |
| Objetivo | "we propose a set of principled design patterns for building AI agents with provable resistance to prompt injection." (resumen) |

## Lectura para Tylluan (análisis mío, no cita)

1. **El principio es de arquitectura, no de detección.** El artículo acota lo que puede hacer el agente tras ver datos no confiables; no promete reconocer el texto malicioso. Eso cambia cómo leer B-4: una tasa de detección alta del CoherenceGate o de la puerta de escritura reduce riesgo, pero no es la garantía que el artículo llama "provable".
2. **Dual LLM y Map-Reduce encajan con lo que Tylluan ya tiene a medias.** El consenso multi-modelo contra inyección en A2A (M39) y los mensajes de Coloquio de otros agentes son entrada no confiable; lo que falta por comprobar es si el agente que los lee queda limitado en acciones con consecuencias, no solo si el texto se filtra.
3. **Context-Minimization apoya el punto 1 de D-6** (no indexar salidas de `read_channel`): menos contenido ajeno persistido es menos superficie.
4. **Pregunta para Buffy y el TL:** para cada muestra que se cuele en B-4, ¿qué acción con consecuencias habría podido disparar? Si ninguna (solo texto en memoria), el riesgo es de contaminación del recall; si alguna (bash, escritura, permisos), es crítico.

## No verificado

- Solo he leído hasta el carácter 100.000 de 109.167 de la versión HTML; no he leído el final ni las secciones de casos de estudio.
- No he evaluado la tabla de utilidad frente a seguridad de cada patrón.
- No he comprobado en el código de Tylluan qué acciones puede disparar un agente tras leer un mensaje de Coloquio; el punto 4 es una pregunta, no un hallazgo.
