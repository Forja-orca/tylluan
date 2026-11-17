import React, { useEffect, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { cn } from '../../lib/utils';

declare const __DASHBOARD_COMMIT__: string | undefined;

interface HealthCommitPayload {
  commit?: string;
  version?: string;
}

export interface HeaderInstanceBadgeProps {
  bridge: NexusBridge | null;
  buildCommit?: string;
  locationHref?: string;
}

function extractPortLabel(rawUrl: string): string {
  try {
    const parsed = new URL(rawUrl);
    if (parsed.port) return `:${parsed.port}`;
    return parsed.protocol === 'https:' ? ':443' : ':80';
  } catch {
    return rawUrl || 'local';
  }
}

function commitsMatch(a: string, b: string): boolean {
  const left = a.trim().toLowerCase();
  const right = b.trim().toLowerCase();
  if (!left || !right) return true;
  return left === right || left.startsWith(right) || right.startsWith(left);
}

/**
 * Insignia de instancia en cabecera: muestra puerto · commit de /health
 * y avisa si el commit del kernel difiere del commit con que se construyó el dashboard.
 */
export function HeaderInstanceBadge({
  bridge,
  buildCommit = typeof __DASHBOARD_COMMIT__ !== 'undefined' ? __DASHBOARD_COMMIT__ : '',
  locationHref = typeof window !== 'undefined' ? window.location.href : '',
}: HeaderInstanceBadgeProps) {
  const [kernelCommit, setKernelCommit] = useState<string>('');
  const [kernelVersion, setKernelVersion] = useState<string>('');

  const targetUrl = bridge?.getBaseUrl?.() || locationHref;
  const portLabel = extractPortLabel(targetUrl);

  useEffect(() => {
    let cancelled = false;
    if (!bridge) {
      setKernelCommit('');
      setKernelVersion('');
      return;
    }
    void (async () => {
      try {
        const res = await bridge.fetchRaw('/health');
        if (!res || !res.ok) return;
        const data = (await res.json()) as HealthCommitPayload;
        if (!cancelled) {
          setKernelCommit(typeof data?.commit === 'string' ? data.commit.trim() : '');
          setKernelVersion(typeof data?.version === 'string' ? data.version.trim() : '');
        }
      } catch {
        // Ignorar fallo de lectura auxiliar
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bridge]);

  const hasMismatch = Boolean(buildCommit && kernelCommit && !commitsMatch(buildCommit, kernelCommit));
  const commitLabel = kernelCommit || '—';

  return (
    <div
      role="status"
      data-testid="header-instance-badge"
      title={
        hasMismatch
          ? `Kernel ${kernelVersion || ''} (${kernelCommit}) en ${targetUrl} difiere del dashboard (${buildCommit})`
          : `Instancia ${targetUrl} · v${kernelVersion || '?'} (${commitLabel})`
      }
      className={cn(
        'hidden sm:flex items-center gap-1.5 px-2 py-1 rounded border text-[11px] font-mono shrink-0',
        hasMismatch
          ? 'bg-amber-500/15 border-amber-500/40 text-amber-800 dark:text-amber-200'
          : 'bg-card border-border text-muted-foreground'
      )}
    >
      <span>
        {portLabel} · {commitLabel}
      </span>
      {hasMismatch && (
        <span className="inline-flex items-center gap-1 font-sans font-semibold text-[10px]">
          <AlertTriangle className="w-3 h-3 text-amber-600 dark:text-amber-400" aria-hidden="true" />
          Commit distinto ({buildCommit})
        </span>
      )}
    </div>
  );
}
