import React from 'react';
import { Radio, RefreshCw, ShieldCheck, Clock } from 'lucide-react';
import { cn } from '../../lib/utils';

export interface StandbyGracefulBannerProps {
  countdownSecs: number;
  totalStepSecs?: number;
  reconnectAttempts: number;
  retrying: boolean;
  onRetryNow: () => void;
  lastKnownDetail?: string | null;
}

/**
 * StandbyGracefulBanner — Modo "Standby Graceful" con reconexión asistida y backoff real (AG-28).
 * Muestra los segundos exactos hasta la próxima sonda real a /health, sin datos inventados.
 * Sustituye los errores crudos de red cuando el kernel está reiniciando, compilando o en espera,
 * manteniendo la interfaz estable sin alarmar con estados de fallo fatal.
 */
export const StandbyGracefulBanner: React.FC<StandbyGracefulBannerProps> = ({
  countdownSecs,
  totalStepSecs,
  reconnectAttempts,
  retrying,
  onRetryNow,
  lastKnownDetail,
}) => {
  const backoffStage = reconnectAttempts <= 1
    ? '5s'
    : reconnectAttempts === 2
    ? '10s'
    : reconnectAttempts === 3
    ? '15s'
    : reconnectAttempts === 4
    ? '30s'
    : '60s';

  const defaultStep = reconnectAttempts <= 1 ? 5 : reconnectAttempts === 2 ? 10 : reconnectAttempts === 3 ? 15 : reconnectAttempts === 4 ? 30 : 60;
  const effectiveTotal = Math.max(1, totalStepSecs ?? defaultStep);
  const clampedSecs = Math.max(0, countdownSecs);
  const progressPct = retrying ? 100 : Math.round(((effectiveTotal - Math.min(effectiveTotal, clampedSecs)) / effectiveTotal) * 100);

  return (
    <div
      role="status"
      aria-live="polite"
      data-testid="standby-graceful-banner"
      className="m-3 md:m-6 p-4 md:p-5 rounded border border-amber-500/30 bg-amber-500/5 text-foreground transition-all"
    >
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-start gap-3.5">
          <div className="p-2 rounded bg-amber-500/15 border border-amber-500/30 text-amber-400 shrink-0 mt-0.5">
            <Radio className="w-5 h-5 animate-pulse" aria-hidden="true" />
          </div>
          <div className="space-y-1">
            <div className="flex flex-wrap items-center gap-2">
              <h4 className="text-sm font-bold text-amber-300 font-mono">
                Kernel en Espera (Standby Graceful)
              </h4>
              <span className="px-2 py-0.5 rounded bg-amber-500/15 border border-amber-500/30 text-amber-300 text-[11px] font-mono font-semibold">
                {retrying ? 'Sondeando /health...' : `Próxima sonda: ${clampedSecs}s`}
              </span>
              <span className="px-2 py-0.5 rounded bg-card border border-border text-muted-foreground text-[11px] font-mono">
                Backoff AG-28: {backoffStage} · Intento #{Math.max(1, reconnectAttempts)}
              </span>
            </div>
            <p className="text-xs text-muted-foreground leading-relaxed max-w-3xl">
              El proceso soberano <span className="font-mono text-foreground">tylluan-nexus</span> está en ventana de reinicio, compilación o apagado temporal.
              El panel conserva tu contexto local y sondeará <span className="font-mono text-foreground">/health</span> automáticamente sin saturar el transporte.
            </p>
            {lastKnownDetail && (
              <div className="flex items-center gap-2 pt-1 text-[11px] font-mono text-muted-foreground">
                <ShieldCheck className="w-3.5 h-3.5 text-emerald-400 shrink-0" aria-hidden="true" />
                <span>Estado local preservado · Detalle técnico: {lastKnownDetail}</span>
              </div>
            )}
          </div>
        </div>

        <div className="flex items-center gap-3 shrink-0 self-end md:self-center">
          <div className="hidden sm:flex items-center gap-1.5 text-xs font-mono text-amber-300/90 bg-card/80 px-2.5 py-1.5 rounded border border-border">
            <Clock className="w-3.5 h-3.5 text-amber-400" aria-hidden="true" />
            <span>{retrying ? 'En curso' : `${clampedSecs}s`}</span>
          </div>
          <button
            type="button"
            disabled={retrying}
            onClick={onRetryNow}
            className={cn(
              "flex items-center gap-1.5 px-3.5 py-1.5 rounded border text-xs font-mono font-semibold transition-colors cursor-pointer",
              "bg-amber-500/20 hover:bg-amber-500/30 border-amber-500/40 text-amber-200",
              "disabled:opacity-50 disabled:cursor-not-allowed focus-visible:ring-1 focus-visible:ring-amber-400 focus-visible:outline-none"
            )}
          >
            <RefreshCw className={cn("w-3.5 h-3.5", retrying && "animate-spin")} aria-hidden="true" />
            <span>{retrying ? 'Sondeando kernel...' : 'Reconectar ahora'}</span>
          </button>
        </div>
      </div>

      {/* Real next-probe countdown progress bar */}
      <div className="mt-3.5 h-1.5 w-full rounded-full bg-muted/60 overflow-hidden border border-border/50">
        <div
          className="h-full bg-amber-500/80 transition-all duration-500 ease-out"
          style={{ width: `${progressPct}%` }}
        />
      </div>
    </div>
  );
};
