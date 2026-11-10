import React, { useState } from 'react';
import {
  ChevronDown,
  ChevronUp,
  Sliders,
  HardDrive,
  Save,
  CheckCircle2,
  Copy,
  Check,
  Folder,
  Info
} from 'lucide-react';
import { cn } from '../../../lib/utils';
import type { DetectedLocalModel } from './RoleModelAssignmentSection';

export interface EngineAdvancedAccordionProps {
  device: string;
  setDevice: (val: string) => void;
  deviceOptions: string[];
  maxThreads: number;
  setMaxThreads: (val: number) => void;
  hasEngineChanges: boolean;
  setHasEngineChanges: (val: boolean) => void;
  savingEngine: boolean;
  onSaveEngine: () => void;
  engineSuccess: boolean;
  detectedModels: DetectedLocalModel[];
  modelsDirectory?: string | null;
  modelsDir?: string;
  modelsDirFromEnv?: boolean;
  hint?: string | null;
}

export function EngineAdvancedAccordion({
  device,
  setDevice,
  deviceOptions,
  maxThreads,
  setMaxThreads,
  hasEngineChanges,
  setHasEngineChanges,
  savingEngine,
  onSaveEngine,
  engineSuccess,
  detectedModels,
  modelsDirectory,
  modelsDir,
  modelsDirFromEnv = false,
  hint,
}: EngineAdvancedAccordionProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [copiedPath, setCopiedPath] = useState(false);

  const effectiveModelsDir = modelsDir || modelsDirectory;

  const handleCopyPath = async (path: string) => {
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(path);
        setCopiedPath(true);
        setTimeout(() => setCopiedPath(false), 2000);
      }
    } catch (err) {
      console.error('Error al copiar ruta al portapapeles:', err);
    }
  };

  return (
    <section
      aria-labelledby="heading-advanced-engine"
      className="border border-border bg-card rounded-lg shadow-sm overflow-hidden"
      data-testid="engine-advanced-accordion"
    >
      <button
        type="button"
        id="heading-advanced-engine"
        onClick={() => setIsOpen(!isOpen)}
        className="w-full p-4 flex items-center justify-between text-left hover:bg-muted/30 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
        aria-expanded={isOpen}
        aria-controls="engine-advanced-panel"
      >
        <div className="flex items-center gap-2.5">
          <Sliders className="w-4 h-4 text-muted-foreground" aria-hidden="true" />
          <div>
            <h3 className="text-sm font-bold text-foreground">
              Ajustes Avanzados & Motor de Inferencia
            </h3>
            <p className="text-xs text-muted-foreground">
              Aceleración hardware, concurrencia de hilos y catálogo físico en disco.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {hasEngineChanges && (
            <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
              Cambios pendientes
            </span>
          )}
          {isOpen ? (
            <ChevronUp className="w-4 h-4 text-muted-foreground" />
          ) : (
            <ChevronDown className="w-4 h-4 text-muted-foreground" />
          )}
        </div>
      </button>

      {isOpen && (
        <div
          id="engine-advanced-panel"
          role="region"
          aria-labelledby="heading-advanced-engine"
          className="p-4 pt-1 border-t border-border space-y-5"
        >
          {/* Hardware & Motor */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 pt-2">
            {/* Dispositivo de Ejecución */}
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <label htmlFor="field-device" className="text-xs font-semibold text-foreground">
                  Dispositivo de Aceleración
                </label>
                <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                  Requiere Reinicio
                </span>
              </div>
              <select
                id="field-device"
                value={device}
                onChange={(e) => {
                  setDevice(e.target.value);
                  setHasEngineChanges(true);
                }}
                className="w-full text-xs font-mono bg-muted/30 border border-border rounded-md px-3 py-2 text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                {deviceOptions.map((opt) => (
                  <option key={opt} value={opt}>
                    {opt.toUpperCase()} {opt === 'directml' ? '(GPU Windows)' : opt === 'cuda' ? '(NVIDIA)' : opt === 'cpu' ? '(CPU Fallback)' : ''}
                  </option>
                ))}
              </select>
              <p className="text-[11px] text-muted-foreground">
                Proveedor de ejecución hardware para ONNX y tensores.
              </p>
            </div>

            {/* Hilos de Inferencia */}
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <label htmlFor="field-max-threads" className="text-xs font-semibold text-foreground">
                  Hilos Máximos de Inferencia
                </label>
                <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                  Requiere Reinicio
                </span>
              </div>
              <input
                id="field-max-threads"
                type="number"
                min={1}
                max={256}
                value={maxThreads}
                onChange={(e) => {
                  setMaxThreads(Math.max(1, Math.min(256, parseInt(e.target.value, 10) || 1)));
                  setHasEngineChanges(true);
                }}
                className="w-full text-xs font-mono bg-muted/30 border border-border rounded-md px-3 py-2 text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              />
              <p className="text-[11px] text-muted-foreground">
                Concurrencia local asignada para CPU (Rango: 1 – 256).
              </p>
            </div>
          </div>

          {/* Inventario Físico en Disco */}
          <div className="space-y-2 pt-2 border-t border-border">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <HardDrive className="w-4 h-4 text-muted-foreground" />
                <h4 className="text-xs font-bold text-foreground">Inventario Físico de Modelos en Disco</h4>
              </div>
              <span className="text-[11px] font-mono text-muted-foreground">
                {effectiveModelsDir || 'models/'}
              </span>
            </div>

            {detectedModels.length === 0 ? (
              hint || effectiveModelsDir ? (
                <div
                  role="region"
                  aria-label="Directorio de modelos"
                  className="p-3 rounded-lg border border-border/80 bg-muted/20 space-y-2.5 text-xs text-foreground"
                  data-testid="empty-models-hint-accordion"
                >
                  {hint && (
                    <div className="flex items-start gap-2">
                      <Info className="w-4 h-4 text-muted-foreground shrink-0 mt-0.5" aria-hidden="true" />
                      <p className="text-muted-foreground leading-relaxed">{hint}</p>
                    </div>
                  )}
                  {effectiveModelsDir && (
                    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 p-2 rounded bg-background/80 border border-border/60">
                      <div className="flex items-center gap-2 min-w-0">
                        <Folder className="w-3.5 h-3.5 text-muted-foreground shrink-0" aria-hidden="true" />
                        <span className="font-mono text-[11px] text-foreground break-all select-all font-medium">
                          {effectiveModelsDir}
                        </span>
                        {modelsDirFromEnv && (
                          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono border border-border bg-muted text-muted-foreground shrink-0">
                            TYLLUAN_MODELS_DIR
                          </span>
                        )}
                      </div>
                      <button
                        type="button"
                        onClick={() => handleCopyPath(effectiveModelsDir)}
                        className="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded border border-border bg-card hover:bg-accent text-foreground transition-colors shrink-0 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        aria-label="Copiar ruta"
                      >
                        {copiedPath ? (
                          <Check className="w-3.5 h-3.5 text-emerald-500" aria-hidden="true" />
                        ) : (
                          <Copy className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                        )}
                        <span>{copiedPath ? 'Copiada' : 'Copiar ruta'}</span>
                      </button>
                    </div>
                  )}
                </div>
              ) : (
                <p className="text-xs text-muted-foreground italic p-2 bg-muted/20 rounded">
                  No se detectaron archivos de modelo adicionales en el directorio local de modelos.
                </p>
              )
            ) : (
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2 pt-1">
                {detectedModels.map((m, idx) => {
                  const sizeDisplay = m.size_mb ? `${m.size_mb} MB` : m.size_bytes ? `${Math.round(m.size_bytes / 1024 / 1024)} MB` : null;

                  return (
                    <div
                      key={m.id || m.name || idx}
                      className="p-2.5 rounded border text-xs flex flex-col justify-between space-y-1 bg-muted/20 border-border text-foreground"
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-semibold truncate max-w-[170px]">{m.name}</span>
                      </div>
                      <div className="flex items-center justify-between text-[10px] text-muted-foreground font-mono">
                        <span className={cn(!m.role && !m.model_type && "italic text-muted-foreground/70")}>
                          {m.role || m.model_type || 'sin rol declarado'}
                        </span>
                        <span>{sizeDisplay || 'instalado'}</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          {/* Botón Guardar Motor */}
          <div className="pt-3 border-t border-border flex items-center justify-between">
            <div className="flex items-center gap-2 text-xs">
              <span className="text-[11px] font-mono text-muted-foreground">
                Los cambios se validan por ETag y aplican al reiniciar el kernel
              </span>
              {engineSuccess && (
                <span className="text-xs text-emerald-600 dark:text-emerald-400 font-semibold flex items-center gap-1">
                  <CheckCircle2 className="w-3.5 h-3.5" />
                  Guardado en tylluan.toml
                </span>
              )}
            </div>
            <button
              type="button"
              onClick={onSaveEngine}
              disabled={!hasEngineChanges || savingEngine}
              className={cn(
                "inline-flex items-center gap-2 px-3.5 py-1.5 text-xs font-semibold rounded-md transition-all shadow-sm cursor-pointer",
                hasEngineChanges
                  ? "bg-primary text-primary-foreground hover:bg-primary/90 focus-visible:ring-2 focus-visible:ring-primary"
                  : "bg-muted text-muted-foreground cursor-not-allowed opacity-60"
              )}
            >
              <Save className={cn("w-3.5 h-3.5", savingEngine && "animate-spin")} />
              {savingEngine ? 'Guardando...' : 'Guardar Motor de Inferencia'}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
