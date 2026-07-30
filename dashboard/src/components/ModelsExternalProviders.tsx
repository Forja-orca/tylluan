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
  const [testResults, setTestResults] = useState<Record<string, TestResult>>({});
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
    setTestResults(prev => ({ ...prev, [provider.name]: undefined as any }));
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
    } catch (err: any) {
      setTestResults(prev => ({
        ...prev,
        [provider.name]: { ok: false, error: err.message || 'Connection failed' }
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
    <div className="space-y-6">
      <div className="rounded-lg bg-slate-900/50 p-4">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
              <Globe className="w-4 h-4 text-amber-400" /> External Providers
            </h3>
            <p className="text-xs text-slate-400 mt-1">
              Proveedores de inferencia configurados en <code className="text-slate-300 font-mono text-[10px]">tylluan.toml</code> bajo <code className="text-slate-300 font-mono text-[10px]">[[external_providers]]</code>.
            </p>
          </div>
          <button
            onClick={fetchProviders}
            className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg text-xs font-mono flex items-center gap-2 transition-colors"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            Actualizar
          </button>
        </div>

        {providers.length === 0 ? (
          <div className="p-8 bg-slate-950/40 border border-dashed border-slate-800 rounded-lg flex flex-col items-center justify-center gap-2 text-center">
            <Globe className="w-8 h-8 text-slate-600" />
            <p className="text-sm font-mono text-slate-400 font-semibold">Sin external providers configurados</p>
            <p className="text-xs text-slate-500 max-w-md leading-relaxed">
              Añade un bloque <code className="text-slate-400">[[external_providers]]</code> en tu <code className="text-slate-400">tylluan.toml</code> con name, provider_type, base_url, api_key y models.
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
                  className="bg-slate-950/50 rounded-lg p-4 space-y-3"
                >
                  {/* Header */}
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <span className="text-lg">{providerTypeIcon(provider.provider_type)}</span>
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-semibold text-slate-200">{provider.name}</span>
                          <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-400">
                            {providerTypeLabel(provider.provider_type)}
                          </span>
                        </div>
                        <div className="flex items-center gap-2 mt-0.5">
                          <span className="text-[10px] font-mono text-slate-500 flex items-center gap-1">
                            <ExternalLink className="w-2.5 h-2.5" />
                            {provider.base_url}
                          </span>
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center gap-2">
                      {/* API Key status */}
                      <span className={cn(
                        "text-[10px] font-mono px-2 py-0.5 rounded-full flex items-center gap-1",
                        provider.api_key_set
                          ? "bg-emerald-500/10 text-emerald-400"
                          : "bg-slate-800 text-slate-500"
                      )}>
                        {provider.api_key_set ? (
                          <><CheckCircle2 className="w-3 h-3" /> API key configurada</>
                        ) : (
                          <><XCircle className="w-3 h-3" /> Sin API key</>
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
                          "text-[10px] font-mono px-2 py-0.5 rounded cursor-pointer transition-colors",
                          (selectedModels[provider.name] || provider.models[0]) === model
                            ? "bg-amber-500/20 text-amber-300 ring-1 ring-amber-500/30"
                            : "bg-slate-800 text-slate-400 hover:text-slate-300"
                        )}
                        onClick={() => setSelectedModels(prev => ({ ...prev, [provider.name]: model }))}
                      >
                        {model}
                      </span>
                    ))}
                    {(!provider.models || provider.models.length === 0) && (
                      <span className="text-[10px] text-slate-600 font-mono">Sin modelos listados</span>
                    )}
                  </div>

                  {/* Test result */}
                  {result && (
                    <div className={cn(
                      "p-2 rounded text-xs flex items-center gap-2",
                      result.ok
                        ? "bg-emerald-950/20 text-emerald-300"
                        : "bg-rose-950/20 text-rose-300"
                    )}>
                      {result.ok ? (
                        <><CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" /> Conexión exitosa{result.latency_ms !== undefined ? ` — ${result.latency_ms}ms` : ''}</>
                      ) : (
                        <><XCircle className="w-3.5 h-3.5 text-rose-400" /> {result.error || 'Connection failed'}</>
                      )}
                    </div>
                  )}

                  {/* Test button */}
                  <div className="flex justify-end">
                    <button
                      onClick={() => handleTest(provider)}
                      disabled={isTesting}
                      className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-300 rounded-lg text-xs font-mono flex items-center gap-2 transition-colors"
                    >
                      {isTesting ? (
                        <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                      ) : (
                        <Zap className="w-3.5 h-3.5 text-amber-400" />
                      )}
                      {isTesting ? 'Probando...' : 'Test connection'}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
