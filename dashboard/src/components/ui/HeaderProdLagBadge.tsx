import React, { useEffect, useState } from 'react';
import { AlertTriangle, CheckCircle2, Loader2, WifiOff } from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { cn } from '../../lib/utils';

declare const __DASHBOARD_COMMIT__: string | undefined;

interface HealthPayload {
  commit?: string;
  commits_behind?: number;
}

export interface HeaderProdLagBadgeProps {
  bridge: NexusBridge | null;
  /** Commit de main que expone la UI (por defecto __DASHBOARD_COMMIT__). */
  mainCommit?: string;
  /** Número de commits sin desplegar si se conoce (mock o calculado) y /health no incluye commits_behind.
   *  Si no se conoce, NO se inventa una cifra: se muestra solo «atrasada». */
  undeployedCommits?: number;
}

function commitsMatch(a: string, b: string): boolean {
  const left = a.trim().toLowerCase();
  const right = b.trim().toLowerCase();
  if (!left || !right) return true;
  return left === right || left.startsWith(right) || right.startsWith(left);
}

/**
 * Indicador de «Producción atrasada» en la cabecera (SOLO lectura).
 * Compara el commit en ejecución de /health con el commit de main expuesto por la UI.
 */
export function HeaderProdLagBadge({
  bridge,
  mainCommit = typeof __DASHBOARD_COMMIT__ !== 'undefined' ? __DASHBOARD_COMMIT__ : '',
  undeployedCommits,
}: HeaderProdLagBadgeProps) {
  const [loading, setLoading] = useState<boolean>(Boolean(bridge));
  const [error, setError] = useState<boolean>(false);
  const [kernelCommit, setKernelCommit] = useState<string>('');
  const [behindFromHealth, setBehindFromHealth] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (!bridge) {
      setLoading(false);
      setError(true);
      setKernelCommit('');
      setBehindFromHealth(null);
      return;
    }

    setLoading(true);
    setError(false);

    void (async () => {
      try {
        const data = (await bridge.fetchRaw('/health')) as HealthPayload | null;
        if (!cancelled) {
          const commit = typeof data?.commit === 'string' ? data.commit.trim() : '';
          if (!commit) {
            setError(true);
            setKernelCommit('');
          } else {
            setKernelCommit(commit);
            setBehindFromHealth(
              typeof data?.commits_behind === 'number' && data.commits_behind >= 0
                ? data.commits_behind
                : null,
            );
            setError(false);
          }
        }
      } catch {
        if (!cancelled) {
          setError(true);
          setKernelCommit('');
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [bridge]);

  if (loading) {
    return (
      <div
        role="status"
        aria-busy="true"
        data-testid="header-prod-lag-badge"
        className="hidden md:inline-flex items-center gap-1.5 px-2 py-1 rounded border border-border bg-card text-muted-foreground text-[11px] font-mono shrink-0"
      >
        <Loader2 className="w-3 h-3 animate-spin" aria-hidden="true" />
        <span>Comprobando despliegue...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div
        role="status"
        data-testid="header-prod-lag-badge"
        title="No se pudo leer el commit activo en /health"
        className="hidden md:inline-flex items-center gap-1.5 px-2 py-1 rounded border border-rose-500/30 bg-rose-500/10 text-rose-800 dark:text-rose-300 text-[11px] font-sans font-medium shrink-0"
      >
        <WifiOff className="w-3 h-3" aria-hidden="true" />
        <span>Estado de despliegue no disponible</span>
      </div>
    );
  }

  const isLagging = Boolean(mainCommit && kernelCommit && !commitsMatch(mainCommit, kernelCommit));
  const lagCount = behindFromHealth ?? (typeof undeployedCommits === 'number' && undeployedCommits >= 0 ? undeployedCommits : null);
  const lagText =
    lagCount === null ? 'versión anterior a main' : lagCount === 1 ? '1 commit sin desplegar' : `${lagCount} commits sin desplegar`;

  return (
    <div
      role="status"
      data-testid="header-prod-lag-badge"
      title={
        isLagging
          ? `Producción en ${kernelCommit} frente a main ${mainCommit} (${lagText}). Solo lectura.`
          : `Producción al día en ${kernelCommit || mainCommit}`
      }
      className={cn(
        'hidden md:inline-flex items-center gap-1.5 px-2 py-1 rounded border text-[11px] font-sans font-semibold shrink-0',
        isLagging
          ? 'bg-amber-500/15 border-amber-500/40 text-amber-800 dark:text-amber-200'
          : 'bg-emerald-500/10 border-emerald-500/30 text-emerald-800 dark:text-emerald-300',
      )}
    >
      {isLagging ? (
        <>
          <AlertTriangle className="w-3 h-3 text-amber-700 dark:text-amber-400" aria-hidden="true" />
          <span>Producción atrasada · {lagText}</span>
        </>
      ) : (
        <>
          <CheckCircle2 className="w-3 h-3 text-emerald-700 dark:text-emerald-400" aria-hidden="true" />
          <span>Producción al día</span>
        </>
      )}
    </div>
  );
}
