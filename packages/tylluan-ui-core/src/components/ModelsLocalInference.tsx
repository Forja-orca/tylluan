import { useState } from 'react';
import { Save, AlertTriangle, RefreshCw, Cpu, Sparkles, ShieldCheck, ChevronDown, Zap, Gauge, Target, Settings } from 'lucide-react';
import { cn } from '../lib/utils';
import { StatusPill } from './ui/StatusPill';
import type { NexusBridge } from '../lib/api-client';

/** Inference presets — each maps to concrete llama-server values. */
const PROFILES = [
  {
    id: 'fast',
    name: 'Rápido',
    icon: Zap,
    color: 'emerald',
    description: 'Respuestas rápidas y deterministas. Ideal para tareas simples y consultas rápidas.',
    values: { temperature: 0.3, topP: 0.85, topK: 20, repeatPenalty: 1.05, batchSize: 256 },
  },
  {
    id: 'balanced',
    name: 'Equilibrado',
    icon: Gauge,
    color: 'violet',
    description: 'Equilibrio entre velocidad y calidad. Bueno para la mayoría de tareas.',
    values: { temperature: 0.7, topP: 0.95, topK: 40, repeatPenalty: 1.10, batchSize: 512 },
  },
  {
    id: 'precise',
    name: 'Preciso',
    icon: Target,
    color: 'amber',
    description: 'Máxima precisión, menor alucinación. Ideal para código y análisis crítico.',
    values: { temperature: 0.1, topP: 0.80, topK: 10, repeatPenalty: 1.20, batchSize: 1024 },
  },
] as const;

function matchProfile(temperature: number, topP: number, topK: number, repeatPenalty: number): string | null {
  for (const p of PROFILES) {
    if (
      Math.abs(p.values.temperature - temperature) < 0.01 &&
      Math.abs(p.values.topP - topP) < 0.01 &&
      Math.abs(p.values.topK - topK) < 1 &&
      Math.abs(p.values.repeatPenalty - repeatPenalty) < 0.01
    ) return p.id;
  }
  return null;
}

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
  bridge, config, models: _models, selectedDevice, initialDevice, onDeviceChange, onRestartRequired,
}: Props) {
  const [selectedGgufModel, _setSelectedGgufModel] = useState(
    (config?.inference?.primary_model || config?.inference?.llama?.primary_model || 'qwen2.5-1.5b-instruct') as string
  );
  const [activeProvider, _setActiveProvider] = useState(
    (config?.inference?.llama?.provider || config?.inference?.provider || 'llama-server') as string
  );
  const [providerUrl, _setProviderUrl] = useState(
    (config?.inference?.llama?.endpoint || config?.inference?.endpoint || 'http://127.0.0.1:9000') as string
  );
  const [llamaPort, _setLlamaPort] = useState(
    (config?.inference?.llama?.port || config?.inference?.port || 9000) as number
  );
  const [contextLen, _setContextLen] = useState(
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
  const [showAdvanced, setShowAdvanced] = useState(false);

  const activeProfileId = matchProfile(temperature, topP, topK, repeatPenalty);

  const applyProfile = (profileId: string) => {
    const profile = PROFILES.find(p => p.id === profileId);
    if (!profile) return;
    setTemperature(profile.values.temperature);
    setTopP(profile.values.topP);
    setTopK(profile.values.topK);
    setRepeatPenalty(profile.values.repeatPenalty);
    setBatchSize(profile.values.batchSize);
  };

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
        setConnStatus({ ok: true, msg: `Servidor ${res.provider || activeProvider} respondiendo (${res.endpoint}). Status: HTTP ${res.http_status || 200}`, latency: res.latency_ms });
      } else {
        setConnStatus({ ok: false, msg: res?.error || `Servidor offline en ${providerUrl}.`, latency: res?.latency_ms });
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
          primary_model: selectedGgufModel, provider: activeProvider, endpoint: providerUrl,
          port: llamaPort, ctx_size: contextLen, n_gpu_layers: gpuLayers, threads: cpuThreads,
          batch_size: batchSize, temperature, top_p: topP, top_k: topK, repeat_penalty: repeatPenalty,
        })
      });
      if (res?.error) throw new Error(res.error);
      alert('Configuración guardada exitosamente.');
    } catch (e: any) {
      alert(`Error guardando configuración: ${e.message || String(e)}`);
    }
    setSavingGguf(false);
  };

  const handleSaveDevice = async () => {
    if (!bridge) return;
    try {
      await bridge.fetch('/api/v1/config/device', { method: 'POST', body: JSON.stringify({ device: selectedDevice }) });
      onRestartRequired();
    } catch (e) {
      alert(`Error guardando: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return (
    <div className="space-y-6">
      {/* Hardware Acceleration */}
      <div className="rounded-lg bg-slate-900/50 p-4">
        <h3 className="text-sm font-semibold text-slate-300 mb-4 flex items-center gap-2">
          <Cpu className="w-4 h-4 text-amber-400" /> Aceleración de Inferencia (GPU/CPU)
        </h3>
        <p className="text-xs text-slate-400 mb-4">
          Selecciona el dispositivo de ejecución para ONNX Runtime (embeddings y vision).
        </p>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
          {[
            { device: 'cpu', label: 'CPU Standalone', icon: <Cpu className="w-3.5 h-3.5 text-slate-400" />, desc: 'Procesador del sistema. Estable, pero lento.' },
            { device: 'directml', label: 'DirectML (GPU)', icon: <span className="text-emerald-400">⚡</span>, desc: 'Recomendado para Windows. Acelera en cualquier GPU.' },
            { device: 'cuda', label: 'NVIDIA CUDA', icon: <span className="text-amber-400">🔥</span>, desc: 'Requiere feature `cuda` compilada.' },
          ].map(({ device, label, icon, desc }) => (
            <button key={device} type="button" onClick={() => onDeviceChange(device)}
              className={cn("flex flex-col items-start p-3 rounded-lg text-left transition-all",
                selectedDevice === device ? "bg-slate-800/80 text-slate-200 ring-1 ring-slate-500" : "bg-slate-950/40 text-slate-500 hover:text-slate-400"
              )}>
              <span className="text-xs font-semibold mb-1 flex items-center gap-1.5">{icon} {label}</span>
              <span className="text-[10px] opacity-80 leading-relaxed text-slate-400">{desc}</span>
              {selectedDevice === device && <span className="mt-2"><StatusPill status="online" label="Activo" /></span>}
            </button>
          ))}
        </div>
        {selectedDevice !== initialDevice && (
          <div className="bg-amber-950/20 rounded-lg p-3 text-xs text-amber-300 flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div>
              <p className="font-semibold">Cambio pendiente de guardar.</p>
              <p className="mt-0.5 opacity-80">'{initialDevice}' → '{selectedDevice}'. Guarda y reinicia el kernel.</p>
            </div>
          </div>
        )}
        {selectedDevice !== initialDevice && (
          <div className="mt-3 flex justify-end">
            <button type="button" onClick={handleSaveDevice}
              className="px-4 py-2 bg-amber-500/20 hover:bg-amber-500/30 text-amber-400 rounded-lg text-xs font-semibold flex items-center gap-2 transition-colors">
              <Save className="w-4 h-4" /> Guardar Dispositivo
            </button>
          </div>
        )}
      </div>

      {/* Profile Selector */}
      <div className="rounded-lg bg-slate-900/60 p-5 space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-slate-200 flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-violet-400" /> Perfil de Inferencia
          </h3>
          {activeProfileId
            ? <StatusPill status="online" label={`Perfil: ${PROFILES.find(p => p.id === activeProfileId)?.name}`} />
            : <StatusPill status="idle" label="Personalizado" />
          }
        </div>
        <p className="text-xs text-slate-400">Elige un perfil o ajusta manualmente en la sección avanzada.</p>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {PROFILES.map((profile) => {
            const Icon = profile.icon;
            const isActive = activeProfileId === profile.id;
            return (
              <button key={profile.id} type="button" onClick={() => applyProfile(profile.id)}
                className={cn("flex flex-col items-start p-4 rounded-lg text-left transition-all",
                  isActive ? `bg-${profile.color}-950/30 text-slate-100 ring-1 ring-${profile.color}-500`
                    : "bg-slate-950/50 text-slate-400 hover:text-slate-300"
                )}>
                <div className="flex items-center justify-between w-full mb-2">
                  <span className="flex items-center gap-2 text-xs font-semibold">
                    <Icon className={`w-4 h-4 text-${profile.color}-400`} /> {profile.name}
                  </span>
                  {isActive && <StatusPill status="online" label="Activo" />}
                </div>
                <p className="text-[10px] text-slate-400 leading-relaxed mb-3">{profile.description}</p>
                <div className="w-full pt-2 border-t border-slate-800/60 text-[9px] text-slate-500 flex flex-wrap gap-x-3 gap-y-1">
                  <span>temp={profile.values.temperature}</span>
                  <span>top_p={profile.values.topP}</span>
                  <span>top_k={profile.values.topK}</span>
                  <span>penalty={profile.values.repeatPenalty}</span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Advanced Parameters */}
      <div className="rounded-lg bg-slate-900/60 overflow-hidden">
        <button type="button" onClick={() => setShowAdvanced(!showAdvanced)}
          className="w-full px-5 py-4 flex items-center justify-between text-sm font-semibold text-slate-300 hover:bg-slate-800/30 transition-colors">
          <span className="flex items-center gap-2"><Settings className="w-4 h-4 text-slate-400" /> Configuración Avanzada</span>
          <ChevronDown className={cn("w-4 h-4 text-slate-500 transition-transform", showAdvanced && "rotate-180")} />
        </button>
        {showAdvanced && (
          <div className="px-5 pb-5 space-y-4 border-t border-slate-800/50 pt-4">
            <p className="text-[10px] text-slate-500">Parámetros raw de llama-server. Cambiar estos valores anula el perfil.</p>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-[10px] font-semibold text-slate-400 mb-1">Capas GPU</label>
                <input type="number" value={gpuLayers} onChange={(e) => setGpuLayers(Number(e.target.value))}
                  className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs text-slate-200 focus:border-violet-500 focus:outline-none" />
              </div>
              <div>
                <label className="block text-[10px] font-semibold text-slate-400 mb-1">Hilos CPU</label>
                <input type="number" value={cpuThreads} onChange={(e) => setCpuThreads(Number(e.target.value))}
                  className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs text-slate-200 focus:border-violet-500 focus:outline-none" />
              </div>
              <div>
                <label className="block text-[10px] font-semibold text-slate-400 mb-1">Batch Size</label>
                <select value={batchSize} onChange={(e) => setBatchSize(Number(e.target.value))}
                  className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs text-slate-200 focus:border-violet-500 focus:outline-none">
                  <option value={256}>256</option><option value={512}>512</option><option value={1024}>1024</option>
                </select>
              </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
              {[
                { label: 'Temperatura', value: temperature, set: setTemperature, min: 0, max: 1, step: 0.05 },
                { label: 'Top-P', value: topP, set: setTopP, min: 0.5, max: 1, step: 0.01 },
                { label: 'Top-K', value: topK, set: (v: number) => setTopK(Math.round(v)), min: 1, max: 100, step: 1 },
                { label: 'Repeat Penalty', value: repeatPenalty, set: setRepeatPenalty, min: 1, max: 1.5, step: 0.02 },
              ].map(({ label, value, set, min, max, step }) => (
                <div key={label}>
                  <div className="flex justify-between items-center mb-1">
                    <span className="text-[10px] font-semibold text-slate-400">{label}</span>
                    <span className="text-[10px] text-violet-400 font-mono">{value}</span>
                  </div>
                  <input type="range" min={min} max={max} step={step} value={value}
                    onChange={(e) => set(parseFloat(e.target.value))}
                    className="w-full accent-violet-500 bg-slate-950 rounded h-1.5 cursor-pointer" />
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Connection status */}
      {connStatus && (
        <div className={cn("p-3 rounded-lg text-xs flex items-center justify-between",
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
        <button type="button" onClick={handleTestConnection} disabled={testingConn}
          className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-300 rounded-lg text-xs font-mono flex items-center gap-2 transition-colors">
          {testingConn ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Cpu className="w-3.5 h-3.5 text-amber-400" />}
          Probar Conexión
        </button>
        <button type="button" onClick={handleSaveGgufConfig} disabled={savingGguf}
          className="px-4 py-2 bg-violet-600 hover:bg-violet-500 disabled:opacity-50 text-slate-50 rounded-lg text-xs font-semibold flex items-center gap-2 transition-colors shadow-lg shadow-violet-900/30">
          {savingGguf ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
          Guardar Configuración
        </button>
      </div>
    </div>
  );
}
