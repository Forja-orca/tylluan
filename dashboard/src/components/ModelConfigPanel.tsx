import React, { useState, useEffect } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Save, AlertTriangle, RefreshCw, Cpu, Database, Image as ImageIcon, Sparkles } from 'lucide-react';
import { cn } from '../lib/utils';

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

  const handleSave = async () => {
    if (!bridge) return;
    setSaving(true);
    try {
      // Device: targeted server-side TOML edit — never round-trip the whole
      // config through the browser (JSON.stringify once bricked tylluan.toml).
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
              "flex flex-col items-start p-3 rounded-lg border text-left transition-all",
              selectedDevice === 'directml'
                ? "bg-emerald-950/20 border-emerald-500/80 text-emerald-300 ring-1 ring-emerald-500"
                : "bg-slate-950/40 border-slate-900 text-slate-500 hover:border-slate-850 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-bold uppercase tracking-wider mb-1 flex items-center gap-1.5">
              <span className="text-emerald-400">⚡</span> DirectML (GPU)
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Recomendado para Windows. Acelera la inferencia en cualquier GPU (AMD, Intel, NVIDIA).
            </span>
            {selectedDevice === 'directml' && (
              <span className="text-[9px] text-emerald-400 font-mono mt-2 uppercase tracking-wide">
                ● Activo
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={() => setSelectedDevice('cuda')}
            className={cn(
              "flex flex-col items-start p-3 rounded-lg border text-left transition-all",
              selectedDevice === 'cuda'
                ? "bg-violet-950/20 border-violet-500/80 text-violet-300 ring-1 ring-violet-500"
                : "bg-slate-950/40 border-slate-900 text-slate-500 hover:border-slate-850 hover:text-slate-400"
            )}
          >
            <span className="text-xs font-bold uppercase tracking-wider mb-1 flex items-center gap-1.5">
              <span className="text-violet-400">🔥</span> NVIDIA CUDA
            </span>
            <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">
              Para GPUs NVIDIA con drivers CUDA y cuDNN instalados. Rendimiento óptimo de frontera.
            </span>
            {selectedDevice === 'cuda' && (
              <span className="text-[9px] text-violet-400 font-mono mt-2 uppercase tracking-wide">
                ● Activo
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
              La aceleración por hardware se ha configurado a <span className="font-mono text-cyan-400 font-semibold">{selectedDevice}</span>. Para cargar los Execution Providers adecuados y aplicar los cambios, el Kernel de Tylluan debe reiniciarse.
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
