import type { ModelCatalogResponse, LlamaServerStatus } from './model-catalog-contract';

/**
 * Fixture de prueba del catálogo vivo de modelos según la arquitectura acordada
 * en T3140, T3144, T3149 (§1.3) y las directivas de Suelo/Techo y RAM Privada (T3166, T3168).
 */
export const FIXTURE_MODEL_CATALOG: ModelCatalogResponse = {
  contract_version: '1.0.0',
  generated_at: 1794102000,
  checked_at: 1794101950,
  active_decision_model: 'local::laya-8b',
  decision_plane_enabled: true,
  decision_confidence_threshold: 0.75,
  tylluan_ram_budget: {
    total_allowed_mb: 3072,         // 3072 MB de tope en RAM Privada (PrivateMemorySize64)
    current_used_private_mb: 1420,  // mxbai (610 MB priv) + jina (260 MB priv) + sentinel (550 MB priv)
    current_peak_private_mb: 2420   // peor caso de dos efímeros: sentinel (1220 MB) + modelo Grupo C (1200 MB)
  },
  providers: [
    { provider: 'llama.cpp (user)', available: true, endpoint: 'http://127.0.0.1:8080' },
    { provider: 'ollama (user)', available: true, endpoint: 'http://127.0.0.1:11434' },
    { provider: 'vllm (local)', available: true, endpoint: 'http://127.0.0.1:8000' },
    { provider: 'anthropic', available: true },
    { provider: 'openai', available: false, unavailable_reason: 'missing_api_key' },
    { provider: 'tylluan (internal)', available: true }
  ],
  models: [
    // TECHO (Operador / Usuario): Modelos grandes o externos que NO computan en el tope de 3072 MB de Tylluan
    {
      id: 'local::qwen2.5-coder-7b',
      name: 'Qwen 2.5 Coder 7B (GGUF Q4_K_M)',
      origin: 'user', // Techo del operador
      provider: 'llama.cpp (user)',
      family: 'qwen',
      version: '2.5',
      is_latest_in_family: true,
      pinned: false,
      model_type: 'generative',
      capabilities: ['coding', 'tools', 'json_output'],
      context_window: 32768,
      locality: 'local',
      execution_mode: 'external_server',
      available: true,
      size_mb: 4480,
      ram_private_mb: 4120,
      ram_peak_private_mb: 4350,
      roles: ['chat']
    },
    {
      id: 'local::gemma-2-9b',
      name: 'Gemma 2 9B Instruct',
      origin: 'user', // Techo del operador
      provider: 'ollama (user)',
      family: 'gemma',
      version: '2.0',
      is_latest_in_family: true,
      pinned: false,
      model_type: 'generative',
      capabilities: ['reasoning', 'tools', 'json_output'],
      context_window: 8192,
      locality: 'local',
      execution_mode: 'external_server',
      available: true,
      size_mb: 5400,
      ram_private_mb: 5100,
      ram_peak_private_mb: 5350,
      roles: ['chat']
    },
    {
      id: 'cloud::claude-sonnet-4-6',
      name: 'Claude 4.6 Sonnet',
      origin: 'user', // Techo del operador
      provider: 'anthropic',
      family: 'claude',
      version: '4.6',
      is_latest_in_family: true,
      pinned: false,
      model_type: 'generative',
      capabilities: ['reasoning', 'coding', 'tools', 'json_output', 'vision'],
      context_window: 200000,
      locality: 'remote',
      execution_mode: 'cloud',
      available: true,
      roles: ['chat']
    },
    {
      id: 'cloud::claude-3-5-sonnet',
      name: 'Claude 3.5 Sonnet (Legacy)',
      origin: 'user', // Techo del operador
      provider: 'anthropic',
      family: 'claude',
      version: '3.5',
      is_latest_in_family: false, // Nivel 2: versión secundaria
      pinned: false,
      model_type: 'generative',
      capabilities: ['reasoning', 'coding', 'tools', 'json_output', 'vision'],
      context_window: 200000,
      locality: 'remote',
      execution_mode: 'cloud',
      available: true,
      roles: ['chat']
    },
    {
      id: 'cloud::gpt-5-2',
      name: 'GPT-5.2 Omni',
      origin: 'user', // Techo del operador
      provider: 'openai',
      family: 'gpt',
      version: '5.2',
      is_latest_in_family: true,
      pinned: false,
      model_type: 'generative',
      capabilities: ['reasoning', 'coding', 'tools', 'json_output', 'vision'],
      context_window: 128000,
      locality: 'remote',
      execution_mode: 'cloud',
      available: false,
      unavailable_reason: 'missing_api_key',
      roles: ['chat']
    },

    // Modelos de DECISIÓN (T3144 - Primera Clase)
    {
      id: 'local::laya-8b',
      name: 'Laya 8B Decision Router',
      origin: 'tylluan', // Suelo soberano: rol decisión clavado
      provider: 'vllm (local)',
      family: 'drex',
      version: '1.2',
      is_latest_in_family: true,
      pinned: true,
      model_type: 'decision',
      decision_metadata: {
        interface: 'Choice',
        max_options: 64,
        calibrated_confidence: true,
        typical_latency_ms: 18
      },
      capabilities: ['reasoning'],
      context_window: 16384,
      locality: 'local',
      execution_mode: 'local_ephemeral',
      available: true,
      size_mb: 4800,
      ram_private_mb: 1800,
      ram_peak_private_mb: 1990,
      roles: ['decision', 'router']
    },
    {
      id: 'cloud::jev-decision-v1',
      name: 'Jev Decision Engine 1.0',
      origin: 'user', // Techo del operador
      provider: 'jev',
      family: 'jev',
      version: '1.0',
      is_latest_in_family: true,
      pinned: false,
      model_type: 'decision',
      decision_metadata: {
        interface: 'Choice',
        max_options: 255,
        calibrated_confidence: true,
        typical_latency_ms: 35
      },
      capabilities: ['reasoning'],
      context_window: 8192,
      locality: 'remote',
      execution_mode: 'cloud',
      available: true,
      roles: ['decision']
    },
    {
      id: 'local::arch-router-1.5b',
      name: 'Arch-Router 1.5B Fast Intent',
      origin: 'tylluan', // Suelo Tylluan
      provider: 'tylluan (internal)',
      family: 'arch-router',
      version: '1.5',
      is_latest_in_family: true,
      pinned: false,
      model_type: 'decision',
      decision_metadata: {
        interface: 'Noul',
        max_options: 2,
        calibrated_confidence: true,
        typical_latency_ms: 15
      },
      capabilities: ['reasoning'],
      context_window: 4096,
      locality: 'local',
      execution_mode: 'local_ephemeral',
      available: true,
      size_mb: 850,
      ram_private_mb: 480,
      ram_peak_private_mb: 550,
      roles: ['decision', 'router']
    },

    // SUELO SOBERANO (Modelos de Tylluan, origin='tylluan', computan contra 3072 MB privados)
    {
      id: 'tylluan::mxbai-embed-large',
      name: 'mxbai-embed-large (ONNX FP16)',
      origin: 'tylluan',
      provider: 'tylluan (internal)',
      family: 'mxbai',
      version: '1.0',
      is_latest_in_family: true,
      pinned: true,
      model_type: 'embedding',
      capabilities: [],
      context_window: 512,
      locality: 'local',
      execution_mode: 'local_persistent',
      available: true,
      size_mb: 670,
      ram_private_mb: 610,
      ram_peak_private_mb: 650,
      roles: ['embedding']
    },
    {
      id: 'tylluan::jina-reranker-v1-turbo',
      name: 'jina-reranker-v1-turbo-en (ONNX FP16)',
      origin: 'tylluan',
      provider: 'tylluan (internal)',
      family: 'jina',
      version: '1.0',
      is_latest_in_family: true,
      pinned: true,
      model_type: 'reranker',
      capabilities: [],
      context_window: 512,
      locality: 'local',
      execution_mode: 'local_persistent',
      available: true,
      size_mb: 280,
      ram_private_mb: 260,
      ram_peak_private_mb: 290,
      roles: ['reranker']
    },
    {
      id: 'tylluan::qwen3guard-0.6b',
      name: 'Qwen3Guard 0.6B (Runtime Sentinel / ASI06)',
      origin: 'tylluan',
      provider: 'tylluan (internal)',
      family: 'qwen',
      version: '3.0',
      is_latest_in_family: true,
      pinned: true,
      model_type: 'guard',
      capabilities: ['reasoning'],
      context_window: 2048,
      locality: 'local',
      execution_mode: 'local_persistent',
      available: true,
      size_mb: 580,
      ram_private_mb: 550,
      ram_peak_private_mb: 620,
      roles: ['sentinel', 'grammar_judge']
    }
  ]
};

export const FIXTURE_LLAMA_SERVER_STATUS: LlamaServerStatus = {
  running: true,
  pid: 41208,
  port: 8080,
  endpoint: 'http://127.0.0.1:8080',
  version: 'b4528 (ggerganov/llama.cpp)',
  models_directory: 'D:\\models\\gguf',
  update_available: false,
  last_check_timestamp: 1794101400
};
