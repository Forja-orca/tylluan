# Cómo integran la comunidad y las empresas los modelos de decisión (System One / Jev / Laya) — 2026-10-15

> **Estado:** nota de investigación para ADR-018 (Decision Fabric) y el ADR de servicio de decisión que viene después.
> **Encargo:** José (2026-10-15) — investigar cómo se integran estos modelos antes de seguir con el diseño.
> **Método y límites:** búsqueda web (2026-10-15) + lectura directa de dos fuentes (documentación de LiteLLM y el preprint arXiv 2609.28940).
> Todo lo marcado **[FUENTE LEÍDA]** salió de una página que se descargó y leyó. Lo marcado **[SECUNDARIO]** viene de blogs o resúmenes de terceros y **no está verificado**.
> No se ha ejecutado ningún benchmark nuevo. Las cifras de latencia y calibración son de los autores, no nuestras.

## 1. Qué es la categoría y cómo se está estandarizando

- Los modelos "System One" son clasificadores **no generativos**: reciben un estado y responden preguntas tipadas (elección, puntuación, sí/no con probabilidad), sin generar texto token a token. **[SECUNDARIO]**
- **El formato se está estandarizando en dos dialectos de API [FUENTE LEÍDA: documentación de LiteLLM]:**
  - `POST /v1/systemone` (formato TypeSafe): campos `model`, `state`, `questions` (mapa por nombre), `images`/`assets` opcionales. Tipos: `noul` (sí/no), `choice`, `score`, `box`.
  - `POST /v1/decisions` (formato OpenAI, beta pública según prensa): campos `model`, `input`, `questions`, `safety_identifier`. Tipos: `predicate`, `choice`, `score`. Una pregunta puede devolver `{"type":"refusal"}` y las demás siguen contestadas.
  - Límites comunes: 1–128 preguntas por petición; hasta 255 opciones en `choice`; 2–10 niveles en `score`; sin streaming.
- **LiteLLM actúa de pasarela unificadora** [FUENTE LEÍDA]: enruta ambos formatos hacia proveedores distintos y devuelve la respuesta en el formato de la ruta llamada. Proveedores documentados: OpenAI, TypeSafe Jev, Fireworks (preview), Perplexity, Cloudflare Clef, Databricks, Microsoft Foundry, y autoalojados (Strands Decider, vLLM, Laya, Bespoke Nimble).
- **Soporte en runtimes locales:**
  - llama.cpp: soporte nativo de modelos de decisión desde la PR 29818 (fusionada el 2026-10-02), expone `/v1/systemone`, no `/v1/decisions`. **[SECUNDARIO]**. Coincide con lo que ya medimos nosotros con Clef-Flash (tuvimos que subir el binario porque el pin viejo no soportaba la arquitectura).
  - vLLM: solo preguntas `choice`, modelos Qwen3/Qwen3.5, requiere una build posterior a v0.31.0 [FUENTE LEÍDA, vía LiteLLM].
  - Ollama: no se encontró soporte. Sin confirmar.

**Consecuencia para Tylluan:** el cliente HTTP que ya habla `/v1/systemone` (ADR-018, variante B) cubre el dialecto local. Falta decidir si se añade el dialecto `/v1/decisions` o si se delega en LiteLLM como pasarela opcional. "OpenAI-compatible" a secas **no** basta: chat/completions y decisions son contratos distintos.

## 2. Cómo se integran de verdad: patrón "aditivo", con reglas deterministas al mando

Fuente principal: preprint arXiv 2609.28940 (Jev/Laya como capas de decisión en un agente de pentest) **[FUENTE LEÍDA]**.

- **Cuatro puntos de decisión, todos acotados:** adjudicación de hallazgos (confirmado / revisar / rechazado), recalibración de severidad, poda de agentes irrelevantes y bucle de confirmación para clases enumerables.
- **Principio aditivo:** el modelo puede **bajar la confianza, marcar o podar**, pero **no puede anular el rechazo de un validador determinista ni resucitar un hallazgo sin respaldo**. Es exactamente la regla de ADR-018 ("el modelo nunca tiene autoridad directa").
- **Umbrales configurables por contexto:** 0,8 para trabajo de alta garantía y 0,5 para barridos de descubrimiento (valores citados en el preprint). Un umbral fijo de 0,5 universal no es la práctica recomendada.
- **Calibración:** escalado de temperatura posterior, ajustado con pares (evidencia, veredicto) del dominio. Valor de ECE de Laya tras escalado: 0,081 (crudo 0,466). El preprint dice que **aún no se hizo con datos de pentest**.
- **Degradación:** si la capa falla o está desactivada, el arnés vuelve a funcionar sin ella. Falta de respuesta equivale a 0,5 y, al podar, se conserva un mínimo de agentes.
- **Latencia reportada (hardware distinto, no comparable):** Jev 236–276 ms, Laya 33–40 ms en GPU T4, un LLM generativo 1,5–3 s.
- **Limitaciones que declaran los propios autores:** una sola ejecución por brazo, sin potencia estadística; en su caso de estudio la cobertura con Jev fue **9/13 frente a 10/13 sin él**, con menos tiempo (27 min frente a 32). Sin conclusión causal. Los modelos se midieron en tareas y hardware distintos.

**Lectura honesta:** la integración real es *estrecha, aditiva y con respaldo determinista*. Nadie documenta (en lo que leí) una mejora de precisión general por poner un modelo de decisión delante de todo. Eso refuerza nuestra regla de adopción por módulo, solo si gana en sombra.

## 3. Modelos pequeños como guardarraíl y router en producción

Todo **[SECUNDARIO]** salvo que se indique (blogs de vendedores y preprints de 2026 vistos solo por resumen):

- **Patrón común de guardarraíl:** clasificador pequeño y rápido como prefiltro (ShieldGemma 2B, Qwen3-Guard 0.6B, Llama Guard 3 1B/8B, Prompt Guard) y un modelo mayor que solo revisa lo marcado. Varios clasificadores en paralelo, con corte al primer bloqueo.
- **Veredictos de tres estados por llamada a herramienta:** pasa / corrección con feedback / bloqueo, y el bloqueo va a revisión humana (preprint AgentTrust 2606.08539, solo metadatos leídos).
- **Enrutado con seguridad:** modelo pequeño para pasos fáciles y de alta confianza; si el guardarraíl marca riesgo, se **sobrescribe el enrutado barato** y se escala al modelo grande (arXiv 2603.12823). El benchmark de abril 2026 sobre "front-door routing" (2604.02367) indica que **falta literatura específica de clasificación para enrutado**.
- **La cifra "80–90 % de los pasos agénticos en local"** es una estimación de un blog de práctica, no un dato medido.

## 4. Autorización de agentes: el modelo clasifica, la política decide

Convergencia clara en la literatura de 2026 **[SECUNDARIO, resúmenes de arXiv y blogs]**:

- **MetaPermit** (2609.31039): el LLM solo elige valores de un vocabulario fijo; un motor de políticas aplica la primera regla que coincide, con **denegar por defecto**.
- **ActGov** (2609.24446): el LLM convierte contexto en registros finitos; un paquete de políticas verificado decide la autorización en ejecución.
- **FAVA** (2607.27267): grafos de permisos con un solver SMT para comprobar consistencia.
- **Arquitectura en capas (blog Zylos):** (1) límites de tasa y listas de bloqueo embebidas, (2) autorización estructural con OPA o Cedar, (3) capa semántica asíncrona con aprobación humana para acciones de riesgo. El clasificador LLM se reserva para casos ambiguos (p. ej. si una ruta es sensible).
- **Estado de producción:** los motores deterministas dominan; Claude Code usa lista de bloqueo y Codex usa reglas (arXiv 2605.24309).
- **Hueco:** no encontré resultados sobre *evaluación en modo sombra* de clasificadores de riesgo ni métricas de calibración con humano en el bucle. Es una laguna de la literatura, no solo de la búsqueda.

## 5. REA (Reverse Engineer Anything)

- Servidor MCP y CLI de ingeniería inversa (binarios nativos, apps Electron/JS, ensamblados .NET, webs). Licencia **MIT** según varias fuentes **[SECUNDARIO: no se leyó el fichero LICENSE del repositorio]**.
- **Datos que no cuadran entre fuentes:** el nombre del paquete npm (`@morluto/rea` 0.2.1 frente a `rea-agents` 6.3.0), el número de herramientas (de 41/42 a 139) y la popularidad (de 62 mil a 145 mil estrellas). Un artículo avisa de que los listados con `@morluto/rea` están obsoletos.
- **Pendiente de verificar antes de integrarlo (checklist de entrada):** leer `LICENSE` y `package.json` del repositorio oficial, fijar nombre y versión del paquete, auditar qué ejecuta sobre el objetivo y con qué permisos, y revisar la cadena de dependencias (como se hizo con iroh).

## 6. Información relevante para Tylluan (decisiones que esto informa)

1. **Contrato de interfaz:** soportar `/v1/systemone` (ya existe) y valorar `/v1/decisions` o LiteLLM como pasarela opcional. Los tipos de pregunta del estándar (`choice`, `score`, `predicate`/`noul`, `refusal`) deberían ser el vocabulario del contrato por módulo.
2. **Abstención como ciudadano de primera:** el estándar ya incluye `refusal` y la práctica usa un valor neutro (0,5) ante ausencia de respuesta. El servicio debe poder abstenerse y registrarlo.
3. **Umbral por contexto, no global**, y **calibración por módulo** con nuestros propios pares (evidencia, veredicto). Nuestros 245 casos HITL son la base; el escalado de temperatura es una técnica barata y estándar que ADR-018 aún no menciona.
4. **Regla aditiva** (el modelo solo baja confianza, marca o poda; nunca revive lo rechazado por una regla): adoptarla literalmente como invariante del servicio.
5. **Estados desactivado / sombra / advisory / gate** son coherentes con la práctica; la laguna de literatura sobre sombra significa que nuestro diseño de evaluación en sombra será original y debe documentarse bien.
6. **Filtro de salida:** la práctica es un clasificador pequeño como prefiltro más verificación de un modelo mayor solo sobre lo marcado, con tres veredictos (pasa / corrige / bloquea) y revisión humana en el bloqueo. Siempre separado del generador.
7. **REA:** entra como candidato con checklist de verificación previo (sección 5); no se integra por la fama del repositorio.
8. **Ninguna fuente demuestra mejora general de precisión.** La evidencia disponible es de integraciones estrechas con respaldo determinista.

## 7. Anexo — arnés interno (Inference Gateway): análisis y decisiones del 2026-10-15

> Origen: pregunta de José — ¿se puede crear un arnés interno como capa intermedia para que todo el sistema sea agnóstico al motor de inferencia, y qué conviene cambiar ahora para dejar abierto el futuro? Decisión de José: **dos ADR separados** (ADR-020 Inference Gateway y ADR-021 servicio de decisión sobre él).

### 7.1 Puntos de inferencia reales hoy (verificado con grep sobre `crates/` y `guilds/`)

| Capacidad | Dónde vive | Estado |
|---|---|---|
| Decidir | trait `DecisionProvider` (`router/decision/mod.rs`) | Único trait de proveedor que existe. Las variantes LAYA, Clef-Flash y Decima viven en el harness de evaluación (`examples/decision_fabric_spike.rs`), no en producción. |
| Embeddings | struct `EmbeddingEngine` (fastembed/ort, en proceso) | Unos 12 puntos de llamada, todos vía `embed_batch_coalesced*`. Tipo concreto, no trait. |
| Reranker y MLP | `router/light_reranker.rs`, `mlp/mod.rs` | Sesiones `ort` directas sobre ficheros ONNX locales. |
| Generar texto | guild Python `guilds/core/llama_backend.py` | Ya habla `/v1/chat/completions`; endpoint configurable, pero `[inference.llama]` mezcla parámetros propios de llama-server. |
| Visión | guild `guilds/core/vision.py` | `onnxruntime` dentro del guild. |
| Gobierno de carga | `memory/inference_budget.rs` | Gobierna embed + rerank. Semilla de un gobernador común. |

### 7.2 Idea central: abstraer por CAPACIDAD, no por motor

Cambiar de motor no es cambiar de modelo. Los embeddings son un contrato de datos (dimensión y hash del modelo invalidan los vectores guardados) y las decisiones tienen calibración propia. El gateway define capacidades tipadas (`embed`, `rerank`, `decide`, `generate`, más adelante `vision`), un trait por capacidad, adaptadores por dialecto (ONNX en proceso, HTTP compatible OpenAI, `/v1/systemone`, `/v1/decisions`, Anthropic `/messages`) y un registro único de proveedores con presupuesto, métrica y auditoría compartidos. El servicio de decisión (ADR-021) es la capacidad `decide` de este gateway, con dos puertas: módulos internos y operación directa para el agente.

### 7.3 Cambios a hacer AHORA (baratos hoy, caros después)

1. `EmbeddingEngine` a trait, con el motor actual por defecto (comportamiento idéntico).
2. Identidad del modelo (id, dimensión, versión) guardada junto a cada vector y filtrado por ella en la búsqueda. Es la propuesta pendiente de Deep (filtro por `model_hash` en `search.rs`): pasa a **requisito previo de cualquier segundo modelo**.
3. Un registro de proveedores único (hoy `[inference]`, `[inference.llama]` y la configuración de decisión van por separado).
4. **Clase de privacidad por proveedor** (local / red del equipo / nube) con política de qué datos pueden salir, modelada sobre los espacios de ADR-019. Mantiene la regla "sin nube en la ruta crítica".
5. Auditoría de cada llamada (proveedor, versión del modelo, latencia, coste, resultado) en el registro existente. Sin esto no hay modo sombra ni calibración.
6. Semántica de fallo por capacidad en el contrato (bajo impacto continúa con reglas; acción crítica se abstiene o pide aprobación).
7. Separar los parámetros de llama-server del bloque genérico de inferencia, compatible hacia atrás.
8. Que los guilds Python usen el gateway en vez de cada uno su motor.

### 7.4 Ganchos futuros (solo diseñar la puerta, no implementar)

Más dialectos (`/v1/decisions`, LiteLLM como pasarela opcional), multi-embedding versionado (hoy archivado como backlog), umbral y calibración por módulo, cambio de modelo en caliente y comparación en sombra, selección de proveedor según hardware (ancla del médico con Raspberry Pi), un par del equipo como proveedor de inferencia vía `dispatch_allow` de ADR-019, cortacircuitos y salud por proveedor.

### 7.5 Reglas de adopción y límites fijados (José, 2026-10-15)

- El modelo de decisión se consulta solo en decisiones semánticas concretas, acotadas y evaluables. Las reglas deterministas conservan la autoridad sobre permisos, seguridad y límites; las acciones de riesgo exigen aprobación humana.
- Un módulo adopta el modelo solo si en modo sombra supera a sus propias reglas en sus métricas (los datos propios dicen que no está garantizado: LAYA 0,351 frente a 0,531 de las reglas).
- Estados de activación: desactivado / sombra / advisory / gate. Desactivado = flujo actual intacto.
- Filtro de salida: capa de verificación independiente del generador, no una pregunta al mismo modelo. "Cero alucinación" es aspiración; lo medible es menos afirmaciones sin respaldo y abstención cuando falta evidencia.
- **Sexta tool: DECIDIDO por José (2026-10-15, pregunta directa) — se crea una sexta tool real.** El TL había recomendado exponer la decisión como operación de `tylluan_do`; José eligió la tool real. Cambia CONTRACT-01 (exactamente cinco): exige una enmienda formal en el ADR-021 (nombre, contrato sin autoridad para ejecutar ni conceder permisos, interruptor de desactivado) y actualizar `tools.rs`, el test de las cinco tools, `AGENTS.md`, README y las guías de clientes MCP. Sin código hasta que el ADR-021 esté aprobado.
- **Invariante propuesto para ADR-020 (pendiente de confirmar por José):** *Tylluan consume inferencia; no la ofrece como servicio general.* El Inference Gateway es una **capa cliente solo de salida**: las conexiones a proveedores (OpenAI `/v1`, Anthropic `/v1/messages`, llama.cpp, Ollama, LM Studio, LiteLLM opcional) son salientes, configuradas por el usuario y específicas por capacidad. Tylluan no inicia ni expone un servidor de modelos como requisito de arquitectura (única excepción: el llama.cpp local explícito). Sin cambio silencioso de proveedor, sobre todo de local a nube, ni llamadas externas con la capacidad desactivada. «Agnóstico al proveedor» no significa modelos equivalentes: se negocian capacidades y se calibra cada combinación de modelo, proveedor y tarea.
- No es reescritura: capa fina detrás de los caminos actuales, comportamiento por defecto byte-idéntico, validada con los tests existentes. No toca las cinco tools ni los timeouts de los guilds.

### 7.6 REA (opiniones de Deep y Antigravity, verificadas en parte por el TL)

GO como herramienta **opcional vía MCP externo** (existe `external_mcp.rs`, `[[external_mcp]]` y `/api/v1/mcp/external`), fuera de la ruta crítica y sin ser sexta tool. Aportes: (a) la lista de familias peligrosas del paso 2 de ADR-019 es fija (`bash`, `filesystem`, `code`) y debe volverse extensible por descriptor; (b) respetar `untrusted_content: true` en CoherenceGate sería trabajo nuevo (en Rust no hay ningún manejo; solo el helper `flag_untrusted_content` en un guild de Python); (c) sus conclusiones en memoria llevan procedencia `agent_generated`, nunca `verified`. Sin verificar por el TL: licencia (no se leyó `LICENSE`), versión del paquete y popularidad (las fuentes discrepan). Pendiente comprobar si un MCP externo es alcanzable por el dispatch P2P.

## Fuentes

- LiteLLM, "/v1/decisions and /v1/systemone" — https://docs.litellm.ai/docs/decisions **[leída]**
- arXiv 2609.28940, "Calibrated Decision Models for Autonomous Penetration-Testing Harnesses" — https://arxiv.org/html/2609.28940v1 **[leída hasta el carácter 100 000 de 159 361]**
- arXiv 2606.08539 (AgentTrust), 2603.12823, 2604.02367, 2609.31039 (MetaPermit), 2609.24446 (ActGov), 2607.27267 (FAVA), 2605.24309 — solo resúmenes de búsqueda **[no leídas]**
- OpenAI Decisions API: **documentación oficial leída el 2026-10-15** (https://developers.openai.com/api/docs/guides/decisions): beta pública, `POST /v1/decisions`, tipos `predicate`/`choice`/`score` (`refusal` aparece como tipo de respuesta), solo `gpt-6-luna`, 0,10 USD por millón de tokens de entrada, imágenes solo en base64. Corrige la primera versión de esta nota, que decía no haber encontrado documentación primaria (sí existe; la había señalado una revisión externa). Cobertura secundaria adicional: LetsDataScience, Apidog, DataCamp.
- llama.cpp decision models: AICoder, explainx.ai, DEV Community **[SECUNDARIO]**
- REA: Socket.dev, capitalandcompute.net, kunalganglani.com, apidog.com **[SECUNDARIO]**
