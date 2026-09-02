import { useState, useCallback } from 'react';
import { Cpu, Zap, HelpCircle } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import type { DeviceStatus } from '../lib/api/security';

export interface DeviceStatusBadgeProps {
  bridge: NexusBridge | null;
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
export default function DeviceStatusBadge({ bridge }: DeviceStatusBadgeProps) {
  const [status, setStatus] = useState<DeviceStatus | null>(null);
  const [failed, setFailed] = useState(false);

  const fetchStatus = useCallback(async () => {
    if (!bridge) return;
    try {
      const data = await bridge.getDeviceStatus();
      setStatus(data);
      setFailed(false);
    } catch {
      setFailed(true);
    }
  }, [bridge]);

  usePolling('device-status', fetchStatus, { interval: 'slow', enabled: !!bridge });

  const visionEntry = status?.guilds?.vision;
  const activeProvider = visionEntry?.active_provider;
  const hasRealData = !failed && !!activeProvider && visionEntry?.status !== 'error';
  const isGpu = hasRealData && GPU_PROVIDERS.has(activeProvider!);

  const label = !hasRealData
    ? 'Not detected'
    : isGpu
    ? `GPU (${shortProvider(activeProvider)})`
    : 'CPU';

  const tooltip = !hasRealData
    ? visionEntry?.error
      ? `Guild vision: ${visionEntry.error}`
      : 'No telemetry yet — endpoint did not respond or guild not running'
    : `Active provider: ${activeProvider}${status?.configured_device ? ` | Configured: ${status.configured_device}` : ''}${visionEntry?.model_loaded === false ? ' (model not loaded yet, cold detection)' : ''}`;

  return (
    <div
      className={cn(
        'inline-flex items-center gap-2 px-3 py-1.5 rounded-full border text-[11px] font-mono font-semibold tracking-wider transition-all duration-300 backdrop-blur-md cursor-help shadow-sm select-none',
        !hasRealData
          ? 'bg-slate-900/60 border-slate-800 text-slate-500 hover:border-slate-700'
          : isGpu
          ? 'bg-emerald-950/40 border-emerald-500/40 text-emerald-400 hover:border-emerald-500/60 shadow-[0_0_12px_rgba(16,185,129,0.15)]'
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

      <span className="uppercase tracking-widest text-[10px] font-bold">{label}</span>
    </div>
  );
}
