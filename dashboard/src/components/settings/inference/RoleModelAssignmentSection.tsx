import React from 'react';
import { Sliders, Save } from 'lucide-react';

export interface CognitivePointConfig {
  provider?: string | null;
  model?: string | null;
  endpoint?: string | null;
  api_key_env?: string | null;
  cloud_enabled?: boolean;
  locality?: string;
}

export interface DetectedLocalModel {
  id?: string;
  name: string;
  path?: string;
  size_bytes?: number;
  size_mb?: number;
  installed?: boolean;
  model_type?: string;
  role?: string;
  chat_capable?: boolean;
}

export interface RoleModelAssignmentSectionProps {
  pointsState: Record<string, CognitivePointConfig>;
  onUpdatePoint: (pointId: string, partial: Partial<CognitivePointConfig>) => void;
  onSavePoint: (pointId: string) => void;
  savingPoint: Record<string, boolean>;
  detectedModels: DetectedLocalModel[];
  activeVision: string;
  visionExists: boolean;
  activeEmbedding: string;
  vectorDimensions: number;
  onChangeVision?: (model: string) => void;
}

function formatModelLabel(name: string, role?: string): string {
  if (!role) return name;
  const roleLower = role.toLowerCase();
  if (name.toLowerCase().includes(`(${roleLower})`) || name.toLowerCase().endsWith(roleLower)) {
    return name;
  }
  return `${name} (${role})`;
}

export function RoleModelAssignmentSection({
  pointsState,
  onUpdatePoint,
  onSavePoint,
  savingPoint,
  detectedModels,
  activeVision,
  visionExists,
  activeEmbedding,
  vectorDimensions,
  onChangeVision,
}: RoleModelAssignmentSectionProps) {
  // Modelos para Decisión: filtrados por role === 'decision' o nombres que contengan 'decision'/'decima'
  const decisionModels = detectedModels.filter(
    m => m.role === 'decision' || m.name?.toLowerCase().includes('decision') || m.name?.toLowerCase().includes('decima')
  );
  // Si no hay ninguno clasificado aún como decision, mostrar todos los modelos excepto embeddings y vision
  const fallbackDecisionOptions = decisionModels.length > 0
    ? decisionModels
    : detectedModels.filter(m => m.role !== 'embedding' && m.role !== 'vision');

  // Modelos para Visión: filtrados por role === 'vision' o modelos smolvlm
  const visionModels = detectedModels.filter(
    m => (m.role === 'vision' || m.name?.toLowerCase().includes('smolvlm') || m.name?.toLowerCase().includes('vlm')) &&
         !m.name?.toLowerCase().includes('guard')
  );

  const currentDecisionModel = pointsState.decision?.model || (fallbackDecisionOptions[0]?.id || fallbackDecisionOptions[0]?.name || '');

  return (
    <section aria-labelledby="heading-modelos-por-uso" className="space-y-3" data-testid="role-assignment-section">
      <div className="pb-2 border-b border-border">
        <h3 id="heading-modelos-por-uso" className="text-sm font-bold text-foreground flex items-center gap-2">
          <Sliders className="w-4 h-4 text-primary" aria-hidden="true" />
          Modelos por uso
        </h3>
        <p className="text-xs text-muted-foreground">
          Asigna los modelos internos del kernel para decisión, embeddings y visión.
        </p>
      </div>

      <div className="border border-border rounded-lg divide-y divide-border bg-card shadow-sm overflow-hidden" data-testid="modelos-por-uso-list">
        {/* FILA 1: Decisión */}
        <div className="p-3.5 flex flex-col sm:flex-row sm:items-center justify-between gap-3 hover:bg-muted/20 transition-colors">
          <div className="space-y-0.5 sm:w-1/3">
            <span className="text-xs font-bold text-foreground block">Decision</span>
            <span className="text-[11px] text-muted-foreground">
              Consenso cognitivo y arbitraje de agentes
            </span>
          </div>

          <div className="flex items-center gap-2 flex-1 max-w-md">
            <select
              aria-label="Modelo para Decisión"
              value={currentDecisionModel}
              onChange={e => onUpdatePoint('decision', { model: e.target.value })}
              className="w-full px-3 py-1.5 text-xs font-mono rounded-md border border-border bg-background text-foreground focus:outline-none focus:ring-1 focus:ring-primary cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={fallbackDecisionOptions.length === 0}
            >
              {fallbackDecisionOptions.length > 0 ? (
                fallbackDecisionOptions.map(m => (
                  <option key={m.id || m.name} value={m.id || m.name}>
                    {formatModelLabel(m.name, m.role)}
                  </option>
                ))
              ) : (
                <option value="" disabled>Sin modelos de decisión detectados en disco</option>
              )}
            </select>

            <button
              type="button"
              onClick={() => onSavePoint('decision')}
              disabled={savingPoint['decision'] || fallbackDecisionOptions.length === 0}
              aria-label="Guardar modelo de decisión"
              className="px-2.5 py-1.5 text-xs font-medium rounded-md border border-border bg-muted/40 hover:bg-muted text-foreground transition-colors disabled:opacity-50 cursor-pointer shrink-0 flex items-center gap-1 min-h-[24px]"
              title="Guardar modelo de decisión"
            >
              <Save className="w-3 h-3" />
              <span>{savingPoint['decision'] ? 'Guardando...' : 'Guardar'}</span>
            </button>
          </div>
        </div>

        {/* FILA 2: Embeddings */}
        <div className="p-3.5 flex flex-col sm:flex-row sm:items-center justify-between gap-3 hover:bg-muted/20 transition-colors">
          <div className="space-y-0.5 sm:w-1/3">
            <span className="text-xs font-bold text-foreground block">Embeddings</span>
            <span className="text-[11px] text-muted-foreground">
              Vectores de SilvaDB (fijo a 1024 dimensiones)
            </span>
          </div>

          <div className="flex items-center gap-2 flex-1 max-w-md">
            <select
              aria-label="Modelo para Embeddings"
              value={activeEmbedding}
              disabled
              className="w-full px-3 py-1.5 text-xs font-mono rounded-md border border-border bg-muted/30 text-foreground cursor-not-allowed opacity-90"
              title="Anclado por arquitectura SilvaDB a 1024 dimensiones"
            >
              <option value="mxbai-embed-large">mxbai-embed-large (1024d)</option>
              <option value="bge-m3">bge-m3 (1024d)</option>
            </select>

            <span className="px-2 py-1 rounded text-[11px] font-mono border border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 shrink-0 font-semibold" title="1024 dimensiones">
              {vectorDimensions}d fijas
            </span>
          </div>
        </div>

        {/* FILA 3: Visión */}
        <div className="p-3.5 flex flex-col sm:flex-row sm:items-center justify-between gap-3 hover:bg-muted/20 transition-colors">
          <div className="space-y-0.5 sm:w-1/3">
            <span className="text-xs font-bold text-foreground block">Vision</span>
            <span className="text-[11px] text-muted-foreground">
              VLM para análisis multimodales de pantalla y capturas
            </span>
          </div>

          <div className="flex items-center gap-2 flex-1 max-w-md">
            <select
              aria-label="Modelo para Visión"
              value={activeVision}
              onChange={e => onChangeVision?.(e.target.value)}
              disabled={visionModels.length <= 1}
              className="w-full px-3 py-1.5 text-xs font-mono rounded-md border border-border bg-background text-foreground focus:outline-none focus:ring-1 focus:ring-primary cursor-pointer disabled:bg-muted/30 disabled:cursor-default"
            >
              {visionModels.length > 0 ? (
                visionModels.map(m => (
                  <option key={m.id || m.name} value={m.path || m.id || m.name}>
                    {formatModelLabel(m.name, m.role || 'vision')}
                  </option>
                ))
              ) : (
                <option value={activeVision || ''} disabled>
                  {activeVision ? `${activeVision.split(/[/\\]/).pop()} (no detectado en disco)` : 'Sin modelos de visión en disco'}
                </option>
              )}
            </select>

            <span
              className={`px-2 py-1 rounded text-[11px] font-medium border shrink-0 flex items-center gap-1 ${
                visionExists
                  ? 'border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                  : 'border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-400'
              }`}
            >
              <span className={`w-1.5 h-1.5 rounded-full ${visionExists ? 'bg-emerald-500' : 'bg-amber-500'}`} />
              <span>{visionExists ? 'en disco' : 'no disponible'}</span>
            </span>
          </div>
        </div>
      </div>
    </section>
  );
}
