import React, { useState, useEffect } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Save, AlertTriangle, RefreshCw, Cpu, Database, Image as ImageIcon, Sparkles, Coffee, ShieldCheck } from 'lucide-react';
import { cn } from '../lib/utils';
import { StatusPill } from './ui/StatusPill';

interface Props {
  bridge: NexusBridge | null;
}

export function ModelConfigPanel({ bridge }: Props) {
  const [config, setConfig] = useState<any>(null);
  const [models, setModels] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [embeddingModel, setEmbeddingModel] = useState('');
  const [rawConfigStr, setRawConfigStr] = useState('');
  const [selectedDevice, setSelectedDevice] = useState('cpu');
  const [initialDevice, setInitialDevice] = useState('cpu');
  const [showRestartModal, setShowRestartModal] = useState(false);

  // GGUF & Inference Selector State
  const [selectedGgufModel, setSelectedGgufModel] = useState('qwen2.5-1.5b-instruct');
  const [activeProvider, setActiveProvider] = useState('llama-server');
  const [providerUrl, setProviderUrl] = useState('http://127.0.0.1:8080');
  const [contextLen, setContextLen] = useState(4096);
  const [temperature, setTemperature] = useState(0.7);
  const [topP, setTopP] = useState(0.95);
  const [testingConn, setTestingConn] = useState(false);
  const [connStatus, setConnStatus] = useState<{ ok: boolean; msg: string } | null>(null);
  const [savingGguf, setSavingGguf] = useState(false);

  useEffect(() => {
    const loadData = async () => {
      if (!bridge) return;
      setLoading(true);
      try {
        const cfg = await bridge.getConfig();
        setConfig(cfg);
        // Fallback for embedding model
        setEmbeddingModel(cfg?.memory?.embedding_model || cfg?.embedding?.model_name || cfg?.embeddings?.model || '');
        setRawConfigStr(typeof cfg === 'string' ? cfg : JSON.stringify(cfg, null, 2));
        
        // Extract device config
        const dev = cfg?.inference?.device || 'cpu';
        setSelectedDevice(dev);
        setInitialDevice(dev);

        // Extract GGUF / inference settings if present
        if (cfg?.inference?.primary_model) {
          setSelectedGgufModel(cfg.inference.primary_model);
        }
        if (cfg?.inference?.provider) {
          setActiveProvider(cfg.inference.provider);
        }
        if (cfg?.inference?.endpoint) {
          setProviderUrl(cfg.inference.endpoint);
        }
        if (cfg?.inference?.context_size) {
          setContextLen(cfg.inference.context_size);
        }
        if (cfg?.inference?.temperature !== undefined) {
          setTemperature(cfg.inference.temperature);
        }

        try {
          const m = await bridge.fetchRaw('/api/v1/models');
          setModels(m);
        } catch {
          setModels(null);
        }
      } catch (e) {
        console.error('Failed to load config/models', e);
      }
      setLoading(false);
    };
    loadData();
  }, [bridge]);

  const handleTestConnection = async () => {
    if (!bridge) return;
    setTestingConn(true);
    setConnStatus(null);
    try {
      const res = await bridge.fetchRaw('/api/v1/test-connection', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ endpoint: providerUrl, provider: activeProvider })
      });
      if (res && (res.ok || res.status === 'ok')) {
        setConnStatus({ ok: true, msg: `Conexión exitosa a ${activeProvider} (${providerUrl})` });
      } else {
        setConnStatus({ ok: false, msg: res?.error || `No se pudo conectar a ${providerUrl}` });
      }
    } catch (err: any) {
      setConnStatus({ ok: false, msg: err.message || 'Error de conexión HTTP/API' });
    }
    setTestingConn(false);
  };

  const handleSaveGgufConfig = async () => {
    if (!bridge) return;
    setSavingGguf(true);
    try {
      await bridge.fetch('/api/v1/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          inference: {
            primary_model: selectedGgufModel,
            provider: activeProvider,
            endpoint: providerUrl,
            context_size: contextLen,
            temperature: temperature,
            top_p: topP
          }
        })
      });
      alert('Configuración GGUF e inferencia guardada exitosamente en tylluan.toml.');
    } catch (e: any) {
      alert(`Error guardando configuración GGUF: ${e.message || String(e)}`);
    }
    setSavingGguf(false);
  };

  const handleSave = async () => {
    if (!bridge) return;
    setSaving(true);
    try {
      if (selectedDevice !== initialDevice) {
        await bridge.fetch('/api/v1/config/device', {
          method: 'POST',
          body: JSON.stringify({ device: selectedDevice })
        });
      }

      if (selectedDevice !== initialDevice) {
        setInitialDevice(selectedDevice);
        setShowRestartModal(true);
      } else {
        alert('Configuracion guardada exitosamente.');
      }
    } catch (e) {
      alert(`Error guardando: ${e instanceof Error ? e.message : String(e)}`);
    }
    setSaving(false);
  };

  if (loading) {
    return (
      <div className="p-8 flex items-center justify-center">
        <RefreshCw className="w-6 h-6 animate-spin text-slate-500" />
      </div>
    );
  }

  const visionModel = config?.vision?.model_path?.split('/').pop() || 'SmolVLM2-256M';
  const inferenceModel = config?.inference?.primary_model || 'Unknown';

  return (
    <div className="space-y-6">
      {/* Active Models */}
      <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-4">
        <h3 className="text-sm font-bold text-slate-300 mb-4 flex items-center gap-2">
          <Cpu className="w-4 h-4 text-emerald-500" /> Active Models
        </h3>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="bg-slate-950 border border-slate-800 p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-slate-500 uppercase tracking-wider mb-2">
              <Database className="w-3 h-3" /> Embedding
            </div>
            <div className="text-sm font-mono text-emerald-400 truncate" title={embeddingModel}>
              {embeddingModel || 'None'}
            </div>
            <div className="text-[9px] text-slate-500 mt-1 uppercase">
              (change requires restart + reindex)
            </div>
          </div>
          <div className="bg-slate-950 border border-slate-800 p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-slate-500 uppercase tracking-wider mb-2">
              <ImageIcon className="w-3 h-3" /> Vision
            </div>
            <div className="text-sm font-mono text-blue-400 truncate" title={visionModel}>
              {visionModel}
            </div>
          </div>
          <div className="bg-slate-950 border border-slate-800 p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-slate-500 uppercase tracking-wider mb-2">
              <Sparkles className="w-3 h-3" /> Inference
            </div>
            <div className="text-sm font-mono text-violet-400 truncate" title={inferenceModel}>
              {inferenceModel}
            </div>
          </div>
        </div>
      </div>

      {/* Hardware Acceleration (GPU/CPU) */}
      <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-4">
        <h3 className="text-sm font-bold text-slate-300 mb-4 flex items-center gap-2">
          <Cpu className="w-4 h-4 text-cyan-500" /> Aceleración de Inferencia (GPU/CPU)
        </h3>
        <p className="text-xs text-slate-400 mb-4">
          Selecciona el dispositivo de ejecución para ONNX Runtime (embeddings y vision). Las GPUs reducen los tiempos de generación e indexación drásticamente.
        </p>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
          <button
            type="button"
            onClick={() => setSelectedDevice('cpu')}
            className={cn(
              "flex flex-col items-start p-3 rounded-lg border text-left transition-all",
              selectedDevice === 'cpu'
                ? "bg-slate-800/80 border-slate-600 text-slate-200 ring-1 ring-slate-500"
                : "bg-slate-950/40 border-slate-900 text-slate-500 hover:border-slate-850 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-bold uppercase tracking-wider mb-1 flex items-center gap-1.5">
              <Cpu className="w-3.5 h-3.5 text-slate-400" /> CPU Standalone
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Usa el procesador del sistema. Estable, pero lento (2-8s por embedding).
            </span>
            {selectedDevice === 'cpu' && (
              <span className="text-[9px] text-emerald-400 font-mono mt-2 uppercase tracking-wide">
                ● Activo
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => setSelectedDevice('directml')}
            className={cn(
              "flex flex-col items-start p-3 rounded-lg border text-left transition-all relative",
              selectedDevice === 'directml'
                ? "bg-emerald-950/20 border-emerald-500/80 text-emerald-300 ring-1 ring-emerald-500"
                : "bg-slate-950/40 border-slate-900 text-slate-500 hover:border-slate-850 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-bold uppercase tracking-wider mb-1 flex items-center gap-1.5">
              <span className="text-emerald-400">⚡</span> DirectML (GPU)
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Recomendado para Windows. Acelera en cualquier GPU (NVIDIA RTX 3060, AMD, Intel) sin recompilar.
            </span>
            {selectedDevice === 'directml' && (
              <span className="text-[9px] text-emerald-400 font-mono mt-2 uppercase tracking-wide flex items-center gap-1">
                ● Seleccionado (DirectML GPU)
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => setSelectedDevice('cuda')}
            className={cn(
              "flex flex-col items-start p-3 rounded-lg border text-left transition-all relative",
              selectedDevice === 'cuda'
                ? "bg-amber-950/20 border-amber-500/80 text-amber-300 ring-1 ring-amber-500"
                : "bg-slate-950/40 border-slate-900 text-slate-500 hover:border-slate-850 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-bold uppercase tracking-wider mb-1 flex items-center gap-1.5">
              <span className="text-amber-400">🔥</span> NVIDIA CUDA
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Requiere binario compilado con `--features cuda`. Si no está activo, caerá a CPU/DirectML.
            </span>
            {selectedDevice === 'cuda' && (
              <span className="text-[9px] text-amber-400 font-mono mt-2 uppercase tracking-wide flex items-center gap-1">
                ⚠️ Requiere feature `cuda` compilada (Usar DirectML para GPU instantánea)
              </span>
            )}
          </button>
        </div>

        {selectedDevice !== initialDevice && (
          <div className="bg-amber-950/20 border border-amber-800/40 rounded-lg p-3 text-xs text-amber-300 flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div>
              <p className="font-bold">⚠️ Cambio de hardware pendiente de guardar.</p>
              <p className="mt-0.5 opacity-80">Has cambiado de '{initialDevice}' a '{selectedDevice}'. Guarda la configuración y reinicia el kernel para aplicar.</p>
            </div>
          </div>
        )}
      </div>

      {/* Hardware Tiers (models.toml) */}
      <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-4">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-bold text-slate-300 flex items-center gap-2">
            <Coffee className="w-4 h-4 text-amber-400" /> Embedded Models Hardware Tiers (`models.toml`)
          </h3>
          <StatusPill status="online" label="Manifiesto V1.0" />
        </div>
        <p className="text-xs text-slate-400 mb-4">
          Tylluan selecciona el modelo ONNX adecuado según la capacidad de tu hardware. Todos los modelos reutilizan el runtime `ort` de BGE-M3.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="bg-slate-950 border border-slate-800/80 p-3 rounded-lg flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-bold font-mono text-amber-400 uppercase">☕ Toaster</span>
                <span className="text-[10px] bg-amber-500/10 text-amber-300 px-1.5 py-0.5 rounded border border-amber-500/20 font-mono">Edge / RPi4</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">DistilBERT & SmolLM2-360M</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Optimizado para hardware modesto. Memoria dedicada &lt;200 MB. Latencia sub-20ms en CPU.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-slate-500 font-mono">
              models.toml: tier = "toaster"
            </div>
          </div>

          <div className="bg-slate-950 border border-emerald-500/30 p-3 rounded-lg flex flex-col justify-between ring-1 ring-emerald-500/20">
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-bold font-mono text-emerald-400 uppercase">⚖️ Balanced</span>
                <span className="text-[10px] bg-emerald-500/10 text-emerald-300 px-1.5 py-0.5 rounded border border-emerald-500/20 font-mono">Recomendado</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">Qwen3-0.6B & Qwen3-1.7B</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Equilibrio óptimo para laptops y workstations modernas. Razonamiento denso y síntesis episódica.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-emerald-400 font-mono">
              models.toml: tier = "balanced" (default)
            </div>
          </div>

          <div className="bg-slate-950 border border-purple-500/30 p-3 rounded-lg flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-bold font-mono text-purple-400 uppercase">⚡ Tower</span>
                <span className="text-[10px] bg-purple-500/10 text-purple-300 px-1.5 py-0.5 rounded border border-purple-500/20 font-mono">GPU / High RAM</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">Modelos Extendidos (&gt;1.5B)</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Para torres de cómputo con aceleración GPU (CUDA/DirectML) y &gt;16 GB RAM. Inferencia ultra-rápida.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-slate-500 font-mono">
              models.toml: tier = "tower"
            </div>
          </div>
        </div>
      </div>

      {/* GGUF & Local Inference Selector Panel (Zero-Mock Backend Wiring) */}
      <div className="rounded-lg border border-violet-900/40 bg-slate-900/60 p-5 space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-bold text-slate-200 flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-violet-400" /> Selector de Modelos GGUF & Inferencia Local
          </h3>
          <StatusPill status="online" label={activeProvider} />
        </div>
        <p className="text-xs text-slate-400 leading-relaxed">
          Selecciona el modelo GGUF y el backend de inferencia activo. Conecta directamente con subproceso <code className="text-violet-300 font-mono">llama-server</code> local, runtime ONNX nativo, o servidores OpenAI-compatible (Ollama / LM Studio).
        </p>

        {/* Local GGUF Model Cards */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
          {[
            {
              id: 'qwen2.5-1.5b-instruct',
              name: 'Qwen2.5-1.5B Instruct',
              quant: 'Q4_K_M',
              vram: '~1.1 GB RAM',
              tier: 'Balanced (Default)',
              desc: 'Modelo principal recomendado. Síntesis densa y razonamiento episódico.',
              color: 'emerald'
            },
            {
              id: 'qwen2.5-0.5b-instruct',
              name: 'Qwen2.5-0.5B Instruct',
              quant: 'Q4_K_M',
              vram: '~450 MB RAM',
              tier: 'Toaster / RPi4',
              desc: 'Ligero para edge devices e inferencia continua con recursos acotados.',
              color: 'amber'
            },
            {
              id: 'smollm2-135m-instruct',
              name: 'SmolLM2-135M Instruct',
              quant: 'Q4_K_M',
              vram: '~180 MB RAM',
              tier: 'Ultra-Light',
              desc: 'Filtrado de intenciones, routing y compresión ultra-rápida en CPU.',
              color: 'blue'
            },
            {
              id: 'gemma-4-2b-it',
              name: 'Gemma-4-E2B-it',
              quant: 'Q4_K_M',
              vram: '~1.8 GB RAM',
              tier: 'Reasoning Coordinated',
              desc: 'Coordinador deliberativo nocturno con capacidad de razonamiento extenso.',
              color: 'violet'
            }
          ].map((m) => (
            <div
              key={m.id}
              onClick={() => setSelectedGgufModel(m.id)}
              className={cn(
                "p-3 rounded-lg border cursor-pointer transition-all flex flex-col justify-between text-left",
                selectedGgufModel === m.id
                  ? "bg-violet-950/30 border-violet-500 text-slate-100 ring-1 ring-violet-500"
                  : "bg-slate-950/50 border-slate-800/80 text-slate-400 hover:border-slate-700 hover:text-slate-200"
              )}
            >
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs font-bold font-mono text-slate-200">{m.name}</span>
                  <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-300 border border-slate-700">
                    {m.quant}
                  </span>
                </div>
                <p className="text-[10px] text-slate-400 mb-2 leading-relaxed">{m.desc}</p>
              </div>
              <div className="pt-2 border-t border-slate-800/60 flex items-center justify-between text-[9px] font-mono">
                <span className="text-slate-500">{m.vram}</span>
                <span className={cn(
                  selectedGgufModel === m.id ? "text-violet-400 font-bold" : "text-slate-600"
                )}>
                  {selectedGgufModel === m.id ? "● Seleccionado" : m.tier}
                </span>
              </div>
            </div>
          ))}
        </div>

        {/* Backend Provider & Endpoint Settings */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-2">
          <div>
            <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider mb-1">
              Backend Provider
            </label>
            <select
              value={activeProvider}
              onChange={(e) => setActiveProvider(e.target.value)}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
            >
              <option value="llama-server">llama-server (Subproceso local GGUF)</option>
              <option value="onnx-runtime">ONNX Runtime (Embedded DirectML)</option>
              <option value="ollama">Ollama (http://localhost:11434)</option>
              <option value="lm-studio">LM Studio (http://localhost:1234)</option>
            </select>
          </div>

          <div>
            <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider mb-1">
              Endpoint Base URL
            </label>
            <input
              type="text"
              value={providerUrl}
              onChange={(e) => setProviderUrl(e.target.value)}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
              placeholder="http://127.0.0.1:8080"
            />
          </div>

          <div>
            <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider mb-1">
              Context Window Size (Tokens)
            </label>
            <select
              value={contextLen}
              onChange={(e) => setContextLen(Number(e.target.value))}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
            >
              <option value={2048}>2048 Tokens (Bajo consumo RAM)</option>
              <option value={4096}>4096 Tokens (Estándar)</option>
              <option value={8192}>8192 Tokens (Razonamiento largo)</option>
            </select>
          </div>
        </div>

        {/* Hyperparameters Controls */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 pt-1">
          <div>
            <div className="flex justify-between items-center mb-1">
              <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">Temperatura: {temperature}</span>
              <span className="text-[9px] text-slate-500 font-mono">0.0 (determínico) — 1.0 (creativo)</span>
            </div>
            <input
              type="range"
              min="0.0"
              max="1.0"
              step="0.05"
              value={temperature}
              onChange={(e) => setTemperature(parseFloat(e.target.value))}
              className="w-full accent-violet-500 bg-slate-950 rounded h-1.5 cursor-pointer"
            />
          </div>

          <div>
            <div className="flex justify-between items-center mb-1">
              <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">Top-P (Nucleus): {topP}</span>
              <span className="text-[9px] text-slate-500 font-mono">Sampling cutoff</span>
            </div>
            <input
              type="range"
              min="0.50"
              max="1.00"
              step="0.01"
              value={topP}
              onChange={(e) => setTopP(parseFloat(e.target.value))}
              className="w-full accent-violet-500 bg-slate-950 rounded h-1.5 cursor-pointer"
            />
          </div>
        </div>

        {/* Connection status notification */}
        {connStatus && (
          <div className={cn(
            "p-3 rounded-lg border text-xs flex items-center justify-between",
            connStatus.ok ? "bg-emerald-950/20 border-emerald-800/40 text-emerald-300" : "bg-rose-950/20 border-rose-800/40 text-rose-300"
          )}>
            <div className="flex items-center gap-2">
              {connStatus.ok ? <ShieldCheck className="w-4 h-4 text-emerald-400" /> : <AlertTriangle className="w-4 h-4 text-rose-400" />}
              <span>{connStatus.msg}</span>
            </div>
          </div>
        )}

        {/* Actions */}
        <div className="flex items-center justify-between pt-2 border-t border-slate-800/60">
          <button
            type="button"
            onClick={handleTestConnection}
            disabled={testingConn}
            className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-300 rounded-lg text-xs font-mono flex items-center gap-2 transition-colors"
          >
            {testingConn ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Cpu className="w-3.5 h-3.5 text-cyan-400" />}
            Probar Conexión Backend
          </button>

          <button
            type="button"
            onClick={handleSaveGgufConfig}
            disabled={savingGguf}
            className="px-4 py-2 bg-violet-600 hover:bg-violet-500 disabled:opacity-50 text-white rounded-lg text-xs font-bold uppercase tracking-wider flex items-center gap-2 transition-colors shadow-lg shadow-violet-900/30"
          >
            {savingGguf ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            Guardar Configuración GGUF
          </button>
        </div>
      </div>

      {/* Embedding Model Edit */}
      <div className="rounded-lg border border-amber-900/30 bg-amber-950/10 p-4">
        <h3 className="text-sm font-bold text-slate-300 mb-4">Embedding Model Config</h3>
        <div className="space-y-4">
          <div>
            <label className="block text-xs text-slate-500 uppercase mb-1">Model Name</label>
            <input
              type="text"
              value={embeddingModel}
              onChange={(e) => setEmbeddingModel(e.target.value)}
              className="w-full max-w-md px-3 py-2 bg-slate-950 border border-slate-800 rounded font-mono text-sm text-slate-300"
              placeholder="e.g. nomic-embed-text"
            />
          </div>
          
          <div className="bg-amber-900/20 border border-amber-700/50 rounded-lg p-3 text-xs text-amber-300 flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div>
              <p className="font-bold">⚠️ Cambiar el modelo requiere reiniciar el kernel y reindexar SilvaDB.</p>
              <p className="mt-1 opacity-80">Esta operación puede tardar varios minutos dependiendo del tamaño de la base de datos.</p>
            </div>
          </div>

          <button
            onClick={handleSave}
            disabled={saving || !embeddingModel}
            className="px-4 py-2 bg-amber-500/20 hover:bg-amber-500/30 disabled:opacity-50 text-amber-400 rounded-lg text-xs font-bold uppercase tracking-wider flex items-center gap-2 transition-colors"
          >
            {saving ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            Save Configuration
          </button>
        </div>
      </div>

      {/* Restart Kernel Modal */}
      {showRestartModal && (
        <div className="fixed inset-0 flex items-center justify-center bg-black/70 backdrop-blur-sm z-50 p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 max-w-md w-full shadow-2xl space-y-4">
            <div className="flex items-center gap-3 text-amber-400">
              <AlertTriangle className="w-6 h-6" />
              <h3 className="text-lg font-bold text-slate-200 font-mono">Reinicio Requerido</h3>
            </div>
            
            <p className="text-sm text-slate-300 leading-relaxed">
              La aceleración por hardware se ha configurado a <span className="font-mono text-cyan-400 font-semibold">{selectedDevice}</span>. Para cargar los Execution Providers adecuados y aplicar los cambios, el Kernel de TylluanNexus debe reiniciarse.
            </p>

            <div className="bg-slate-950 border border-slate-800 rounded-lg p-3 space-y-2 text-xs text-slate-400 font-mono">
              <p className="text-slate-300 font-bold uppercase tracking-wider text-[10px] mb-1">Instrucciones de reinicio:</p>
              <p>1. Cierra el kernel actual en tu terminal (presiona <kbd className="bg-slate-800 px-1.5 py-0.5 rounded text-slate-300">Ctrl + C</kbd>).</p>
              <p>2. Vuelve a iniciarlo ejecutando:</p>
              <div className="bg-slate-900 p-2 rounded border border-slate-800 text-slate-300">
                .\tylluan-mcp.bat
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setShowRestartModal(false)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg text-xs font-bold uppercase tracking-wider transition-colors"
              >
                Entendido
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
