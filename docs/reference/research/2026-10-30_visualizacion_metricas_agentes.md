# Investigación: Visualización Gráfica de Métricas de Agentes de IA

**Fecha:** 2026-10-30 | **Autor:** @antigravity | **Tarea:** AG-18

---

## 1. Evidencia Empírica de Sistemas de Observabilidad

| Sistema | Vista Analizada | Datos Concretos Requeridos | Decisión Humana que Permite |
| :--- | :--- | :--- | :--- |
| **Langfuse** | Timeline & Trace View | Spans jerárquicos (LLM, tool, retriever), inicio/fin (`timestamp`), latencia, metadata | Aislar cuellos de botella de latencia entre agentes y podar herramientas lentas |
| **Arize Phoenix** | Tracing & Application Latency View | Árbol de spans OpenTelemetry/OpenInference, tipo de invocación, tiempos por componente | Detectar ejecuciones degradadas y diagnosticar fallos paso a paso en el flujo |
| **LangGraph Studio** | Graph Mode State Inspection | Nodos y aristas del grafo de agentes, esquema de estado (`MessagesState`), checkpoints | Modificar estados intermedios ("time travel") y corregir bifurcaciones erróneas |
| **LangSmith** | Run Tree & Trajectory View | `RunTree` (orden jerárquico punteado), entradas/salidas, tokens acumulados, coste | Detectar acumulación descontrolada de contexto y depurar llamadas fallidas |
| **Helicone** | Sessions & Agent Workflow View | Cabeceras (`Helicone-Session-Id`, `Helicone-Session-Path`), llamadas LLM/tools/vectores encadenadas | Evaluar la secuencia completa de razonamiento y detectar bucles infinitos |
| **AgentOps** | Session Drawer & Summary | Sesiones de agentes, trazas de llamadas LLM estilo chat, tiempo de ejecución total | Evaluar el éxito de sesiones de agentes y comparar versiones de frameworks |
| **Datadog** | Agent Observability Trace Explorer | Trazas completas de requests, spans de inferencia/tools, anomalías de prompt | Identificar degradaciones operativas, evaluar coste y frenar ataques de prompt |
| **Grafana** | OpenLIT GenAI & MCP Observability | Métricas OTel, salud de protocolos de transporte MCP, métricas de hardware/GPU | Decidir escalado de infraestructura y supervisar disponibilidad de servidores MCP |

---

## 2. Citas literales (revisadas por el TL el 2026-10-30)

1. **Langfuse** (2026-10-30, `https://langfuse.com/docs/observability/overview`):
   > "structured logs of every request that capture the exact prompt sent, the model's response, token usage, latency"
2. **Arize Phoenix** (2026-10-30, `https://arize.com/docs/phoenix/tracing/llm-traces`):
   > "Identify and address slow invocations of LLMs, Retrievers, and other components within your application, enabling you to optimize performance and responsiveness."
3. **LangGraph Studio** (2026-10-30, `https://docs.langchain.com/langsmith/studio`):
   > "Graph mode exposes the full feature-set and is useful when you would like as many details about the execution of your agent"
4. **LangSmith** (2026-10-30, `https://docs.langchain.com/langsmith/observability-concepts`):
   > "While a thread groups the traces of a session and keeps their nested structure, a trajectory flattens the entire session into an ordered list"
5. **Helicone** (2026-10-30, `https://docs.helicone.ai/features/sessions`):
   > "Sessions group these related requests together, letting you trace the entire agent flow from initial user input to final response in one unified view."
6. **AgentOps** (2026-10-30, `https://docs.agentops.ai/v2/introduction`):
   > "Here you will find a list of all of your previously recorded sessions and useful data about each such as total execution time."
7. **Datadog** (2026-10-30, `https://docs.datadoghq.com/es/llm_observability/`):
   > "You can investigate the root cause of issues, monitor operational performance, and evaluate the quality, privacy, and safety"
8. **Grafana** (2026-10-30, `https://grafana.com/docs/grafana-cloud/observe-and-act/monitor-applications/ai-observability/`):
   > "OpenTelemetry-native observability with distributed tracing across your complete AI stack. Monitor and visualize real-time performance of LLMs, vector databases, GPUs and MCP Servers"

---

## 3. Propuesta: 3 Vistas Gráficas para la Sala de Control de Tylluan

Basadas estrictamente en los endpoints que ya operan en el kernel en vivo (sin dependencias externas ni código ficticio):

### Vista 1: Topología Viva de Agentes y Tareas Activas (`/api/v1/runtime/tasks` + `/health/detailed`)
- **Diseño Gráfico:** Grafo de topología reactivo con nodos para cada subagente/runtime en ejecución (`runtime.active_tasks`) y badges de estado (`running`, `idle`, `blocked`). Aristas direccionales que muestran el origen de despacho y tiempo de vida de la tarea.
- **Decisión Humana:** Identificar bloqueos de subagentes y sobrecargas de hilos para cancelar tareas colgadas o rebalancear recursos.

### Vista 2: Cascada Temporal de Intenciones y Latencia de Guilds (`/api/v1/coloquio/messages` + `/guilds`)
- **Diseño Gráfico:** Vista de tipo Waterfall/Gantt sincronizada con los turnos de Coloquio. Muestra las llamadas a guilds ejecutadas en cada interacción, desglosando la latencia en barras horizontales coloreadas según el guild y el estado de retorno.
- **Decisión Humana:** Detectar discrepancias de latencia en modelos locales (ej. ONNX vs llama-server) y bucles repetitivos de mensajes en Coloquio.

### Vista 3: Matriz de Salud Cognitiva y Fricciones Operativas (`/api/v1/system/status` + `/health`)
- **Diseño Gráfico:** Tablero sintético de cuadrantes que correlaciona saturación de CPU del centinela, latencia del Coherence Gate, índice de fricciones acumuladas y estado de memoria activa (SilvaDB).
- **Decisión Humana:** Ajustar la agresividad del centinela de CPU o pausar tareas en background cuando el sistema entra en zona crítica.

---

## Impact
- Documento de referencia formal que fundamenta el rediseño gráfico de la Sala de Control sobre estándares reales de la industria.
- Cero líneas de código alteradas en el kernel o en el frontend.

## No verificado
- Revisión del TL (WebFetch de las 8 URL): Phoenix, LangGraph Studio, LangSmith, Helicone y AgentOps confirmadas; Langfuse solo hasta "latency" (el resto de la frase original no se confirmó y se quitó); Grafana confirmada sin el cierre "(Model Context Protocol)"; Datadog: la página devolvió la versión en inglés y se citó esa frase (la versión en español del informe original era una traducción, no literal).
- Las vistas descritas en la tabla de la sección 1 (por ejemplo "orden jerárquico punteado" de LangSmith, "Session Drawer" de AgentOps) las infirió Antigravity de las páginas y el TL no las contrastó una a una.
- No verificado el comportamiento visual en entornos móviles o resoluciones ultra-anchas.
