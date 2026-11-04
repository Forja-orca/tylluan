import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  Cpu,
  Zap,
  CheckCircle2,
  AlertTriangle,
  RefreshCw,
  Save,
  Sliders,
  Cloud,
  Info,
  ShieldAlert,
  HardDrive,
  Globe,
  Activity,
  Server,
  Clock,
  Sparkles
} from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { cn } from '../../lib/utils';

export interface InferenceSettingsPanelProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

interface SchemaFieldMeta {
  key: string;
  section: string;
  type: string;
  editable_via_api: boolean;
  requires_restart: boolean;
  sensitive: boolean;
  reason?: string;
  options?: string[];
  min?: number;
  max?: number;
}

interface DetectedLocalModel {
  id?: string;
  name: string;
  path?: string;
  size_bytes?: number;
  size_mb?: number;
  installed?: boolean;
  model_type?: 'generative' | 'embedding' | 'vision' | string;
}

interface AvailableEmbedding {
  name: string;
  dimensions: number;
  multilingual?: boolean;
  note?: string;
}

interface AvailableVision {
  name: string;
  path: string;
  note?: string;
}

interface ModelsResponse {
  active?: {
    embedding?: string;
    inference?: string;
    vector_dimensions?: number;
    vision?: string;
  };
  detected_local_models?: DetectedLocalModel[];
  available_embeddings?: AvailableEmbedding[];
  available_vision?: AvailableVision[];
}

interface CognitivePointConfig {
  provider?: string | null;
  model?: string | null;
  endpoint?: string | null;
  api_key_env?: string | null;
  cloud_enabled?: boolean;
  locality?: string;
}

interface InferencePointsResponse {
  points?: {
    chat?: CognitivePointConfig;
    decision?: CognitivePointConfig;
    grammar_judge?: CognitivePointConfig;
    sentinel?: CognitivePointConfig;
    [key: string]: CognitivePointConfig | undefined;
  };
  valid?: boolean;
  note?: string;
}

interface ExternalProviderConfig {
  name: string;
  type: string;
  base_url: string;
  api_key_env: string;
  models: string[];
  api_key_set: boolean;
}

interface ExternalProviderTestResult {
  ok: boolean;
  status: string;
  provider: string;
  type: string;
  base_url: string;
  model_tested?: string;
  http_status: number;
  latency_ms: number;
  response_snippet?: string;
  error?: string;
}

interface ValidateResponse {
  ok: boolean;
  dry_run: boolean;
  etag: string;
  requires_restart: boolean;
  diff: Array<{ key: string; old: string; new: string }>;
  errors: string[];
}

const CANONICAL_POINTS: Array<{
  id: 'chat' | 'decision' | 'grammar_judge' | 'sentinel';
  title: string;
  role: string;
  description: string;
}> = [
  {
    id: 'chat',
    title: 'Chat y Diálogo',
    role: 'Interacción primaria',
    description: 'Deliberación con el usuario y ejecución directa de herramientas soberanas.',
  },
  {
    id: 'decision',
    title: 'Decisión y Consenso',
    role: 'Resolución de conflictos',
    description: 'Arbitraje cognitivo, desempate de agentes y juicio de discrepancias.',
  },
  {
    id: 'grammar_judge',
    title: 'Juez Gramatical',
    role: 'Sintaxis y Esquemas',
    description: 'Validación estricta de esquemas JSON y salidas estructuradas.',
  },
  {
    id: 'sentinel',
    title: 'Centinela de Coherencia',
    role: 'Seguridad y CoherenceGate',
    description: 'Supervisión en tiempo real de intenciones y detección de anomalías.',
  },
];

type GenerationPreset = 'fast' | 'balanced' | 'precise';

const PRESETS: Record<GenerationPreset, { name: string; desc: string; temp: number; top_p: number }> = {
  fast: {
    name: 'Rápido (Baja Latencia)',
    desc: 'Búsqueda ágil y streaming veloz. Ideal para clasificación y tareas directas.',
    temp: 0.2,
    top_p: 0.9,
  },
  balanced: {
    name: 'Equilibrado (Recomendado)',
    desc: 'Balance óptimo entre creatividad y rigor lógico en deliberación multi-agente.',
    temp: 0.7,
    top_p: 0.95,
  },
  precise: {
    name: 'Preciso (Determinista)',
    desc: 'Temperatura mínima para auditoría de código, refactorización y esquemas formales.',
    temp: 0.0,
    top_p: 0.8,
  },
};

export function InferenceSettingsPanel({ bridge, notify }: InferenceSettingsPanelProps) {
  const [loading, setLoading] = useState(true);
  const [savingEngine, setSavingEngine] = useState(false);
  const [savingPoint, setSavingPoint] = useState<Record<string, boolean>>({});
  const [error, setError] = useState<string | null>(null);
  const [engineSuccess, setEngineSuccess] = useState(false);
  const [showRestartAlert, setShowRestartAlert] = useState(false);

  // Raw fetched data
  const [schemaFields, setSchemaFields] = useState<Record<string, SchemaFieldMeta>>({});
  const [modelsData, setModelsData] = useState<ModelsResponse | null>(null);
  const [externalProviders, setExternalProviders] = useState<ExternalProviderConfig[]>([]);

  // Telemetría de proveedores externos
  const [testingProvider, setTestingProvider] = useState<Record<string, boolean>>({});
  const [providerTestResults, setProviderTestResults] = useState<Record<string, ExternalProviderTestResult>>({});

  // Preset activo en UI
  const [activePreset, setActivePreset] = useState<GenerationPreset>('balanced');

  // Form states: Engine & Hardware
  const [device, setDevice] = useState('cpu');
  const [maxThreads, setMaxThreads] = useState(4);
  const [primaryModel, setPrimaryModel] = useState('qwen2.5-1.5b');
  const [embeddingModel, setEmbeddingModel] = useState('mxbai-embed-large');
  const [rerankerEnabled, setRerankerEnabled] = useState(false);
  const [hasEngineChanges, setHasEngineChanges] = useState(false);

  // Form states: Cognitive Points
  const [pointsState, setPointsState] = useState<Record<string, CognitivePointConfig>>({});

  const abortControllerRef = useRef<AbortController | null>(null);
  const restartModalBtnRef = useRef<HTMLButtonElement>(null);

  // Consumo mínimo: una sola lectura combinada al montar con AbortController, sin sondeo
  const loadData = useCallback(async () => {
    if (!bridge) return;

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setLoading(true);
    setError(null);

    try {
      const [cfgRes, schemaRes, modelsRes, pointsRes, extProvidersRes] = await Promise.all([
        bridge.fetchRaw('/api/v1/config', { signal: controller.signal }) as Promise<Record<string, unknown>>,
        bridge.fetchRaw('/api/v1/config/schema', { signal: controller.signal }) as Promise<{ fields?: SchemaFieldMeta[] }>,
        (bridge.fetchRaw('/api/v1/models', { signal: controller.signal }) as Promise<ModelsResponse>).catch(() => null),
        (bridge.fetchRaw('/api/v1/inference/points', { signal: controller.signal }) as Promise<InferencePointsResponse>).catch(() => null),
        (bridge.fetchRaw('/api/v1/external-providers', { signal: controller.signal }) as Promise<ExternalProviderConfig[]>).catch(() => []),
      ]);

      if (schemaRes?.fields) {
        const map: Record<string, SchemaFieldMeta> = {};
        for (const f of schemaRes.fields) {
          map[f.key] = f;
        }
        setSchemaFields(map);
      }

      if (cfgRes) {
        const inf = (cfgRes.inference as Record<string, unknown>) || {};
        const mem = (cfgRes.memory as Record<string, unknown>) || {};
        const silva = (cfgRes.silva as Record<string, unknown>) || {};

        if (typeof inf.device === 'string') setDevice(inf.device);
        if (typeof inf.max_threads === 'number') setMaxThreads(inf.max_threads);
        if (typeof inf.primary_model === 'string') setPrimaryModel(inf.primary_model);
        if (typeof mem.embedding_model === 'string') setEmbeddingModel(mem.embedding_model);
        if (typeof silva.reranker_enabled === 'boolean') setRerankerEnabled(silva.reranker_enabled);
      }

      if (modelsRes) {
        setModelsData(modelsRes);
      }

      if (pointsRes?.points) {
        const clean: Record<string, CognitivePointConfig> = {};
        for (const [k, v] of Object.entries(pointsRes.points)) {
          if (v) clean[k] = v;
        }
        setPointsState(clean);
      }

      if (Array.isArray(extProvidersRes)) {
        setExternalProviders(extProvidersRes);
      }
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') return;
      const msg = err instanceof Error ? err.message : String(err);
      setError(`Error al cargar configuración de inferencia: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    loadData();
    return () => {
      if (abortControllerRef.current) abortControllerRef.current.abort();
    };
  }, [loadData]);

  // Manejo de guardado seguro del Motor (POST /validate -> POST /edit con ETag)
  const handleSaveEngine = async () => {
    if (!bridge) return;

    setSavingEngine(true);
    setError(null);
    setEngineSuccess(false);

    try {
      const edits = [
        { key: 'inference.device', value: device },
        { key: 'inference.max_threads', value: Number(maxThreads) },
        { key: 'inference.primary_model', value: primaryModel },
        { key: 'memory.embedding_model', value: embeddingModel },
        { key: 'silva.reranker_enabled', value: Boolean(rerankerEnabled) },
      ];

      // 1. Dry-run validate
      const validateRes = (await bridge.fetchRaw('/api/v1/config/validate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ edits }),
      })) as ValidateResponse;

      if (!validateRes.ok || (validateRes.errors && validateRes.errors.length > 0)) {
        throw new Error(validateRes.errors?.join(', ') || 'Fallo de validación en el kernel');
      }

      // 2. Edit con etag obligatorio
      const editRes = (await bridge.fetchRaw('/api/v1/config/edit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          edits,
          etag: validateRes.etag,
        }),
      })) as { ok: boolean; error?: string; requires_restart?: boolean };

      if (!editRes.ok) {
        throw new Error(editRes.error || 'No se pudo guardar la configuración');
      }

      setEngineSuccess(true);
      setHasEngineChanges(false);
      notify('Parámetros de inferencia actualizados con éxito', 'info');

      if (validateRes.requires_restart || editRes.requires_restart) {
        setShowRestartAlert(true);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      notify(`Error al guardar: ${msg}`, 'error');
    } finally {
      setSavingEngine(false);
    }
  };

  // Manejo de guardado individual de punto cognitivo (T-DSH-116: POST /api/v1/inference/points/{point})
  const handleSavePoint = async (pointId: string) => {
    if (!bridge) return;

    const current = pointsState[pointId] || {};
    setSavingPoint((prev) => ({ ...prev, [pointId]: true }));
    setError(null);

    try {
      const payload = {
        provider: current.provider || 'local_ephemeral',
        endpoint: current.endpoint || null,
        model: current.model || null,
        cloud_enabled: Boolean(current.cloud_enabled),
        persist: true,
      };

      const res = (await bridge.fetchRaw(`/api/v1/inference/points/${pointId}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      })) as { ok: boolean; error?: string };

      if (res && res.ok === false) {
        throw new Error(res.error || `Error al actualizar punto ${pointId}`);
      }

      notify(`Punto cognitivo "${pointId}" actualizado y persistido`, 'info');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(`Error en punto ${pointId}: ${msg}`);
      notify(`Fallo al actualizar ${pointId}: ${msg}`, 'error');
    } finally {
      setSavingPoint((prev) => ({ ...prev, [pointId]: false }));
    }
  };

  const updatePointField = (pointId: string, field: keyof CognitivePointConfig, value: unknown) => {
    setPointsState((prev) => ({
      ...prev,
      [pointId]: {
        ...(prev[pointId] || {}),
        [field]: value,
      },
    }));
  };

  // Test de latencia para proveedor externo (POST /api/v1/external-providers/{name}/test)
  const handleTestProvider = async (providerName: string) => {
    if (!bridge) return;

    setTestingProvider((prev) => ({ ...prev, [providerName]: true }));
    try {
      const result = (await bridge.fetchRaw(`/api/v1/external-providers/${encodeURIComponent(providerName)}/test`, {
        method: 'POST',
      })) as ExternalProviderTestResult;

      setProviderTestResults((prev) => ({
        ...prev,
        [providerName]: result,
      }));

      if (result.ok) {
        notify(`Proveedor ${providerName} en línea (${result.latency_ms} ms)`, 'info');
      } else {
        notify(`Prueba de ${providerName}: ${result.status} (${result.error || 'Fallo'})`, 'error');
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setProviderTestResults((prev) => ({
        ...prev,
        [providerName]: {
          ok: false,
          status: 'error',
          provider: providerName,
          type: 'unknown',
          base_url: '',
          http_status: 0,
          latency_ms: 0,
          error: msg,
        },
      }));
      notify(`Fallo al probar proveedor ${providerName}: ${msg}`, 'error');
    } finally {
      setTestingProvider((prev) => ({ ...prev, [providerName]: false }));
    }
  };

  // Establecer modelo local como primario
  const handleSetPrimaryModel = (modelId: string) => {
    setPrimaryModel(modelId);
    setHasEngineChanges(true);
    notify(`Modelo primario cambiado a "${modelId}". Recuerde guardar los cambios.`, 'info');
  };

  if (loading) {
    return (
      <div className="p-8 flex items-center justify-center space-x-2 text-muted-foreground font-sans">
        <RefreshCw className="w-5 h-5 animate-spin text-primary" aria-hidden="true" />
        <span className="text-xs font-semibold">Cargando parámetros de inferencia y modelos...</span>
      </div>
    );
  }

  const deviceOptions = schemaFields['inference.device']?.options || ['cpu', 'directml', 'cuda', 'coreml'];
  const localModels = modelsData?.detected_local_models || [];
  const activeEmbedding = modelsData?.active?.embedding || embeddingModel;
  const activeInference = modelsData?.active?.inference || primaryModel;
  const activeVision = modelsData?.active?.vision || 'HuggingFaceTB/SmolVLM2-256M-Instruct';

  return (
    <div className="space-y-6 font-sans">
      {/* Mensaje de error general */}
      {error && (
        <div
          role="alert"
          className="p-3 bg-destructive/10 border border-destructive/30 rounded-lg flex items-start gap-2.5 text-destructive text-xs"
        >
          <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" aria-hidden="true" />
          <div className="flex-1">
            <strong className="font-semibold">Error de configuración:</strong>
            <p className="mt-0.5">{error}</p>
          </div>
        </div>
      )}

      {/* Alerta de reinicio requerido */}
      {showRestartAlert && (
        <div
          role="status"
          className="p-4 bg-amber-500/10 border border-amber-500/30 rounded-lg flex items-start gap-3 text-amber-600 dark:text-amber-400 text-xs"
        >
          <AlertTriangle className="w-5 h-5 shrink-0 text-amber-500 mt-0.5" aria-hidden="true" />
          <div className="flex-1 space-y-1">
            <h4 className="font-bold text-sm text-foreground">Reinicio del Kernel Requerido</h4>
            <p className="text-muted-foreground">
              Se han modificado parámetros de hardware o modelos que requieren reconstruir los proveedores de ejecución en caliente.
            </p>
            <div className="pt-2 flex items-center gap-2">
              <span className="font-mono text-[11px] bg-card px-2 py-1 rounded border border-border text-foreground">
                tylluan-cli start
              </span>
              <button
                ref={restartModalBtnRef}
                type="button"
                onClick={() => setShowRestartAlert(false)}
                className="px-2.5 py-1 text-[11px] font-semibold bg-muted hover:bg-muted/80 text-foreground rounded border border-border transition-colors cursor-pointer"
              >
                Entendido
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Resumen de Modelos Activos (Top Bar) */}
      <div className="border border-border bg-card/60 backdrop-blur-sm rounded-lg p-3.5 flex flex-wrap items-center justify-between gap-3 text-xs">
        <div className="flex flex-wrap items-center gap-4">
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <span className="text-muted-foreground">Inferencia Primaria:</span>
            <span className="px-2 py-0.5 rounded bg-primary/10 text-primary border border-primary/20 font-semibold">
              {activeInference}
            </span>
          </div>
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <span className="text-muted-foreground">Embeddings SilvaDB:</span>
            <span className="px-2 py-0.5 rounded bg-muted text-foreground border border-border font-semibold">
              {activeEmbedding} (1024d)
            </span>
          </div>
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <span className="text-muted-foreground">Visión:</span>
            <span className="px-2 py-0.5 rounded bg-muted text-foreground border border-border font-semibold">
              {activeVision.split('/').pop() || activeVision}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-2 font-mono text-[11px] text-muted-foreground">
          <span className="flex items-center gap-1">
            <span className="w-2 h-2 rounded-full bg-emerald-500 inline-block animate-pulse" />
            Motor Activo ({device.toUpperCase()})
          </span>
        </div>
      </div>

      {/* CAPA 1: Hardware y Parámetros del Motor de Inferencia */}
      <section aria-labelledby="heading-inference-engine" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Cpu className="w-4 h-4 text-primary" aria-hidden="true" />
            <h3 id="heading-inference-engine" className="text-sm font-bold text-foreground">
              Capa 1 — Motor de Inferencia y Aceleración Local
            </h3>
          </div>
          <span className="text-[11px] font-mono text-muted-foreground bg-muted/50 px-2 py-0.5 rounded border border-border">
            ONNX Runtime / LLama.cpp
          </span>
        </div>

        {/* Presets de Generación Cognitiva */}
        <div className="space-y-2">
          <label className="text-xs font-semibold text-foreground flex items-center gap-1.5">
            <Sparkles className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
            Perfil de Generación Cognitiva (Presets)
          </label>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-2.5">
            {(Object.keys(PRESETS) as GenerationPreset[]).map((pKey) => {
              const p = PRESETS[pKey];
              const isSelected = activePreset === pKey;
              return (
                <button
                  key={pKey}
                  type="button"
                  onClick={() => setActivePreset(pKey)}
                  className={cn(
                    "text-left p-3 rounded-md border transition-all cursor-pointer text-xs space-y-1 focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none",
                    isSelected
                      ? "bg-primary/10 border-primary text-foreground shadow-sm"
                      : "bg-muted/30 border-border text-muted-foreground hover:bg-muted/50"
                  )}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-foreground">{p.name}</span>
                    <span className="font-mono text-[10px] opacity-75">T={p.temp}</span>
                  </div>
                  <p className="text-[11px] leading-relaxed opacity-90">{p.desc}</p>
                </button>
              );
            })}
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 pt-2">
          {/* Dispositivo de Ejecución */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="field-device" className="text-xs font-semibold text-foreground">
                Dispositivo de Aceleración
              </label>
              <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                Requiere Reinicio
              </span>
            </div>
            <select
              id="field-device"
              value={device}
              onChange={(e) => {
                setDevice(e.target.value);
                setHasEngineChanges(true);
              }}
              className="w-full text-xs font-mono bg-muted/30 border border-border rounded-md px-3 py-2 text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            >
              {deviceOptions.map((opt) => (
                <option key={opt} value={opt}>
                  {opt.toUpperCase()} {opt === 'directml' ? '(GPU Windows)' : opt === 'cuda' ? '(NVIDIA)' : opt === 'cpu' ? '(CPU Fallback)' : ''}
                </option>
              ))}
            </select>
            <p className="text-[11px] text-muted-foreground">
              Proveedor de ejecución hardware para ONNX y tensores.
            </p>
          </div>

          {/* Hilos de Inferencia */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="field-max-threads" className="text-xs font-semibold text-foreground">
                Hilos Máximos de Inferencia
              </label>
              <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                Requiere Reinicio
              </span>
            </div>
            <input
              id="field-max-threads"
              type="number"
              min={1}
              max={256}
              value={maxThreads}
              onChange={(e) => {
                setMaxThreads(Math.max(1, Math.min(256, parseInt(e.target.value, 10) || 1)));
                setHasEngineChanges(true);
              }}
              className="w-full text-xs font-mono bg-muted/30 border border-border rounded-md px-3 py-2 text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            />
            <p className="text-[11px] text-muted-foreground">
              Concurrencia local asignada para CPU (Rango: 1 – 256).
            </p>
          </div>

          {/* Modelo Primario */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="field-primary-model" className="text-xs font-semibold text-foreground">
                Modelo Local Primario
              </label>
              <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                Requiere Reinicio
              </span>
            </div>
            <input
              id="field-primary-model"
              type="text"
              list="detected-models-list"
              value={primaryModel}
              onChange={(e) => {
                setPrimaryModel(e.target.value);
                setHasEngineChanges(true);
              }}
              placeholder="qwen2.5-1.5b"
              className="w-full text-xs font-mono bg-muted/30 border border-border rounded-md px-3 py-2 text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            />
            <datalist id="detected-models-list">
              {localModels.map((m) => (
                <option key={m.id || m.name} value={m.id || m.name}>
                  {m.name} ({m.size_mb} MB)
                </option>
              ))}
            </datalist>
            <p className="text-[11px] text-muted-foreground">
              Modelo generativo por defecto para tareas cognitivas locales.
            </p>
          </div>

          {/* Modelo de Embeddings */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="field-embedding-model" className="text-xs font-semibold text-foreground">
                Modelo de Embeddings (SilvaDB)
              </label>
              <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                Requiere Reinicio
              </span>
            </div>
            <input
              id="field-embedding-model"
              type="text"
              value={embeddingModel}
              onChange={(e) => {
                setEmbeddingModel(e.target.value);
                setHasEngineChanges(true);
              }}
              placeholder="mxbai-embed-large"
              className="w-full text-xs font-mono bg-muted/30 border border-border rounded-md px-3 py-2 text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            />
            <p className="text-[11px] text-muted-foreground">
              Por defecto soberano: <code className="font-mono text-primary font-semibold">mxbai-embed-large</code> (1024d).
            </p>
          </div>

          {/* Dimensiones Vectoriales (Invariante) */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <span className="text-xs font-semibold text-foreground">
                Dimensiones Vectoriales
              </span>
              <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-muted text-muted-foreground border border-border">
                Invariante
              </span>
            </div>
            <div className="w-full text-xs font-mono bg-muted/60 border border-border rounded-md px-3 py-2 text-muted-foreground flex items-center justify-between">
              <span>1024 dimensiones</span>
              <span className="text-[10px] text-primary font-semibold">ADR-021</span>
            </div>
            <p className="text-[11px] text-muted-foreground">
              Invariante de arquitectura CONTRACT-01: no modificable vía API.
            </p>
          </div>

          {/* Reranker SilvaDB */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="field-reranker" className="text-xs font-semibold text-foreground">
                Reranker Cruzado (SilvaDB)
              </label>
              <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                Requiere Reinicio
              </span>
            </div>
            <div className="flex items-center h-[38px] px-3 bg-muted/30 border border-border rounded-md">
              <label className="flex items-center gap-2 cursor-pointer text-xs">
                <input
                  id="field-reranker"
                  type="checkbox"
                  checked={rerankerEnabled}
                  onChange={(e) => {
                    setRerankerEnabled(e.target.checked);
                    setHasEngineChanges(true);
                  }}
                  className="w-4 h-4 rounded text-primary focus-visible:ring-primary focus-visible:ring-2"
                />
                <span className="text-foreground font-medium">
                  {rerankerEnabled ? 'Habilitado (Cross-Encoder)' : 'Deshabilitado'}
                </span>
              </label>
            </div>
            <p className="text-[11px] text-muted-foreground">
              Reordenamiento neuronal de memoria episódica post-RRF.
            </p>
          </div>
        </div>

        {/* Botón Guardar Motor */}
        <div className="pt-3 border-t border-border flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs">
            {engineSuccess && (
              <span className="text-emerald-700 dark:text-emerald-400 flex items-center gap-1 font-semibold">
                <CheckCircle2 className="w-3.5 h-3.5" aria-hidden="true" />
                Configuración del motor guardada correctamente
              </span>
            )}
            {hasEngineChanges && !engineSuccess && (
              <span className="text-amber-600 dark:text-amber-400 font-semibold">
                Hay cambios pendientes en el motor de inferencia
              </span>
            )}
          </div>

          <button
            type="button"
            onClick={handleSaveEngine}
            disabled={savingEngine || !hasEngineChanges}
            className={cn(
              "flex items-center gap-1.5 px-3.5 py-1.5 rounded-md text-xs font-semibold border transition-all cursor-pointer shadow-sm focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
              hasEngineChanges && !savingEngine
                ? "bg-primary text-primary-foreground border-primary hover:bg-primary/90"
                : "bg-muted text-muted-foreground border-border cursor-not-allowed opacity-60"
            )}
          >
            {savingEngine ? (
              <>
                <RefreshCw className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
                <span>Validando y Guardando...</span>
              </>
            ) : (
              <>
                <Save className="w-3.5 h-3.5" aria-hidden="true" />
                <span>Guardar Parámetros del Motor</span>
              </>
            )}
          </button>
        </div>
      </section>

      {/* CAPA 2: Inventario Físico de Modelos en Disco (Absorbido de Model Hub) */}
      <section aria-labelledby="heading-disk-models" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <HardDrive className="w-4 h-4 text-primary" aria-hidden="true" />
            <h3 id="heading-disk-models" className="text-sm font-bold text-foreground">
              Capa 2 — Inventario Físico de Modelos en Disco
            </h3>
          </div>
          <span className="text-[11px] font-mono text-muted-foreground bg-muted/50 px-2 py-0.5 rounded border border-border">
            {localModels.length} modelos detectados en directorio local
          </span>
        </div>

        {localModels.length === 0 ? (
          <div className="p-6 text-center text-xs text-muted-foreground border border-dashed border-border rounded-lg">
            <HardDrive className="w-8 h-8 text-muted-foreground/50 mx-auto mb-2" aria-hidden="true" />
            <p>No se encontraron modelos locales en el directorio <code className="font-mono text-foreground">models/</code>.</p>
            <p className="mt-1 text-[11px]">Los modelos descargados mediante HuggingFace o GGUF aparecerán aquí automáticamente.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3.5">
            {localModels.map((m) => {
              const modelKey = m.id || m.name;
              const isCurrentPrimary = primaryModel.toLowerCase().includes(modelKey.toLowerCase()) || modelKey.toLowerCase().includes(primaryModel.toLowerCase());
              const isEmbedding = m.model_type === 'embedding';
              const isVision = m.model_type === 'vision';

              return (
                <div
                  key={modelKey}
                  className={cn(
                    "border rounded-lg p-3.5 flex flex-col justify-between space-y-3 transition-colors",
                    isCurrentPrimary
                      ? "bg-primary/5 border-primary/40 shadow-sm"
                      : "bg-muted/20 border-border hover:bg-muted/30"
                  )}
                >
                  <div className="space-y-1.5">
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <h4 className="text-xs font-bold text-foreground truncate" title={m.name}>
                          {m.name}
                        </h4>
                        <span className="text-[10px] font-mono text-muted-foreground block truncate">
                          id: {modelKey}
                        </span>
                      </div>
                      <span className={cn(
                        "text-[10px] font-semibold px-1.5 py-0.5 rounded border shrink-0",
                        isEmbedding
                          ? "bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/20"
                          : isVision
                          ? "bg-sky-500/10 text-sky-600 dark:text-sky-400 border-sky-500/20"
                          : "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
                      )}>
                        {isEmbedding ? 'Embeddings' : isVision ? 'Visión VLM' : 'LLM Generativo'}
                      </span>
                    </div>

                    <div className="pt-1 flex items-center justify-between text-[11px] font-mono text-muted-foreground">
                      <span>Tamaño:</span>
                      <span className="text-foreground font-semibold">
                        {m.size_mb ? `${m.size_mb} MB` : 'Desconocido'}
                      </span>
                    </div>
                  </div>

                  <div className="pt-2 border-t border-border flex items-center justify-between gap-2">
                    {isCurrentPrimary ? (
                      <span className="text-[11px] font-semibold text-primary flex items-center gap-1">
                        <CheckCircle2 className="w-3.5 h-3.5" aria-hidden="true" />
                        Modelo Primario Activo
                      </span>
                    ) : (
                      <button
                        type="button"
                        onClick={() => handleSetPrimaryModel(modelKey)}
                        disabled={isEmbedding || isVision}
                        title={isEmbedding || isVision ? 'No asignable como LLM generativo' : 'Asignar como modelo primario'}
                        className={cn(
                          "w-full text-center px-2.5 py-1 text-[11px] font-semibold rounded border transition-colors cursor-pointer",
                          isEmbedding || isVision
                            ? "bg-muted text-muted-foreground border-border cursor-not-allowed opacity-50"
                            : "bg-card hover:bg-muted text-foreground border-border hover:border-primary/50"
                        )}
                      >
                        Establecer como Primario
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* CAPA 3: Matriz de Enrutamiento de los 4 Puntos Canónicos (T-DSH-116) */}
      <section aria-labelledby="heading-cognitive-points" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-primary" aria-hidden="true" />
            <h3 id="heading-cognitive-points" className="text-sm font-bold text-foreground">
              Capa 3 — Matriz de Enrutamiento Cognitivo (T-DSH-116)
            </h3>
          </div>
          <span className="text-[11px] text-muted-foreground">
            Escritura atómica por punto con persistencia en <code className="font-mono text-primary">tylluan.toml</code>
          </span>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {CANONICAL_POINTS.map((pt) => {
            const current = pointsState[pt.id] || {};
            const isSaving = Boolean(savingPoint[pt.id]);

            return (
              <div
                key={pt.id}
                className="border border-border bg-muted/20 rounded-lg p-4 space-y-3 flex flex-col justify-between"
              >
                <div>
                  <div className="flex items-center justify-between pb-2 border-b border-border">
                    <div>
                      <h4 className="text-xs font-bold text-foreground flex items-center gap-1.5">
                        <Zap className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
                        {pt.title}
                      </h4>
                      <p className="text-[11px] text-muted-foreground mt-0.5">{pt.description}</p>
                    </div>
                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-card border border-border text-foreground font-semibold">
                      {pt.id}
                    </span>
                  </div>

                  <div className="space-y-2.5 mt-3">
                    {/* Proveedor */}
                    <div>
                      <label htmlFor={`provider-${pt.id}`} className="block text-[11px] font-semibold text-foreground mb-1">
                        Proveedor
                      </label>
                      <select
                        id={`provider-${pt.id}`}
                        value={current.provider || 'local_ephemeral'}
                        onChange={(e) => updatePointField(pt.id, 'provider', e.target.value)}
                        className="w-full text-xs font-mono bg-card border border-border rounded px-2.5 py-1.5 text-foreground focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
                      >
                        <option value="local_ephemeral">local_ephemeral (Embebido)</option>
                        <option value="ollama">ollama (Servicio Local)</option>
                        <option value="openai_compatible">openai_compatible (Local/Remoto)</option>
                      </select>
                    </div>

                    {/* Modelo */}
                    <div>
                      <label htmlFor={`model-${pt.id}`} className="block text-[11px] font-semibold text-foreground mb-1">
                        Modelo Asignado
                      </label>
                      <input
                        id={`model-${pt.id}`}
                        type="text"
                        value={current.model || ''}
                        onChange={(e) => updatePointField(pt.id, 'model', e.target.value)}
                        placeholder="ej. qwen2.5-1.5b / llama3.2:3b"
                        className="w-full text-xs font-mono bg-card border border-border rounded px-2.5 py-1.5 text-foreground focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
                      />
                    </div>

                    {/* Endpoint */}
                    <div>
                      <label htmlFor={`endpoint-${pt.id}`} className="block text-[11px] font-semibold text-foreground mb-1">
                        Endpoint (Opcional)
                      </label>
                      <input
                        id={`endpoint-${pt.id}`}
                        type="text"
                        value={current.endpoint || ''}
                        onChange={(e) => updatePointField(pt.id, 'endpoint', e.target.value)}
                        placeholder="ej. http://127.0.0.1:11434"
                        className="w-full text-xs font-mono bg-card border border-border rounded px-2.5 py-1.5 text-foreground focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
                      />
                    </div>

                    {/* Cloud Fallback Toggle */}
                    <div className="pt-1 flex items-center justify-between">
                      <label htmlFor={`cloud-${pt.id}`} className="text-[11px] font-semibold text-foreground flex items-center gap-1.5 cursor-pointer">
                        <Cloud className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                        <span>Permitir Fallback Cloud</span>
                      </label>
                      <input
                        id={`cloud-${pt.id}`}
                        type="checkbox"
                        checked={Boolean(current.cloud_enabled)}
                        onChange={(e) => updatePointField(pt.id, 'cloud_enabled', e.target.checked)}
                        className="w-4 h-4 rounded text-primary focus-visible:ring-primary focus-visible:ring-1"
                      />
                    </div>
                  </div>
                </div>

                <div className="pt-2 border-t border-border flex justify-end">
                  <button
                    type="button"
                    onClick={() => handleSavePoint(pt.id)}
                    disabled={isSaving}
                    className="flex items-center gap-1.5 px-3 py-1 bg-card hover:bg-muted border border-border rounded text-xs font-semibold text-foreground transition-colors cursor-pointer shadow-sm focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
                  >
                    {isSaving ? (
                      <>
                        <RefreshCw className="w-3 h-3 animate-spin text-primary" aria-hidden="true" />
                        <span>Guardando...</span>
                      </>
                    ) : (
                      <>
                        <Save className="w-3 h-3 text-primary" aria-hidden="true" />
                        <span>Guardar {pt.id}</span>
                      </>
                    )}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </section>

      {/* CAPA 4: Proveedores Externos y Telemetría de Latencia (Cloudflare AI Gateway / Open WebUI) */}
      <section aria-labelledby="heading-external-providers" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Globe className="w-4 h-4 text-primary" aria-hidden="true" />
            <h3 id="heading-external-providers" className="text-sm font-bold text-foreground">
              Capa 4 — Proveedores Externos y Telemetría de Latencia
            </h3>
          </div>
          <span className="text-[11px] font-mono text-muted-foreground bg-muted/50 px-2 py-0.5 rounded border border-border">
            Ping activo POST /api/v1/external-providers/:name/test
          </span>
        </div>

        {externalProviders.length === 0 ? (
          <div className="p-6 text-center text-xs text-muted-foreground border border-dashed border-border rounded-lg">
            <Server className="w-8 h-8 text-muted-foreground/50 mx-auto mb-2" aria-hidden="true" />
            <p>No hay proveedores externos registrados en <code className="font-mono text-foreground">tylluan.toml</code>.</p>
            <p className="mt-1 text-[11px]">Puedes configurar proveedores OpenAI-compatible, Anthropic u Ollama en la sección <code className="font-mono text-foreground">[[external_providers]]</code>.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {externalProviders.map((provider) => {
              const isTesting = Boolean(testingProvider[provider.name]);
              const testResult = providerTestResults[provider.name];

              return (
                <div
                  key={provider.name}
                  className="border border-border bg-muted/20 rounded-lg p-4 space-y-3 flex flex-col justify-between"
                >
                  <div className="space-y-2">
                    <div className="flex items-start justify-between gap-2 pb-2 border-b border-border">
                      <div>
                        <h4 className="text-xs font-bold text-foreground flex items-center gap-1.5 capitalize">
                          <Server className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
                          {provider.name}
                        </h4>
                        <span className="text-[10px] font-mono text-muted-foreground block truncate">
                          {provider.base_url}
                        </span>
                      </div>
                      <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-card border border-border text-foreground font-semibold">
                        {provider.type}
                      </span>
                    </div>

                    <div className="space-y-1.5 font-mono text-[11px]">
                      <div className="flex items-center justify-between">
                        <span className="text-muted-foreground">Variable API Key:</span>
                        <span className="text-foreground flex items-center gap-1 font-semibold">
                          <code>{provider.api_key_env}</code>
                          {provider.api_key_set ? (
                            <span className="text-[9px] px-1 py-0.2 rounded bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
                              Configurada
                            </span>
                          ) : (
                            <span className="text-[9px] px-1 py-0.2 rounded bg-destructive/10 text-destructive border border-destructive/20">
                              Sin definir
                            </span>
                          )}
                        </span>
                      </div>

                      {provider.models && provider.models.length > 0 && (
                        <div className="flex items-start justify-between gap-2 pt-1">
                          <span className="text-muted-foreground shrink-0">Modelos:</span>
                          <span className="text-foreground text-right truncate" title={provider.models.join(', ')}>
                            {provider.models.join(', ')}
                          </span>
                        </div>
                      )}
                    </div>

                    {/* Resultado de la prueba de latencia */}
                    {testResult && (
                      <div className={cn(
                        "mt-2 p-2.5 rounded border text-xs space-y-1 font-mono",
                        testResult.ok
                          ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-700 dark:text-emerald-300"
                          : "bg-destructive/10 border-destructive/30 text-destructive"
                      )}>
                        <div className="flex items-center justify-between">
                          <span className="font-bold flex items-center gap-1">
                            {testResult.ok ? (
                              <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" />
                            ) : (
                              <AlertTriangle className="w-3.5 h-3.5 text-destructive" />
                            )}
                            Estado: {testResult.status.toUpperCase()}
                          </span>
                          <span className="text-[11px] font-semibold flex items-center gap-1">
                            <Clock className="w-3 h-3" />
                            {testResult.latency_ms} ms
                          </span>
                        </div>
                        {testResult.error && (
                          <p className="text-[10px] text-destructive leading-tight break-words">
                            {testResult.error}
                          </p>
                        )}
                      </div>
                    )}
                  </div>

                  <div className="pt-2 border-t border-border flex justify-end">
                    <button
                      type="button"
                      onClick={() => handleTestProvider(provider.name)}
                      disabled={isTesting}
                      className="flex items-center gap-1.5 px-3 py-1 bg-card hover:bg-muted border border-border rounded text-xs font-semibold text-foreground transition-colors cursor-pointer shadow-sm focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
                    >
                      {isTesting ? (
                        <>
                          <RefreshCw className="w-3 h-3 animate-spin text-primary" aria-hidden="true" />
                          <span>Midiendo Latencia...</span>
                        </>
                      ) : (
                        <>
                          <Activity className="w-3 h-3 text-primary" aria-hidden="true" />
                          <span>Comprobar Latencia</span>
                        </>
                      )}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* CAPA 5: Política de Fallback Soberano (ADR-028) */}
      <section aria-labelledby="heading-sovereignty" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-3">
        <div className="flex items-center gap-2">
          <ShieldAlert className="w-4 h-4 text-primary" aria-hidden="true" />
          <h3 id="heading-sovereignty" className="text-sm font-bold text-foreground">
            Capa 5 — Política de Resiliencia y Fallback Soberano (ADR-028)
          </h3>
        </div>
        <p className="text-xs text-muted-foreground leading-relaxed">
          Tylluan opera bajo el principio de <strong className="text-foreground">Soberanía Absoluta Fail-Closed</strong>: todas las operaciones cognitivas se resuelven de forma predeterminada en hardware local o en la malla federada. Los puntos de inferencia rechazarán cualquier conexión externa si el fallback cloud no está habilitado explícitamente y respaldado por una variable de entorno en la bóveda.
        </p>
        <div className="p-3 bg-muted/40 border border-border rounded-md flex items-center gap-2 text-[11px] text-muted-foreground">
          <Info className="w-4 h-4 text-primary shrink-0" aria-hidden="true" />
          <span>
            Las claves de API nunca se guardan en texto plano en la configuración del punto; se referencian de forma segura por variable de entorno.
          </span>
        </div>
      </section>
    </div>
  );
}
