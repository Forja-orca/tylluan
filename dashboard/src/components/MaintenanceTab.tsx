import React, { useState, useEffect, useCallback } from 'react';
import {
  Database, Clock, Activity, Trash2, Save,
  RefreshCw, Download, Network, HardDrive, WifiOff
} from 'lucide-react';
import { cn } from '../lib/utils';
import type { NexusBridge, MetricsHistory } from '../lib/nexus-bridge';
import { useNexus } from '../hooks/useNexus';
import { usePolling } from '../hooks/usePolling';
import { SparklineChart } from './SparklineChart';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

interface MaintenanceStatus {
  status: string;
  brain_size_bytes: number;
  brain_size_human: string;
  last_export: string;
  storage_mode: string;
  node_count: number;
  edge_count: number;
  orphan_node_count?: number;
}

interface ProbeResult {
  kernel_version?: string;
  port?: number | string;
  detected_dialect?: string;
}

export function MaintenanceTab({ bridge, notify }: Props) {
  const { sysStatus } = useNexus();
  const [loading, setLoading] = useState<string | null>(null);
  const [status, setStatus] = useState<MaintenanceStatus | null>(null);
  const [lastOp, setLastOp] = useState<{ action: string; time: string } | null>(null);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [metricsHistory, setMetricsHistory] = useState<MetricsHistory | null>(null);

  const loadStatus = useCallback(async () => {
    if (!bridge) return;
    try {
      const results = await Promise.allSettled([
        bridge.maintenance_status(),
        bridge.probe()
      ]);
      const res = results[0].status === 'fulfilled' ? results[0].value : null;
      const probeRes = results[1].status === 'fulfilled' ? results[1].value : null;
      if (res) setStatus(res as MaintenanceStatus);
      if (probeRes) setProbe(probeRes);
    } catch (e) {
      console.error('Failed to load maintenance status:', e);
    }
  }, [bridge]);

  useEffect(() => {
    loadStatus();
  }, [loadStatus]);

  useEffect(() => {
    if (!bridge) return;
    let cancelled = false;

    const fetchMetrics = async () => {
      if (document.visibilityState === 'hidden') return;
      const result = await bridge.getMetricsHistory();
      if (!cancelled) setMetricsHistory(result);
    };

    fetchMetrics();
    return () => { cancelled = true; };
  }, [bridge]);

  // Polling via centralized coordinator (replaces 2 scattered setInterval calls)
  usePolling('maintenance-status', loadStatus, { interval: 'idle', enabled: !!bridge });
  usePolling('maintenance-metrics', async () => {
    if (!bridge) return;
    try { setMetricsHistory(await bridge.getMetricsHistory()); } catch {}
  }, { interval: 'fast', enabled: !!bridge });

  const runAction = async (action: string, label: string) => {
    if (!bridge) return;
    setLoading(action);
    try {
      if (action === 'vacuum') await bridge.maintenance_vacuum();
      if (action === 'checkpoint') await bridge.maintenance_checkpoint();
      if (action === 'decay') await bridge.maintenance_decay();
      if (action === 'onnx-clean') await bridge.maintenance_onnx_clean();
      if (action === 'logs-compact') await bridge.maintenance_logs_compact();
      if (action === 'communities') await bridge.fetchRaw('/api/v1/silva/communities', { method: 'POST' });
      if (action === 'clean-orphans') {
        const res = await bridge.fetchRaw('/api/v1/maintenance/clean-orphans', { method: 'POST' });
        if (res?.status === 'success') {
          notify(`Cleanup completed: ${res.deleted_count} nodes deleted`, 'info');
        } else {
          throw new Error('Cleanup failed');
        }
      }
      if (action === 'purge') {
        if (!confirm('ARE YOU SURE? This action will delete ALL knowledge in SilvaDB. Cannot be undone.')) return;
        await bridge.maintenance_purge();
      }
      setLastOp({ action: label, time: new Date().toLocaleTimeString() });
      notify(`${label} completed`, 'info');
      await loadStatus();
    } catch {
      notify(`${label} failed`, 'error');
    }
    setLoading(null);
  };

  const statCards = [
    {
      icon: HardDrive,
      label: 'Tamaño de Memoria',
      value: status?.brain_size_human ?? '—',
      sub: status ? `${status.brain_size_bytes.toLocaleString()} bytes` : 'calculando...',
      color: 'text-emerald-700 dark:text-emerald-400',
    },
    {
      icon: Database,
      label: 'Nodos en SilvaDB',
      value: status ? String(status.node_count) : '—',
      sub: `${status?.edge_count ?? 0} aristas · ${status?.orphan_node_count ?? 0} huérfanos`,
      color: 'text-[#9E4024] dark:text-blue-400',
    },
    {
      icon: Clock,
      label: 'Último Respaldo',
      value: status?.last_export ?? '—',
      sub: status?.storage_mode ?? 'SQLite WAL',
      color: 'text-[#444444] dark:text-slate-300',
    },
    {
      icon: Activity,
      label: 'Última Operación',
      value: lastOp?.action ?? 'Ninguna',
      sub: lastOp?.time ?? 'en esta sesión',
      color: 'text-[#B8860B] dark:text-amber-400',
    },
  ];

  const operations = [
    {
      id: 'vacuum',
      label: 'Compactar Base (VACUUM)',
      icon: Trash2,
      iconColor: 'text-rose-600',
      desc: 'Recupera espacio libre y desfragmenta ficheros SQLite. Recomendado tras eliminaciones masivas.',
      btnClass: 'bg-white hover:bg-rose-50 border border-rose-300 text-rose-700',
    },
    {
      id: 'checkpoint',
      label: 'Sincronizar WAL (CHECKPOINT)',
      icon: Save,
      iconColor: 'text-blue-600',
      desc: 'Vuelca el Write-Ahead Log (WAL) a los ficheros principales de SQLite. Asegura la persistencia.',
      btnClass: 'bg-white hover:bg-blue-50 border border-blue-300 text-blue-700',
    },
    {
      id: 'decay',
      label: 'Decaimiento Biológico FSRS',
      icon: Activity,
      iconColor: 'text-amber-600',
      desc: 'Aplica reducción de peso biológico en SilvaDB según la curva FSRS para mantener la relevancia.',
      btnClass: 'bg-white hover:bg-amber-50 border border-amber-300 text-amber-700',
    },
    {
      id: 'export',
      label: 'Exportar Respaldo',
      icon: Download,
      iconColor: 'text-emerald-600',
      desc: 'Genera una instantánea del grafo en el directorio de respaldos. Recomendado antes de tareas críticas.',
      btnClass: 'bg-white hover:bg-emerald-50 border border-emerald-300 text-emerald-700',
    },
    {
      id: 'communities',
      label: 'Detectar Comunidades',
      icon: Network,
      iconColor: 'text-indigo-600',
      desc: 'Ejecuta el algoritmo de Louvain en SilvaDB para agrupar nodos por afinidad semántica.',
      btnClass: 'bg-white hover:bg-indigo-50 border border-indigo-300 text-indigo-700',
    },
    {
      id: 'clean-orphans',
      label: 'Limpiar Huérfanos',
      icon: Trash2,
      iconColor: 'text-indigo-600',
      desc: 'Elimina nodos huérfanos sin relaciones entrantes ni salientes de SilvaDB.',
      btnClass: 'bg-white hover:bg-indigo-50 border border-indigo-300 text-indigo-700',
    },
    {
      id: 'purge',
      label: 'Reinicio de Memoria (PURGE)',
      icon: Trash2,
      iconColor: 'text-rose-700',
      desc: 'PELIGRO: Elimina TODOS los nodos y relaciones de SilvaDB. Útil para reiniciar el contexto.',
      btnClass: 'bg-rose-50 hover:bg-rose-100 border border-rose-400 text-rose-800 font-bold',
    },
  ];

  return (
    <div className="space-y-3 font-sans">
      {/* Header */}
      <fieldset className="border border-[#ACA899] dark:border-border p-2.5 bg-[#ECE9D8] dark:bg-card rounded-sm">
        <legend className="text-[11px] font-bold text-[#9E4024] dark:text-primary px-1">Mantenimiento del Sustrato Soberano</legend>
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
          <div>
            <h2 className="text-xs font-bold text-[#01285F] dark:text-foreground">Operaciones de Integridad de Bases SQLite</h2>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">Mantenimiento de SilvaDB, HybridMemory, FSRS y desfragmentación física de almacenamiento.</p>
          </div>
          <button
            type="button"
            onClick={loadStatus}
            className="flex items-center gap-1 px-2.5 py-1 bg-white hover:bg-[#F5F4EA] border border-[#ACA899] rounded-sm text-[11px] font-bold text-[#000000] dark:text-foreground transition-colors cursor-pointer self-start sm:self-auto"
          >
            <RefreshCw className="w-3 h-3 text-[#9E4024]" /> Actualizar
          </button>
        </div>
      </fieldset>

      {/* Connection Diagnostic */}
      <div className="flex items-center gap-2.5 p-2 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none">
        <div className={cn("p-1.5 border border-[#ACA899]", probe ? "bg-emerald-50 text-emerald-700" : "bg-rose-50 text-rose-700")}>
          {probe ? <Network className="w-4 h-4" /> : <WifiOff className="w-4 h-4" />}
        </div>
        <div>
          <h3 className="text-xs font-bold text-[#01285F] dark:text-foreground">Diagnóstico de Enlace con el Kernel</h3>
          <p className="text-[11px] text-[#444444] dark:text-muted-foreground font-mono mt-0.2">
            {probe ? `Kernel v${probe.kernel_version} (Puerto ${probe.port}) · Dialecto: ${probe.detected_dialect}` : 'Desconectado / Sin respuesta del kernel'}
          </p>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-2">
        {statCards.map(({ label, value, sub, color }) => (
          <div key={label} className="p-2.5 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none">
            <div className="text-[10px] font-bold text-[#01285F] dark:text-muted-foreground mb-1">{label}</div>
            <div className={cn("text-base font-bold font-mono truncate", color)}>{value}</div>
            <p className="text-[10px] text-[#555555] dark:text-muted-foreground mt-0.5 truncate">{sub}</p>
          </div>
        ))}
      </div>

      {/* Operations */}
      <fieldset className="p-2.5 bg-[#ECE9D8] dark:bg-card border border-[#ACA899] dark:border-border rounded-sm">
        <legend className="text-[11px] font-bold text-[#9E4024] dark:text-primary px-1">Acciones de Mantenimiento Soberano</legend>
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-2.5">
          {operations.map(({ id, label, icon: Icon, iconColor, desc, btnClass }) => (
            <div key={id} className="flex flex-col justify-between gap-2 p-2.5 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none">
              <div>
                <h3 className="text-xs font-bold flex items-center gap-1.5 text-[#01285F] dark:text-foreground">
                  <Icon className={cn("w-3.5 h-3.5", iconColor)} />
                  {label}
                </h3>
                <p className="text-[10px] text-[#444444] dark:text-muted-foreground leading-normal mt-1">{desc}</p>
              </div>
              <button
                type="button"
                onClick={() => runAction(id, label)}
                disabled={loading === id}
                className={cn(
                  "w-full py-1 px-2 rounded-sm text-[11px] font-bold transition-colors cursor-pointer flex items-center justify-center gap-1 border",
                  btnClass
                )}
              >
                {loading === id ? (
                  <>
                    <RefreshCw className="w-3 h-3 animate-spin" />
                    <span>Ejecutando...</span>
                  </>
                ) : (
                  <span>Ejecutar {label.split(' ')[0]}</span>
                )}
              </button>
            </div>
          ))}
        </div>
      </fieldset>

      {/* Host Resources Viewer */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
        <fieldset className="p-3 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none">
          <legend className="text-[11px] font-bold text-[#9E4024] dark:text-primary px-1">
            Uso de CPU del Host
          </legend>
          {metricsHistory ? (
            <SparklineChart
              data={metricsHistory.snapshots.map(s => s.cpu)}
              color="hsl(var(--blue-400))"
              label="CPU"
              unit="%"
              height={40}
              showLast
            />
          ) : (
            <div className="flex items-end gap-1 h-10">
              {[4, 7, 2, 8, 5, 9, 3, 6, 4, 8].map((v, i) => {
                const cpu = sysStatus?.system?.cpu_usage ?? 0;
                const barHeight = cpu > 0 ? Math.min(100, cpu * v / 9) : 0;
                return <div key={i} className="flex-1 bg-[#9E4024]/20 dark:bg-blue-500/20 rounded-none transition-all duration-1000" style={{ height: `${barHeight}%` }} />;
              })}
            </div>
          )}
          <p className="text-[10px] text-[#555555] dark:text-muted-foreground mt-1.5 font-mono text-center">Monitor de telemetría de CPU activo</p>
        </fieldset>

        <fieldset className="p-3 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none">
          <legend className="text-[11px] font-bold text-[#9E4024] dark:text-primary px-1">
            Presión de Memoria RAM del Host
          </legend>
          {metricsHistory ? (
            <SparklineChart
              data={metricsHistory.snapshots.map(s => s.mem)}
              color="hsl(var(--emerald-400))"
              label="RAM"
              unit="%"
              height={40}
              showLast
            />
          ) : (
            <>
              <div className="h-2 bg-[#E0E0E0] dark:bg-muted border border-[#ACA899] rounded-none overflow-hidden">
                <div className="h-full bg-emerald-600 transition-all duration-1000" style={{ width: `${sysStatus?.system?.memory_percent ?? 0}%` }} />
              </div>
              <p className="text-[10px] text-[#555555] dark:text-muted-foreground mt-1.5 font-mono text-center">
                {sysStatus?.system?.used_memory_mb ?? 0} MB / {sysStatus?.system?.total_memory_mb ?? 0} MB detectados
              </p>
            </>
          )}
        </fieldset>
      </div>

      {/* Info footer */}
      <div className="p-2.5 rounded-none bg-[#ECE9D8] dark:bg-muted/40 border border-[#ACA899] dark:border-border text-[11px] text-[#444444] dark:text-muted-foreground leading-normal">
        Las operaciones de mantenimiento actúan sobre las bases SQLite de <span className="font-bold text-[#9E4024] dark:text-primary">SilvaDB</span> (grafo cognitivo) y <span className="font-bold text-[#9E4024] dark:text-primary">HybridMemory</span> (FTS5 + búsqueda vectorial).
        VACUUM y CHECKPOINT son seguras en producción. El DECAY biológico es irreversible y atenúa nodos desusados según la curva de retención. Se recomienda ejecutar CHECKPOINT antes de VACUUM para garantizar consistencia física.
      </div>

      {/* Token + Config Section */}
      <fieldset className="rounded-none bg-white dark:bg-card border border-[#ACA899] dark:border-border p-3">
        <legend className="text-[11px] font-bold text-[#9E4024] dark:text-primary px-1">Credencial de Administración del Kernel</legend>
        <div className="flex gap-2">
          <input
            type="password"
            title="Token de administración de API"
            placeholder="Token Bearer maestro"
            id="nexus-token-input"
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const input = e.currentTarget as HTMLInputElement;
                document.cookie = `nexus_token=${input.value}; path=/`;
                window.dispatchEvent(new CustomEvent('nexus_token_update', { detail: input.value }));
                notify('Token actualizado en la sesión', 'info');
              }
            }}
            className="flex-1 px-2.5 py-1 bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none text-[11px] font-mono text-[#000000] dark:text-foreground focus:outline-none"
          />
          <button
            type="button"
            onClick={() => {
              const i = document.getElementById('nexus-token-input') as HTMLInputElement;
              if (i) {
                document.cookie = `nexus_token=${i.value}; path=/`;
                window.dispatchEvent(new CustomEvent('nexus_token_update', { detail: i.value }));
                notify('Token actualizado en la sesión', 'info');
              }
            }}
            className="px-3 py-1 bg-white hover:bg-[#F5F4EA] border border-[#ACA899] text-[#000000] dark:text-foreground rounded-sm text-[11px] font-bold flex items-center gap-1.5 transition-colors cursor-pointer"
          >
            Guardar Token
          </button>
        </div>
        <div className="mt-2 text-[10px] text-[#555555] dark:text-muted-foreground">
          Pulse Enter o Guardar Token para persistir en la sesión. Utilizado en APIs administrativas protegidas.
        </div>
      </fieldset>
    </div>
  );
}
