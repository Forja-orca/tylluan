/**
 * Contrato de Manifiesto y Consentimiento de Descarga de Modelos (INSTALACION-LIMPIA-SIN-MODELOS).
 *
 * Directivas de José (T3140, T3898) y Revisión Técnica del TL (T3903):
 * 1. Los modelos se DESCARGAN CON CONSENTIMIENTO EXPLÍCITO (no van incluidos en instalador).
 * 2. Cero mocks: la UI no inventa hashes SHA-256 ni simula descargas falsas.
 * 3. En producción, la información de modelos activos y rutas proviene del backend (/api/v1/models).
 * 4. Operación 100% soberana offline: el usuario puede elegir continuar solo con BM25 en cualquier momento.
 */

export interface ModelDownloadCandidate {
  id: string;
  name: string;
  role: 'embedding' | 'reranker' | 'vision';
  architecture: string;
  dimensions?: number;
  size_mb?: number;
  license: string;
  description: string;
  expected_path?: string;
  recommended: boolean;
}

export interface FirstBootManifest {
  manifest_version: string;
  models_dir?: string;
  disk_free_mb?: number;
  candidates: ModelDownloadCandidate[];
}

export interface FirstBootSystemStatus {
  silva_healthy: boolean;
  silva_mode: string;
  gateway_url: string;
  embeddings_loaded: boolean;
  embeddings_model?: string | null;
  expected_embeddings_path?: string;
  vision_installed: boolean;
  expected_vision_path?: string;
  chat_status: string;
}

/**
 * Predicado puro de guarda de consentimiento explícito (T4009 / T4011).
 * Exige `consentChecked === true` y que no haya una descarga en curso o ya completada.
 */
export function puedeDescargar(
  consentChecked: boolean,
  downloadState: 'idle' | 'downloading' | 'completed' | 'error' = 'idle'
): boolean {
  return Boolean(consentChecked) && downloadState !== 'downloading' && downloadState !== 'completed';
}

/**
 * Fixture tipado reservado exclusivamente para suites de test y entornos de desarrollo desacoplados.
 * En producción, el catálogo vivo se consulta directamente a la API del kernel.
 */
export const TEST_FIRST_BOOT_MANIFEST: FirstBootManifest = {
  manifest_version: '1.0.0',
  candidates: [
    {
      id: 'mxbai-embed-large',
      name: 'mxbai-embed-large (1024d)',
      role: 'embedding',
      architecture: 'ONNX / DirectML',
      dimensions: 1024,
      size_mb: 670,
      license: 'Apache-2.0',
      description: 'Modelo oficial para búsqueda semántica híbrida vectorial (BM25 + FTS5 + Embeddings + RRF).',
      expected_path: 'models/fastembed/mxbai-embed-large',
      recommended: true,
    },
    {
      id: 'smolvlm2-256m',
      name: 'SmolVLM2-256M-Instruct',
      role: 'vision',
      architecture: 'ONNX CPU-friendly',
      size_mb: 480,
      license: 'Apache-2.0',
      description: 'Motor de visión multimodal ligero para análisis local de imágenes en guilds.',
      expected_path: 'models/smolvlm2-256m',
      recommended: false,
    },
  ],
};
