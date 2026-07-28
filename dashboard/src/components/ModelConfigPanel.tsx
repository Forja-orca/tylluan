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
  const [providerUrl, setProviderUrl] = useState('http://127.0.0.1:9000');
  const [llamaPort, setLlamaPort] = useState(9000);
  const [contextLen, setContextLen] = useState(4096);
  const [gpuLayers, setGpuLayers] = useState(99);
  const [cpuThreads, setCpuThreads] = useState(4);
  const [batchSize, setBatchSize] = useState(512);
  const [temperature, setTemperature] = useState(0.7);
  const [topP, setTopP] = useState(0.95);
  const [topK, setTopK] = useState(40);
  const [repeatPenalty, setRepeatPenalty] = useState(1.10);
  const [testingConn, setTestingConn] = useState(false);
  const [connStatus, setConnStatus] = useState<{ ok: boolean; msg: string; latency?: number } | null>(null);
  const [savingGguf, setSavingGguf] = useState(false);

  // Dynamic system status & Role Assignment
  const [sysStatus, setSysStatus] = useState<any>(null);
  const [rolePrimary, setRolePrimary] = useState('qwen2.5-1.5b-instruct');
  const [roleCoordinator, setRoleCoordinator] = useState('gemma-4-2b-it');
  const [roleRouting, setRoleRouting] = useState('smollm2-135m-instruct');
  const [roleVision, setRoleVision] = useState('SmolVLM2-256M-Instruct');

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

        // Extract GGUF / inference settings from [inference.llama] with fallback to [inference]
        const llamaCfg = cfg?.inference?.llama || cfg?.inference || {};
        if (cfg?.inference?.primary_model) {
          setSelectedGgufModel(cfg.inference.primary_model);
          setRolePrimary(cfg.inference.primary_model);
        }
        if (llamaCfg.provider) {
          setActiveProvider(llamaCfg.provider);
        }
        if (llamaCfg.endpoint) {
          setProviderUrl(llamaCfg.endpoint);
        }
        if (llamaCfg.port) {
          setLlamaPort(llamaCfg.port);
        }
        if (llamaCfg.ctx_size || llamaCfg.context_size) {
          setContextLen(llamaCfg.ctx_size || llamaCfg.context_size);
        }
        if (llamaCfg.n_gpu_layers !== undefined) {
          setGpuLayers(llamaCfg.n_gpu_layers);
        }
        if (llamaCfg.threads !== undefined) {
          setCpuThreads(llamaCfg.threads);
        }
        if (llamaCfg.batch_size) {
          setBatchSize(llamaCfg.batch_size);
        }
        if (llamaCfg.temperature !== undefined) {
          setTemperature(llamaCfg.temperature);
        }
        if (llamaCfg.top_p !== undefined) {
          setTopP(llamaCfg.top_p);
        }
        if (llamaCfg.top_k !== undefined) {
          setTopK(llamaCfg.top_k);
        }
        if (llamaCfg.repeat_penalty !== undefined) {
          setRepeatPenalty(llamaCfg.repeat_penalty);
        }

        // Fetch real models and system status
        try {
          const [m, sys] = await Promise.all([
            bridge.fetchRaw('/api/v1/models'),
            bridge.fetchRaw('/api/v1/system/status')
          ]);
          setModels(m);
          setSysStatus(sys);
        } catch (err) {
          console.warn('Failed fetching models/system status:', err);
        }

        // Auto-probe llama-server/provider backend status on load
        try {
          const lCfg = cfg?.inference?.llama || cfg?.inference || {};
          const url = lCfg.endpoint || 'http://127.0.0.1:9000';
          const provider = lCfg.provider || 'llama-server';
          const res = await bridge.fetchRaw('/api/v1/test-connection', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ endpoint: url, provider })
          });
          if (res?.ok) {
            setConnStatus({ ok: true, msg: `Backend ${provider} online (${url})`, latency: res.latency_ms });
          } else {
            setConnStatus({ ok: false, msg: res?.error || `Backend ${provider} offline en ${url}` });
          }
        } catch {
          // No llama-server running — silent, user can test manually
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
      // Uses /api/v1/config/inference-llama — a safe TOML-patch endpoint that
      // reads tylluan.toml, patches [inference] and [inference.llama] fields,
      // validates the result, then atomic-writes. Never sends the full TOML
      // through the browser (that pattern bricked the config once).
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

      {/* Hardware Tiers (models.toml) - Dynamic Hardware Telemetry */}
      <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-4">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-bold text-slate-300 flex items-center gap-2">
            <Coffee className="w-4 h-4 text-amber-400" /> Embedded Models Hardware Tiers & Dynamic Specs
          </h3>
          <StatusPill
            status="online"
            label={
              sysStatus?.system?.total_memory_mb
                ? `${sysStatus.system.total_memory_mb} MB RAM (${sysStatus.system.cpu_usage ?? 0}% CPU)`
                : "Manifiesto V1.0"
            }
          />
        </div>
        <p className="text-xs text-slate-400 mb-4">
          Tylluan selecciona dinámicamente el tier de cómputo en base a la memoria total del sistema.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className={cn(
            "bg-slate-950 border p-3 rounded-lg flex flex-col justify-between transition-all",
            (sysStatus?.system?.total_memory_mb ?? 16000) < 4096 ? "border-amber-500 ring-1 ring-amber-500/30" : "border-slate-800/80 opacity-70"
          )}>
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-bold font-mono text-amber-400 uppercase">☕ Toaster</span>
                <span className="text-[10px] bg-amber-500/10 text-amber-300 px-1.5 py-0.5 rounded border border-amber-500/20 font-mono">Edge / RPi4</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">SmolLM2-135M & Qwen2.5-0.5B</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Optimizado para RAM &lt; 4 GB. Memoria dedicada &lt;500 MB. Inferencia ultra-ligera.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-slate-500 font-mono flex justify-between">
              <span>models.toml: tier = "toaster"</span>
              {(sysStatus?.system?.total_memory_mb ?? 16000) < 4096 && <span className="text-amber-400 font-bold">● ACTIVO</span>}
            </div>
          </div>

          <div className={cn(
            "bg-slate-950 border p-3 rounded-lg flex flex-col justify-between transition-all",
            (sysStatus?.system?.total_memory_mb ?? 16000) >= 4096 && (sysStatus?.system?.total_memory_mb ?? 16000) <= 16384 ? "border-emerald-500/80 ring-1 ring-emerald-500/30" : "border-slate-800/80 opacity-70"
          )}>
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-bold font-mono text-emerald-400 uppercase">⚖️ Balanced</span>
                <span className="text-[10px] bg-emerald-500/10 text-emerald-300 px-1.5 py-0.5 rounded border border-emerald-500/20 font-mono">Recomendado</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">Qwen2.5-1.5B & BGE-M3</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Equilibrio óptimo (4–16 GB RAM). Síntesis de memoria y razonamiento episódico denso.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-emerald-400 font-mono flex justify-between">
              <span>models.toml: tier = "balanced"</span>
              {(sysStatus?.system?.total_memory_mb ?? 16000) >= 4096 && (sysStatus?.system?.total_memory_mb ?? 16000) <= 16384 && <span className="text-emerald-400 font-bold">● ACTIVO</span>}
            </div>
          </div>

          <div className={cn(
            "bg-slate-950 border p-3 rounded-lg flex flex-col justify-between transition-all",
            (sysStatus?.system?.total_memory_mb ?? 16000) > 16384 ? "border-purple-500 ring-1 ring-purple-500/30" : "border-slate-800/80 opacity-70"
          )}>
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-bold font-mono text-purple-400 uppercase">⚡ Tower</span>
                <span className="text-[10px] bg-purple-500/10 text-purple-300 px-1.5 py-0.5 rounded border border-purple-500/20 font-mono">GPU / High RAM</span>
              </div>
              <p className="text-[11px] text-slate-300 font-semibold mb-1">Gemma-4-E2B & Extensiones</p>
              <p className="text-[10px] text-slate-400 leading-relaxed">
                Para torres con aceleración GPU (DirectML/CUDA) y &gt;16 GB RAM. Inferencia paralela nocturna.
              </p>
            </div>
            <div className="mt-3 pt-2 border-t border-slate-800/60 text-[9px] text-slate-500 font-mono flex justify-between">
              <span>models.toml: tier = "tower"</span>
              {(sysStatus?.system?.total_memory_mb ?? 16000) > 16384 && <span className="text-purple-400 font-bold">● ACTIVO</span>}
            </div>
          </div>
        </div>
      </div>

      {/* Real Local Detected Model Inventory on Disk (`models/`) */}
      <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-bold text-slate-300 flex items-center gap-2">
            <Database className="w-4 h-4 text-emerald-400" /> Inventario Real de Modelos en Disco (`/api/v1/models`)
          </h3>
          <span className="text-xs font-mono text-slate-400">
            {models?.detected_local_models?.length ?? 0} modelos detectados
          </span>
        </div>
        <p className="text-xs text-slate-400 mb-4">
          Archivos reales escaneados por el Kernel en la carpeta local <code className="text-slate-300 font-mono">models/</code>.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {models?.detected_local_models?.length > 0 ? (
            models.detected_local_models.map((m: any) => (
              <div key={m.id} className="p-3 bg-slate-950 border border-slate-800 rounded-lg flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-bold font-mono text-slate-200">{m.name}</span>
                    <span className="text-[9px] bg-emerald-500/10 text-emerald-400 px-1.5 py-0.5 rounded border border-emerald-500/20 font-mono uppercase">
                      INSTALADO EN DISCO
                    </span>
                  </div>
                  <p className="text-[10px] text-slate-500 font-mono mt-1">{m.path}</p>
                </div>
                <div className="text-right">
                  <span className="text-xs font-mono text-emerald-400 font-bold">{m.size_mb || Math.round((m.size_bytes || 0) / 1048576)} MB</span>
                </div>
              </div>
            ))
          ) : (
            <div className="col-span-2 p-6 bg-slate-950 border border-dashed border-slate-700 rounded-lg flex flex-col items-center justify-center gap-2 text-center">
              <Database className="w-8 h-8 text-slate-600" />
              <p className="text-sm font-mono text-slate-400 font-semibold">Sin modelos detectados en disco</p>
              <p className="text-xs text-slate-500 max-w-xs leading-relaxed">
                El kernel no encontró archivos en <code className="text-slate-400">models/</code>. Descarga un modelo GGUF y colócalo en esa carpeta, luego reinicia el kernel.
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Model Assignment Per Role */}
      <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-4 space-y-4">
        <div>
          <h3 className="text-sm font-bold text-slate-300 flex items-center gap-2">
            <ShieldCheck className="w-4 h-4 text-cyan-400" /> Asignación de Modelos por Rol Cognitivo
          </h3>
          <p className="text-xs text-slate-400 mt-1">
            Cada módulo dentro de Tylluan requiere diferentes características de tamaño, velocidad y capacidad de razonamiento.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {[
            {
              id: 'primary',
              title: '1. Inferencia Principal (Primary Agent)',
              desc: 'Responde a consultas de usuario y ejecuta herramientas (tylluan_do). Requiere buen seguimiento de instrucciones.',
              rec: 'Recomendado: 1.5B – 3B (ej. Qwen2.5-1.5B)',
              value: rolePrimary,
              setter: setRolePrimary,
              color: 'border-emerald-500/30'
            },
            {
              id: 'coordinator',
              title: '2. Coordinador Nocturno (Night Reasoner)',
              desc: 'Consolidación de memoria episódica en background. Prioriza razonamiento profundo sobre velocidad.',
              rec: 'Recomendado: 2B – 4B (ej. Gemma-2B / Qwen2.5-3B)',
              value: roleCoordinator,
              setter: setRoleCoordinator,
              color: 'border-violet-500/30'
            },
            {
              id: 'routing',
              title: '3. Enrutamiento e Intenciones (Routing & Intent)',
              desc: 'Clasificación ultra-rápida de intenciones y filtrado. Requiere latencia <50ms en CPU.',
              rec: 'Recomendado: <500M (ej. SmolLM2-135M / Qwen-0.5B)',
              value: roleRouting,
              setter: setRoleRouting,
              color: 'border-blue-500/30'
            },
            {
              id: 'vision',
              title: '4. Análisis Visual (Vision Model)',
              desc: 'Extracción de texto (OCR) y descripción de imágenes para la guild de visión.',
              rec: 'Recomendado: SmolVLM2-256M / Moondream',
              value: roleVision,
              setter: setRoleVision,
              color: 'border-cyan-500/30'
            },
          ].map((role) => (
            <div key={role.id} className={cn("bg-slate-950 p-3.5 rounded-lg border flex flex-col justify-between space-y-2", role.color)}>
              <div>
                <label className="block text-xs font-bold text-slate-200 tracking-wide mb-1">
                  {role.title}
                </label>
                <p className="text-[10px] text-slate-400 leading-relaxed mb-2">
                  {role.desc}
                </p>
              </div>

              <div>
                {models?.detected_local_models?.length > 0 ? (
                  <select
                    value={role.value}
                    onChange={(e) => role.setter(e.target.value)}
                    className="w-full px-3 py-1.5 bg-slate-900 border border-slate-800 rounded text-xs font-mono text-slate-200 focus:border-cyan-500 focus:outline-none"
                  >
                    {models.detected_local_models.map((m: any) => (
                      <option key={m.id || m.name} value={m.id || m.name}>
                        {m.name} {m.size_mb ? `(${m.size_mb} MB)` : ''}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    type="text"
                    value={role.value}
                    onChange={(e) => role.setter(e.target.value)}
                    className="w-full px-3 py-1.5 bg-slate-900 border border-slate-700 rounded text-xs font-mono text-slate-400"
                    placeholder="Sin modelos en disco — introduce nombre"
                  />
                )}
                <span className="text-[9px] text-slate-500 font-mono block mt-1.5">
                  💡 {role.rec}
                </span>
              </div>
            </div>
          ))}
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

        {/* Real Local GGUF / ONNX Model Cards from /api/v1/models */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
          {models?.detected_local_models && models.detected_local_models.length > 0 ? (
            models.detected_local_models.map((m: any) => (
              <div
                key={m.id || m.name}
                onClick={() => setSelectedGgufModel(m.id || m.name)}
                className={cn(
                  "p-3 rounded-lg border cursor-pointer transition-all flex flex-col justify-between text-left",
                  selectedGgufModel === (m.id || m.name)
                    ? "bg-violet-950/30 border-violet-500 text-slate-100 ring-1 ring-violet-500"
                    : "bg-slate-950/50 border-slate-800/80 text-slate-400 hover:border-slate-700 hover:text-slate-200"
                )}
              >
                <div>
                  <div className="flex items-center justify-between mb-1.5">
                    <span className="text-xs font-bold font-mono text-slate-200 truncate">{m.name}</span>
                    <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 uppercase">
                      INSTALADO
                    </span>
                  </div>
                  <p className="text-[10px] text-slate-500 font-mono mb-2 truncate" title={m.path}>
                    {m.path}
                  </p>
                </div>
                <div className="pt-2 border-t border-slate-800/60 flex items-center justify-between text-[9px] font-mono">
                  <span className="text-slate-400 font-bold">{m.size_mb || Math.round((m.size_bytes || 0) / 1048576)} MB</span>
                  <span className={cn(
                    selectedGgufModel === (m.id || m.name) ? "text-violet-400 font-bold" : "text-slate-600"
                  )}>
                    {selectedGgufModel === (m.id || m.name) ? "● Seleccionado" : "Disponible"}
                  </span>
                </div>
              </div>
            ))
          ) : (
            <div className="col-span-full p-4 bg-slate-950/40 border border-dashed border-slate-800 rounded-lg text-center">
              <p className="text-xs font-mono text-slate-400">Sin modelos detectados en el directorio local <code className="text-violet-400">models/</code></p>
              <p className="text-[10px] text-slate-500 mt-1">Coloca archivos .gguf o carpetas de modelo en <code className="text-slate-400">models/</code> para que aparezcan aquí automáticamente.</p>
            </div>
          )}
        </div>

        {/* Backend Provider & Endpoint Settings */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 pt-2">
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
              Puerto HTTP Local (`[inference] port`)
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
            <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider mb-1">
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

        {/* llama-server Execution Settings (GPU layers, Threads, Batch) */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-1">
          <div>
            <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider mb-1">
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
            <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider mb-1">
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
            <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider mb-1">
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
              <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">Temperatura: {temperature}</span>
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
              <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">Top-P: {topP}</span>
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
              <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">Top-K: {topK}</span>
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
              <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">Repeat Penalty: {repeatPenalty}</span>
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
