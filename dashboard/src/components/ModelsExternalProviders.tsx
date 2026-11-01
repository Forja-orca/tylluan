import React, { useState, useEffect, useCallback } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { RefreshCw, CheckCircle2, XCircle, Zap, Globe, ExternalLink } from 'lucide-react';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
}

interface ExternalProvider {
  name: string;
  provider_type: string;
  base_url: string;
  models: string[];
  api_key_set: boolean;
}

interface TestResult {
  ok: boolean;
  latency_ms?: number;
  error?: string;
}

export function ModelsExternalProviders({ bridge }: Props) {
  const [providers, setProviders] = useState<ExternalProvider[]>([]);
  const [loading, setLoading] = useState(true);
  const [testResults, setTestResults] = useState<Record<string, TestResult | undefined>>({});
  const [testing, setTesting] = useState<string | null>(null);
  const [selectedModels, setSelectedModels] = useState<Record<string, string>>({});

  const fetchProviders = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const res = await bridge.fetchRaw('/api/v1/external-providers');
      setProviders(res?.providers || []);
    } catch (err) {
      console.warn('Failed to fetch external providers:', err);
      setProviders([]);
    }
    setLoading(false);
  }, [bridge]);

  useEffect(() => {
    fetchProviders();
  }, [fetchProviders]);

  const handleTest = async (provider: ExternalProvider) => {
    if (!bridge) return;
    const model = selectedModels[provider.name] || provider.models[0] || '';
    setTesting(provider.name);
    setTestResults(prev => ({ ...prev, [provider.name]: undefined }));
    try {
      const res = await bridge.fetchRaw(`/api/v1/external-providers/${encodeURIComponent(provider.name)}/test?model=${encodeURIComponent(model)}`);
      setTestResults(prev => ({
        ...prev,
        [provider.name]: {
          ok: res?.ok ?? false,
          latency_ms: res?.latency_ms,
          error: res?.error,
        }
      }));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || 'Connection failed');
      setTestResults(prev => ({
        ...prev,
        [provider.name]: { ok: false, error: msg }
      }));
    }
    setTesting(null);
  };

  const providerTypeIcon = (type: string) => {
    switch (type?.toLowerCase()) {
      case 'openai': return '🟢';
      case 'anthropic': return '🟠';
      case 'ollama': return '🦙';
      case 'openai-compatible': return '🔵';
      default: return '⚪';
    }
  };

  const providerTypeLabel = (type: string) => {
    switch (type?.toLowerCase()) {
      case 'openai': return 'OpenAI';
      case 'anthropic': return 'Anthropic';
      case 'ollama': return 'Ollama';
      case 'openai-compatible': return 'OpenAI-compatible';
      default: return type || 'Unknown';
    }
  };

  if (loading) {
    return (
      <div className="p-8 flex items-center justify-center">
        <RefreshCw className="w-6 h-6 animate-spin text-slate-500" />
      </div>
    );
  }

  return (
    <div className="space-y-4 font-sans">
      <fieldset className="rounded-lg bg-card border border-border p-4 shadow-sm">
        <legend className="text-xs font-bold text-primary px-1">
          Proveedores Externos de Inferencia
        </legend>
        <div className="flex items-center justify-between mb-3 border-b border-border pb-3">
          <div>
            <h3 className="text-sm font-bold text-foreground">
              Conexión a Endpoints de Inferencia
            </h3>
            <p className="text-xs text-muted-foreground mt-0.5">
              Proveedores de inferencia configurados en <code className="font-mono text-foreground font-semibold">tylluan.toml</code> bajo <code className="font-mono text-foreground font-semibold">[[external_providers]]</code>.
            </p>
          </div>
          <button
            onClick={fetchProviders}
            className="px-3 py-1.5 bg-muted/60 hover:bg-muted border border-border text-foreground rounded-md text-xs font-semibold shadow-sm flex items-center gap-1.5 transition-colors cursor-pointer"
          >
            <RefreshCw className="w-3.5 h-3.5 text-primary" />
            Actualizar
          </button>
        </div>

        {providers.length === 0 ? (
          <div className="p-6 bg-muted/30 border border-border rounded-lg flex flex-col items-center justify-center gap-1.5 text-center">
            <Globe className="w-6 h-6 text-muted-foreground" />
            <p className="text-xs font-bold text-foreground">Sin proveedores externos configurados</p>
            <p className="text-xs text-muted-foreground max-w-md leading-normal">
              Añada un bloque <code className="font-mono">[external_providers]</code> en su archivo <code className="font-mono">tylluan.toml</code> para integrar modelos remotos compatibles con OpenAI/Anthropic/Ollama.
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {providers.map((provider) => {
              const result = testResults[provider.name];
              const isTesting = testing === provider.name;
              return (
                <div
                  key={provider.name}
                  className="bg-muted/30 border border-border rounded-lg p-3 space-y-2.5"
                >
                  {/* Header */}
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-base">{providerTypeIcon(provider.provider_type)}</span>
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-bold text-foreground">{provider.name}</span>
                          <span className="text-[10px] font-mono px-1.5 py-0.5 rounded-md border border-border bg-muted text-foreground font-bold">
                            {providerTypeLabel(provider.provider_type)}
                          </span>
                        </div>
                        <div className="flex items-center gap-1 mt-0.5">
                          <span className="text-[10px] font-mono text-muted-foreground flex items-center gap-1">
                            <ExternalLink className="w-2.5 h-2.5" />
                            {provider.base_url}
                          </span>
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center gap-2">
                      {/* API Key status */}
                      <span className={cn(
                        "text-[10px] font-mono px-1.5 py-0.5 rounded-md border flex items-center gap-1 font-bold",
                        provider.api_key_set
                          ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/30"
                          : "bg-muted text-muted-foreground border-border"
                      )}>
                        {provider.api_key_set ? (
                          <><CheckCircle2 className="w-3 h-3" /> Clave API configurada</>
                        ) : (
                          <><XCircle className="w-3 h-3" /> Sin clave API</>
                        )}
                      </span>
                    </div>
                  </div>

                  {/* Models list */}
                  <div className="flex flex-wrap gap-1.5">
                    {provider.models?.map((model) => (
                      <span
                        key={model}
                        className={cn(
                          "text-[10px] font-mono px-2 py-0.5 rounded-md border cursor-pointer transition-colors",
                          (selectedModels[provider.name] || provider.models[0]) === model
                            ? "bg-primary text-primary-foreground border-primary font-bold"
                            : "bg-card text-foreground border-border hover:bg-muted"
                        )}
                        onClick={() => setSelectedModels(prev => ({ ...prev, [provider.name]: model }))}
                      >
                        {model}
                      </span>
                    ))}
                    {(!provider.models || provider.models.length === 0) && (
                      <span className="text-[10px] text-muted-foreground font-mono">Sin modelos listados</span>
                    )}
                  </div>

                  {/* Test result */}
                  {result && (
                    <div className={cn(
                      "p-2 rounded-md border text-xs flex items-center gap-1.5 font-sans",
                      result.ok
                        ? "bg-emerald-500/10 text-emerald-800 dark:text-emerald-300 border-emerald-500/30"
                        : "bg-rose-500/10 text-rose-800 dark:text-rose-300 border-rose-500/30"
                    )}>
                      {result.ok ? (
                        <><CheckCircle2 className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" /> Conexión establecida correctamente{result.latency_ms !== undefined ? ` — ${result.latency_ms} ms` : ''}</>
                      ) : (
                        <><XCircle className="w-3.5 h-3.5 text-rose-600 dark:text-rose-400" /> {result.error || 'Fallo de conexión'}</>
                      )}
                    </div>
                  )}

                  {/* Test button */}
                  <div className="flex justify-end pt-1">
                    <button
                      onClick={() => handleTest(provider)}
                      disabled={isTesting}
                      className="px-3 py-1.5 bg-card hover:bg-muted disabled:opacity-50 border border-border text-foreground rounded-md text-xs font-semibold shadow-sm flex items-center gap-1.5 transition-colors cursor-pointer"
                    >
                      {isTesting ? (
                        <RefreshCw className="w-3.5 h-3.5 animate-spin text-primary" />
                      ) : (
                        <Zap className="w-3.5 h-3.5 text-primary" />
                      )}
                      {isTesting ? 'Comprobando...' : 'Probar conexión'}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </fieldset>
    </div>
  );
}
