import React, { useState } from 'react';
import {
  Server,
  Play,
  Square,
  ExternalLink,
  FolderOpen,
  RefreshCw
} from 'lucide-react';
import type { LlamaServerStatus } from './model-catalog-contract';

export interface LlamaCppLauncherCardProps {
  status: LlamaServerStatus;
  onToggleServer: (start: boolean) => Promise<void>;
  onUpdateDirectory: (newPath: string) => Promise<void>;
  onCheckUpdate: () => Promise<void>;
  className?: string;
}

export const LlamaCppLauncherCard: React.FC<LlamaCppLauncherCardProps> = ({
  status,
  onToggleServer,
  onUpdateDirectory,
  onCheckUpdate,
  className = ''
}) => {
  const [isBusy, setIsBusy] = useState(false);
  const [customDir, setCustomDir] = useState(status.models_directory || '');
  const [isEditingDir, setIsEditingDir] = useState(false);

  const handleToggle = async () => {
    setIsBusy(true);
    try {
      await onToggleServer(!status.running);
    } finally {
      setIsBusy(false);
    }
  };

  const handleSaveDir = async () => {
    if (!customDir.trim()) return;
    setIsBusy(true);
    try {
      await onUpdateDirectory(customDir.trim());
      setIsEditingDir(false);
    } finally {
      setIsBusy(false);
    }
  };

  return (
    <div
      className={`rounded border border-border bg-card p-4 space-y-3.5 text-foreground ${className}`}
      data-testid="llama-cpp-launcher-card"
    >
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2.5 border-b border-border/60 pb-3">
        <div className="flex items-center gap-2.5">
          <div className="p-2 rounded bg-amber-500/15 text-amber-400 border border-amber-500/30">
            <Server className="w-4 h-4" />
          </div>
          <div>
            <h4 className="text-sm font-semibold font-mono">Servidor llama.cpp del Usuario</h4>
            <p className="text-[11px] font-mono text-muted-foreground">
              Binario nativo original (fuera de Tylluan) · El chat es del usuario
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {status.running ? (
            <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 text-xs font-mono font-semibold">
              <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
              Activo (:{status.port ?? 8080})
            </span>
          ) : (
            <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-muted border border-border text-muted-foreground text-xs font-mono">
              <span className="w-2 h-2 rounded-full bg-muted-foreground/60" />
              Detenido
            </span>
          )}

          <button
            type="button"
            disabled={isBusy}
            onClick={handleToggle}
            className={`flex items-center gap-1.5 px-3 py-1 text-xs font-mono font-semibold rounded border transition-colors cursor-pointer disabled:opacity-50 ${
              status.running
                ? 'bg-rose-500/15 hover:bg-rose-500/25 border-rose-500/40 text-rose-300'
                : 'bg-emerald-500/15 hover:bg-emerald-500/25 border-emerald-500/40 text-emerald-300'
            }`}
          >
            {status.running ? (
              <>
                <Square className="w-3.5 h-3.5" />
                Detener
              </>
            ) : (
              <>
                <Play className="w-3.5 h-3.5" />
                Lanzar
              </>
            )}
          </button>
        </div>
      </div>

      {/* Métricas y Acceso Rápido */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-2 text-xs font-mono">
        <div className="p-2.5 rounded bg-muted/40 border border-border flex flex-col justify-between">
          <span className="text-[10px] text-muted-foreground uppercase">Endpoint Local:</span>
          <div className="flex items-center justify-between mt-1">
            <span className="font-semibold">{status.endpoint}</span>
            {status.running && (
              <a
                href={status.endpoint}
                target="_blank"
                rel="noreferrer"
                className="text-amber-400 hover:text-amber-300 flex items-center gap-0.5 text-[11px]"
              >
                Abrir Web UI
                <ExternalLink className="w-3 h-3 ml-0.5" />
              </a>
            )}
          </div>
        </div>

        <div className="p-2.5 rounded bg-muted/40 border border-border flex flex-col justify-between">
          <span className="text-[10px] text-muted-foreground uppercase">Versión y Upstream:</span>
          <div className="flex items-center justify-between mt-1">
            <span className="font-semibold truncate">{status.version || 'Auto-detectada'}</span>
            <button
              type="button"
              disabled={isBusy}
              onClick={onCheckUpdate}
              className="text-muted-foreground hover:text-foreground text-[10px] flex items-center gap-1 cursor-pointer"
              title="Comprobar última versión oficial en GitHub"
            >
              <RefreshCw className="w-3 h-3" />
              Actualizar
            </button>
          </div>
        </div>

        <div className="p-2.5 rounded bg-muted/40 border border-border flex flex-col justify-between">
          <span className="text-[10px] text-muted-foreground uppercase">Proceso / PID:</span>
          <div className="flex items-center justify-between mt-1">
            <span className="font-semibold">{status.pid ? `PID ${status.pid}` : 'Inactivo'}</span>
            <span className="text-[10px] text-muted-foreground">Puerto {status.port}</span>
          </div>
        </div>
      </div>

      {/* Selector de Carpeta de Modelos GGUF */}
      <div className="p-3 rounded bg-muted/20 border border-border/80 space-y-2 text-xs font-mono">
        <div className="flex items-center justify-between">
          <span className="text-muted-foreground flex items-center gap-1.5">
            <FolderOpen className="w-4 h-4 text-amber-400" />
            Carpeta de modelos personales (GGUF):
          </span>
          {!isEditingDir ? (
            <button
              type="button"
              onClick={() => setIsEditingDir(true)}
              className="text-[11px] text-amber-400 hover:text-amber-300 underline cursor-pointer"
            >
              Cambiar ruta
            </button>
          ) : (
            <button
              type="button"
              onClick={handleSaveDir}
              className="text-[11px] text-emerald-400 hover:text-emerald-300 font-semibold cursor-pointer"
            >
              Guardar
            </button>
          )}
        </div>

        {!isEditingDir ? (
          <p className="text-foreground bg-background px-2.5 py-1.5 rounded border border-border text-[11px] font-mono truncate">
            {status.models_directory || 'No configurada (elige tu ruta local)'}
          </p>
        ) : (
          <div className="flex gap-2">
            <input
              type="text"
              value={customDir}
              onChange={e => setCustomDir(e.target.value)}
              placeholder="D:\models\gguf o /path/to/models"
              className="flex-1 px-2.5 py-1 text-[11px] font-mono rounded bg-background border border-border focus:ring-1 focus:ring-amber-400 focus:outline-none"
            />
          </div>
        )}
      </div>
    </div>
  );
};
