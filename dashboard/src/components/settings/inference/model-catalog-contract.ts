/**
 * Contrato formal del catálogo vivo de modelos de Tylluan (ADR-020 v8 / Directiva José T3140 + T3144 + T3149 + T3166 + T3168).
 * Backend = Fuente de verdad única de disponibilidad.
 * Frontend = Consumidor inteligente sin listas estáticas ni mocks inventados.
 *
 * Principios maestros (José T3156, T3157, T3166, T3168):
 * - "Tylluan es el dueño del estado del agente. El router y los modelos de decisión
 *   son clientes/consumidores que leen y escriben en Tylluan por API estable."
 * - Suelo vs Techo: Los modelos de Tylluan (origin='tylluan') forman el SUELO mínimo
 *   garantizado y computan en el tope de 3072 MB. Los modelos del operador (origin='user')
 *   forman el TECHO opcional y NO computan en el presupuesto de Tylluan.
 * - Métrica de presupuesto: RAM PRIVADA (PrivateMemorySize64), no Working Set.
 */

export type ModelType = 'generative' | 'decision' | 'embedding' | 'reranker' | 'guard';
export type DecisionInterface = 'Choice' | 'Noul' | 'Score';
export type LocalityType = 'local' | 'remote';
export type ExecutionMode = 'local_ephemeral' | 'local_persistent' | 'external_server' | 'cloud';
export type ModelOrigin = 'tylluan' | 'user';

export type ModelCapability = 'reasoning' | 'coding' | 'tools' | 'json_output' | 'vision';
export type ModelRole = 'chat' | 'decision' | 'consensus' | 'sentinel' | 'grammar_judge' | 'embedding' | 'reranker' | 'router';

export interface DecisionMetadata {
  /** Interfaz de decisión tipada: Choice (N opciones), Noul (sí/no), Score (escala ordenada) */
  interface: DecisionInterface;
  /** Número máximo de opciones evaluables en Choice (2 - 255) */
  max_options?: number;
  /** Si el modelo devuelve distribución de probabilidad calibrada 0.0 - 1.0 */
  calibrated_confidence: boolean;
  /** Latencia típica estimada o medida en pase único (ms) */
  typical_latency_ms?: number;
}

export interface ProviderHealthInfo {
  provider: string;
  available: boolean;
  unavailable_reason?: 'missing_api_key' | 'service_unreachable' | 'cooling_down' | null;
  endpoint?: string;
  last_checked_at?: number;
}

export interface ModelCatalogEntry {
  /** Identificador canónico del modelo en el router (ej. 'local::qwen2.5-coder-7b', 'jev::decision-v1') */
  id: string;
  /** Nombre visual legible para la interfaz */
  name: string;
  /** Origen obligatorio: 'tylluan' (suelo soberano <= 2-3GB) o 'user' (techo del operador) [Deep §1.3 / Jose T3166] */
  origin: ModelOrigin;
  /** Proveedor que sirve el modelo (llama.cpp, ollama, vllm, anthropic, openai, tylluan) */
  provider: string;
  /** Familia de modelos (ej. 'qwen', 'claude', 'gpt', 'gemma', 'drex', 'mxbai') */
  family: string;
  /** Versión semántica o generacional del modelo */
  version: string;
  /** Indica si es la versión más reciente/puntera de su familia (puebla el Nivel 1 del selector) */
  is_latest_in_family: boolean;
  /** Si el modelo está clavado (pinned) por directiva técnica (ej. DECISIÓN obligatoria) */
  pinned?: boolean;
  /** Tipo arquitectónico de modelo */
  model_type: ModelType;
  /** Metadatos adicionales para modelos de decisión */
  decision_metadata?: DecisionMetadata;
  /** Capacidades soportadas verificadas por el backend */
  capabilities: ModelCapability[];
  /** Ventana de contexto máxima en tokens */
  context_window: number;
  /** Ámbito de cómputo: local vs remoto */
  locality: LocalityType;
  /** Modo de ciclo de vida del proceso de inferencia */
  execution_mode: ExecutionMode;
  /** Verdad de disponibilidad calculada por el backend */
  available: boolean;
  /** Motivo estricto cuando available === false */
  unavailable_reason?: 'missing_api_key' | 'model_not_downloaded' | 'service_unreachable' | 'cooling_down' | 'unsupported_hardware' | null;
  /** Tamaño del archivo o peso en disco (MB) */
  size_mb?: number;
  /** Memoria RAM privada (PrivateMemorySize64) en runtime (MB) [Jose T3168] */
  ram_private_mb?: number;
  /** Memoria RAM privada pico (PrivateMemorySize64) medida durante arranque en frío efímero (MB) [Deep §1.3 / Jose T3168] */
  ram_peak_private_mb?: number;
  /** Roles soberanos en plural que puede desempeñar el modelo [Deep §1.3] */
  roles: ModelRole[];
}

export interface ModelCatalogResponse {
  /** Versión del contrato firmado (ej. '1.0.0') */
  contract_version: string;
  /** Timestamp de generación de la respuesta */
  generated_at: number;
  /** Timestamp del último escaneo real de proveedores */
  checked_at: number;
  /** Catálogo completo o filtrado de modelos */
  models: ModelCatalogEntry[];
  /** Salud y estado de los proveedores a nivel superior [Deep §1.3] */
  providers: ProviderHealthInfo[];
  /** ID del modelo de decisión actualmente asignado en el plano de decisión automático */
  active_decision_model?: string;
  /** Toggle general de enrutamiento automático */
  decision_plane_enabled: boolean;
  /** Umbral de confianza de corte (ej. 0.75) */
  decision_confidence_threshold: number;
  /** Presupuesto de memoria de los modelos internos de Tylluan (SUELO soberano tope 3072 MB privados) [Jose T3166, T3168] */
  tylluan_ram_budget: {
    total_allowed_mb: number;        // 3072 MB
    current_used_private_mb: number; // suma real de RAM privada de modelos Tylluan cargados
    current_peak_private_mb: number; // peor caso medido de dos efímeros simultáneos
  };
}

export interface LlamaServerStatus {
  running: boolean;
  pid?: number | null;
  port: number;
  endpoint: string;
  version?: string | null;
  models_directory?: string | null;
  update_available?: boolean;
  last_check_timestamp?: number | null;
}
