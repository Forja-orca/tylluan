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
    <div className="space-y-3 font-sans">
      <fieldset className="rounded-none bg-white dark:bg-card border border-[#ACA899] dark:border-border p-3">
        <legend className="text-[11px] font-bold text-[#9E4024] dark:text-primary px-1">
          Proveedores Externos de Inferencia
        </legend>
        <div className="flex items-center justify-between mb-3 border-b border-[#ACA899] dark:border-border pb-2">
          <div>
            <h3 className="text-xs font-bold text-[#01285F] dark:text-foreground">
              Conexión a Endpoints de Inferencia
            </h3>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">
              Proveedores de inferencia configurados en <code className="font-mono text-[#000000] dark:text-foreground font-bold">tylluan.toml</code> bajo <code className="font-mono text-[#000000] dark:text-foreground font-bold">[[external_providers]]</code>.
            </p>
          </div>
          <button
            onClick={fetchProviders}
            className="px-2.5 py-1 bg-white hover:bg-[#F5F4EA] border border-[#ACA899] text-[#000000] dark:text-foreground rounded-sm text-[11px] font-bold flex items-center gap-1.5 transition-colors cursor-pointer"
          >
            <RefreshCw className="w-3 h-3 text-[#9E4024]" />
            Actualizar
          </button>
        </div>

        {providers.length === 0 ? (
          <div className="p-6 bg-[#F5F4EA] dark:bg-muted/30 border border-[#ACA899] dark:border-border rounded-none flex flex-col items-center justify-center gap-1.5 text-center">
            <Globe className="w-6 h-6 text-[#7F9DB9]" />
            <p className="text-xs font-bold text-[#01285F] dark:text-foreground">Sin proveedores externos configurados</p>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground max-w-md leading-normal">
              Añada un bloque <code className="font-mono">[external_providers]</code> en su archivo <code className="font-mono">tylluan.toml</code> para integrar modelos remotos compatibles con OpenAI/Anthropic/Ollama.
            </p>
          </div>
        ) : (
          <div className="space-y-2.5">
            {providers.map((provider) => {
              const result = testResults[provider.name];
              const isTesting = testing === provider.name;
              return (
                <div
                  key={provider.name}
                  className="bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border rounded-none p-2.5 space-y-2"
                >
                  {/* Header */}
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-base">{providerTypeIcon(provider.provider_type)}</span>
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-bold text-[#01285F] dark:text-foreground">{provider.name}</span>
                          <span className="text-[10px] font-mono px-1 py-0.2 border border-[#ACA899] bg-[#ECE9D8] dark:bg-muted text-[#000000] dark:text-foreground font-bold">
                            {providerTypeLabel(provider.provider_type)}
                          </span>
                        </div>
                        <div className="flex items-center gap-1 mt-0.5">
                          <span className="text-[10px] font-mono text-[#555555] dark:text-muted-foreground flex items-center gap-1">
                            <ExternalLink className="w-2.5 h-2.5" />
                            {provider.base_url}
                          </span>
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center gap-2">
                      {/* API Key status */}
                      <span className={cn(
                        "text-[10px] font-mono px-1.5 py-0.2 rounded-none border flex items-center gap-1 font-bold",
                        provider.api_key_set
                          ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/30"
                          : "bg-slate-100 text-[#555555] border-[#ACA899]"
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
                          "text-[10px] font-mono px-1.5 py-0.5 rounded-none border cursor-pointer transition-colors",
                          (selectedModels[provider.name] || provider.models[0]) === model
                            ? "bg-[#9E4024] text-white border-[#9E4024] font-bold"
                            : "bg-[#ECE9D8] dark:bg-muted text-[#000000] dark:text-foreground border-[#ACA899] hover:bg-[#F5F4EA]"
                        )}
                        onClick={() => setSelectedModels(prev => ({ ...prev, [provider.name]: model }))}
                      >
                        {model}
                      </span>
                    ))}
                    {(!provider.models || provider.models.length === 0) && (
                      <span className="text-[10px] text-[#555555] dark:text-muted-foreground font-mono">Sin modelos listados</span>
                    )}
                  </div>

                  {/* Test result */}
                  {result && (
                    <div className={cn(
                      "p-1.5 rounded-none border text-[11px] flex items-center gap-1.5 font-sans",
                      result.ok
                        ? "bg-emerald-50 text-emerald-800 border-emerald-500/30"
                        : "bg-rose-50 text-rose-800 border-rose-500/30"
                    )}>
                      {result.ok ? (
                        <><CheckCircle2 className="w-3.5 h-3.5 text-emerald-600" /> Conexión establecida correctamente{result.latency_ms !== undefined ? ` — ${result.latency_ms} ms` : ''}</>
                      ) : (
                        <><XCircle className="w-3.5 h-3.5 text-rose-600" /> {result.error || 'Fallo de conexión'}</>
                      )}
                    </div>
                  )}

                  {/* Test button */}
                  <div className="flex justify-end pt-1">
                    <button
                      onClick={() => handleTest(provider)}
                      disabled={isTesting}
                      className="px-2.5 py-1 bg-white hover:bg-[#F5F4EA] disabled:opacity-50 border border-[#ACA899] text-[#000000] dark:text-foreground rounded-sm text-[11px] font-bold flex items-center gap-1.5 transition-colors cursor-pointer"
                    >
                      {isTesting ? (
                        <RefreshCw className="w-3 h-3 animate-spin text-[#9E4024]" />
                      ) : (
                        <Zap className="w-3 h-3 text-[#9E4024]" />
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
