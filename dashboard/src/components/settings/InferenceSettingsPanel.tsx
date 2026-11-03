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
  ShieldAlert
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
  model_type?: string;
  size_mb?: number;
}

interface ModelsResponse {
  active?: {
    embedding?: string;
    inference?: string;
    vector_dimensions?: number;
    vision?: string;
  };
  detected_local_models?: DetectedLocalModel[];
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

  // Consumo mínimo: una sola lectura al montar, sin sondeo ni bucles
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
      const [cfgRes, schemaRes, modelsRes, pointsRes] = await Promise.all([
        bridge.fetchRaw('/api/v1/config', { signal: controller.signal }) as Promise<Record<string, unknown>>,
        bridge.fetchRaw('/api/v1/config/schema', { signal: controller.signal }) as Promise<{ fields?: SchemaFieldMeta[] }>,
        (bridge.fetchRaw('/api/v1/models', { signal: controller.signal }) as Promise<ModelsResponse>).catch(() => null),
        (bridge.fetchRaw('/api/v1/inference/points', { signal: controller.signal }) as Promise<InferencePointsResponse>).catch(() => null),
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

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
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

      {/* CAPA 2: Matriz de Enrutamiento de los 4 Puntos Canónicos (T-DSH-116) */}
      <section aria-labelledby="heading-cognitive-points" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-primary" aria-hidden="true" />
            <h3 id="heading-cognitive-points" className="text-sm font-bold text-foreground">
              Capa 2 — Matriz de Enrutamiento Cognitivo (T-DSH-116)
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

      {/* CAPA 3: Política de Fallback Soberano (ADR-028) */}
      <section aria-labelledby="heading-sovereignty" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-3">
        <div className="flex items-center gap-2">
          <ShieldAlert className="w-4 h-4 text-primary" aria-hidden="true" />
          <h3 id="heading-sovereignty" className="text-sm font-bold text-foreground">
            Capa 3 — Política de Resiliencia y Fallback Soberano (ADR-028)
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
