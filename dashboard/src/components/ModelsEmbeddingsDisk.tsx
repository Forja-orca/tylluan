import React, { useState } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Save, AlertTriangle, RefreshCw, Database, Image as ImageIcon } from 'lucide-react';


interface Props {
  bridge: NexusBridge | null;
  config: any;
  models: any;
}

export function ModelsEmbeddingsDisk({ bridge, config, models }: Props) {
  const [embeddingModel, setEmbeddingModel] = useState(
    (config?.memory?.embedding_model || config?.embedding?.model_name || config?.embeddings?.model || '') as string
  );
  const [saving, setSaving] = useState(false);

  const visionModel = config?.vision?.model_path?.split('/').pop() || 'SmolVLM2-256M';
  const inferenceModel = config?.inference?.primary_model || 'Unknown';

  const handleSave = async () => {
    if (!bridge) return;
    setSaving(true);
    try {
      const res = await bridge.fetch('/api/v1/config/device', {
        method: 'POST',
        body: JSON.stringify({ embedding_model: embeddingModel })
      });
      if (res?.error) throw new Error(res.error);
      alert('Embedding configuration saved. Restart the kernel and reindex SilvaDB to apply.');
    } catch (e: any) {
      alert(`Error saving configuration: ${e.message || String(e)}`);
    }
    setSaving(false);
  };

  return (
    <div className="space-y-6">
      {/* Active Models Status */}
      <div className="rounded-lg bg-slate-900/50 p-4">
        <h3 className="text-sm font-semibold text-slate-300 mb-4 flex items-center gap-2">
          <Database className="w-4 h-4 text-amber-400" /> Active Models
        </h3>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="bg-slate-950 p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-slate-500 mb-2">
              <Database className="w-3 h-3" /> Embedding
            </div>
            <div className="text-sm font-mono text-emerald-400 truncate" title={embeddingModel}>
              {embeddingModel || 'None'}
            </div>
            <div className="text-[9px] text-slate-500 mt-1">
              (change requires restart + reindex)
            </div>
          </div>
          <div className="bg-slate-950 p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-slate-500 mb-2">
              <ImageIcon className="w-3 h-3" /> Vision
            </div>
            <div className="text-sm font-mono text-blue-400 truncate" title={visionModel}>
              {visionModel}
            </div>
          </div>
          <div className="bg-slate-950 p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-slate-500 mb-2">
              <Database className="w-3 h-3" /> Inference
            </div>
            <div className="text-sm font-mono text-violet-400 truncate" title={inferenceModel}>
              {inferenceModel}
            </div>
          </div>
        </div>
      </div>

      {/* Real Local Detected Model Inventory on Disk */}
      <div className="rounded-lg bg-slate-900/50 p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <Database className="w-4 h-4 text-emerald-400" /> Local Model Inventory on Disk
          </h3>
          <span className="text-xs font-mono text-slate-400">
            {models?.detected_local_models?.length ?? 0} models detected
          </span>
        </div>
        <p className="text-xs text-slate-400 mb-4">
          Files scanned by the kernel in the local <code className="text-slate-300 font-mono">models/</code> directory.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {models?.detected_local_models?.length > 0 ? (
            models.detected_local_models.map((m: any) => (
              <div key={m.id} className="p-3 bg-slate-950 rounded-lg flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-semibold font-mono text-slate-200">{m.name}</span>
                    <span className={`text-[9px] px-1.5 py-0.5 rounded font-mono uppercase ${
                      m.model_type === 'embedding' ? 'bg-emerald-500/10 text-emerald-400'
                        : m.model_type === 'vision' ? 'bg-blue-500/10 text-blue-400'
                        : 'bg-violet-500/10 text-violet-400'
                    }`}>
                      {m.model_type === 'embedding' ? 'Embedding' : m.model_type === 'vision' ? 'Vision' : 'Generative'}
                    </span>
                  </div>
                  <p className="text-[10px] text-slate-500 font-mono mt-1">{m.path}</p>
                </div>
                <div className="text-right">
                  <span className="text-xs font-mono text-emerald-400 font-semibold">{m.size_mb || Math.round((m.size_bytes || 0) / 1048576)} MB</span>
                </div>
              </div>
            ))
          ) : (
            <div className="col-span-2 p-6 bg-slate-950/40 border border-dashed border-slate-700 rounded-lg flex flex-col items-center justify-center gap-2 text-center">
              <Database className="w-8 h-8 text-slate-600" />
              <p className="text-sm font-mono text-slate-400 font-semibold">No models detected on disk</p>
              <p className="text-xs text-slate-500 max-w-xs leading-relaxed">
                The kernel found no files in <code className="text-slate-400">models/</code>. Place a GGUF model in that folder and restart the kernel.
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Embedding Model Edit */}
      <div className="rounded-lg bg-amber-950/10 p-4">
        <h3 className="text-sm font-semibold text-slate-300 mb-4">Embedding Model Config</h3>
        <div className="space-y-4">
          <div>
            <label className="block text-xs text-slate-500 mb-1">Model Name</label>
            <input
              type="text"
              value={embeddingModel}
              onChange={(e) => setEmbeddingModel(e.target.value)}
              className="w-full max-w-md px-3 py-2 bg-slate-950 border border-slate-800 rounded font-mono text-sm text-slate-300"
              placeholder="e.g. nomic-embed-text"
            />
          </div>
          
          <div className="bg-amber-900/20 rounded-lg p-3 text-xs text-amber-300 flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div>
              <p className="font-semibold">⚠️ Changing the model requires restarting the kernel and reindexing SilvaDB.</p>
              <p className="mt-1 opacity-80">This operation may take several minutes depending on the database size.</p>
            </div>
          </div>

          <button
            onClick={handleSave}
            disabled={saving || !embeddingModel}
            className="px-4 py-2 bg-amber-500/20 hover:bg-amber-500/30 disabled:opacity-50 text-amber-400 rounded-lg text-xs font-semibold flex items-center gap-2 transition-colors"
          >
            {saving ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            Save Configuration
          </button>
        </div>
      </div>
    </div>
  );
}
