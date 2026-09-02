import React, { useState, useEffect, useCallback } from 'react';
import {
  Database, Clock, Activity, Trash2, Save,
  RefreshCw, Download, Network, HardDrive, Cpu, WifiOff, Settings, ShieldCheck
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

export function MaintenanceTab({ bridge, notify }: Props) {
  const { sysStatus } = useNexus();
  const [loading, setLoading] = useState<string | null>(null);
  const [status, setStatus] = useState<MaintenanceStatus | null>(null);
  const [lastOp, setLastOp] = useState<{ action: string; time: string } | null>(null);
  const [probe, setProbe] = useState<any>(null);
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
      label: 'Brain Size',
      value: status?.brain_size_human ?? '—',
      sub: status ? `${status.brain_size_bytes.toLocaleString()} bytes` : 'calculating...',
      color: 'text-emerald-400',
    },
    {
      icon: Database,
      label: 'Graph Nodes',
      value: status ? String(status.node_count) : '—',
      sub: `${status?.edge_count ?? 0} edges · ${status?.orphan_node_count ?? 0} orphans`,
      color: 'text-blue-400',
    },
    {
      icon: Clock,
      label: 'Last Export',
      value: status?.last_export ?? '—',
      sub: status?.storage_mode ?? 'SQLite WAL',
      color: 'text-slate-300',
    },
    {
      icon: Activity,
      label: 'Last Operation',
      value: lastOp?.action ?? 'None',
      sub: lastOp?.time ?? 'in this session',
      color: 'text-amber-400',
    },
  ];

  const operations = [
    {
      id: 'vacuum',
      label: 'VACUUM',
      icon: Trash2,
      iconColor: 'text-red-400',
      desc: 'Reclaims free space and defragments database files. Recommended after bulk deletions.',
      btnClass: 'bg-red-500/10 hover:bg-red-500/20 text-red-400',
    },
    {
      id: 'checkpoint',
      label: 'CHECKPOINT',
      icon: Save,
      iconColor: 'text-blue-400',
      desc: 'Flushes the Write-Ahead Log (WAL) to the main database file. Ensures persistence integrity.',
      btnClass: 'bg-blue-500/10 hover:bg-blue-500/20 text-blue-400',
    },
    {
      id: 'decay',
      label: 'BIOLOGICAL DECAY',
      icon: Activity,
      iconColor: 'text-amber-400',
      desc: 'Applies biological weight reduction in SilvaDB. Decays stale memories to maintain relevance.',
      btnClass: 'bg-amber-500/10 hover:bg-amber-500/20 text-amber-400',
    },
    {
      id: 'export',
      label: 'EXPORT BACKUP',
      icon: Download,
      iconColor: 'text-emerald-400',
      desc: 'Exports a knowledge graph snapshot to ./data/exports/. Recommended before high-risk operations.',
      btnClass: 'bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400',
    },
    {
      id: 'communities',
      label: 'DETECT COMMUNITIES',
      icon: Network,
      iconColor: 'text-indigo-400',
      desc: 'Runs the Louvain community detection algorithm on SilvaDB to group nodes by semantic communities. Useful for visualization.',
      btnClass: 'bg-indigo-500/10 hover:bg-indigo-500/20 text-indigo-400',
    },
    {
      id: 'clean-orphans',
      label: 'CLEAN ORPHANS',
      icon: Trash2,
      iconColor: 'text-indigo-400',
      desc: 'Removes unprotected orphan nodes (isolated, zero incoming/outgoing relationships) from SilvaDB.',
      btnClass: 'bg-indigo-500/10 hover:bg-indigo-500/20 text-indigo-400',
    },
    {
      id: 'purge',
      label: 'HARD RESET MEMORY',
      icon: Trash2,
      iconColor: 'text-rose-600',
      desc: '⚠️ WARNING: Deletes ALL nodes and edges from SilvaDB. Useful to clear context if experiencing hallucinations.',
      btnClass: 'bg-rose-500/20 hover:bg-rose-500/40 text-rose-500 font-bold',
    },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-sm font-medium text-slate-100">Sovereign Maintenance</h2>
          <p className="text-xs text-slate-500 mt-0.5">Operations across SilvaDB and HybridMemory</p>
        </div>
        <button
          type="button"
          onClick={loadStatus}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 text-xs text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none"
        >
          <RefreshCw className="w-3 h-3" /> Refresh
        </button>
      </div>

      {/* Connection Diagnostic */}
      <div className="flex items-center gap-4 p-4 rounded-xl bg-slate-900/50">
        <div className={cn("p-2 rounded-lg", probe ? "bg-emerald-500/10 text-emerald-400" : "bg-red-500/10 text-red-400")}>
          {probe ? <Network className="w-5 h-5" /> : <WifiOff className="w-5 h-5" />}
        </div>
        <div>
          <h3 className="text-sm font-bold text-slate-200">Connection Diagnostics</h3>
          <p className="text-xs text-slate-500 font-mono mt-0.5">
            {probe ? `Kernel v${probe.kernel_version} (Port ${probe.port}) · Dialect: ${probe.detected_dialect}` : 'Offline / No connection'}
          </p>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {statCards.map(({ icon: Icon, label, value, sub, color }) => (
          <div key={label} className="p-4 rounded-2xl bg-slate-900/40">
            <div className="flex items-center gap-2 text-slate-500 mb-2">
              <Icon className="w-3.5 h-3.5" />
              <span className="text-[11px] font-medium">{label}</span>
            </div>
            <div className={cn("text-xl font-bold font-mono truncate", color)}>{value}</div>
            <p className="text-[10px] text-slate-600 mt-1 truncate">{sub}</p>
          </div>
        ))}
      </div>

      {/* Operations */}
      <div className="rounded-2xl bg-slate-900/40 overflow-hidden">
        <div className="px-5 py-3 border-b border-slate-800/50 flex items-center gap-2">
          <Network className="w-4 h-4 text-slate-500" />
          <span className="text-xs font-medium text-slate-400">Operations</span>
        </div>
        <div className="p-5 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
          {operations.map(({ id, label, icon: Icon, iconColor, desc, btnClass }) => (
            <div key={id} className="flex flex-col gap-3 p-4 rounded-xl bg-slate-950/40">
              <h3 className="text-xs font-medium flex items-center gap-2 text-slate-200">
                <Icon className={cn("w-4 h-4", iconColor)} />
                {label}
              </h3>
              <p className="text-[11px] text-slate-500 leading-relaxed flex-1">{desc}</p>
              <button
                type="button"
                onClick={() => runAction(id, label)}
                disabled={!!loading}
                className={cn(
                  "w-full py-2 rounded-lg text-[11px] font-medium flex items-center justify-center gap-2 transition-colors disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none",
                  btnClass
                )}
              >
                {loading === id
                  ? <><RefreshCw className="w-3 h-3 animate-spin" /> Executing...</>
                  : `Execute ${label.split(' ')[0]}`
                }
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Host Resources Viewer */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="p-6 rounded-2xl bg-slate-950/40">
          <div className="flex items-center gap-2 mb-4">
            <h3 className="text-xs font-medium text-slate-400 flex items-center gap-2">
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
        <div className="p-6 rounded-2xl bg-slate-950/40">
          <div className="flex items-center gap-2 mb-4">
            <h3 className="text-xs font-medium text-slate-400 flex items-center gap-2">
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
                <div className="h-full bg-emerald-500/50 transition-all duration-1000" style={{ width: `${sysStatus?.system?.memory_percent ?? 0}%` }} />
              </div>
              <p className="text-[10px] text-slate-600 mt-2 font-mono text-center italic">
                {sysStatus?.system?.used_memory_mb ?? 0} MB / {sysStatus?.system?.total_memory_mb ?? 0} MB RAM detected
              </p>
            </>
          )}
        </div>
      </div>

      {/* Info footer */}
      <div className="flex items-start gap-3 p-4 rounded-2xl bg-slate-900/30">
        <Database className="w-4 h-4 text-slate-600 shrink-0 mt-0.5" />
        <p className="text-[11px] text-slate-600 leading-relaxed">
          Maintenance operations target <span className="text-slate-400">SilvaDB</span> (knowledge graph)
          and <span className="text-slate-400">HybridMemory</span> (FTS5 + vector search).
          VACUUM and CHECKPOINT are safe in production. DECAY is irreversible — reduces stale memory weights.
          It is recommended to run CHECKPOINT before VACUUM to ensure consistency.
        </p>
      </div>

      {/* Token + Config Section */}
      <div className="rounded-2xl bg-slate-900/50 p-6">
        <h3 className="text-xs font-medium text-slate-400 flex items-center gap-2 mb-4">
          <ShieldCheck className="w-4 h-4 text-violet-400" /> Admin Access
        </h3>
        <div className="flex gap-3">
          <input
            type="password"
            title="API management token"
            placeholder="Bearer token"
            id="nexus-token-input"
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const input = e.currentTarget as HTMLInputElement;
                document.cookie = `nexus_token=${input.value}; path=/`;
                window.dispatchEvent(new CustomEvent('nexus_token_update', { detail: input.value }));
              }
            }}
            className="flex-1 px-3 py-2 bg-slate-950 rounded-xl text-xs font-mono text-slate-300"
          />
          <button type="button" onClick={() => { const i = document.getElementById('nexus-token-input') as HTMLInputElement; if (i) { document.cookie = `nexus_token=${i.value}; path=/`; window.dispatchEvent(new CustomEvent('nexus_token_update', { detail: i.value })); notify('Token updated', 'info'); }}} className="px-4 py-2 bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 rounded-xl text-xs font-medium focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none">
            Save Token
          </button>
        </div>
        <div className="mt-3 text-[10px] text-slate-600">
          Press Enter to save. Token used for protected kernel APIs.
        </div>
      </div>
    </div>
  );
}
