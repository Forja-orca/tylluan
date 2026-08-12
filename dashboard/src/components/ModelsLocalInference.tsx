import React, { useState } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Save, AlertTriangle, RefreshCw, Cpu, Sparkles, ShieldCheck } from 'lucide-react';
import { cn } from '../lib/utils';
import { StatusPill } from './ui/StatusPill';

interface Props {
  bridge: NexusBridge | null;
  config: any;
  models: any;
  selectedDevice: string;
  initialDevice: string;
  onDeviceChange: (device: string) => void;
  onRestartRequired: () => void;
}

export function ModelsLocalInference({
  bridge,
  config,
  models,
  selectedDevice,
  initialDevice,
  onDeviceChange,
  onRestartRequired,
}: Props) {
  // GGUF & Inference Selector State
  const [selectedGgufModel, setSelectedGgufModel] = useState(
    (config?.inference?.primary_model || config?.inference?.llama?.primary_model || 'qwen2.5-1.5b-instruct') as string
  );
  const [activeProvider, setActiveProvider] = useState(
    (config?.inference?.llama?.provider || config?.inference?.provider || 'llama-server') as string
  );
  const [providerUrl, setProviderUrl] = useState(
    (config?.inference?.llama?.endpoint || config?.inference?.endpoint || 'http://127.0.0.1:9000') as string
  );
  const [llamaPort, setLlamaPort] = useState(
    (config?.inference?.llama?.port || config?.inference?.port || 9000) as number
  );
  const [contextLen, setContextLen] = useState(
    (config?.inference?.llama?.ctx_size || config?.inference?.llama?.context_size || config?.inference?.ctx_size || 4096) as number
  );
  const [gpuLayers, setGpuLayers] = useState(
    (config?.inference?.llama?.n_gpu_layers ?? config?.inference?.n_gpu_layers ?? 99) as number
  );
  const [cpuThreads, setCpuThreads] = useState(
    (config?.inference?.llama?.threads ?? config?.inference?.threads ?? 4) as number
  );
  const [batchSize, setBatchSize] = useState(
    (config?.inference?.llama?.batch_size || config?.inference?.batch_size || 512) as number
  );
  const [temperature, setTemperature] = useState(
    (config?.inference?.llama?.temperature ?? config?.inference?.temperature ?? 0.7) as number
  );
  const [topP, setTopP] = useState(
    (config?.inference?.llama?.top_p ?? config?.inference?.top_p ?? 0.95) as number
  );
  const [topK, setTopK] = useState(
    (config?.inference?.llama?.top_k ?? config?.inference?.top_k ?? 40) as number
  );
  const [repeatPenalty, setRepeatPenalty] = useState(
    (config?.inference?.llama?.repeat_penalty ?? config?.inference?.repeat_penalty ?? 1.10) as number
  );
  const [testingConn, setTestingConn] = useState(false);
  const [connStatus, setConnStatus] = useState<{ ok: boolean; msg: string; latency?: number } | null>(null);
  const [savingGguf, setSavingGguf] = useState(false);

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
      if (res && res.ok) {
        setConnStatus({
          ok: true,
          msg: `Servidor ${res.provider || activeProvider} respondiendo (${res.endpoint}). Status: HTTP ${res.http_status || 200}`,
          latency: res.latency_ms
        });
      } else {
        setConnStatus({
          ok: false,
          msg: res?.error || `Servidor offline en ${providerUrl}. Inicia llama-server u Ollama para activar.`,
          latency: res?.latency_ms
        });
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
      const res = await bridge.fetch('/api/v1/config/inference-llama', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          primary_model: selectedGgufModel,
          provider: activeProvider,
          endpoint: providerUrl,
          port: llamaPort,
          ctx_size: contextLen,
          n_gpu_layers: gpuLayers,
          threads: cpuThreads,
          batch_size: batchSize,
          temperature: temperature,
          top_p: topP,
          top_k: topK,
          repeat_penalty: repeatPenalty,
        })
      });
      if (res?.error) throw new Error(res.error);
      alert('Configuración [inference.llama] guardada exitosamente en tylluan.toml.');
    } catch (e: any) {
      alert(`Error guardando configuración GGUF: ${e.message || String(e)}`);
    }
    setSavingGguf(false);
  };

  const handleSaveDevice = async () => {
    if (!bridge) return;
    try {
      await bridge.fetch('/api/v1/config/device', {
        method: 'POST',
        body: JSON.stringify({ device: selectedDevice })
      });
      onRestartRequired();
    } catch (e) {
      alert(`Error guardando: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return (
    <div className="space-y-6">
      {/* Hardware Acceleration (GPU/CPU) */}
      <div className="rounded-lg bg-slate-900/50 p-4">
        <h3 className="text-sm font-semibold text-slate-300 mb-4 flex items-center gap-2">
          <Cpu className="w-4 h-4 text-amber-400" /> Aceleración de Inferencia (GPU/CPU)
        </h3>
        <p className="text-xs text-slate-400 mb-4">
          Selecciona el dispositivo de ejecución para ONNX Runtime (embeddings y vision). Las GPUs reducen los tiempos de generación e indexación drásticamente.
        </p>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
          <button
            type="button"
            onClick={() => onDeviceChange('cpu')}
            className={cn(
              "flex flex-col items-start p-3 rounded-lg text-left transition-all",
              selectedDevice === 'cpu'
                ? "bg-slate-800/80 text-slate-200 ring-1 ring-slate-500"
                : "bg-slate-950/40 text-slate-500 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-semibold mb-1 flex items-center gap-1.5">
              <Cpu className="w-3.5 h-3.5 text-slate-400" /> CPU Standalone
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Usa el procesador del sistema. Estable, pero lento (2-8s por embedding).
            </span>
            {selectedDevice === 'cpu' && (
              <span className="text-[9px] text-emerald-400 font-mono mt-2">
                ● Activo
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => onDeviceChange('directml')}
            className={cn(
              "flex flex-col items-start p-3 rounded-lg text-left transition-all relative",
              selectedDevice === 'directml'
                ? "bg-emerald-950/20 text-emerald-300 ring-1 ring-emerald-500"
                : "bg-slate-950/40 text-slate-500 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-semibold mb-1 flex items-center gap-1.5">
              <span className="text-emerald-400">⚡</span> DirectML (GPU)
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Recomendado para Windows. Acelera en cualquier GPU (NVIDIA RTX 3060, AMD, Intel) sin recompilar.
            </span>
            {selectedDevice === 'directml' && (
              <span className="text-[9px] text-emerald-400 font-mono mt-2 flex items-center gap-1">
                ● Seleccionado (DirectML GPU)
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => onDeviceChange('cuda')}
            className={cn(
              "flex flex-col items-start p-3 rounded-lg text-left transition-all relative",
              selectedDevice === 'cuda'
                ? "bg-amber-950/20 text-amber-300 ring-1 ring-amber-500"
                : "bg-slate-950/40 text-slate-500 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-semibold mb-1 flex items-center gap-1.5">
              <span className="text-amber-400">🔥</span> NVIDIA CUDA
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Requiere binario compilado con `--features cuda`. Si no está activo, caerá a CPU/DirectML.
            </span>
            {selectedDevice === 'cuda' && (
              <span className="text-[9px] text-amber-400 font-mono mt-2 flex items-center gap-1">
                ⚠️ Requiere feature `cuda` compilada (Usar DirectML para GPU instantánea)
              </span>
            )}
          </button>
        </div>

        {selectedDevice !== initialDevice && (
          <div className="bg-amber-950/20 rounded-lg p-3 text-xs text-amber-300 flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div>
              <p className="font-semibold">⚠️ Cambio de hardware pendiente de guardar.</p>
              <p className="mt-0.5 opacity-80">Has cambiado de '{initialDevice}' a '{selectedDevice}'. Guarda la configuración y reinicia el kernel para aplicar.</p>
            </div>
          </div>
        )}

        {selectedDevice !== initialDevice && (
          <div className="mt-3 flex justify-end">
            <button
              type="button"
              onClick={handleSaveDevice}
              className="px-4 py-2 bg-amber-500/20 hover:bg-amber-500/30 text-amber-400 rounded-lg text-xs font-semibold flex items-center gap-2 transition-colors"
            >
              <Save className="w-4 h-4" />
              Guardar Dispositivo
            </button>
          </div>
        )}
      </div>

      {/* Hardware Tiers (models.toml) */}
      <div className="rounded-lg bg-slate-900/50 p-4">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <Cpu className="w-4 h-4 text-amber-400" /> Embedded Models Hardware Tiers & Dynamic Specs
          </h3>
          <StatusPill
            status="online"
            label={
              config?.system?.total_memory_mb
                ? `${config.system.total_memory_mb} MB RAM`
                : "Manifiesto V1.0"
            }
          />
        </div>
        <p className="text-xs text-slate-400 mb-4">
          Tylluan selecciona dinámicamente el tier de cómputo en base a la memoria total del sistema.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className={cn(
            "bg-slate-950 p-3 rounded-lg flex flex-col justify-between transition-all",
            (config?.system?.total_memory_mb ?? 16000) < 4096 ? "ring-1 ring-amber-500/30" : "opacity-70"
          )}>
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-semibold font-mono text-amber-400">☕ Toaster</span>
                <span className="text-[10px] bg-amber-500/10 text-amber-300 px-1.5 py-0.5 rounded font-mono">Edge / RPi4</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">Generativo: SmolLM2-135M / Qwen2.5-0.5B</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Optimizado para RAM &lt; 4 GB. Memoria dedicada &lt;500 MB. Inferencia ultra-ligera.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-slate-500 font-mono flex justify-between">
              <span>tier = "toaster"</span>
              {(config?.system?.total_memory_mb ?? 16000) < 4096 && <span className="text-amber-400 font-semibold">● ACTIVO</span>}
            </div>
          </div>

          <div className={cn(
            "bg-slate-950 p-3 rounded-lg flex flex-col justify-between transition-all",
            (config?.system?.total_memory_mb ?? 16000) >= 4096 && (config?.system?.total_memory_mb ?? 16000) <= 16384 ? "ring-1 ring-emerald-500/30" : "opacity-70"
          )}>
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-semibold font-mono text-emerald-400">⚖️ Balanced</span>
                <span className="text-[10px] bg-emerald-500/10 text-emerald-300 px-1.5 py-0.5 rounded font-mono">Recomendado</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">Generativo: Qwen2.5-1.5B <span className="text-slate-500">(embedding: BGE-M3, ver "Embeddings & Disk")</span></p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Equilibrio óptimo (4–16 GB RAM). Síntesis de memoria y razonamiento episódico denso.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-emerald-400 font-mono flex justify-between">
              <span>tier = "balanced"</span>
              {(config?.system?.total_memory_mb ?? 16000) >= 4096 && (config?.system?.total_memory_mb ?? 16000) <= 16384 && <span className="text-emerald-400 font-semibold">● ACTIVO</span>}
            </div>
          </div>

          <div className={cn(
            "bg-slate-950 p-3 rounded-lg flex flex-col justify-between transition-all",
            (config?.system?.total_memory_mb ?? 16000) > 16384 ? "ring-1 ring-violet-500/30" : "opacity-70"
          )}>
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-semibold font-mono text-violet-400">⚡ Tower</span>
                <span className="text-[10px] bg-violet-500/10 text-violet-300 px-1.5 py-0.5 rounded font-mono">GPU / High RAM</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">Generativo: Gemma-4-E2B & Extensiones</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Para torres con aceleración GPU (DirectML/CUDA) y &gt;16 GB RAM. Inferencia paralela nocturna.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-slate-500 font-mono flex justify-between">
              <span>tier = "tower"</span>
              {(config?.system?.total_memory_mb ?? 16000) > 16384 && <span className="text-violet-400 font-semibold">● ACTIVO</span>}
            </div>
          </div>
        </div>
      </div>

      {/* GGUF & Local Inference Selector Panel */}
      <div className="rounded-lg bg-slate-900/60 p-5 space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-slate-200 flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-violet-400" /> Selector de Modelos GGUF & Inferencia Local
          </h3>
          <StatusPill status="online" label={activeProvider} />
        </div>
        <p className="text-xs text-slate-400 leading-relaxed">
          Selecciona el modelo GGUF y el backend de inferencia activo. Conecta directamente con subproceso <code className="text-violet-300 font-mono">llama-server</code> local, runtime ONNX nativo, o servidores OpenAI-compatible (Ollama / LM Studio).
        </p>

        {/* Real Local GGUF / ONNX Model Cards -- generative models only.
            Embedding and vision models are detected in the same models/
            directory scan but belong in Embeddings & Disk, not here. */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
          {(() => {
            const generativeModels = (models?.detected_local_models || []).filter(
              (m: any) => (m.model_type || 'generative') === 'generative'
            );
            return generativeModels.length > 0 ? (
              generativeModels.map((m: any) => (
              <div
                key={m.id || m.name}
                onClick={() => setSelectedGgufModel(m.id || m.name)}
                className={cn(
                  "p-3 rounded-lg cursor-pointer transition-all flex flex-col justify-between text-left",
                  selectedGgufModel === (m.id || m.name)
                    ? "bg-violet-950/30 text-slate-100 ring-1 ring-violet-500"
                    : "bg-slate-950/50 text-slate-400 hover:text-slate-200"
                )}
              >
                <div>
                  <div className="flex items-center justify-between mb-1.5">
                    <span className="text-xs font-semibold font-mono text-slate-200 truncate">{m.name}</span>
                    <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 uppercase">
                      INSTALADO
                    </span>
                  </div>
                  <p className="text-[10px] text-slate-500 font-mono mb-2 truncate" title={m.path}>
                    {m.path}
                  </p>
                </div>
                <div className="pt-2 border-t border-slate-800/60 flex items-center justify-between text-[9px] font-mono">
                  <span className="text-slate-400 font-semibold">{m.size_mb || Math.round((m.size_bytes || 0) / 1048576)} MB</span>
                  <span className={cn(
                    selectedGgufModel === (m.id || m.name) ? "text-violet-400 font-semibold" : "text-slate-600"
                  )}>
                    {selectedGgufModel === (m.id || m.name) ? "● Seleccionado" : "Disponible"}
                  </span>
                </div>
              </div>
              ))
            ) : (
              <div className="col-span-full p-4 bg-slate-950/40 border border-dashed border-slate-800 rounded-lg text-center">
                <p className="text-xs font-mono text-slate-400">Sin modelos generativos detectados en el directorio local <code className="text-violet-400">models/</code></p>
                <p className="text-[10px] text-slate-500 mt-1">Coloca archivos .gguf o carpetas de modelo en <code className="text-slate-400">models/</code> para que aparezcan aquí automáticamente. Los modelos de embedding y visión se gestionan en la pestaña "Embeddings & Disk".</p>
              </div>
            );
          })()}
        </div>

        {/* Backend Provider & Endpoint Settings */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 pt-2">
          <div>
            <label className="block text-[10px] font-semibold text-slate-400 mb-1">
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
            <label className="block text-[10px] font-semibold text-slate-400 mb-1">
              Puerto HTTP Local
            </label>
            <input
              type="number"
              value={llamaPort}
              onChange={(e) => {
                const p = Number(e.target.value);
                setLlamaPort(p);
                setProviderUrl(`http://127.0.0.1:${p}`);
              }}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
              placeholder="9000"
            />
          </div>

          <div>
            <label className="block text-[10px] font-semibold text-slate-400 mb-1">
              Endpoint Base URL
            </label>
            <input
              type="text"
              value={providerUrl}
              onChange={(e) => setProviderUrl(e.target.value)}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
              placeholder="http://127.0.0.1:9000"
            />
          </div>

          <div>
            <label className="block text-[10px] font-semibold text-slate-400 mb-1">
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

        {/* llama-server Execution Settings */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-1">
          <div>
            <label className="block text-[10px] font-semibold text-slate-400 mb-1">
              Capas en GPU (`--n-gpu-layers`)
            </label>
            <input
              type="number"
              value={gpuLayers}
              onChange={(e) => setGpuLayers(Number(e.target.value))}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
              placeholder="99 (99 = Offload GPU total, 0 = CPU solo)"
            />
          </div>

          <div>
            <label className="block text-[10px] font-semibold text-slate-400 mb-1">
              Hilos CPU (`--threads`)
            </label>
            <input
              type="number"
              value={cpuThreads}
              onChange={(e) => setCpuThreads(Number(e.target.value))}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
              placeholder="4"
            />
          </div>

          <div>
            <label className="block text-[10px] font-semibold text-slate-400 mb-1">
              Batch Size (`--batch-size`)
            </label>
            <select
              value={batchSize}
              onChange={(e) => setBatchSize(Number(e.target.value))}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 focus:border-violet-500 focus:outline-none"
            >
              <option value={256}>256 Tokens</option>
              <option value={512}>512 Tokens (Estándar)</option>
              <option value={1024}>1024 Tokens</option>
            </select>
          </div>
        </div>

        {/* Hyperparameters Controls */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 pt-1">
          <div>
            <div className="flex justify-between items-center mb-1">
              <span className="text-[10px] font-semibold text-slate-400">Temperatura: {temperature}</span>
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
              <span className="text-[10px] font-semibold text-slate-400">Top-P: {topP}</span>
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

          <div>
            <div className="flex justify-between items-center mb-1">
              <span className="text-[10px] font-semibold text-slate-400">Top-K: {topK}</span>
            </div>
            <input
              type="range"
              min="1"
              max="100"
              step="1"
              value={topK}
              onChange={(e) => setTopK(parseInt(e.target.value))}
              className="w-full accent-violet-500 bg-slate-950 rounded h-1.5 cursor-pointer"
            />
          </div>

          <div>
            <div className="flex justify-between items-center mb-1">
              <span className="text-[10px] font-semibold text-slate-400">Repeat Penalty: {repeatPenalty}</span>
            </div>
            <input
              type="range"
              min="1.00"
              max="1.50"
              step="0.02"
              value={repeatPenalty}
              onChange={(e) => setRepeatPenalty(parseFloat(e.target.value))}
              className="w-full accent-violet-500 bg-slate-950 rounded h-1.5 cursor-pointer"
            />
          </div>
        </div>

        {/* Connection status notification */}
        {connStatus && (
          <div className={cn(
            "p-3 rounded-lg text-xs flex items-center justify-between",
            connStatus.ok ? "bg-emerald-950/20 text-emerald-300" : "bg-rose-950/20 text-rose-300"
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
            {testingConn ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Cpu className="w-3.5 h-3.5 text-amber-400" />}
            Probar Conexión Backend
          </button>

          <button
            type="button"
            onClick={handleSaveGgufConfig}
            disabled={savingGguf}
            className="px-4 py-2 bg-violet-600 hover:bg-violet-500 disabled:opacity-50 text-slate-50 rounded-lg text-xs font-semibold flex items-center gap-2 transition-colors shadow-lg shadow-violet-900/30"
          >
            {savingGguf ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            Guardar Configuración GGUF
          </button>
        </div>
      </div>
    </div>
  );
}
