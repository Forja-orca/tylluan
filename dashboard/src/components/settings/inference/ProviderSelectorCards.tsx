import React, { useState } from 'react';
import {
  RefreshCw,
  ExternalLink,
  FolderOpen,
  Play,
  Square,
  Zap,
  ChevronDown,
  ChevronUp,
  Plus,
  Trash2,
  Copy,
  Check,
  Folder,
  Info
} from 'lucide-react';
import type { LlamaServerStatus } from './model-catalog-contract';
import type { DetectedLocalModel } from './RoleModelAssignmentSection';

export interface DiscoveredProvider {
  id: string;
  label: string;
  base_url: string;
  detected: boolean;
  latency_ms?: number;
  models?: string[];
  models_count?: number;
  detail?: string;
  body_truncated?: boolean;
}

export interface DiscoverResponse {
  scanned: number;
  any_detected: boolean;
  providers: DiscoveredProvider[];
  configured_external?: ExternalProviderConfig[];
  how_to_read?: string;
}

export interface ExternalProviderConfig {
  name: string;
  type: string;
  base_url: string;
  api_key_env?: string;
  models: string[];
  api_key_set: boolean;
  key_source?: 'vault' | 'env' | 'none';
}

export interface ExternalProviderTestResult {
  ok: boolean;
  status: string;
  provider: string;
  type: string;
  base_url: string;
  model_tested?: string;
  models?: string[];
  http_status: number;
  latency_ms: number;
  response_snippet?: string;
  error?: string;
}

export interface ProviderSelectorCardsProps {
  discoverData: DiscoverResponse | null;
  discoverLoading: boolean;
  onRefreshDiscover: () => void;
  externalProviders: ExternalProviderConfig[];
  llamaStatus: LlamaServerStatus;
  onToggleLlamaServer: (start: boolean) => void;
  onOpenLlamaDirectory: () => void;
  onOpenLlamaWebUI: () => void;
  testingProvider: Record<string, boolean>;
  providerTestResults: Record<string, ExternalProviderTestResult>;
  onTestProvider: (providerName: string) => void;
  onRemoveProvider?: (providerName: string) => void;
  detectedModels?: DetectedLocalModel[];
  modelsDir?: string;
  modelsDirFromEnv?: boolean;
  hint?: string | null;
  onOpenAddProvider: () => void;
  kernelSupportsPost?: boolean;
}

export function ProviderSelectorCards({
  discoverData,
  discoverLoading,
  onRefreshDiscover,
  externalProviders,
  llamaStatus,
  onToggleLlamaServer,
  onOpenLlamaDirectory,
  onOpenLlamaWebUI,
  testingProvider,
  providerTestResults,
  onTestProvider,
  onRemoveProvider,
  detectedModels = [],
  modelsDir,
  modelsDirFromEnv = false,
  hint,
  onOpenAddProvider,
  kernelSupportsPost = false,
}: ProviderSelectorCardsProps) {
  const [showTylluanModels, setShowTylluanModels] = useState(() => Boolean(hint && detectedModels.length === 0));
  const [copiedPath, setCopiedPath] = useState(false);

  const handleCopyPath = async (path: string) => {
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(path);
        setCopiedPath(true);
        setTimeout(() => setCopiedPath(false), 2000);
      }
    } catch (err) {
      console.error('Error al copiar ruta al portapapeles:', err);
    }
  };

  // Buscar datos descubiertos para proveedores loopback
  const lmStudio = discoverData?.providers?.find(p => p.id === 'lm-studio');
  const llamaCppDiscovered = discoverData?.providers?.find(p => p.id === 'llama-cpp');
  const ollama = discoverData?.providers?.find(p => p.id === 'ollama');

  const isLlamaRunning = Boolean(llamaStatus.running || llamaCppDiscovered?.detected);
  const llamaPort = llamaStatus.port || 9000;

  return (
    <section aria-labelledby="heading-inference-providers" className="space-y-3" data-testid="provider-selector-container">
      {/* Cabecera de la lista densa con botón [+ Añadir proveedor] */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-border">
        <div>
          <h3 id="heading-inference-providers" className="text-sm font-bold text-foreground">
            Proveedores de inferencia
          </h3>
          <p className="text-xs text-muted-foreground">
            Servidores locales detectados en loopback y proveedores externos configurados.
          </p>
        </div>

        <div className="flex items-center gap-2">
          {/* Botón Añadir proveedor (T-UI-2 / SPEC §4) */}
          <button
            type="button"
            onClick={onOpenAddProvider}
            disabled={!kernelSupportsPost}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold rounded-md transition-all shadow-sm ${
              kernelSupportsPost
                ? 'bg-primary text-primary-foreground hover:bg-primary/90 cursor-pointer focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-1'
                : 'bg-muted text-muted-foreground border border-border cursor-not-allowed opacity-60'
            }`}
            data-testid="add-provider-btn"
            title={kernelSupportsPost ? 'Añadir proveedor de inferencia' : 'disponible cuando el kernel lo soporte'}
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Añadir proveedor</span>
            {!kernelSupportsPost && (
              <span className="text-[10px] opacity-75 font-normal ml-1">
                (disponible cuando el kernel lo soporte)
              </span>
            )}
          </button>
        </div>
      </div>

      {/* Lista densa con punto de estado por fila (SPEC §4 y §8) */}
      <div className="border border-border rounded-lg divide-y divide-border bg-card shadow-sm overflow-hidden" data-testid="dense-provider-list">
        {/* FILA 1: Tylluan (integrado) - Fijo, siempre presente */}
        <div className="p-3 hover:bg-muted/30 transition-colors" data-testid="provider-row-tylluan">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
            <div className="flex items-center gap-2.5 min-w-0">
              {/* Punto de estado verde */}
              <span className="w-2.5 h-2.5 rounded-full bg-emerald-500 shrink-0" title="Activo y disponible" />
              <div className="min-w-0">
                <span className="text-xs font-bold text-foreground">Tylluan (integrado)</span>
                <span className="text-[11px] text-muted-foreground ml-2 font-mono">ONNX · CPU</span>
              </div>
            </div>

            <div className="flex items-center gap-3 self-end sm:self-auto shrink-0">
              <span className="text-xs text-muted-foreground font-mono">
                {detectedModels.length} modelo{detectedModels.length === 1 ? '' : 's'}
              </span>
              <button
                type="button"
                onClick={() => setShowTylluanModels(!showTylluanModels)}
                className="inline-flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded border border-border bg-muted/40 hover:bg-muted text-foreground transition-colors cursor-pointer min-h-[24px]"
                aria-expanded={showTylluanModels}
                aria-label="Ver modelos de Tylluan en disco"
              >
                <span>Ver</span>
                {showTylluanModels ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
              </button>
            </div>
          </div>

          {/* Sublista desplegable de modelos Tylluan en disco */}
          {showTylluanModels && (
            <div className="mt-3 pt-3 border-t border-border/60 space-y-2 animate-in fade-in duration-150">
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-1 text-[11px] text-muted-foreground">
                <span className="font-semibold uppercase tracking-wider">
                  Modelos internos en disco:
                </span>
                {!hint && (
                  <span className="italic">
                    Para chatear, copia un modelo en la carpeta models
                  </span>
                )}
              </div>
              {detectedModels.length === 0 ? (
                hint || modelsDir ? (
                  <div
                    role="region"
                    aria-label="Directorio de modelos"
                    className="p-3 rounded-lg border border-border/80 bg-muted/20 space-y-2.5 text-xs text-foreground"
                    data-testid="empty-models-hint-box"
                  >
                    {hint && (
                      <div className="flex items-start gap-2">
                        <Info className="w-4 h-4 text-muted-foreground shrink-0 mt-0.5" aria-hidden="true" />
                        <p className="text-muted-foreground leading-relaxed" data-testid="empty-models-hint-text">
                          {hint}
                        </p>
                      </div>
                    )}
                    {modelsDir && (
                      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 p-2 rounded bg-background/80 border border-border/60">
                        <div className="flex items-center gap-2 min-w-0">
                          <Folder className="w-3.5 h-3.5 text-muted-foreground shrink-0" aria-hidden="true" />
                          <span
                            className="font-mono text-[11px] text-foreground break-all select-all font-medium"
                            data-testid="models-dir-path"
                          >
                            {modelsDir}
                          </span>
                          {modelsDirFromEnv && (
                            <span className="px-1.5 py-0.5 rounded text-[9px] font-mono border border-border bg-muted text-muted-foreground shrink-0">
                              TYLLUAN_MODELS_DIR
                            </span>
                          )}
                        </div>
                        <button
                          type="button"
                          onClick={() => handleCopyPath(modelsDir)}
                          className="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded border border-border bg-card hover:bg-accent text-foreground transition-colors shrink-0 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          aria-label="Copiar ruta"
                          data-testid="copy-models-dir-btn"
                        >
                          {copiedPath ? (
                            <Check className="w-3.5 h-3.5 text-emerald-500" aria-hidden="true" />
                          ) : (
                            <Copy className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                          )}
                          <span>{copiedPath ? 'Copiada' : 'Copiar ruta'}</span>
                        </button>
                      </div>
                    )}
                  </div>
                ) : (
                  <p className="text-xs text-muted-foreground italic">No se detectaron modelos en la carpeta models/.</p>
                )
              ) : (
                <div className="space-y-1.5 max-h-56 overflow-y-auto pr-1">
                  {detectedModels.map(model => {
                    const modelId = model.id || model.name;
                    const sizeText = model.size_mb
                      ? `${model.size_mb} MB`
                      : model.size_bytes
                      ? `${(model.size_bytes / (1024 * 1024)).toFixed(0)} MB`
                      : '';
                    const declaredRole = model.role || 'sin rol declarado';

                    return (
                      <div
                        key={modelId}
                        className="flex flex-col sm:flex-row sm:items-center justify-between gap-1.5 p-2 rounded bg-muted/20 border border-border/50 text-xs"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="font-mono font-medium text-foreground truncate">{model.name}</span>
                          <span className="px-1.5 py-0.2 rounded text-[10px] font-mono border border-border bg-muted text-muted-foreground shrink-0">
                            {declaredRole}
                          </span>
                          {sizeText && (
                            <span className="text-[10px] text-muted-foreground font-mono shrink-0">{sizeText}</span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </div>

        {/* FILA 2: LM Studio */}
        <div className="p-3 hover:bg-muted/30 transition-colors" data-testid="provider-row-lmstudio">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
            <div className="flex items-center gap-2.5 min-w-0">
              <span
                className={`w-2.5 h-2.5 rounded-full shrink-0 ${
                  lmStudio?.detected ? 'bg-emerald-500' : 'bg-muted-foreground/30 border border-muted-foreground/40'
                }`}
                title={lmStudio?.detected ? `Detectado en :1234${lmStudio.latency_ms != null ? ` (${lmStudio.latency_ms} ms)` : ''}` : 'No detectado :1234'}
              />
              <div className="min-w-0">
                <span className="text-xs font-bold text-foreground">LM Studio</span>
                <span className="text-[11px] text-muted-foreground ml-2 font-mono">
                  {lmStudio?.detected ? `detectado :1234${lmStudio.latency_ms != null ? ` (${lmStudio.latency_ms} ms)` : ''}` : 'no detectado :1234'}
                </span>
              </div>
            </div>

            <div className="flex items-center gap-3 self-end sm:self-auto shrink-0">
              {lmStudio?.detected && (
                <span className="text-xs text-muted-foreground font-mono">
                  {lmStudio.models_count ?? lmStudio.models?.length ?? 0} modelo(s)
                </span>
              )}
              <button
                type="button"
                onClick={onRefreshDiscover}
                disabled={discoverLoading}
                className="inline-flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded border border-border bg-muted/40 hover:bg-muted text-foreground transition-colors disabled:opacity-50 cursor-pointer min-h-[24px]"
                aria-label="Comprobar LM Studio"
              >
                <RefreshCw className={`w-3 h-3 ${discoverLoading ? 'animate-spin' : ''}`} />
                <span>Comprobar</span>
              </button>
            </div>
          </div>
        </div>

        {/* FILA 3: Ollama */}
        <div className="p-3 hover:bg-muted/30 transition-colors" data-testid="provider-row-ollama">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
            <div className="flex items-center gap-2.5 min-w-0">
              <span
                className={`w-2.5 h-2.5 rounded-full shrink-0 ${
                  ollama?.detected ? 'bg-emerald-500' : 'bg-muted-foreground/30 border border-muted-foreground/40'
                }`}
                title={ollama?.detected ? `Detectado en :11434${ollama.latency_ms != null ? ` (${ollama.latency_ms} ms)` : ''}` : 'No detectado :11434'}
              />
              <div className="min-w-0">
                <span className="text-xs font-bold text-foreground">Ollama</span>
                <span className="text-[11px] text-muted-foreground ml-2 font-mono">
                  {ollama?.detected ? `detectado :11434${ollama.latency_ms != null ? ` (${ollama.latency_ms} ms)` : ''}` : 'no detectado :11434'}
                </span>
              </div>
            </div>

            <div className="flex items-center gap-3 self-end sm:self-auto shrink-0">
              {ollama?.detected && (
                <span className="text-xs text-muted-foreground font-mono">
                  {ollama.models_count ?? ollama.models?.length ?? 0} modelo(s)
                </span>
              )}
              <button
                type="button"
                onClick={onRefreshDiscover}
                disabled={discoverLoading}
                className="inline-flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded border border-border bg-muted/40 hover:bg-muted text-foreground transition-colors disabled:opacity-50 cursor-pointer min-h-[24px]"
                aria-label="Reintentar detección de Ollama"
              >
                <RefreshCw className={`w-3 h-3 ${discoverLoading ? 'animate-spin' : ''}`} />
                <span>Reintentar</span>
              </button>
            </div>
          </div>
        </div>

        {/* FILA 4: llama.cpp del usuario */}
        <div className="p-3 hover:bg-muted/30 transition-colors" data-testid="provider-row-llamacpp">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
            <div className="flex items-center gap-2.5 min-w-0">
              <span
                className={`w-2.5 h-2.5 rounded-full shrink-0 ${
                  isLlamaRunning ? 'bg-emerald-500' : 'bg-muted-foreground/30 border border-muted-foreground/40'
                }`}
                title={isLlamaRunning ? `En ejecución :${llamaPort}` : `Parado :${llamaPort}`}
              />
              <div className="min-w-0">
                <span className="text-xs font-bold text-foreground">llama.cpp</span>
                <span className="text-[11px] text-muted-foreground ml-2 font-mono">
                  {isLlamaRunning ? `en ejecución :${llamaPort}` : `parado :${llamaPort}`}
                </span>
                <span className="text-[11px] text-muted-foreground block sm:inline-block sm:ml-2">
                  · Para chatear, copia un modelo en la carpeta models
                </span>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-1.5 self-end sm:self-auto shrink-0">
              <button
                type="button"
                onClick={() => onToggleLlamaServer(!isLlamaRunning)}
                aria-label={isLlamaRunning ? "Detener llama.cpp" : "Arrancar llama.cpp"}
                className={`inline-flex items-center gap-1 px-2.5 py-1 text-xs font-semibold rounded border transition-colors cursor-pointer min-h-[24px] ${
                  isLlamaRunning
                    ? 'bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/20 hover:bg-rose-500/20'
                    : 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20 hover:bg-emerald-500/20'
                }`}
              >
                {isLlamaRunning ? <Square className="w-3 h-3" /> : <Play className="w-3 h-3" />}
                <span>{isLlamaRunning ? 'Detener' : 'Arrancar'}</span>
              </button>

              <button
                type="button"
                onClick={onOpenLlamaWebUI}
                aria-label="Abrir UI web de llama.cpp"
                className="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded border border-border bg-muted/30 hover:bg-muted text-foreground transition-colors cursor-pointer min-h-[24px]"
              >
                <ExternalLink className="w-3 h-3 text-muted-foreground" />
                <span>Abrir web</span>
              </button>

              <button
                type="button"
                onClick={onOpenLlamaDirectory}
                aria-label="Abrir carpeta models"
                title="Copiar ruta de la carpeta models al portapapeles"
                className="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded border border-border bg-muted/30 hover:bg-muted text-foreground transition-colors cursor-pointer min-h-[24px]"
              >
                <FolderOpen className="w-3 h-3 text-muted-foreground" />
                <span>Carpeta models</span>
              </button>
            </div>
          </div>
        </div>

        {/* FILAS 5..N: Proveedores de Nube (externalProviders) */}
        {externalProviders.map(provider => {
          const isTesting = testingProvider[provider.name];
          const testResult = providerTestResults[provider.name];
          const keyStatusText = provider.api_key_set ? 'definida' : 'no definida';
          const keySourceText = provider.api_key_env
            ? `variable ${provider.api_key_env}`
            : provider.key_source === 'vault'
            ? 'SecretStore'
            : 'clave';

          return (
            <div
              key={provider.name}
              className="p-3 hover:bg-muted/30 transition-colors"
              data-testid={`provider-row-${provider.name}`}
            >
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                <div className="flex items-center gap-2.5 min-w-0">
                  <span
                    className={`w-2.5 h-2.5 rounded-full shrink-0 ${
                      testResult
                        ? testResult.ok
                          ? testResult.models && testResult.models.length === 0
                            ? 'bg-amber-500' // Responde 200 pero sin modelos disponibles (SPEC §8 Amber)
                            : 'bg-emerald-500'
                          : 'bg-rose-500'
                        : provider.api_key_set
                        ? 'bg-emerald-500/80'
                        : 'bg-muted-foreground/30 border border-muted-foreground/40'
                    }`}
                    title={
                      testResult
                        ? testResult.ok
                          ? testResult.models && testResult.models.length === 0
                            ? 'Conectado pero sin modelos disponibles'
                            : `Conectado (${testResult.latency_ms ?? 0} ms)`
                          : `Error: ${testResult.error || 'fallo de conexión'}`
                        : provider.api_key_set
                        ? 'Clave configurada (pendiente de comprobación)'
                        : 'Sin clave configurada'
                    }
                  />
                  <div className="min-w-0">
                    <span className="text-xs font-bold text-foreground capitalize">{provider.name}</span>
                    <span className="text-[11px] text-muted-foreground ml-2 font-mono">
                      nube · clave: {keySourceText} ({keyStatusText})
                      {testResult?.ok && testResult.latency_ms != null && ` · ${testResult.latency_ms} ms`}
                    </span>
                  </div>
                </div>

                <div className="flex items-center gap-2 self-end sm:self-auto shrink-0">
                  {provider.models && provider.models.length > 0 && (
                    <span className="text-xs text-muted-foreground font-mono">
                      {provider.models.length} modelo(s)
                    </span>
                  )}
                  <button
                    type="button"
                    onClick={() => onTestProvider(provider.name)}
                    disabled={isTesting}
                    className="inline-flex items-center gap-1 px-2.5 py-1 text-xs font-semibold rounded border border-border bg-muted/40 hover:bg-muted text-foreground transition-colors cursor-pointer min-h-[24px]"
                    aria-label={`Comprobar ${provider.name}`}
                  >
                    <Zap className={`w-3 h-3 text-amber-500 ${isTesting ? 'animate-pulse' : ''}`} />
                    <span>{isTesting ? 'Probando...' : 'Comprobar'}</span>
                  </button>

                  <button
                    type="button"
                    onClick={() => onRemoveProvider?.(provider.name)}
                    disabled={!kernelSupportsPost}
                    title={kernelSupportsPost ? 'Quitar proveedor' : 'disponible cuando el kernel lo soporte'}
                    aria-label={`Quitar ${provider.name}`}
                    className={`inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded border border-border transition-colors min-h-[24px] ${
                      kernelSupportsPost
                        ? 'bg-rose-500/10 text-rose-600 dark:text-rose-400 hover:bg-rose-500/20 cursor-pointer'
                        : 'opacity-50 text-muted-foreground cursor-not-allowed'
                    }`}
                  >
                    <Trash2 className="w-3 h-3" />
                    <span>Quitar</span>
                  </button>
                </div>
              </div>
            </div>
          );
        })}

        {/* Mensaje vacío para proveedores de nube (SPEC §7) */}
        {externalProviders.length === 0 && (
          <div className="p-3 text-center text-xs text-muted-foreground bg-muted/10 font-sans">
            Aún no has añadido ningún proveedor de nube.
          </div>
        )}
      </div>
    </section>
  );
}
