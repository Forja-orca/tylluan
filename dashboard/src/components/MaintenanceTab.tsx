import React, { useState, useEffect, useCallback } from 'react';
import {
  Database, Clock, Activity, Trash2, Save,
  RefreshCw, Download, Network, HardDrive, Cpu, WifiOff, Settings, ShieldCheck
} from 'lucide-react';
import { cn } from '../lib/utils';
import type { NexusBridge, MetricsHistory } from '../lib/nexus-bridge';
import { useNexus } from '../hooks/useNexus';
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

export function MaintenanceTab({ bridge, notify }: Props) {
  const { sysStatus, setToken } = useNexus();
  const [loading, setLoading] = useState<string | null>(null);
  const [status, setStatus] = useState<MaintenanceStatus | null>(null);
  const [lastOp, setLastOp] = useState<{ action: string; time: string } | null>(null);
  const [probe, setProbe] = useState<any>(null);
  const [metricsHistory, setMetricsHistory] = useState<MetricsHistory | null>(null);
  const [tokenInput, setTokenInput] = useState(() => localStorage.getItem('tylluan_token') || '');

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
    // Poll maintenance status every 60s (data rarely changes, not available via SSE)
    const interval = setInterval(loadStatus, 60000);
    return () => clearInterval(interval);
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
    const interval = setInterval(fetchMetrics, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [bridge]);

  const runAction = async (action: string, label: string) => {
    if (!bridge) return;
    setLoading(action);
    try {
      if (action === 'vacuum') await bridge.maintenance_vacuum();
      if (action === 'checkpoint') await bridge.maintenance_checkpoint();
      if (action === 'decay') await bridge.maintenance_decay();
      if (action === 'communities') await bridge.fetchRaw('/api/v1/silva/communities', { method: 'POST' });
      if (action === 'clean-orphans') {
        const res = await bridge.fetchRaw('/api/v1/maintenance/clean-orphans', { method: 'POST' });
        if (res.ok) {
          const data = await res.json();
          notify(`Cleanup completed: ${data.deleted_count} nodes deleted`, 'info');
        } else {
          throw new Error('Cleanup failed');
        }
      }
      if (action === 'purge') {
        if (!confirm('¿ESTÁS SEGURO? Esta acción borrará TODO el conocimiento acumulado en SilvaDB. No se puede deshacer.')) return;
        await bridge.maintenance_purge();
      }
      setLastOp({ action: label, time: new Date().toLocaleTimeString() });
      notify(`${label} completado`, 'info');
      await loadStatus();
    } catch {
      notify(`${label} falló`, 'error');
    }
    setLoading(null);
  };

  const statCards = [
    {
      icon: HardDrive,
      label: 'Brain Size',
      value: status?.brain_size_human ?? '—',
      sub: status ? `${status.brain_size_bytes.toLocaleString()} bytes` : 'calculando...',
      color: 'text-emerald-400',
    },
    {
      icon: Database,
      label: 'Graph nodes',
      value: status ? String(status.node_count) : '—',
      sub: `${status?.edge_count ?? 0} edges · ${status?.orphan_node_count ?? 0} orphans`,
      color: 'text-blue-400',
    },
    {
      icon: Clock,
      label: 'Último export',
      value: status?.last_export ?? '—',
      sub: status?.storage_mode ?? 'SQLite WAL',
      color: 'text-slate-300',
    },
    {
      icon: Activity,
      label: 'Última operación',
      value: lastOp?.action ?? 'Ninguna',
      sub: lastOp?.time ?? 'en esta sesión',
      color: 'text-amber-400',
    },
  ];

  const operations = [
    {
      id: 'vacuum',
      label: 'VACUUM',
      icon: Trash2,
      iconColor: 'text-red-400',
      desc: 'Recupera espacio libre y desfragmenta los archivos de base de datos. Recomendado tras borrados masivos.',
      btnClass: 'bg-red-500/10 hover:bg-red-500/20 text-red-400 border border-red-500/20',
    },
    {
      id: 'checkpoint',
      label: 'CHECKPOINT',
      icon: Save,
      iconColor: 'text-blue-400',
      desc: 'Flushes the Write-Ahead Log (WAL) to the main file. Ensures persistence integrity.',
      btnClass: 'bg-blue-500/10 hover:bg-blue-500/20 text-blue-400 border border-blue-500/20',
    },
    {
      id: 'decay',
      label: 'BIOLOGICAL DECAY',
      icon: Activity,
      iconColor: 'text-amber-400',
      desc: 'Applies biological weight decay on SilvaDB. Reduces stale memories to maintain relevance.',
      btnClass: 'bg-amber-500/10 hover:bg-amber-500/20 text-amber-400 border border-amber-500/30',
    },
    {
      id: 'export',
      label: 'EXPORT BACKUP',
      icon: Download,
      iconColor: 'text-emerald-400',
      desc: 'Exports a knowledge graph snapshot to ./data/exports/. Useful before risky operations.',
      btnClass: 'bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 border border-emerald-500/20',
    },
    {
      id: 'communities',
      label: 'DETECT COMMUNITIES',
      icon: Network,
      iconColor: 'text-indigo-400',
      desc: 'Runs the Louvain algorithm on SilvaDB to cluster nodes by semantic communities. Useful for visualization.',
      btnClass: 'bg-indigo-500/10 hover:bg-indigo-500/20 text-indigo-400 border border-indigo-500/20',
    },
    {
      id: 'clean-orphans',
      label: 'LIMPIAR HUÉRFANOS',
      icon: Trash2,
      iconColor: 'text-indigo-400',
      desc: 'Removes orphan nodes from SilvaDB (isolated, no incoming or outgoing relations, not protected).',
      btnClass: 'bg-indigo-500/10 hover:bg-indigo-500/20 text-indigo-400 border border-indigo-500/20',
    },
    {
      id: 'purge',
      label: 'HARD RESET MEMORIA',
      icon: Trash2,
      iconColor: 'text-rose-600',
      desc: '⚠️ WARNING: Deletes ALL SilvaDB nodes and relations. Useful for clearing context if hallucinations occur.',
      btnClass: 'bg-rose-500/20 hover:bg-rose-500/40 text-rose-500 border border-rose-500/40 font-black',
    },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-sm font-bold uppercase tracking-widest text-slate-300">Mantenimiento Soberano</h2>
          <p className="text-xs text-slate-500 mt-0.5">Operaciones sobre SilvaDB y HybridMemory</p>
        </div>
        <button
          type="button"
          onClick={loadStatus}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 border border-slate-800 text-xs text-slate-400 hover:text-slate-200 transition-colors"
        >
          <RefreshCw className="w-3 h-3" /> Actualizar
        </button>
      </div>

      {/* Connection Diagnostic */}
      <div className="flex items-center gap-4 p-4 rounded-xl border border-slate-800 bg-slate-900/50">
        <div className={cn("p-2 rounded-lg", probe ? "bg-emerald-500/10 text-emerald-400" : "bg-red-500/10 text-red-400")}>
          {probe ? <Network className="w-5 h-5" /> : <WifiOff className="w-5 h-5" />}
        </div>
        <div>
          <h3 className="text-sm font-bold text-slate-200">Diagnóstico de Conexión</h3>
          <p className="text-xs text-slate-500 font-mono mt-0.5">
            {probe ? `Kernel v${probe.kernel_version} (Port ${probe.port}) · Dialect: ${probe.detected_dialect}` : 'Offline / No connection'}
          </p>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {statCards.map(({ icon: Icon, label, value, sub, color }) => (
          <div key={label} className="p-4 rounded-2xl border border-slate-800 bg-slate-900/40">
            <div className="flex items-center gap-2 text-slate-500 mb-2">
              <Icon className="w-3.5 h-3.5" />
              <span className="text-[10px] font-bold uppercase tracking-widest">{label}</span>
            </div>
            <div className={cn("text-xl font-bold font-mono truncate", color)}>{value}</div>
            <p className="text-[10px] text-slate-600 mt-1 truncate">{sub}</p>
          </div>
        ))}
      </div>

      {/* Operations */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/40 overflow-hidden">
        <div className="px-5 py-3 border-b border-slate-800 flex items-center gap-2">
          <Network className="w-4 h-4 text-slate-500" />
          <span className="text-xs font-bold uppercase tracking-widest text-slate-400">Operaciones</span>
        </div>
        <div className="p-5 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
          {operations.map(({ id, label, icon: Icon, iconColor, desc, btnClass }) => (
            <div key={id} className="flex flex-col gap-3 p-4 rounded-xl border border-slate-800 bg-slate-950/40">
              <h3 className="text-xs font-bold flex items-center gap-2 text-slate-200">
                <Icon className={cn("w-4 h-4", iconColor)} />
                {label}
              </h3>
              <p className="text-[11px] text-slate-500 leading-relaxed flex-1">{desc}</p>
              <button
                type="button"
                onClick={() => runAction(id, label)}
                disabled={!!loading}
                className={cn(
                  "w-full py-2 rounded-lg text-[11px] font-bold flex items-center justify-center gap-2 transition-colors disabled:opacity-50",
                  btnClass
                )}
              >
                {loading === id
                  ? <><RefreshCw className="w-3 h-3 animate-spin" /> Ejecutando...</>
                  : `Ejecutar ${label.split(' ')[0]}`
                }
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Host Resources Viewer */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="p-6 rounded-2xl border border-slate-800 bg-slate-950/40">
          <div className="flex items-center gap-2 mb-4">
            <h3 className="text-xs font-bold uppercase tracking-widest text-slate-400 flex items-center gap-2">
              <Cpu className="w-4 h-4 text-blue-400" /> Host CPU Usage
            </h3>
          </div>
          {metricsHistory ? (
            <SparklineChart
              data={metricsHistory.snapshots.map(s => s.cpu)}
              color="#60a5fa"
              label="CPU"
              unit="%"
              height={48}
              showLast
            />
          ) : (
            <div className="flex items-end gap-1 h-12">
              {[4, 7, 2, 8, 5, 9, 3, 6, 4, 8].map((v, i) => {
                const cpu = sysStatus?.system?.cpu_usage ?? 0;
                const barHeight = cpu > 0 ? Math.min(100, cpu * v / 9) : 0;
                return <div key={i} className="flex-1 bg-blue-500/20 rounded-t-sm transition-all duration-1000" style={{ height: `${barHeight}%` }} />;
              })}
            </div>
          )}
          <p className="text-[10px] text-slate-600 mt-2 font-mono text-center italic">Kernel process monitor active</p>
        </div>
        <div className="p-6 rounded-2xl border border-slate-800 bg-slate-950/40">
          <div className="flex items-center gap-2 mb-4">
            <h3 className="text-xs font-bold uppercase tracking-widest text-slate-400 flex items-center gap-2">
              <Database className="w-4 h-4 text-emerald-400" /> Host RAM Pressure
            </h3>
          </div>
          {metricsHistory ? (
            <SparklineChart
              data={metricsHistory.snapshots.map(s => s.mem)}
              color="#34d399"
              label="RAM"
              unit="%"
              height={48}
              showLast
            />
          ) : (
            <>
              <div className="h-2 bg-slate-800 rounded-full overflow-hidden">
                <div className="h-full bg-emerald-500/50 shadow-[0_0_10px_rgba(16,185,129,0.3)] transition-all duration-1000" style={{ width: `${sysStatus?.system?.memory_percent ?? 0}%` }} />
              </div>
              <p className="text-[10px] text-slate-600 mt-2 font-mono text-center italic">
                {sysStatus?.system?.used_memory_mb ?? 0} MB / {sysStatus?.system?.total_memory_mb ?? 0} MB RAM detectados
              </p>
            </>
          )}
        </div>
      </div>

      {/* Info footer */}
      <div className="flex items-start gap-3 p-4 rounded-2xl bg-slate-900/30 border border-slate-800/50">
        <Database className="w-4 h-4 text-slate-600 shrink-0 mt-0.5" />
        <p className="text-[11px] text-slate-600 leading-relaxed">
          Maintenance operations act on <span className="text-slate-400">SilvaDB</span> (knowledge graph)
          and <span className="text-slate-400">HybridMemory</span> (FTS5+vector search).
          VACUUM and CHECKPOINT are safe in production. DECAY is irreversible — reduces weights of old memories.
          It is recommended to run CHECKPOINT before VACUUM to ensure integrity.
        </p>
      </div>

      {/* Token + Config Section */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-6">
        <h3 className="text-xs font-bold uppercase tracking-widest text-slate-400 flex items-center gap-2 mb-4">
          <ShieldCheck className="w-4 h-4 text-violet-400" /> Admin Access
        </h3>
        <div className="flex gap-3">
          <input
            type="password"
            title="API management token"
            placeholder="Bearer token"
            id="nexus-token-input"
            value={tokenInput}
            onChange={(e) => setTokenInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                setToken(tokenInput);
                notify('Token actualizado, reconectando...', 'info');
                setTimeout(() => window.location.reload(), 800);
              }
            }}
            className="flex-1 px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs font-mono text-slate-300"
          />
          <button
            type="button"
            onClick={() => {
              setToken(tokenInput);
              notify('Token actualizado, reconectando...', 'info');
              setTimeout(() => window.location.reload(), 800);
            }}
            className="px-4 py-2 bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 rounded-xl text-xs font-bold transition-colors"
          >
            Save Token
          </button>
        </div>
        <div className="mt-3 text-[10px] text-slate-600">
          Presiona Enter o haz clic en Guardar para actualizar. Esto reconectará el dashboard de forma segura al kernel de Tylluan.
        </div>
      </div>
    </div>
  );
}
