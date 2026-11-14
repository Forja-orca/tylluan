import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  Cpu,
  AlertTriangle,
  RefreshCw,
  Info,
  Server
} from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { cn } from '../../lib/utils';
import type { LlamaServerStatus } from './inference/model-catalog-contract';
import {
  ProviderSelectorCards,
  type DiscoverResponse,
  type ExternalProviderConfig,
  type ExternalProviderTestResult,
  type TylluanEngineStatus
} from './inference/ProviderSelectorCards';
import {
  RoleModelAssignmentSection,
  type CognitivePointConfig,
  type DetectedLocalModel,
  type EngineCapabilitiesStatus
} from './inference/RoleModelAssignmentSection';
import {
  EngineAdvancedAccordion
} from './inference/EngineAdvancedAccordion';
import { AddProviderDrawer } from './inference/AddProviderDrawer';

export interface InferenceSettingsPanelProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  kernelSupportsAddProvider?: boolean;
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

interface ModelsResponse {
  active?: {
    embedding?: string;
    inference?: string;
    vector_dimensions?: number;
    vision?: string;
  };
  models_dir?: string;
  models_dir_from_env?: boolean;
  hint?: string | null;
  detected_local_models?: DetectedLocalModel[];
  available_embeddings?: Array<{ name: string; dimensions: number }>;
  available_vision?: Array<{ name: string; path: string }>;
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

interface ValidateResponse {
  ok: boolean;
  dry_run: boolean;
  etag: string;
  requires_restart: boolean;
  diff: Array<{ key: string; old: string; new: string }>;
  errors: string[];
}

export function InferenceSettingsPanel({
  bridge,
  notify,
  kernelSupportsAddProvider = true,
}: InferenceSettingsPanelProps) {
  const [loading, setLoading] = useState(true);
  const [discoverLoading, setDiscoverLoading] = useState(false);
  const [savingEngine, setSavingEngine] = useState(false);
  const [savingPoint, setSavingPoint] = useState<Record<string, boolean>>({});
  const [error, setError] = useState<string | null>(null);
  const [engineSuccess, setEngineSuccess] = useState(false);
  const [showRestartAlert, setShowRestartAlert] = useState(false);

  // Raw fetched data
  const [schemaFields, setSchemaFields] = useState<Record<string, SchemaFieldMeta>>({});
  const [modelsData, setModelsData] = useState<ModelsResponse | null>(null);
  const [discoverData, setDiscoverData] = useState<DiscoverResponse | null>(null);
  const [externalProviders, setExternalProviders] = useState<ExternalProviderConfig[]>([]);
  const [engineCapabilities, setEngineCapabilities] = useState<EngineCapabilitiesStatus | null>(null);

  // Drawer de Añadir Proveedor (T-UI-2 / SPEC §4)
  const [isAddProviderDrawerOpen, setIsAddProviderDrawerOpen] = useState(false);

  // Telemetría de proveedores externos
  const [testingProvider, setTestingProvider] = useState<Record<string, boolean>>({});
  const [providerTestResults, setProviderTestResults] = useState<Record<string, ExternalProviderTestResult>>({});

  // Form states: Engine & Hardware
  const [device, setDevice] = useState('cpu');
  const [maxThreads, setMaxThreads] = useState(4);
  const [hasEngineChanges, setHasEngineChanges] = useState(false);

  // Form states: Cognitive Points
  const [pointsState, setPointsState] = useState<Record<string, CognitivePointConfig>>({});

  // Llama.cpp user server status (T3259)
  const [llamaStatus, setLlamaStatus] = useState<LlamaServerStatus>({
    running: false,
    port: 9000,
    endpoint: 'http://127.0.0.1:9000',
    models_directory: '',
    pid: null,
    version: null,
  });

  const abortControllerRef = useRef<AbortController | null>(null);
  const restartModalBtnRef = useRef<HTMLButtonElement>(null);

  // Sondeo bajo demanda de proveedores en loopback (GET /api/v1/inference/providers/discover)
  // Directiva José T3312: Cero timers en segundo plano; sondeo solo al montar o al pulsar botón.
  const fetchDiscover = useCallback(async () => {
    if (!bridge) return;
    setDiscoverLoading(true);
    try {
      const res = await (bridge.fetchRaw('/api/v1/inference/providers/discover') as Promise<DiscoverResponse>).catch(() => null);
      if (res && typeof res === 'object' && Array.isArray(res.providers)) {
        setDiscoverData(res);
        if (Array.isArray(res.configured_external) && res.configured_external.length > 0) {
          setExternalProviders(prev => prev.length === 0 ? (res.configured_external ?? []) : prev);
        }
      }
    } catch {
      // Ignorar fallos de red en modo offline
    } finally {
      setDiscoverLoading(false);
    }
  }, [bridge]);

  // Consumo mínimo: una sola lectura combinada al montar con AbortController, sin sondeo en bucle
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
      const [cfgRes, schemaRes, modelsRes, pointsRes, extProvidersRes, llamaRes, discRes, summaryRes] = await Promise.all([
        bridge.fetchRaw('/api/v1/config', { signal: controller.signal }) as Promise<Record<string, unknown>>,
        bridge.fetchRaw('/api/v1/config/schema', { signal: controller.signal }) as Promise<{ fields?: SchemaFieldMeta[] }>,
        (bridge.fetchRaw('/api/v1/models', { signal: controller.signal }) as Promise<ModelsResponse>).catch(() => null),
        (bridge.fetchRaw('/api/v1/inference/points', { signal: controller.signal }) as Promise<InferencePointsResponse>).catch(() => null),
        (bridge.fetchRaw('/api/v1/external-providers', { signal: controller.signal }) as Promise<ExternalProviderConfig[]>).catch(() => []),
        (bridge.fetchRaw('/api/v1/inference/llama/status', { signal: controller.signal }) as Promise<Record<string, unknown>>).catch(() => null),
        (bridge.fetchRaw('/api/v1/inference/providers/discover', { signal: controller.signal }) as Promise<DiscoverResponse>).catch(() => null),
        (bridge.fetchRaw('/api/v1/dashboard/summary', { signal: controller.signal }) as Promise<{ interoception?: { capabilities?: EngineCapabilitiesStatus } }>).catch(() => null),
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

        if (typeof inf.device === 'string') setDevice(inf.device);
        if (typeof inf.max_threads === 'number') setMaxThreads(inf.max_threads);
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

      if (discRes && Array.isArray(discRes.providers)) {
        setDiscoverData(discRes);
      }

      if (summaryRes?.interoception?.capabilities) {
        setEngineCapabilities(summaryRes.interoception.capabilities);
      }

      if (llamaRes && typeof llamaRes === 'object') {
        const port = typeof llamaRes.port === 'number' ? llamaRes.port : 9000;
        const modelsDir = typeof llamaRes.models_dir === 'string' ? llamaRes.models_dir : '';
        setLlamaStatus({
          running: Boolean(llamaRes.running),
          port,
          endpoint: `http://127.0.0.1:${port}`,
          models_directory: modelsDir,
          pid: typeof llamaRes.pid === 'number' ? llamaRes.pid : null,
          version: typeof llamaRes.version === 'string' ? llamaRes.version : null,
        });
      }
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') return;
      const msg = err instanceof Error ? err.message : String(err);
      setError(`Error al cargar configuración de inferencia: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  const fetchLlamaStatus = useCallback(async () => {
    if (!bridge) return;
    try {
      const res = await (bridge.fetchRaw('/api/v1/inference/llama/status') as Promise<Record<string, unknown>>).catch(() => null);
      if (res && typeof res === 'object') {
        const port = typeof res.port === 'number' ? res.port : 9000;
        const modelsDir = typeof res.models_dir === 'string' ? res.models_dir : '';
        setLlamaStatus({
          running: Boolean(res.running),
          port,
          endpoint: `http://127.0.0.1:${port}`,
          models_directory: modelsDir,
          pid: typeof res.pid === 'number' ? res.pid : null,
          version: typeof res.version === 'string' ? res.version : null,
        });
      }
    } catch {
      // Ignorar fallos de red offline
    }
  }, [bridge]);

  const handleToggleLlamaServer = async (start: boolean) => {
    if (!bridge) return;
    try {
      const endpoint = start ? '/api/v1/inference/llama/start' : '/api/v1/inference/llama/stop';
      await bridge.fetchRaw(endpoint, { method: 'POST' });
      await fetchLlamaStatus();
      await fetchDiscover();
      notify(start ? 'Servidor llama.cpp iniciado' : 'Servidor llama.cpp detenido', 'info');
    } catch (err) {
      console.error('Error toggling llama server:', err);
      notify(`Error al ${start ? 'iniciar' : 'detener'} llama.cpp: ${String(err)}`, 'error');
    }
  };

  const handleOpenLlamaWebUI = () => {
    const port = llamaStatus.port || 9000;
    const url = `http://127.0.0.1:${port}`;
    window.open(url, '_blank', 'noopener,noreferrer');
    notify(`Abriendo Web UI en ${url}`, 'info');
  };

  const handleOpenLlamaDirectory = async () => {
    const dir = llamaStatus.models_directory;
    if (!dir) {
      notify('Ruta de carpeta models no reportada por el kernel', 'info');
      return;
    }
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(dir);
        notify(`Ruta de carpeta models copiada al portapapeles: ${dir}`, 'info');
      } else {
        notify(`Carpeta models: ${dir}`, 'info');
      }
    } catch {
      notify(`Carpeta models: ${dir}`, 'info');
    }
  };

  const handleUpdateCognitivePoint = (pointId: string, partial: Partial<CognitivePointConfig>) => {
    setPointsState(prev => ({
      ...prev,
      [pointId]: {
        ...prev[pointId],
        ...partial,
      },
    }));
  };

  const handleSaveCognitivePoint = async (pointId: string) => {
    if (!bridge) return;
    const current = pointsState[pointId];
    if (!current) return;

    setSavingPoint(prev => ({ ...prev, [pointId]: true }));
    try {
      await bridge.fetchRaw(`/api/v1/inference/points/${pointId}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          provider: current.provider,
          model: current.model,
          endpoint: current.endpoint,
          api_key_env: current.api_key_env,
          cloud_enabled: current.cloud_enabled ?? false,
          persist: true,
        }),
      });
      notify(`Punto cognitivo «${pointId}» guardado en configuración persistente`, 'info');
    } catch (err) {
      console.error(`Error saving point ${pointId}:`, err);
      notify(`Error al guardar punto cognitivo «${pointId}»: ${String(err)}`, 'error');
    } finally {
      setSavingPoint(prev => ({ ...prev, [pointId]: false }));
    }
  };

  const handleSaveEngineSettings = async () => {
    if (!bridge) return;

    setSavingEngine(true);
    setEngineSuccess(false);

    try {
      const edits = [
        { key: 'inference.device', value: device },
        { key: 'inference.max_threads', value: maxThreads },
      ];

      const valRes = (await bridge.fetchRaw('/api/v1/config/validate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ edits }),
      })) as ValidateResponse;

      if (!valRes.ok) {
        const errorMsg = valRes.errors?.join(', ') || 'Validación fallida en kernel';
        notify(`Error de validación: ${errorMsg}`, 'error');
        setSavingEngine(false);
        return;
      }

      await bridge.fetchRaw('/api/v1/config/edit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          edits,
          etag: valRes.etag,
        }),
      });

      setEngineSuccess(true);
      setHasEngineChanges(false);

      if (valRes.requires_restart) {
        setShowRestartAlert(true);
        notify('Ajustes del motor guardados. Aplican tras reiniciar el daemon.', 'info');
      } else {
        notify('Ajustes del motor guardados correctamente.', 'info');
      }
    } catch (err) {
      console.error('Error saving engine settings:', err);
      notify(`Error al guardar ajustes del motor: ${String(err)}`, 'error');
    } finally {
      setSavingEngine(false);
    }
  };

  const handleTestExternalProvider = async (providerName: string) => {
    if (!bridge) return;

    setTestingProvider(prev => ({ ...prev, [providerName]: true }));
    try {
      const res = (await bridge.fetchRaw(`/api/v1/external-providers/${providerName}/test`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: providerName }),
      })) as ExternalProviderTestResult;

      setProviderTestResults(prev => ({ ...prev, [providerName]: res }));

      if (res.ok) {
        notify(`Prueba de latencia con «${providerName}» exitosa (${res.latency_ms} ms)`, 'info');
      } else {
        notify(`Fallo al contactar «${providerName}»: ${res.error || res.status}`, 'error');
      }
    } catch (err) {
      console.error(`Error testing provider ${providerName}:`, err);
      notify(`Error al probar proveedor «${providerName}»: ${String(err)}`, 'error');
    } finally {
      setTestingProvider(prev => ({ ...prev, [providerName]: false }));
    }
  };

  const handleRemoveExternalProvider = async (providerName: string) => {
    if (!bridge) return;
    try {
      // Obtener ETag actual de /api/v1/config para cumplir R2 (D-101 / D-157)
      const token = bridge?.getToken();
      const configRes = await window.fetch('/api/v1/config', {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      });
      const etag = configRes.headers.get('x-config-etag') || '';
      const query = etag ? `?etag=${encodeURIComponent(etag)}` : '';

      await bridge.fetchRaw(`/api/v1/external-providers/${encodeURIComponent(providerName)}${query}`, {
        method: 'DELETE',
        headers: {
          'Content-Type': 'application/json',
          ...(etag ? { 'x-config-etag': etag } : {}),
        },
      });
      notify(`Proveedor «${providerName}» eliminado correctamente`, 'info');
      await loadData();
    } catch (err) {
      console.error(`Error deleting provider ${providerName}:`, err);
      notify(`Error al quitar proveedor: ${String(err)}`, 'error');
    }
  };

  useEffect(() => {
    loadData();
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [loadData]);

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground space-y-3">
        <RefreshCw className="w-6 h-6 animate-spin text-primary" aria-hidden="true" />
        <p className="text-xs font-mono">Cargando estado de inferencia y detectando proveedores...</p>
      </div>
    );
  }

  const deviceFieldMeta = schemaFields['inference.device'];
  const deviceOptions = deviceFieldMeta?.options || ['cpu', 'directml', 'cuda', 'coreml'];

  const activeEmbedding = modelsData?.active?.embedding || '';
  const vectorDimensions = modelsData?.active?.vector_dimensions || 1024;
  const activeVision = modelsData?.active?.vision || '';
  const detectedModels = modelsData?.detected_local_models || [];
  const modelsDir = modelsData?.models_dir;
  const modelsDirFromEnv = modelsData?.models_dir_from_env;
  const modelsHint = modelsData?.hint;

  // Comprobar existencia real del modelo de visión en disco (T3294 / T3318 / T3717)
  // No confunde modelos de texto, guardias (qwen3guard) o generativos con modelos de visión
  const isVisionModel = (nameOrPath: string) =>
    (nameOrPath.toLowerCase().includes('smolvlm') || nameOrPath.toLowerCase().includes('vlm')) &&
    !nameOrPath.toLowerCase().includes('guard');

  const visionExists = detectedModels.some(
    m => (m.role === 'vision' || m.model_type === 'vision' || isVisionModel(m.path || '') || isVisionModel(m.id || '') || isVisionModel(m.name || '')) &&
         !m.id?.toLowerCase().includes('guard') &&
         !m.name?.toLowerCase().includes('guard') &&
         !m.path?.toLowerCase().includes('guard')
  );

  // Garantizar que la UI nunca muestre un modelo guardia (Qwen3Guard) como visión
  const displayVision = (activeVision && !activeVision.toLowerCase().includes('guard'))
    ? activeVision
    : visionExists
    ? 'models/smolvlm2-256m'
    : '';

  // Conteo de proveedores activos
  const localDetectedCount = (discoverData?.providers || []).filter(p => p.detected).length + 1; // +1 por Tylluan ONNX
  const externalActiveCount = externalProviders.filter(p => p.api_key_set).length;
  const totalActiveProviders = localDetectedCount + externalActiveCount;

  // Estado dinámico riguroso según telemetría real (cero inventiva sin evidencia, T3813)
  const embeddingsStatus: 'cargado' | 'liberado por inactividad' | 'desconocido' =
    engineCapabilities?.embeddings_loaded === true
      ? 'cargado'
      : engineCapabilities?.embeddings_loaded === false
      ? 'liberado por inactividad'
      : 'desconocido';

  const rerankerStatus: 'cargado' | 'liberado por inactividad' | 'desconocido' =
    engineCapabilities?.reranker_loaded === true
      ? 'cargado'
      : engineCapabilities?.reranker_loaded === false
      ? 'liberado por inactividad'
      : 'desconocido';

  // T4064(a): estado REAL del motor ONNX integrado (antes había un «Activo y disponible» fijo).
  // Fuente: GET /api/v1/models (modelo activo) + capabilities reales de /api/v1/dashboard/summary.
  // Sin dato => neutro; nunca verde por defecto.
  const tylluanStatus: TylluanEngineStatus = (() => {
    if (!modelsData) return { tone: 'unknown', label: 'estado no verificado' };
    const hasActive = Boolean(modelsData.active?.embedding || modelsData.active?.inference);
    if (!hasActive) return { tone: 'unknown', label: 'sin modelo activo' };
    const loaded = engineCapabilities?.embeddings_loaded;
    if (loaded === true) return { tone: 'ok', label: 'Activo y disponible' };
    if (loaded === false) return { tone: 'warn', label: 'liberado por inactividad' };
    return { tone: 'warn', label: 'modelo activo (carga no verificada)' };
  })();

  return (
    <div className="space-y-6" data-testid="inference-settings-panel">
      {/* Cabecera Principal y Barra de Estado Superior */}
      <div className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-3">
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-bold text-foreground flex items-center gap-2">
              <Cpu className="w-5 h-5 text-primary" aria-hidden="true" />
              Configuración de Inferencia & Modelos
            </h2>
            <p className="text-xs text-muted-foreground">
              Selecciona el proveedor y asigna modelos para decisiones, embeddings y visión. Embeddings de SilvaDB anclados a mxbai-embed-large (1024d).
            </p>
          </div>

          {/* Pastillas de estado resumen */}
          <div className="flex flex-wrap items-center gap-2 text-xs font-mono">
            {/* Embeddings SilvaDB (Fijo 1024d + Estado en memoria D-160) */}
            <span
              className={cn(
                "px-2 py-0.5 rounded border font-semibold flex items-center gap-1.5",
                embeddingsStatus === 'cargado'
                  ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
                  : embeddingsStatus === 'liberado por inactividad'
                  ? "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20"
                  : "bg-muted/40 text-muted-foreground border-border"
              )}
              title={activeEmbedding ? `Vectores SilvaDB: ${activeEmbedding} (${vectorDimensions}d) · Estado en memoria: ${embeddingsStatus}` : `SilvaDB desconectado (${embeddingsStatus})`}
              data-testid="embedding-summary-pill"
            >
              <span className={cn(
                "w-1.5 h-1.5 rounded-full",
                embeddingsStatus === 'cargado'
                  ? "bg-emerald-500"
                  : embeddingsStatus === 'liberado por inactividad'
                  ? "bg-amber-500"
                  : "bg-muted-foreground/40"
              )} />
              <span>SilvaDB: {activeEmbedding ? (activeEmbedding.split('/').pop() || activeEmbedding) : 'desconectado'} {activeEmbedding ? `(${vectorDimensions}d)` : ''}</span>
              <span className="text-[10px] font-normal opacity-90">
                ({embeddingsStatus})
              </span>
            </span>

            {/* Reranker SilvaDB (Recall híbrido bajo demanda D-160 f2) */}
            <span
              className={cn(
                "px-2 py-0.5 rounded border font-semibold flex items-center gap-1.5",
                rerankerStatus === 'cargado'
                  ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
                  : rerankerStatus === 'liberado por inactividad'
                  ? "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20"
                  : "bg-muted/40 text-muted-foreground border-border"
              )}
              title={`Reranker semántico: ${engineCapabilities?.reranker_model || '—'} · Estado en memoria: ${rerankerStatus}`}
              data-testid="reranker-summary-pill"
            >
              <span className={cn(
                "w-1.5 h-1.5 rounded-full",
                rerankerStatus === 'cargado'
                  ? "bg-emerald-500"
                  : rerankerStatus === 'liberado por inactividad'
                  ? "bg-amber-500"
                  : "bg-muted-foreground/40"
              )} />
              <span>Reranker</span>
              <span className="text-[10px] font-normal opacity-90">
                ({rerankerStatus})
              </span>
            </span>

            {/* Visión VLM (T3294) */}
            <span
              className={cn(
                "px-2 py-0.5 rounded border font-semibold flex items-center gap-1.5",
                visionExists
                  ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
                  : "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20"
              )}
              title={displayVision ? `Ruta: ${displayVision}` : 'No configurado'}
              data-testid="vision-model-summary"
            >
              <span className={cn("w-1.5 h-1.5 rounded-full", visionExists ? "bg-emerald-500" : "bg-amber-500")} />
              <span className="truncate max-w-[180px]">{displayVision ? (displayVision.split(/[/\\]/).pop() || displayVision) : 'No configurado'}</span>
              <span className="text-[10px] font-normal opacity-80">
                ({visionExists ? 'en disco' : 'no disponible'})
              </span>
            </span>

            {/* Proveedores Activos */}
            <span
              className="px-2 py-0.5 rounded border font-semibold flex items-center gap-1.5 bg-muted/50 text-foreground border-border"
              title="Total de proveedores locales y externos listos"
              data-testid="providers-active-summary"
            >
              <Server className="w-3.5 h-3.5 text-muted-foreground" />
              <span>{totalActiveProviders} proveedor(es) listos</span>
            </span>
          </div>
        </div>

        {error && (
          <div className="p-3 bg-destructive/10 border border-destructive/20 text-destructive text-xs rounded-md flex items-center gap-2">
            <AlertTriangle className="w-4 h-4 shrink-0" aria-hidden="true" />
            <span>{error}</span>
          </div>
        )}
      </div>

      {/* SECCIÓN 1: Proveedores de Inferencia (Lista densa con estado loopback & nube) */}
      <ProviderSelectorCards
        discoverData={discoverData}
        discoverLoading={discoverLoading}
        onRefreshDiscover={fetchDiscover}
        externalProviders={externalProviders}
        llamaStatus={llamaStatus}
        tylluanStatus={tylluanStatus}
        onToggleLlamaServer={handleToggleLlamaServer}
        onOpenLlamaDirectory={handleOpenLlamaDirectory}
        onOpenLlamaWebUI={handleOpenLlamaWebUI}
        testingProvider={testingProvider}
        providerTestResults={providerTestResults}
        onTestProvider={handleTestExternalProvider}
        onRemoveProvider={handleRemoveExternalProvider}
        detectedModels={detectedModels}
        modelsDir={modelsDir}
        modelsDirFromEnv={modelsDirFromEnv}
        hint={modelsHint}
        onOpenAddProvider={() => setIsAddProviderDrawerOpen(true)}
        kernelSupportsPost={kernelSupportsAddProvider}
      />

      {/* Drawer Lateral: Añadir Proveedor (T-UI-2 / SPEC §4) */}
      <AddProviderDrawer
        isOpen={isAddProviderDrawerOpen}
        onClose={() => setIsAddProviderDrawerOpen(false)}
        bridge={bridge}
        notify={notify}
        onProviderAdded={loadData}
        kernelSupportsPost={kernelSupportsAddProvider}
      />

      {/* SECCIÓN 2: Modelos por Uso (Decisión, Embeddings, Visión) */}
      <RoleModelAssignmentSection
        pointsState={pointsState}
        onUpdatePoint={handleUpdateCognitivePoint}
        onSavePoint={handleSaveCognitivePoint}
        savingPoint={savingPoint}
        detectedModels={detectedModels}
        activeVision={displayVision}
        visionExists={visionExists}
        activeEmbedding={activeEmbedding}
        vectorDimensions={vectorDimensions}
        engineCapabilities={engineCapabilities}
      />

      {/* SECCIÓN 3: Ajustes Avanzados & Motor de Inferencia (Acordeón Colapsable) */}
      <EngineAdvancedAccordion
        device={device}
        setDevice={setDevice}
        deviceOptions={deviceOptions}
        maxThreads={maxThreads}
        setMaxThreads={setMaxThreads}
        hasEngineChanges={hasEngineChanges}
        setHasEngineChanges={setHasEngineChanges}
        savingEngine={savingEngine}
        onSaveEngine={handleSaveEngineSettings}
        engineSuccess={engineSuccess}
        detectedModels={detectedModels}
        modelsDirectory={modelsDir || llamaStatus.models_directory || undefined}
        modelsDir={modelsDir}
        modelsDirFromEnv={modelsDirFromEnv}
        hint={modelsHint}
      />

      {/* Alerta Accesible de Reinicio Pendiente (AG-23 / H2) */}
      {showRestartAlert && (
        <div
          role="alertdialog"
          aria-labelledby="restart-title"
          aria-describedby="restart-desc"
          className="p-4 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-800 dark:text-amber-200 text-xs flex flex-col sm:flex-row sm:items-center justify-between gap-3 animate-in fade-in duration-200"
        >
          <div className="flex items-start gap-2.5">
            <Info className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" aria-hidden="true" />
            <div className="space-y-0.5">
              <strong id="restart-title" className="font-semibold block">
                Guardado, aplica al reiniciar
              </strong>
              <p id="restart-desc" className="text-amber-700 dark:text-amber-300">
                Los parámetros de dispositivo, hilos y modelo primario se guardaron en <code className="font-mono">tylluan.toml</code>. Reinicia el daemon Tylluan para cargar las nuevas bibliotecas de tensores.
              </p>
            </div>
          </div>
          <button
            ref={restartModalBtnRef}
            type="button"
            onClick={() => setShowRestartAlert(false)}
            className="self-end sm:self-auto shrink-0 px-3 py-1.5 font-medium rounded border border-amber-500/30 hover:bg-amber-500/20 transition-colors text-amber-900 dark:text-amber-100 cursor-pointer"
          >
            Entendido
          </button>
        </div>
      )}
    </div>
  );
}
