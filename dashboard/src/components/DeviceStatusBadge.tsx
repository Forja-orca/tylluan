import { useState, useCallback, useRef } from 'react';
import { Cpu, Zap, HelpCircle } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import type { DeviceStatus } from '../lib/api/security';

export interface DeviceStatusBadgeProps {
  bridge: NexusBridge | null;
  online?: boolean;
}

const GPU_PROVIDERS = new Set(['DmlExecutionProvider', 'CUDAExecutionProvider']);

function shortProvider(p: string | undefined): string {
  if (!p) return '';
  return p.replace('ExecutionProvider', '');
}

/**
 * Honest device badge: GPU active / CPU / no data. Sourced from
 * GET /api/v1/config/device/status, which queries each inference guild's
 * real onnxruntime provider — never a hardcoded or assumed value.
 * If the endpoint hasn't answered yet (old kernel, guild not started),
 * this renders the "no detectado" state rather than guessing.
 */
export default function DeviceStatusBadge({ bridge, online = true }: DeviceStatusBadgeProps) {
  const [status, setStatus] = useState<DeviceStatus | null>(null);
  const [failed, setFailed] = useState(false);
  const errorStreakRef = useRef<number>(0);
  const ticksUntilRetryRef = useRef<number>(0);

  const fetchStatus = useCallback(async () => {
    if (!bridge || !online) return;
    if (ticksUntilRetryRef.current > 0) {
      ticksUntilRetryRef.current -= 1;
      return;
    }
    try {
      const data = await bridge.getDeviceStatus();
      setStatus(data);
      const visionErr = data?.guilds?.vision?.status === 'error';
      if (visionErr) {
        errorStreakRef.current = Math.min(errorStreakRef.current + 1, 4);
        // Backoff en ticks de 60s: 2m -> 4m -> 8m -> 15m máx para evitar bucles de Pre-Spawn (AG-28)
        ticksUntilRetryRef.current = Math.min(Math.pow(2, errorStreakRef.current), 15);
      } else {
        errorStreakRef.current = 0;
        ticksUntilRetryRef.current = 0;
        setFailed(false);
      }
    } catch {
      setFailed(true);
      errorStreakRef.current = Math.min(errorStreakRef.current + 1, 4);
      ticksUntilRetryRef.current = Math.min(Math.pow(2, errorStreakRef.current), 15);
    }
  }, [bridge, online]);

  // AG-28: Sondeo en bucket 'idle' (60s) con backoff exponencial si el guild vision
  // está en error o inalcanzable, y pausado cuando el kernel está offline.
  const shouldPoll = Boolean(bridge) && online;

  usePolling('device-status', fetchStatus, { interval: 'idle', enabled: shouldPoll });

  const visionEntry = status?.guilds?.vision;
  const activeProvider = visionEntry?.active_provider;
  const hasRealData = !failed && !!activeProvider && visionEntry?.status !== 'error';
  const isGpu = hasRealData && GPU_PROVIDERS.has(activeProvider!);

  const label = !hasRealData
    ? 'No detectado'
    : isGpu
    ? `GPU (${shortProvider(activeProvider)})`
    : 'CPU';

  const tooltip = !hasRealData
    ? visionEntry?.error
      ? `Guild visión: ${visionEntry.error}`
      : 'Sin telemetría aún — el endpoint no respondió o el guild no está en ejecución'
    : `Proveedor activo: ${activeProvider}${status?.configured_device ? ` | Configurado: ${status.configured_device}` : ''}${visionEntry?.model_loaded === false ? ' (modelo aún no cargado, detección en frío)' : ''}`;

  return (
    <div
      className={cn(
        'inline-flex items-center gap-2 px-3 py-1.5 rounded border text-[11px] font-mono font-semibold transition-colors cursor-help select-none',
        !hasRealData
          ? 'bg-slate-900/60 border-slate-800 text-slate-500 hover:border-slate-700'
          : isGpu
          ? 'bg-emerald-950/40 border-emerald-500/40 text-emerald-400 hover:border-emerald-500/60'
          : 'bg-slate-900/80 border-amber-500/40 text-slate-200 hover:border-amber-500/60'
      )}
      title={tooltip}
    >
      <span className="relative flex h-2 w-2 shrink-0">
        {!hasRealData ? (
          <span className="relative inline-flex rounded-full h-2 w-2 bg-slate-500/60" />
        ) : isGpu ? (
          <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500 animate-beacon" />
        ) : (
          <span className="relative inline-flex rounded-full h-2 w-2 bg-amber-500" />
        )}
      </span>

      {!hasRealData ? (
        <HelpCircle className="w-3.5 h-3.5 text-slate-500" />
      ) : isGpu ? (
        <Zap className="w-3.5 h-3.5 text-emerald-400" />
      ) : (
        <Cpu className="w-3.5 h-3.5 text-amber-400" />
      )}

      <span className="text-[11px] font-semibold">
        {!hasRealData ? '○ ' : isGpu ? '● ' : '▲ '}
        {label}
      </span>
    </div>
  );
}
