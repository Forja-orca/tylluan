# ADR-020: Inference Gateway — abstracción por capacidad, contratos tipados y gobierno unificado de inferencia

- **Estado:** 📐 **PROPUESTO (v7 tras revisión del Tech Lead en T1215: nombres de campo reales de ambos dialectos de decisión, lista de endpoints LAN de inferencia propia y desacoplada de `federation_peers.permissions`, endpoint real de reindexación, descripción exacta del guard SSRF actual e higiene documental)** (solo diseño en papel, cero código)
- **Fecha:** 2026-10-15 (v7: 2026-10-16)
- **Origen:** decisión estratégica de José (2026-10-15/16, Coloquio T1186–T1211): separar el *Inference Gateway* (abstracción por capacidad de inferencia) del *Decision Fabric* (ADR-018 / ADR-021, servicio de decisión semántica).
- **Insumos directos:** 
  - `docs/reference/research/2026-10-15_decision_models_industry_integration.md` (commit `cf663f4`).
  - `docs/reference/adr/ADR018_decision_fabric.md` (contrato base de `DecisionProvider`).
  - `docs/reference/adr/ADR019_team_scoped_networks.md` (modelo de permisos por espacio y aislamiento P2P).
  - Informe técnico de reutilización y dependencias (Coloquio T1198–T1205).
- **Ámbito:** diseño de la capa unificada de inferencia en `crates/tylluan-kernel/src/router/inference/` (propuesta), unificando embeddings, reranking, scoring MLP, modelos de decisión, generación y visión bajo un arnés cliente de salida con presupuesto, auditoría, catálogo de modelos y clases de privacidad.

---

## 1. Desglose de Autoridad y Decisiones Formales

### A. Decidido formalmente por José (Coloquio T1186, T1191, T1197, T1201, T1202, T1204):
1. **Dos ADR separados:** Separar formalmente el Gateway de Inferencia (ADR-020) del Servicio de Decisión (ADR-021).
2. **Abstracción por CAPACIDAD, no por motor:** El kernel no debe acoplarse a si un modelo corre en `ort` in-process, en `llama-server`, en un proceso local o en un endpoint HTTP. El código cliente solicita una *capacidad* (`embed`, `rerank`, `decide`, `generate`, `vision`).
3. **Sexta Herramienta Soberana Aprobada:** José decidió formalmente el 2026-10-15 incorporar una **6ª herramienta soberana** en el kernel. CONTRACT-01 evoluciona formalmente de 5 a 6 herramientas soberanas. El contrato detallado de dicha herramienta se define en el **ADR-021** (redactado por el TL); el ADR-020 se enfoca exclusivamente en la infraestructura subyacente del Gateway.
4. **Conexión Hotel → Hospital:** La conexión remota entre el nodo móvil y el servidor central se realiza mediante la **VPN propia del hospital** (camino base ya aprobado; `iroh` archivado como NO-GO por ahora). El nodo entra en la red del hospital a través de su VPN institucional.
5. **Datos Clínicos hacia la Nube:** **SOLO con aprobación explícita humana por petición (HITL)**. La política es denegación por defecto (*fail-closed*). Se reutiliza el patrón de aprobación existente (`dispatches approve/reject`), auditado por llamada con espacio, proveedor y versión de modelo. Quedan prohibidas aprobaciones persistentes o globales; se documenta el riesgo residual de fuga.
6. **Embeddings en la Nube y Dual-Space Planificado:** **Permitidos cuando hay conexión**. José levantó formalmente el requisito de "benchmark real" que frenaba el dual-space (Coloquio T1204). El soporte de varios espacios vectoriales (espacio local garantizado + espacio opcional en la nube) pasa a ser **diseño planificado de la arquitectura** de este ADR, no un gancho futuro ni condicionado a benchmark previo. Como los vectores locales y de nube forman espacios matemáticos distintos, el modelo local debe estar disponible siempre para garantizar el recall offline. El orden de entrega se mantiene como **recomendación técnica del Tech Lead**: primera entrega con un solo espacio local de 1024 dimensiones con identidad completa registrada; fase siguiente con segundo espacio (nube) en tabla paralela y conmutación atómica con rollback. Embeber texto clínico en la nube requiere la aprobación explícita de (5).
7. **Cambio de Perfil:** **Automático SOLO hacia local** (degradación graceful si cae la red; NUNCA subida automática a la nube sin acción deliberada del usuario; con registro y notificación del cambio).
8. **Las Cinco Capacidades con Contrato Tipado Completo YA (incluida Visión):** Decidido formalmente por José (Coloquio T1207). El Gateway define contratos tipados completos en Rust para las 5 capacidades: `embed`, `rerank`, `decide`, `generate` y `vision`. Se diseña formalmente el trait `VisionProvider` cubriendo las operaciones de los guilds actuales `vision.py` y `vision_moondream.py` (descripción, OCR, extracción estructurada JSON, VQA y captioning), con límites estrictos de tamaño de carga (protección anti-DoS), salidas estructuradas y clases de privacidad: las imágenes clínicas (ej. radiografías, escáneres, ecografías) se clasifican como `DataSensitivity::Clinical` y requieren aprobación explícita humana por petición (HITL) para enviarse a la nube. Los guilds de visión pueden delegar en este arnés bajo el perfil `[inference.profiles.vision]`.
9. **Dialectos de Decisión: Ambos Soportados desde el Principio:** Decidido formalmente por José (Coloquio T1207). Se adopta soporte simultáneo para `/v1/systemone` (local en llama.cpp / Clef-Flash) y `/v1/decisions` (OpenAI beta pública en la nube, `gpt-6-luna`). El kernel implementa un **contrato interno neutro** (preguntas `Predicate`, `Choice`, `Score`, `Box`; respuestas equivalentes más `Refusal`) y dos adaptadores dedicados. Se documentan las diferencias de especificación (campos `state`/`input`, `noul`/`predicate`, `criteria`/`choices`/`levels`, mapa/array de preguntas, `box` solo en el dialecto SystemOne) y las consultas no portables (grounding visual por bounding box). Dado que `/v1/decisions` corre en la nube, se aplican las reglas estrictas de privacidad: el estado que contenga datos clínicos exige aprobación explícita humana por petición y queda prohibida la conmutación silenciosa de local a nube.

### B. Decisión de Arquitectura Actual (Revisable):
1. **Tylluan consume inferencia y no ofrece un backend de inferencia propio (revisable):** El Gateway se diseña actualmente como una capa **CLIENTE DE SALIDA (egress)**. Las conexiones hacia proveedores (OpenAI `/v1`, Anthropic `/v1/messages`, Ollama `/api`, LM Studio, llama.cpp, LiteLLM opcional) son salientes, configuradas por el usuario y específicas por función/capacidad. La única excepción es el subproceso local `llama-server`. Esta postura no es un dogma constitucional inmutable, sino una decisión arquitectónica actual orientada a mantener el kernel ligero y soberano; si en el futuro se justifica ofrecer endpoints internos o servidores de inferencia gestionados, podrá revisarse con su correspondiente ADR.

### C. Invariantes Constitucionales del Proyecto:
1. **Sin nube en la ruta crítica (soberanía):** La operación normal de Tylluan (arranque, indexación, memoria, búsqueda, decisión, coordinación) opera 100% en local. Cualquier salida a red externa (equipo o nube) es opt-in explícito del operador.
2. **Calibración no intercambiable:** Desacoplamiento de API no equivale a intercambiabilidad semántica. Cada terna `modelo + proveedor + tarea` debe calibrarse empíricamente; un cambio de proveedor exige reevaluar umbrales.

### D. Requisitos y Recomendaciones del Tech Lead:
1. **Comportamiento por defecto byte-idéntico en migración:** La introducción del gateway no debe alterar los resultados, tiempos ni asignaciones de la suite actual de tests (todos los tests pasando en verde, sin regresiones).
2. **Identidad del modelo desacoplada del motor:** Cambiar de ejecutor físico (ej. de CPU `fastembed` a GPU `ort` o contenedor local con los mismos pesos e idéntica cuantización) no invalida los vectores almacenados en SilvaDB ni altera las identidades.
3. **Spike de coste previo para dependencias externas:** Prohibida la adopción de crates como `Rig`, `genai` o `async-openai` sin un spike medido contra la toolchain fijada (`rust-toolchain.toml`: `channel = "1.88"`), `deny.toml` y peso de binario.
4. **Orden de entrega escalonado para Dual-Space:** Recomendación técnica (no gate) de acotar la primera entrega a un solo espacio local de 1024 dimensiones con identidad completa; y en fase posterior, el segundo espacio (nube) con tabla paralela y conmutación atómica.

### E. Preguntas que siguen abiertas para José:
1. **Federación entre nodos con dimensiones vectoriales dispares:** ¿Cómo sincronizar nodos con perfiles de embedding distintos (ej. servidor central con 1024d vs portátil de campo en perfil clinic de 384d)? (Ver Sección 4.3).

---

## 2. Estado real del código (VERIFICADO contra el árbol al 2026-10-16, HEAD `0cbed49`)

Cada afirmación ha sido contrastada con `git grep` sobre todo `crates/` (y lectura directa de `guilds/core/` para los guilds Python):

| Componente | Estado en código | Fichero y línea (verificado) | Realidad en el código |
|---|---|---|---|
| **Proveedores externos existentes** | Struct `ExternalProvider` con 3 tipos | `crates/tylluan-kernel/src/config.rs:862-881` | Enum `ExternalProviderType`: `OpenAICompatible`, `AnthropicCompatible`, `OllamaCompatible`. Almacena `base_url`, `api_key_env`, `models`. |
| **Prueba de proveedor externo** | Función `test_external_provider` | `crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:125-185` | Implementa llamadas HTTP reales con `reqwest` (1 token de prueba) para `/v1/chat/completions`, `/v1/messages` y `/api/chat`. **Hoy solo prueba conectividad**; no sirve inferencia interna. |
| **Bloqueo SSRF actual** | `check_external_provider`, comprobación `v4.is_private()` | `crates/tylluan-kernel/src/config.rs` L926–L947 (`is_private()` en L943) | Rechaza el host literal `169.254.169.254`, el wildcard `0.0.0.0` y las IPv4 RFC-1918 (`10.x`, `172.16-31.x`, `192.168.x`). Antes de esa comprobación **acepta** `localhost`, `127.0.0.1`, `::1` y cualquier IP con `is_loopback()` (todo `127.0.0.0/8`). No resuelve nombres de host (un nombre LAN no se comprueba) y no bloquea el resto de `169.254.0.0/16` ni rangos IPv6 privados. **Impide apuntar por IP a un modelo servido por otro nodo en la LAN.** |
| **Huella canónica de embeddings** | Función `compute_engine_fingerprint` | `crates/tylluan-kernel/src/router/embeddings.rs:405-464` | Genera `engine_fingerprint_v1` (`sha256:<64 hex>`) combinando `model_type`, `revision`, `dims`, `norm`, `graph_sha256`, `tokenizer_sha256`, `weights_data_size`. |
| **Descargador de modelos** | `ModelDownloader` y `get_model_registry` | `crates/tylluan-kernel/src/maintenance.rs:41-85,133` | Registro escrito en código Rust con URLs fijas a HuggingFace. Comprueba `size_expected`, pero **no verifica SHA-256** de los pesos descargados. |
| **Almacenamiento de embeddings** | Tabla `node_embeddings` (1 vector/nodo) | `crates/tylluan-kernel/src/memory/silva/schema.rs:380-388` | `node_id TEXT PRIMARY KEY`, `embedding BLOB NOT NULL`, `model_id`, `model_name`, `model_hash`, `dimensions INTEGER DEFAULT 1024`. Un solo vector por nodo: **cambiar de modelo exige reindexar** (sobrescribe en sitio). |
| **Reindexación existente** | `POST /api/v1/memory/reindex` → `reindex_handler` | `transport/http/api_v1/routes.rs` L66, `api_v1/api_memory.rs` L74 | Re-embebe en segundo plano los nodos "stale" (`get_stale_embeddings` por `model_id`/`model_hash`) y emite progreso por broadcast. Además existe un reindexador periódico de fondo en `main.rs` (≈L1796). No hay subcomando `reindex` en `crates/tylluan-cli`. |
| **Capacidad ausente como norma** | Perfil portable sin embeddings (`vector_dimensions = 0`) | `crates/tylluan-cli/src/main.rs` L1084 (asignación), L1568/L1583/L1594 (tests) | Los perfiles varían: `portable` (0 dims, sin embeddings), `clinic` (384 dims), `server` (1024 dims). La ausencia de capacidad de embedding es un estado NORMAL y soportado. |
| **Degradación en Remember** | Omisión suave del DCR si falla embedding | `crates/tylluan-kernel/src/transport/server/handler_remember.rs:147-150` | Si el motor falta (`matcher.engine() == None`) o falla, `early_embedding` es `None` y la escritura en SilvaDB **continúa** omitiendo el chequeo DCR. |
| **Trait de inferencia** | **Único:** solo existe `DecisionProvider` | `crates/tylluan-kernel/src/router/decision/mod.rs:74` | Solo cubre decisiones tipadas de ADR-018; no existe trait para embeddings, reranker ni generación. |
| **Embeddings locales** | Struct concreto `EmbeddingEngine` | `crates/tylluan-kernel/src/router/embeddings.rs:186` | Acoplado rígidamente a `fastembed::TextEmbedding` in-process bajo `Mutex`. Recuento por `git grep` fuera de `embeddings.rs` y tests: 10 llamadas a `embed_batch_coalesced*` (síncronas, `_routing` y `_async`) y más de 20 llamadas directas a `embed()` (algunas sobre el motor sparse), muchas dentro de `block_in_place`. |
| **Reranker y MLP** | Structs `LightReranker` y `MlpScorer` | `router/light_reranker.rs:46`, `mlp/mod.rs:11` | Sesiones `ort::session::Session` in-process directas bajo `Mutex`. |
| **Generación LLM** | Subproceso `llama-server` gestionado por Python | `guilds/core/llama_backend.py:12,27` | Gremio FastMCP externo consumiendo `/v1/chat/completions`. Configuración en `config.rs` L745 (`llama: InferenceLlamaConfig`) y L760 (struct). |
| **Presupuesto de inferencia** | `InferenceBudget` / `InferenceBudgetConfig` | `crates/tylluan-kernel/src/memory/inference_budget.rs` L67, L141 | Cola acotada opt-in (`[inference.budget] max_queue_wait_secs > 0`); por defecto espera ilimitada (legacy). |
| **Aprobación humana existente** | Cola de dispatches con approve/reject | `routes.rs` L286–L287 (`/api/v1/dispatches/{id}/approve` y `/reject`), `security/dispatch_queue.rs` L382, L430 | Patrón HITL reutilizable para la aprobación por petición. |
| **Cliente HTTP existente** | `reqwest` 0.12 en dependencias | `crates/tylluan-kernel/Cargo.toml` L106 | `default-features = false`, features `json`, `stream`, `rustls-tls`, `http2`. Sin OpenSSL. Ya presente: no añade dependencia nueva. |

---

## 3. Propuesta de Arquitectura: Un Único Harness de Salida (Decisión Actual Revisable)

El **Inference Gateway** se sitúa en `crates/tylluan-kernel/src/router/inference/` como un arnés cliente de salida.

```
Módulos del Kernel (SilvaDB, CoherenceGate, Scheduler, NightConsolidation, Decision Fabric)
                                      │
                                      ▼
             INFERENCE GATEWAY (Harness Único de Salida)
  ┌─────────────────────────────────────────────────────────────────┐
  │ 1. Selección de Perfil por Función (Elegido por el Usuario)     │
  │    - GenerateProfile │ DecideProfile │ EmbedProfile │ Rerank    │
  │ 2. Controles de Seguridad y Gobernanza                          │
  │    - SSRF Host Pinning (LAN segura) │ InferenceBudget │ Auditoría│
  │ 3. Contratos por Capacidad (Traits Tipados)                     │
  └───────────────────────────────┬─────────────────────────────────┘
                                  │
    ┌─────────────────────────────┼─────────────────────────────┐
    ▼                             ▼                             ▼
Adaptador Nativo reqwest     Adaptador FastEmbed / ONNX    Adaptador llama-server
(OpenAI, Anthropic, Ollama)  (Embeddings / Rerank local)   (Proceso local gestionado)
```

### 3.1 La Idea Simple: Perfil por Función Elegido por el Usuario

Cada punto interno que requiera inferencia llama a un único arnés configurable. El usuario define en `tylluan.toml` **un perfil por función**:
- [inference.profiles.generation]: Proveedor y modelo para texto/chat.
- [inference.profiles.decision]: Proveedor y modelo para decisiones tipadas (System One / Jev / Decima / OpenAI Decisions).
- [inference.profiles.embeddings]: Proveedor y modelo para indexación y búsqueda.
- [inference.profiles.reranker]: Motor de reordenación (ONNX local opcional).
- [inference.profiles.vision]: Motor de visión (SmolVLM2 local ONNX, Moondream o endpoint externo).

Tylluan muestra compatibilidad, dimensiones, nivel de privacidad y latencia, pero **la elección es siempre del usuario**.

### 3.2 Los Dos Ejes Ortogonales: Localidad del Proveedor y Sensibilidad del Dato

A raíz de la revisión del Tech Lead (Coloquio T1211), el Gateway separa estrictamente **dos ejes conceptuales ortogonales**:
1. **Localidad del Proveedor (`ProviderLocality`):** Dónde se ejecuta físicamente la inferencia.
2. **Sensibilidad del Dato (`DataSensitivity`):** Nivel de confidencialidad y gobernanza del dato transmitido.

#### A. Definiciones Tipadas en Rust

```rust
/// Dónde se ejecuta físicamente el modelo de inferencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProviderLocality {
    /// In-process (ort, fastembed) o subproceso gestionado en loopback (llama-server en 127.0.0.1)
    Local,
    /// Nodo de la red de equipo (par federado P2P autorizado o endpoint LAN aprobado por el operador)
    TeamNetwork,
    /// Proveedor externo a través de Internet (OpenAI, Anthropic, Ollama remoto, etc.)
    Cloud,
}

/// Nivel de confidencialidad y gobernanza del dato o payload transmitido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DataSensitivity {
    /// Información pública o sintética; consultas generales sin secretos ni PII
    Public,
    /// Notas internas del operador, memoria propia del nodo, configuración
    Private,
    /// Datos médicos de pacientes, historias clínicas, diagnósticos, imágenes diagnósticas
    Clinical,
}
```

#### B. Álgebra de Composición de Sensibilidad
La sensibilidad de una petición de inferencia no es arbitraria ni estática: **se calcula dinámicamente como la máxima sensibilidad entre todas las fuentes que la componen**:

$$\text{DataSensitivity}(\text{Petición}) = \max_{s \in \text{Fuentes}} \text{DataSensitivity}(s)$$

Bajo el orden estricto de precedencia:
$$\text{Public} < \text{Private} < \text{Clinical}$$

* **Regla de Recall y Contexto:** Si una consulta de búsqueda o recall no especifica espacio ni sensibilidad previa, la petición de decisión o reranking posterior **hereda automáticamente la sensibilidad máxima de los nodos recuperados** antes de emitir cualquier llamada externa.
* **Regla Multimodal:** Si una imagen procede de un espacio con clasificación clínica (ej. metadatos de paciente, DICOM, o subida médica), su sensibilidad es `Clinical`.

#### C. Matriz de Política de Gobernanza (`DataSensitivity` $\times$ `ProviderLocality`)

| `DataSensitivity` | `ProviderLocality::Local` | `ProviderLocality::TeamNetwork` (LAN / VPN) | `ProviderLocality::Cloud` (Internet) |
|---|---|---|---|
| **`Public`** | ✅ Permitido libremente | ✅ Permitido si el endpoint está registrado en `[[inference.allowed_lan_endpoints]]`. | ✅ Permitido |
| **`Private`** | ✅ Permitido libremente | ⚠️ Permitido **solo** si el endpoint está en `[[inference.allowed_lan_endpoints]]` con `max_sensitivity >= Private` y el espacio (`metadata.space`) figura en `allowed_spaces`. | ⛔ Denegado por defecto (*fail-closed*); requiere opt-in explícito del operador en `tylluan.toml`. |
| **`Clinical`** | ✅ Permitido libremente (Garantiza operación soberana offline del médico rural). | ⚠️ Permitido **únicamente** si el endpoint está en `[[inference.allowed_lan_endpoints]]` con `max_sensitivity = "Clinical"`, el espacio clínico figura en `allowed_spaces`, y se opera bajo la VPN hospitalaria. En caso contrario, *fail-closed*. | 🚨 **DENEGADO POR DEFECTO.** Requiere **APROBACIÓN HUMANA EXPLÍCITA POR PETICIÓN (HITL)** vía `dispatches approve/reject`. Prohibidas aprobaciones persistentes o globales. Cero bypasses. |

> [!IMPORTANT]
> **Desacoplamiento Estricto frente a Sincronización de Memoria (ADR-019):**
> Los permisos de par de ADR-019 (`federation_peers[].permissions`, struct `PeerPermissions` en `crates/tylluan-kernel/src/federation/permissions.rs` con `sync_spaces` y `dispatch_allow`) regulan la sincronización de memoria y el despacho de herramientas entre pares federados, **no el envío de payloads o prompts a un endpoint de inferencia**. Este ADR **no reutiliza** `federation_peers.permissions` para autorizar inferencia.
> La autorización de inferencia hacia LAN/VPN se gestiona en su propia lista declarativa independiente: cada entrada de `[[inference.allowed_lan_endpoints]]` define explícitamente su `max_sensitivity` y sus `allowed_spaces`. Un endpoint que no figure en esa lista no recibe ninguna petición, sea cual sea su relación de federación.
> *Pares federados que además sirven inferencia:* si en el futuro un par federado ofrece un endpoint de inferencia, ese endpoint igualmente debe darse de alta en `[[inference.allowed_lan_endpoints]]`; los permisos de federación del par no amplían ni sustituyen esa autorización (fail-closed). Cómo se combinan ambos registros queda para el paso correspondiente de ADR-019, no se decide aquí.

---

### 3.3 Contratos por Capacidad (Traits en Rust) y Fachada Síncrona

Conforme a la decisión de José (Coloquio T1207), el Gateway formaliza contratos tipados completos en Rust para las **cinco capacidades**, donde cada proveedor expone su `ProviderLocality` y la sensibilidad viaja en la petición:

```rust
// ── 1. EMBEDDINGS ────────────────────────────────────────────────────────────
#[async_trait::async_trait]
pub trait EmbedProvider: Send + Sync {
    async fn embed(&self, texts: &[String], sensitivity: DataSensitivity) -> Result<Vec<Vec<f32>>, InferenceError>;
    fn model_identity(&self) -> ModelIdentity;
    fn provider_locality(&self) -> ProviderLocality;
    fn provider_id(&self) -> &'static str;
}

// ── 2. RERANKING ─────────────────────────────────────────────────────────────
#[async_trait::async_trait]
pub trait RerankProvider: Send + Sync {
    async fn rerank(&self, query: &str, candidates: &[String], top_k: usize, sensitivity: DataSensitivity) -> Result<Vec<RerankScore>, InferenceError>;
    fn model_identity(&self) -> ModelIdentity;
    fn provider_locality(&self) -> ProviderLocality;
    fn provider_id(&self) -> &'static str;
}

// ── 3. DECISIÓN (DIALECTOS SYSTEMONE Y OPENAI DECISIONS) ───────────────────────
#[async_trait::async_trait]
pub trait DecideProvider: Send + Sync {
    async fn decide(&self, req: &DecisionRequest) -> Result<HashMap<String, DecisionAnswer>, InferenceError>;
    fn dialect(&self) -> DecisionDialect;
    fn model_identity(&self) -> ModelIdentity;
    fn provider_locality(&self) -> ProviderLocality;
    fn provider_id(&self) -> &'static str;
}

// ── 4. GENERACIÓN DE TEXTO ───────────────────────────────────────────────────
#[async_trait::async_trait]
pub trait GenerateProvider: Send + Sync {
    async fn generate(&self, req: &GenerateRequest) -> Result<GenerateResponse, InferenceError>;
    fn model_identity(&self) -> ModelIdentity;
    fn provider_locality(&self) -> ProviderLocality;
    fn provider_id(&self) -> &'static str;
}

// ── 5. VISIÓN MULTIMODAL ─────────────────────────────────────────────────────
#[async_trait::async_trait]
pub trait VisionProvider: Send + Sync {
    async fn analyze(&self, req: &VisionRequest) -> Result<VisionResponse, InferenceError>;
    fn model_identity(&self) -> ModelIdentity;
    fn provider_locality(&self) -> ProviderLocality;
    fn provider_id(&self) -> &'static str;
}
```

*Fachada Síncrona Byte-Idéntica:* Para no obligar a refactorizar masivamente las decenas de call-sites síncronos (ej. `memory/silva/search.rs` L593, `transport/server/handler_recall.rs` L541, `transport/server/handler_remember.rs` L148; recuento en §2), el Gateway provee `embed_coalesced_sync(&self, text: &str)` compatible con Tokio `block_in_place`.

### 3.4 Identidad del Modelo: Reutilización de `engine_fingerprint_v1`

En lugar de inventar una estructura paralela no probada, `ModelIdentity` **reutiliza formalmente el diseño de `engine_fingerprint_v1` ya implementado en `embeddings.rs:405-464`**:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub model_id: String,                   // Ej: "mxbai-embed-large-v1"
    pub family: String,                     // Ej: "mxbai", "smollm2", "decima"
    pub dimensions: Option<u32>,            // Ej: Some(1024)
    pub quantization: String,               // Ej: "fp32", "q4_k_m", "int8"
    pub engine_fingerprint: Option<String>, // sha256:<64 hex> de embeddings.rs:443
}
```

### 3.5 El Problema Especial de las Embeddings: 1024d, Dual-Space Planificado y Reindexación

La tabla `node_embeddings` almacena hoy una única fila por nodo (`node_id PRIMARY KEY`). Cambiar de modelo o proyectar vectores en distintos espacios vectoriales invalida matemáticamente la similitud de toda la base de datos si se mezclan.

Conforme a la decisión de José (Coloquio T1204), el soporte de **varios espacios vectoriales (Dual-Space: espacio local garantizado + espacio opcional en la nube)** es **diseño planificado de la arquitectura**, no un gancho futuro. El requisito de benchmark previo fue formalmente retirado.

#### A. Principios del Diseño Dual-Space:
1. **Espacios Métricos Incompatibles:** Un vector generado por un modelo local (ej. `mxbai-embed-large-v1` a 1024d) y uno generado por un proveedor en la nube (ej. `text-embedding-3-large`) no pueden compararse mediante producto escalar ni distancia coseno. Coexisten en espacios vectoriales disjuntos e independientes.
2. **Garantía Offline Obligatoria:** El espacio local soberano (ONNX / FastEmbed local) debe estar siempre generado y disponible para garantizar la capacidad de búsqueda semántica offline sin red (el médico en zona rural). La nube es estrictamente un suplemento opcional, nunca una dependencia dura.
3. **Control de Privacidad Estricto:** Enviar texto de nodos con `DataSensitivity::Clinical` hacia un proveedor de embeddings en la nube requiere aprobación explícita humana por petición (HITL, §1.A.5). Para `DataSensitivity::Private` rige la matriz de §3.2.C (denegado por defecto salvo opt-in explícito del operador). `Public` puede embeberse en la nube cuando hay conexión.
4. **Adopción Rigurosa:** Cualquier modelo candidato para embeddings (sea local o nube) debe superar el procedimiento general de adopción de la Sección 4 (licencia, revisión fijada, evaluación empírica con datos propios).

#### B. Fases de Entrega (Recomendación Técnica del Tech Lead):
* **Fase 1 (Espacio Local Soberano de 1024d):**
  1. En esta **primera entrega**, el Gateway **acepta únicamente modelos locales de exactamente 1024 dimensiones** (`bge-m3`, `mxbai-embed-large-v1`). Esto no constituye una regla permanente del sistema, sino el alcance estrictamente acotado para la primera entrega para garantizar estabilidad.
  2. Registra la identidad canónica completa (`ModelIdentity`) y el `engine_fingerprint_v1` en metadatos del índice.
  3. Rechaza cualquier intento de configurar un modelo con dimensiones $\ne 1024$ con un error claro de configuración.
  4. **Proveedores válidos para embeddings:** OpenAI-compatible (`/v1/embeddings`), Ollama (`/api/embed`) y FastEmbed/ONNX local. *Nota:* Anthropic no ofrece API de embeddings.
  5. En esta fase inicial, un cambio de modelo local se resuelve con el endpoint de reindexación existente `POST /api/v1/memory/reindex` (`routes.rs` L66 → `reindex_handler` en `api_memory.rs` L74), que re-embebe en segundo plano los nodos cuyo `model_id`/`model_hash` no coincide con el motor cargado. No existe hoy un subcomando `reindex` en `crates/tylluan-cli`; añadirlo sería opcional y fuera del alcance de este ADR.

* **Fase 2 (Soporte Dual-Space y Tabla Paralela):**
  1. **Evolución del Esquema en SilvaDB:** Se migra la tabla de almacenamiento hacia una clave compuesta `(node_id, space_id)` o tabla paralela `node_embeddings_cloud`, permitiendo almacenar un vector por cada espacio activo sin pisar el espacio local soberano.
  2. **Indexación en Segundo Plano:** El cálculo del espacio en la nube se ejecuta como proceso de fondo no bloqueante.
  3. **Conmutación Atómica con Rollback:** Una vez alcanzada la cobertura del 100% de nodos en el segundo espacio, el kernel conmuta el índice de búsqueda HNSW de forma atómica; si la red falla o el proveedor degrada latencia/precisión, el sistema revierte instantáneamente y en caliente al espacio local soberano.

### 3.6 Política SSRF para Nodos de Inferencia en LAN (Servidor de Inferencia Secundario)

Hoy `check_external_provider` (`config.rs` L943, `v4.is_private()`) rechaza las IPv4 RFC-1918. Eliminar el bloqueo sin más convertiría el arnés en un vector de SSRF contra routers e intranets.

**Loopback y el `llama-server` local:** el bloqueo RFC-1918 **no afecta** a la máquina propia. El mismo guard acepta explícitamente `localhost`, `127.0.0.1` y `::1`, y cualquier IP con `is_loopback()`, *antes* de la comprobación `is_private()` (en Rust, `Ipv4Addr::is_private()` cubre solo `10/8`, `172.16/12` y `192.168/16`; el loopback `127/8` no es "privado" en ese sentido). Por eso el subproceso local `llama-server` en `127.0.0.1` sigue permitido, y es la única excepción "servidor" de §1.B.

**Solución Propuesta: Host Pinning Explícito Aprobado por el Operador:**
1. **Configuración declarativa puntual en `tylluan.toml`:**
   ```toml
   [[inference.allowed_lan_endpoints]]
   host = "192.168.1.50"
   port = 11434            # puerto por defecto de Ollama
   label = "Servidor Ollama en workstation LAN"   # ejemplo ilustrativo
   max_sensitivity = "Private"
   allowed_spaces = ["shared", "research"]
   ```
2. **Reglas de Aprobación Estricta:**
   - **Exención puntual:** solo el par `host`+`port` exacto de cada entrada queda exento de la comprobación `is_private()`; cualquier otra IP RFC-1918 sigue rechazada como hoy.
   - **Cero Comodines:** Prohibidos rangos CIDR (`192.168.1.0/24`) o wildcards (`*`).
   - **Loopback:** se mantiene el comportamiento actual (loopback aceptado, ver arriba); los endpoints en loopback no necesitan entrada en `[[inference.allowed_lan_endpoints]]` y se clasifican como `ProviderLocality::Local`.
   - **Metadata, link-local y wildcard (propuesta):** se mantienen los bloqueos actuales de `169.254.169.254` y `0.0.0.0`, y se **amplía** el bloqueo a todo `169.254.0.0/16` (hoy solo se bloquea el host literal de metadata). Ninguna entrada de la lista puede apuntar a estos rangos.
   - **Nombres de host y DNS rebinding (propuesta):** el guard actual no resuelve nombres. Para endpoints LAN se propone: resolver el nombre antes de conectar, exigir que la IP resuelta coincida con la IP autorizada en la entrada, y desactivar redirecciones automáticas (`reqwest::redirect::Policy::none()`).

### 3.7 El Contrato Neutro de Decisión y los Dos Dialectos (`/v1/systemone` y `/v1/decisions`)

Conforme a la decisión de José (Coloquio T1207), el Gateway soporta **ambos dialectos de decisión desde el primer día**:
1. `POST /v1/systemone`: dialecto TypeSafe, servido localmente por `llama.cpp` (PR 29818, fuente secundaria según la nota de investigación) y **validado en este repo con Clef-Flash** mediante el cliente HTTP ya existente de ADR-018 variante B (`crates/tylluan-kernel/examples/decision_fabric_spike.rs`, `SystemOneDecisionProvider`).
2. `POST /v1/decisions`: dialecto de OpenAI (beta pública, documentación oficial en https://developers.openai.com/api/docs/guides/decisions; solo `gpt-6-luna`), consumido como endpoint en la nube.

#### A. Contrato Interno Neutro en Rust
Para desacoplar el resto del kernel de los detalles de red de cada proveedor, el Gateway define un vocabulario común unificado:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionDialect {
    SystemOne,       // /v1/systemone (llama.cpp, Jev, Clef-Flash local)
    OpenAiDecisions, // /v1/decisions (OpenAI beta, gpt-6-luna nube)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub context_state: String,                         // Contexto base (mapea a `state` en SystemOne o a `input` en OpenAI)
    pub images: Vec<ImageSource>,                      // Opcional; SystemOne: `images`/`assets`; OpenAI: partes `input_image` en base64
    pub questions: Vec<DecisionQuestion>,              // Preguntas tipadas con nombre e instrucciones
    pub sensitivity: DataSensitivity,                  // Sensibilidad calculada (Public, Private, Clinical)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionQuestion {
    pub name: String,                                  // Identificador único (clave del mapa en SystemOne; campo `name` en OpenAI)
    pub instructions: String,                          // Obligatorio en ambos dialectos (SystemOne responde HTTP 400 si falta)
    pub kind: DecisionQuestionKind,                    // Tipo específico de decisión
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionQuestionKind {
    Predicate,                                         // Sí / No (`noul` en SystemOne, `predicate` en OpenAI)
    Choice { choices: Vec<ChoiceOption> },             // Opciones (`criteria` en SystemOne, `choices[]{value,description}` en OpenAI)
    Score { levels: Vec<ScoreLevel> },                 // Niveles ordenados de menor a mayor (`criteria` en SystemOne, `levels[]{label,description}` en OpenAI)
    Box { prompt: String },                            // Grounding visual (solo dialecto SystemOne)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreLevel {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionAnswer {
    Predicate { probability: f32 },
    Choice { choice: String, probabilities: HashMap<String, f32>, confidence: Option<f32> },
    Score { score: String, probabilities: HashMap<String, f32>, confidence: Option<f32> }, // nivel elegido; tipo exacto de `score` en OpenAI a confirmar al implementar
    Box { label: String, coordinates: [f32; 4] },
    Refusal,                                           // El modelo rehúsa esa pregunta; las demás siguen contestadas
}
```

#### B. Mapeo de Dialectos y Diferencias de Especificación

Fuentes: OpenAI, guía oficial de Decisions (leída 2026-10-16). SystemOne, el cliente real del repo (`examples/decision_fabric_spike.rs`, `SystemOneDecisionProvider` y `systemone_question_spec`, probado contra un servidor real con Clef-Flash) más la nota `docs/reference/research/2026-10-15_decision_models_industry_integration.md` (que cita la documentación de LiteLLM). Lo que solo viene de la nota se marca *[nota]*.

| Aspecto | Dialecto `/v1/systemone` (local) | Dialecto `/v1/decisions` (OpenAI, nube) |
|---|---|---|
| **Petición (nivel superior)** | `model`, `state`, `questions` (**mapa** `{ "<name>": pregunta }`); `images`/`assets` opcionales *[nota]* | `model`, `input` (texto o mensajes de usuario con `input_text` + `input_image`), `questions` (**array**) |
| **Campos de cada pregunta** | `type`, `instructions` (obligatorio: el servidor devuelve HTTP 400 si falta), opciones/niveles en `criteria` | `type`, `name`, `instructions` |
| **Tipos de pregunta** | `noul` \| `choice` \| `score` \| `box` | `predicate` \| `choice` \| `score` |
| **Opciones de `choice`** | `criteria`: mapa `etiqueta → descripción` (la descripción puede ser `null`) | `choices[]`, cada una `{ value, description }` |
| **Niveles de `score`** | `criteria` con los niveles | `levels[]`, cada uno `{ label, description }`, ordenados de menor a mayor |
| **Respuesta** | `answers`: mapa por nombre; cada respuesta con `probabilities` (mapa por opción) y, para `noul`, el campo opcional `noul` (probabilidad) | `answers[]` con `type` y `name` en cada respuesta. `predicate`: `probability`. `choice`: `choice`, `probabilities[]{value, probability}`, `confidence`. `score`: `score`, `probabilities[]{value, label, probability}`, `confidence` |
| **Rechazo** | No observado en el cliente del repo; *sin verificar* si el servidor SystemOne lo emite | Tipo de respuesta `refusal` (con `name`); la guía no documenta más campos |
| **Grounding visual** | `box` | No existe tipo equivalente (las imágenes sí se aceptan, solo en base64 inline) |
| **Modelos** | Clef-Flash (validado en el repo); otros runtimes según la nota | Solo `gpt-6-luna` |
| **Límites** | *[nota, vía LiteLLM]* 1–128 preguntas por petición, hasta 255 opciones en `choice`, 2–10 niveles en `score`, sin streaming | La guía oficial no declara máximos |

> [!NOTE]
> **Rechazo frente al valor 0,5:** el rechazo es un **tipo de respuesta** del dialecto `/v1/decisions` (`refusal`) y el contrato neutro lo representa como `DecisionAnswer::Refusal`. El valor 0,5 **no** pertenece a ninguna de las dos APIs: es el comportamiento por defecto del arnés experimental del preprint arXiv 2609.28940 (cuando la capa de decisión no responde, el arnés usa 0,5). No se usa como semántica de abstención en este ADR.

#### C. Consultas No Portables y Semántica de Fuga
1. **Preguntas Visuales (`box`):** El delimitador espacial (`box`) disponible en Jev/SystemOne cuando se suministran activos de imagen **no es portable a `/v1/decisions`**. Si una consulta requiere `box`, el Gateway la enruta exclusivamente a un proveedor `SystemOne` local con soporte visual o devuelve un error tipado `InferenceError::UnsupportedCapability`.
2. **Privacidad Estricta de Datos Clínicos:** Dado que `/v1/decisions` es un servicio en la nube (`ProviderLocality::Cloud`), cualquier consulta cuya sensibilidad sea `DataSensitivity::Clinical` (incluida la heredada de nodos clínicos recuperados, §3.2.B) **exige la aprobación explícita humana por petición (HITL)** vía el patrón existente `/api/v1/dispatches/{id}/approve` / `/reject`. Las consultas `Private` siguen la matriz de §3.2.C (denegadas por defecto salvo opt-in explícito del operador). Queda terminantemente prohibida la conmutación silenciosa o no auditada de local a nube.

---

### 3.8 El Contrato de Visión (`VisionProvider`) y la Relación con los Guilds Python

Conforme a la decisión de José (Coloquio T1207), el Gateway unifica la capacidad multimodal bajo el trait `VisionProvider`, asumiendo formalmente las tareas que hoy realizan los gremios `guilds/core/vision.py` (SmolVLM2 ONNX) y `guilds/core/vision_moondream.py` (Moondream 0.5B).

#### A. Contrato Tipado en Rust

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionRequest {
    pub image: ImageSource,             // Archivo local, bytes en memoria o base64
    pub task: VisionTask,               // Tarea específica solicitada
    pub sensitivity: DataSensitivity,   // Sensibilidad calculada (Public, Private, Clinical)
    pub max_tokens: Option<u32>,        // Límite de generación (ej. 48 descripción, 192 OCR)
    pub max_image_size_bytes: usize,    // Cap anti-DoS (defecto: 10 MB)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSource {
    Path(std::path::PathBuf),
    Bytes(Vec<u8>),
    Base64(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisionTask {
    Analyze { prompt: String },         // Corresponde a vision_analyze / analyze_image
    Caption,                            // Corresponde a caption_image
    Ocr,                                // Corresponde a vision_ocr (extracción completa de texto)
    Extract { schema: String },         // Corresponde a vision_extract (salida estructurada JSON)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResponse {
    pub text: String,
    pub extracted_json: Option<serde_json::Value>,
    pub bounding_boxes: Option<Vec<BoundingBox>>,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub label: String,
    pub box_2d: [f32; 4],               // [ymin, xmin, ymax, xmax] normalizado 0.0..1.0
    pub confidence: f32,
}
```

#### B. Reglas Operativas y de Seguridad de Visión:
1. **Límites de Carga y Anti-DoS:** La carga máxima de una imagen está fijada por defecto en **10 MB**. Imágenes que excedan el límite son rechazadas antes de procesar con `InferenceError::PayloadTooLarge`. Se validan cabeceras mágicas para formatos soportados (PNG, JPEG, WebP).
2. **Semántica de Fallo y Resiliencia GPU:** En Windows, la GPU puede sufrir caídas de TDR (incidente documentado en `CHANGELOG.md`, entrada v0.15.0: el guild de visión se pasó a CPU por ese motivo). El proveedor local de visión está obligado a implementar fallback suave a `CPUExecutionProvider` (usando `SmolVLM2-256M-Instruct` in-process o Moondream local) sin colapsar el proceso del kernel.
3. **Clase de Privacidad para Imágenes Médicas:** Cualquier imagen proveniente de espacios clínicos (radiografías, resonancias magnéticas, ecografías, fotos de pacientes) se marca con `DataSensitivity::Clinical`. **Queda prohibido enviar imágenes clínicas a proveedores en la nube (`ProviderLocality::Cloud`) sin aprobación humana explícita por petición.**
4. **Relación con los Guilds Python:** Los gremios `vision.py` y `vision_moondream.py` se mantienen como adaptadores FastMCP de compatibilidad hacia herramientas externas, pero internamente consumen la configuración soberana de `[inference.profiles.vision]` en `tylluan.toml`.

---

### 3.9 Canal de `instructions` por pregunta (T1252, 2026-10-18)

**Estado previo (hallazgo del duelo P2, 2026-10-18):** `QuestionSpec.instructions` ya existía en el wire (§1217: 1 KiB por pregunta, validado en `validate_request`), pero el canal estaba CORTADO: `to_core_question` (`api_types.rs`) lo descartaba al traducir al núcleo y `question_spec` del proveedor (`systemone_http.rs`) generaba un texto fijo propio ("Answer the {name} question about this state."). Es decir: el wire aceptaba instructions, validaba su longitud… y jamás llegaban al modelo.

**Decisión (T1252):** reabrir el canal extremo a extremo **sin cambiar el comportamiento cuando no hay instructions**.

1. **Núcleo (`decision/mod.rs`):** nuevo struct `CoreQuestion { kind: DecisionQuestion, instructions: Option<String> }`. `DecisionRequest.questions` pasa a `HashMap<String, CoreQuestion>`. El enum `DecisionQuestion` queda INTACTO (las reglas deterministas de `rules.rs` solo cambian el acceso de campo: deciden sobre `kind`, jamás sobre `instructions`).
2. **Traducción (`to_core_question`):** las instructions crudas del wire viajan SIN interpretar hasta la frontera del proveedor — sin validar ni sustituir aquí.
3. **Frontera no-confiable (`systemone_http::resolve_instructions`):**
   - ausente/vacío tras saneado → texto por defecto **byte idéntico** al previo a T1252 (los tres tipos de pregunta);
   - caracteres de control eliminados (se conservan `\t`, `\n`, `\r` — formato legítimo de prompt);
   - longitud > `MAX_PROVIDER_INSTRUCTIONS_BYTES` (1024 bytes UTF-8, tope propio independiente del del wire, defensa en profundidad) → se IGNORA y cae al texto por defecto con `warn` en tracing — **nunca se trunca** (un recorte a medias es un prompt partido);
   - el resto del spec System One (`type`, `criteria`) no se toca.
4. **Alcance:** carril libre y sombra de contrato (`to_core_question` en ambos) — la respuesta, la autoridad de reglas, el rechazo de valores fuera de opciones y el invariante v8 (la sombra jamás altera la respuesta emitida) quedan SIN cambio. Los tests nuevos lo blindan: passthrough verbatim, defecto pre-T1252 exacto, sobre-tope→defecto, saneado de controles (unidad) + E2E carril libre con captura del body en el mock (con y sin instructions).

---

## 4. Origen y Ciclo de Vida de Modelos

### 4.1 Catálogo Declarativo como Fichero de Datos
Hoy `maintenance.rs:41-85` contiene una lista fija de modelos codificada en Rust (`SmolLM2`, `Qwen2.5`, `Nomic-Embed`), comprobando solo el tamaño esperado pero **sin validar hash SHA-256 de integridad**.

Se propone desacoplar el catálogo hacia un fichero de datos (`models.toml` o `models.json` en `config/`) con esquema versionado:
* `id`: Identificador único canónico (ej. `smollm2-135m-instruct-q4`).
* `capability`: `generate`, `embed`, `decide`, `rerank`, `vision`.
* `source`: URL fija a HuggingFace hub o repositorio de procedencia verificado.
* `revision_sha`: Hash commit exacto de la revisión de HuggingFace.
* `weights_sha256`: Hash SHA-256 criptográfico para verificación de integridad de descarga.
* `license`: SPDX identifier (ej. `Apache-2.0`, `MIT`).
* `dimensions`: `Option<u32>`.
* `disk_mb` y `ram_required_mb`.
* `status`: `experimental` | `adopted` | `rejected`.

### 4.2 Tres Vías de Obtención y Procedimiento de Adopción
1. **Descarga con consentimiento explícito:** El operador solicita la descarga; el kernel verifica el `weights_sha256` tras recibir los bytes.
2. **Fichero propio validado:** Pesos suministrados por el operador en `models/`; el kernel valida que su fingerprint coincida con un candidato aprobado.
3. **Proveedor externo por red:** Conexión saliente a un servicio configurado.

*Procedimiento de Adopción de Candidatos:*
Licencia compatible → Fijación de revisión y hash SHA-256 → Compatibilidad con runtime (`ort`/`llama.cpp`) → Medición en benchmark con datos propios de Tylluan → Modo sombra (shadow mode) en producción → Veredicto GO/NO-GO documentado. Revisión periódica por el equipo; **cero actualizaciones automáticas desatendidas** para prevenir roturas de compatibilidad.

### 4.3 El Paquete Offline Limpio y Autocontenido (El Caso del Médico Rural)
Para el escenario de campo sin conexión (médico rural en zona sin red):
* Se suministra un **paquete offline autocontenido**: binario `llama-server` fijado y compilado para la plataforma, carpeta `models/` pre-cargada con el modelo GGUF y embeddings locales, más el runtime de Tylluan.
* **Operación autónoma:** Tylluan + `llama.cpp` + SQLite local operan 100% desconectados sin requerir conexión a internet ni descargas dinámicas con pip (reemplazando el mecanismo frágil de `llama_backend.py`).
* **Sincronización:** Al conectarse por la noche en el hotel con Wi-Fi, el nodo accede a la red del hospital **mediante la VPN propia del hospital** (camino base aprobado; `iroh` archivado como NO-GO).
* **Pregunta Abierta:** ¿Cómo federar nodos con perfiles de embedding dispares (hospital con servidor 1024d vs portátil de campo en perfil clinic de 384d)? Los nodos intercambian contenido textual (`sync_spaces`), pero cada nodo debe computar sus propios vectores en su dimensión configurada; la mezcla de espacios vectoriales heterogéneos permanece prohibida.

---

## 5. Análisis de Dependencias: `reqwest` vs Librerías Externas

| Criterio | `reqwest` Nativo (Existente) | `rig` (`0xPlaygrounds/rig`) | `genai` (Jeremy Chone) | `async-openai` |
|---|---|---|---|---|
| **Presencia en `Cargo.toml`** | **YA EXISTE** (`reqwest = 0.12`, `rustls-tls`, `Cargo.toml` L106) | Nueva dependencia externa | Nueva dependencia externa | Nueva dependencia externa |
| **Compatibilidad con la toolchain fijada (1.88)** | Ya compila en el árbol actual | *Sin verificar — requiere spike* | *Sin verificar — requiere spike* | *Sin verificar — requiere spike* |
| **Cumplimiento `deny.toml`** | Ya forma parte del árbol que `deny.toml` audita | *Sin verificar — requiere auditar el árbol transitivo* | *Sin verificar* | *Sin verificar* |
| **Cobertura de proveedores** | Las 3 familias ya probadas en `test_external_provider` (`OpenAI`, `Anthropic`, `Ollama`) | Orientado a agentes | Multiproveedor | Centrado en la API de OpenAI |
| **Código nuevo requerido** | Adaptadores propios; tamaño *no estimado con datos* | Menos código propio, más superficie de dependencia | Ídem | Ídem, más clientes adicionales para el resto |
| **Peso en binario** | Sin dependencia nueva; el código de los adaptadores *no medido* | *NO MEDIDO — requiere spike* | *NO MEDIDO — requiere spike* | *NO MEDIDO — requiere spike* |

**Directriz de Dependencias:**
1. **La Hipótesis por Defecto es la Oficial:** `reqwest` nativo (con `rustls-tls` y `json`), junto con las tres familias ya probadas en `api_admin.rs:125-185`, es suficiente para gobernar OpenAI, Anthropic y Ollama.
2. **Requisito Obligatorio:** Queda prohibida la adopción de `Rig`, `genai` o `async-openai` sin un spike de coste previo (verificando compatibilidad con la toolchain fijada en `rust-toolchain.toml`, cumplimiento estricto de `deny.toml` y peso medido en binario).
3. **LiteLLM:** Se admite únicamente como un proxy HTTP externo opcional que el usuario puede configurar voluntariamente; **nunca como dependencia interna del kernel**.

---

## 6. Comparativa de Alcance y Estimación de Esfuerzo

El informe técnico externo estimó entre **30 y 56 días-persona** (o hasta 70 con contingencia) para la infraestructura de inferencia. Este ADR no dispone de los supuestos de alcance detallados del informe y por tanto **no** le atribuye ninguno ni explica la diferencia por ellos. Se limita a describir el alcance que asume la estimación propia, para que la comparación se pueda hacer cuando se conozcan los supuestos del informe:

### Alcance que Asume la Estimación de este ADR:
1. **Proveedores y dependencias:** la fase inicial se acota a las tres familias ya probadas en el kernel (`OpenAI`, `Anthropic`, `Ollama`) sobre el `reqwest` existente, sin crates nuevos.
2. **Embeddings y migración:** primera entrega con un único espacio local de 1024d y reindexación mediante el endpoint existente (`POST /api/v1/memory/reindex`, `routes.rs` L66); el segundo espacio con conmutación atómica queda en una fase posterior.
3. **Runtime local:** se reutiliza lo que ya existe (`fastembed`, sesiones ONNX `ort` in-process, subproceso `llama-server`) sin reemplazar la pila local.
4. **Fuera de la estimación:** el adaptador `/v1/decisions`, el contrato de visión completo y el paquete offline no están desglosados en la tabla siguiente; su esfuerzo queda sin estimar.

### Estimación del Autor, sin Medir (Aproximación Cualitativa sin Registro Histórico de Horas):
Al construir sobre la base existente (`reqwest`, `InferenceBudget`, `ExternalProvider`) y acotar la primera entrega a 1024d estricto con reindexación offline:

| Fase | Alcance Propuesto | Estimación del Autor (sin medir) |
|---|---|---|
| **Fase 1 (MVP Egress-Only)** | Traits de capacidad, Fachada síncrona/asíncrona, Adaptadores nativos `reqwest` (OpenAI, Anthropic, Ollama), FastEmbed local, Host Pinning seguro en LAN, 1024d estricto. | **5–8 días-persona** |
| **Fase 2 (Decisión y Gobernanza)** | Adaptador `/v1/systemone`, perfiles de configuración por función, integración con `InferenceBudget` y auditoría. | **4–6 días-persona** |
| **Fase 3 (Reindexación Dual en Caliente)** | Migración de esquema en SilvaDB para doble vector y conmutación atómica. | **5–7 días-persona** (diferida) |
| **Total Estimado Fases 1 y 2:** | **9 a 14 días-persona** (aproximación técnica sujeta a validación real). |

---

## 7. La Sexta Herramienta Soberana: Decisión Formal de José

* **Estado de la Decisión:** **APROBADA POR JOSÉ (2026-10-15, Coloquio T1197).**
* **Resolución:** CONTRACT-01 evoluciona formalmente de 5 a **6 herramientas soberanas**.
* **Alcance:** El diseño de la infraestructura del Gateway en este ADR-020 provee el sustrato técnico para alimentar las capacidades de inferencia. La firma, contrato tipado, parámetros y comportamiento de la nueva 6ª herramienta soberana (orientada a decisiones semánticas estructuradas) se definen formalmente en el **ADR-021** (redactado por el Tech Lead).

---

## 8. Marcado de Veracidad

- **VERIFICADO:**
  - Struct `ExternalProvider` con `OpenAICompatible`, `AnthropicCompatible`, `OllamaCompatible` en `config.rs:862-881`.
  - Código de prueba HTTP en `test_external_provider` (`api_admin.rs:125-185`).
  - Bloqueo SSRF de RFC-1918 en `config.rs:943` (`v4.is_private()`).
  - Huella determinista `engine_fingerprint_v1` en `router/embeddings.rs:405-464`.
  - Esquema `node_embeddings` con `node_id TEXT PRIMARY KEY` y `dimensions INTEGER DEFAULT 1024` en `schema.rs:380-388`.
  - Dependencia `reqwest = 0.12` con `rustls-tls` en `Cargo.toml:106`.
  - Struct `EmbeddingEngine` (`router/embeddings.rs` L186) y `embed_batch_coalesced` (L820); recuento de call-sites en §2.
  - Ausencia de embeddings en perfil `portable` (`vector_dimensions = 0`) en `tylluan-cli/src/main.rs` L1084 (tests en L1568).
  - Endpoint de reindexación `POST /api/v1/memory/reindex` (`routes.rs` L66, `api_memory.rs` L74); sin subcomando `reindex` en `tylluan-cli`.
  - Guard SSRF: loopback aceptado antes de la comprobación RFC-1918 (`config.rs` L935–L947).
  - `PeerPermissions { sync_spaces, dispatch_allow }` en `federation/permissions.rs` (no se reutiliza para inferencia).
  - Rutas HITL `/api/v1/dispatches/{id}/approve` y `/reject` (`routes.rs` L286–L287).
  - Toolchain fijada `1.88` en `rust-toolchain.toml`.
   - Wire SystemOne (`model`, `state`, `questions` como mapa, `instructions` obligatorio, `criteria`) en `examples/decision_fabric_spike.rs`.
   - Canal `instructions` reabierto T1252 (2026-10-18): `CoreQuestion` en `decision/mod.rs`, `to_core_question` con passthrough en `api_types.rs`, `resolve_instructions` con tope 1024 B y saneado en `systemone_http.rs`, E2E con captura de body en `tests/decide_systemone_http.rs` (`free_lane_forwards_wire_instructions_to_the_model`, `free_lane_without_instructions_keeps_the_default_body`) — todo verde con `cargo test` + `cargo clippy -D warnings`.
  - Nombres de campo de OpenAI Decisions según la guía oficial (leída 2026-10-16).
- **SIN VERIFICAR (solo fuente secundaria):**
  - Límites 1–128 preguntas / 255 opciones / 2–10 niveles, y `images`/`assets` en SystemOne (nota de investigación, vía LiteLLM).
  - Si el servidor SystemOne emite respuestas de tipo `refusal`.
- **NO MEDIDO / REQUIERE SPIKE:**
  - El impacto exacto en binario y dependencias transitivas de `Rig`, `genai` y `async-openai`.
  - El tamaño en líneas y en binario de los adaptadores propios sobre `reqwest`.
  - La estimación de esfuerzo de 9 a 14 días-persona (aproximación cualitativa del autor).

---

## 9. Decisiones que Siguen Abiertas para José

1. **Federación con Dimensiones Vectoriales Dispares:** ¿Aprobar que nodos federados con perfiles dispares (ej. hospital 1024d vs portátil 384d) sincronicen exclusivamente contenido textual (`sync_spaces`), computando cada nodo sus vectores locales de forma independiente?

---

## Impact

> Apartado exigido por el TL para el cierre de T1252 (Coloquio turno 1252).

- **Qué cambia:** solo el canal de prompt `instructions` del Decision Fabric: `CoreQuestion` (núcleo), passthrough en `to_core_question`, `resolve_instructions` en la frontera del proveedor `systemone_http`. Ficheros: `crates/tylluan-kernel/src/router/decision/{mod,api_types,systemone_http,rules,service}.rs`, `examples/decision_fabric_spike.rs` (solo acomodos mecánicos de tipo), `tests/decide_systemone_http.rs` (mock captura body + 2 tests nuevos).
- **Qué NO cambia (verificado con la batería de tests):** comportamiento sin instructions (texto por defecto byte idéntico, test `absent_instructions_keep_the_pre_t1252_default` + E2E `free_lane_without_instructions_keeps_the_default_body`); reglas deterministas (deciden sobre `kind`, sin tocar umbrales ni lógica); invariante v8 (sombra jamás altera la respuesta — `decide_systemone_http` v8/rejection en verde); esquema wire §1217 intacto (sin cambios en `DecideRequest`/`QuestionSpec`); cero cambio en la superficie MCP/REST.
- **Seguridad:** instructions se tratan como contenido NO confiable en la frontera del proveedor — saneado de caracteres de control y tope de 1024 bytes propio (defensa en profundidad frente al tope de wire), con fallback al defecto (nunca truncado). Sin ruta de escape del spec JSON: el texto viaja como valor serializado por `serde_json`.
- **Verificación:** `cargo check -p tylluan-kernel --all-targets`, `cargo test` (lib decision 40 + `decide_systemone_http` 13 + `decide_adversarial` 4 + `decide_parity_e2e` 20, todos en verde), `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` sin avisos.
- **Fuera de alcance (orden del TL):** NO se re-miden redacciones de P2 (esperando Buffy v2); ninguna medición del duelo se toca.
