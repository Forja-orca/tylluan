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
    ? 'No detectado'
    : isGpu
    ? `GPU (${shortProvider(activeProvider)})`
    : 'CPU';

  const tooltip = !hasRealData
    ? visionEntry?.error
      ? `Guild vision: ${visionEntry.error}`
      : 'Sin datos reales aún — endpoint no respondió o guild sin arrancar'
    : `Provider activo: ${activeProvider}${status?.configured_device ? ` | Configurado: ${status.configured_device}` : ''}${visionEntry?.model_loaded === false ? ' (modelo aún no cargado, detección en frío)' : ''}`;

  return (
    <div
      className={cn(
        'flex items-center gap-2 px-3 py-1.5 rounded-full border transition-colors cursor-help',
        !hasRealData
          ? 'bg-slate-900 border-slate-800 text-slate-500'
          : isGpu
          ? 'bg-emerald-950/30 border-emerald-500/30 text-emerald-400'
          : 'bg-slate-900 border-slate-800 text-slate-300'
      )}
      title={tooltip}
    >
      {!hasRealData ? (
        <HelpCircle className="w-3 h-3" />
      ) : isGpu ? (
        <Zap className="w-3 h-3" />
      ) : (
        <Cpu className="w-3 h-3" />
      )}
      <span className="text-[10px] font-bold uppercase tracking-wider font-mono">{label}</span>
    </div>
  );
}
