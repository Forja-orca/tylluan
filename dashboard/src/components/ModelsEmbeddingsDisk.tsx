import React, { useState } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Save, RefreshCw, Database, Image as ImageIcon, AlertTriangle } from 'lucide-react';

export interface DetectedLocalModel {
  id: string;
  name: string;
  model_type: 'embedding' | 'vision' | 'generative' | string;
  path: string;
  size_bytes?: number;
  size_mb?: number;
}

export interface ModelsConfigData {
  memory?: { embedding_model?: string };
  embedding?: { model_name?: string };
  embeddings?: { model?: string };
  vision?: { model_path?: string };
  inference?: { primary_model?: string };
  [key: string]: unknown;
}

export interface ModelsResponseData {
  detected_local_models?: DetectedLocalModel[];
  [key: string]: unknown;
}

interface Props {
  bridge: NexusBridge | null;
  config: ModelsConfigData | null;
  models: ModelsResponseData | null;
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
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      alert(`Error saving configuration: ${msg}`);
    }
    setSaving(false);
  };

  return (
    <div className="space-y-4">
      {/* Active Models Status */}
      <div className="rounded-lg bg-card border border-border shadow-sm p-4">
        <h3 className="text-sm font-semibold text-foreground mb-4 flex items-center gap-2">
          <Database className="w-4 h-4 text-primary" /> Active Models
        </h3>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="bg-muted/40 border border-border p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
              <Database className="w-3 h-3" /> Embedding
            </div>
            <div className="text-sm font-mono text-emerald-700 dark:text-emerald-400 truncate" title={embeddingModel}>
              {embeddingModel || 'None'}
            </div>
            <div className="text-[11px] text-muted-foreground mt-1">
              (change requires restart + reindex)
            </div>
          </div>
          <div className="bg-muted/40 border border-border p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
              <ImageIcon className="w-3 h-3" /> Vision
            </div>
            <div className="text-sm font-mono text-blue-700 dark:text-blue-400 truncate" title={visionModel}>
              {visionModel}
            </div>
          </div>
          <div className="bg-muted/40 border border-border p-3 rounded-lg">
            <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
              <Database className="w-3 h-3" /> Inference
            </div>
            <div className="text-sm font-mono text-violet-700 dark:text-violet-400 truncate" title={inferenceModel}>
              {inferenceModel}
            </div>
          </div>
        </div>
      </div>

      {/* Real Local Detected Model Inventory on Disk */}
      <fieldset className="rounded-lg bg-card border border-border p-4 shadow-sm">
        <legend className="text-xs font-bold text-primary px-1">
          Inventario de Modelos Locales en Disco
        </legend>
        <div className="flex items-center justify-between mb-3 pb-2 border-b border-border">
          <p className="text-xs text-muted-foreground">
            Modelos escaneados por el kernel en el directorio local <code className="font-mono text-foreground font-semibold">models/</code>.
          </p>
          <span className="text-xs font-mono text-primary font-bold">
            {models?.detected_local_models?.length ?? 0} detectados
          </span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
          {models?.detected_local_models && models.detected_local_models.length > 0 ? (
            models.detected_local_models.map((m: DetectedLocalModel) => (
              <div key={m.id} className="p-2.5 bg-muted/30 border border-border rounded-lg flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs font-bold font-mono text-foreground">{m.name}</span>
                    <span className={`text-[10px] px-1.5 py-0.5 rounded-md font-mono uppercase border ${
                      m.model_type === 'embedding' ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/30'
                        : m.model_type === 'vision' ? 'bg-blue-500/10 text-blue-700 dark:text-blue-400 border-blue-500/30'
                        : 'bg-purple-500/10 text-purple-700 dark:text-purple-400 border-purple-500/30'
                    }`}>
                      {m.model_type === 'embedding' ? 'Embedding' : m.model_type === 'vision' ? 'Visión' : 'Generativo'}
                    </span>
                  </div>
                  <p className="text-[10px] text-muted-foreground font-mono mt-0.5">{m.path}</p>
                </div>
                <div className="text-right">
                  <span className="text-xs font-mono text-emerald-700 dark:text-emerald-400 font-bold">{m.size_mb || Math.round((m.size_bytes || 0) / 1048576)} MB</span>
                </div>
              </div>
            ))
          ) : (
            <div className="col-span-2 p-6 bg-muted/30 border border-border rounded-lg flex flex-col items-center justify-center gap-1 text-center">
              <Database className="w-6 h-6 text-muted-foreground" />
              <p className="text-xs font-bold text-foreground">Sin modelos detectados en disco</p>
              <p className="text-xs text-muted-foreground max-w-sm leading-normal">
                El kernel no encontró archivos en <code className="font-mono">models/</code>. Ubique un archivo GGUF o modelo ONNX en ese directorio para habilitar inferencia local.
              </p>
            </div>
          )}
        </div>
      </fieldset>

      {/* Embedding Model Edit */}
      <fieldset className="rounded-lg bg-card border border-border p-4 shadow-sm">
        <legend className="text-xs font-bold text-primary px-1">Configuración del Modelo de Embeddings</legend>
        <div className="space-y-3">
          <div>
            <label className="block text-xs font-bold text-foreground mb-1">Nombre del Modelo</label>
            <input
              type="text"
              value={embeddingModel}
              onChange={(e) => setEmbeddingModel(e.target.value)}
              className="w-full max-w-md px-2.5 py-1.5 bg-muted/40 border border-border rounded-md font-mono text-xs text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
              placeholder="ej. mxbai-embed-large"
            />
          </div>
          
          <div className="bg-amber-500/10 border border-amber-500/30 p-3 text-xs text-amber-800 dark:text-amber-200 flex items-start gap-2 rounded-lg">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5 text-amber-600 dark:text-amber-400" />
            <div>
              <p className="font-bold">Advertencia: Cambiar el modelo requiere reiniciar el kernel y reindexar SilvaDB.</p>
              <p className="mt-0.5 text-[11px] text-amber-700 dark:text-amber-300">Esta operación puede requerir varios minutos según el volumen de la base de datos.</p>
            </div>
          </div>

          <button
            onClick={handleSave}
            disabled={saving || !embeddingModel}
            className="px-3.5 py-1.5 bg-muted/60 hover:bg-muted border border-border disabled:opacity-50 text-foreground rounded-md text-xs font-semibold shadow-sm flex items-center gap-1.5 transition-colors cursor-pointer"
          >
            {saving ? <RefreshCw className="w-3.5 h-3.5 animate-spin text-primary" /> : <Save className="w-3.5 h-3.5 text-primary" />}
            {saving ? 'Guardando...' : 'Guardar Configuración'}
          </button>
        </div>
      </fieldset>
    </div>
  );
}
