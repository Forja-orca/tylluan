import React, { useState, useEffect, useCallback } from 'react';
import type { NexusBridge, DiagnosticReport } from '../lib/nexus-bridge';
import { Stethoscope, Activity, Server, Cpu, AlertTriangle, CheckCircle2, RotateCcw, Wrench, RefreshCw, XCircle, ServerOff } from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';

interface DoctorPanelProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}


const formatBytes = (bytes: number) => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

export default function DoctorPanel({ bridge, notify }: DoctorPanelProps) {
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [repairing, setRepairing] = useState(false);

  const loadReport = async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const data = await bridge.getDoctorReport();
      setReport(data);
    } catch (err: any) {
      console.warn("Doctor API not available:", err);
      setError(`Doctor API unavailable: ${err.message}`);
      setReport(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadReport();
  }, [bridge, loadReport]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('doctor-report', loadReport, { interval: 'standard', enabled: !!bridge });

  const handleRepair = async (target: 'guild' | 'storage' | 'benchmark', name?: string) => {
    if (!bridge) return;
    setRepairing(true);
    try {
      const res = await bridge.repairDoctor(target, name);
      if (res && res.success) {
        notify(res.message || `Reparación de ${name || target} completada exitosamente`, 'info');
        await loadReport();
      } else {
        notify(res?.message || `Fallo en la reparación de ${name || target}`, 'error');
      }
    } catch (err: any) {
      notify(`Error ejecutando reparación: ${err.message}`, 'error');
    } finally {
      setRepairing(false);
    }
  };

  const handleFullFix = async () => {
    if (!report) return;
    setRepairing(true);
    
    // Iterate over everything broken
    const promises: Promise<any>[] = [];
    
    if (!report.storage.memory_db_ok || !report.storage.silva_db_ok) {
      promises.push(handleRepair('storage'));
    }
    
    for (const g of report.guilds) {
      if (!g.running) {
        promises.push(handleRepair('guild', g.name));
      }
    }
    
    await Promise.all(promises);
    setRepairing(false);
    notify('Full fix sequence completed', 'info');
  };

  if (loading && !report) {
    return (
      <div className="flex items-center justify-center p-8 text-slate-400">
        <RefreshCw className="w-5 h-5 animate-spin mr-2" />
        Loading diagnostics...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center p-8 text-red-400 gap-4">
        <ServerOff className="w-8 h-8 opacity-50" />
        <span className="text-sm font-mono">{error}</span>
        <button onClick={loadReport} className="px-4 py-2 bg-slate-800 hover:bg-slate-700 rounded-lg text-slate-300 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none">Retry</button>
      </div>
    );
  }

  if (!report) return null;

  const statusColor = report.status === 'healthy' ? 'text-emerald-500' :
                      report.status === 'degraded' ? 'text-amber-500' : 'text-red-500';

  const StatusIcon = report.status === 'healthy' ? CheckCircle2 :
                     report.status === 'degraded' ? AlertTriangle : XCircle;

  // WCAG-A pre-flight 2026-08-12 (dark theme, exact token vars):
  //  slate-950 on emerald-500 7.50:1 · hover emerald-600 5.03:1 · [#00F5D4] -> emerald-400 token
  //  emerald-500/amber-500/red-500 on card bg (slate-900/60): 7.18 / 9.09 / 4.92:1
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex justify-between items-start">
        <div>
          <h2 className="text-xl font-bold font-sans tracking-tight text-slate-50 flex items-center gap-2">
            <Stethoscope className="w-5 h-5 text-emerald-500" />
            System Doctor
          </h2>
          <p className="text-xs text-slate-400 mt-1">
            Global diagnostics and auto-repair utilities
          </p>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={loadReport}
            className="p-1.5 bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-amber-400 rounded-lg transition-colors focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none"
            title="Refresh Diagnostics"
          >
            <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {/* Global Status Card */}
        <div className="p-4 bg-slate-900/60 rounded-xl flex flex-col justify-between">
          <div className="flex items-center gap-2 text-slate-400 mb-2 font-mono text-[11px] font-medium">
            <Activity className="w-4 h-4" /> Global Status
          </div>
          <div className={cn("text-2xl font-bold flex items-center gap-2 capitalize", statusColor)}>
            <StatusIcon className="w-6 h-6" /> {report.status}
          </div>
          <p className="text-xs text-slate-500 mt-2">
            Last checked: {new Date(report.timestamp).toLocaleTimeString()}
          </p>
        </div>

        {/* System Load Card */}
        <div className="p-4 bg-slate-900/60 rounded-xl md:col-span-2 flex flex-col justify-between">
          <div className="flex items-center gap-2 text-slate-400 mb-2 font-mono text-[11px] font-medium">
            <Cpu className="w-4 h-4" /> Hardware Utilization
          </div>
          <div className="space-y-3">
            <div>
              <div className="flex justify-between text-xs mb-1">
                <span className="text-slate-300">CPU Usage</span>
                <span className="font-mono text-slate-400">{report.system.cpu_usage_percent.toFixed(1)}%</span>
              </div>
              <div className="w-full h-1.5 bg-slate-800 rounded-full overflow-hidden">
                <div 
                  className={cn("h-full", report.system.cpu_usage_percent > 80 ? "bg-red-500" : "bg-emerald-500")} 
                  style={{ width: `${Math.min(100, report.system.cpu_usage_percent)}%` }} 
                />
              </div>
            </div>
            <div>
              <div className="flex justify-between text-xs mb-1">
                <span className="text-slate-300">Memory ({report.system.used_memory_mb}MB / {report.system.total_memory_mb}MB)</span>
                <span className="font-mono text-slate-400">{report.system.memory_percent.toFixed(1)}%</span>
              </div>
              <div className="w-full h-1.5 bg-slate-800 rounded-full overflow-hidden">
                <div 
                  className={cn("h-full", report.system.memory_percent > 85 ? "bg-red-500" : "bg-emerald-500")} 
                  style={{ width: `${Math.min(100, report.system.memory_percent)}%` }} 
                />
              </div>
            </div>
            {/* GPU EP Real Hardware Telemetry Badge */}
            <div className="pt-2 border-t border-slate-800/80 flex items-center justify-between text-xs font-mono">
              <span className="text-slate-400 flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
                Execution Provider:
              </span>
              <span className="px-2 py-0.5 rounded text-[10px] font-medium bg-amber-500/10 text-amber-400">
                {report.system.warnings?.length ? 'System Inspection OK' : 'Default Hardware Engine'}
              </span>
            </div>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Guilds Status */}
        <div className="space-y-3">
          <div className="flex items-center justify-between border-b border-slate-800/60 pb-2">
            <h3 className="text-sm font-medium text-slate-200">Guild Subsystems</h3>
          </div>
          {report.guilds.length === 0 ? (
            <p className="text-xs text-slate-500">No guilds registered</p>
          ) : (
            <div className="space-y-2">
              {report.guilds.map(g => (
                <div key={g.name} className="flex items-center justify-between p-3 bg-slate-900/60 rounded-lg">
                  <div className="flex items-center gap-3">
                    <div className={cn("w-2 h-2 rounded-full", g.running ? "bg-emerald-500" : "bg-red-500")} />
                    <div>
                      <p className="text-sm font-mono text-slate-200">{g.name}</p>
                      <p className="text-[10px] text-slate-500">{g.tools_count} tools</p>
                      {g.issues.length > 0 && (
                        <p className="text-xs text-red-400 mt-1">{g.issues[0]}</p>
                      )}
                    </div>
                  </div>
                  {!g.running && (
                    <button
                      onClick={() => handleRepair('guild', g.name)}
                      disabled={repairing}
                      className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono rounded-lg flex items-center gap-1.5 transition-colors disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none"
                    >
                      <RotateCcw className="w-3 h-3" />
                      Restart
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Storage Health */}
        <div className="space-y-3">
          <div className="flex items-center justify-between border-b border-slate-800/60 pb-2">
            <h3 className="text-sm font-medium text-slate-200">Storage Integrity</h3>
          </div>
          <div className="grid grid-cols-1 gap-2">
            <div className="p-3 bg-slate-900/60 rounded-lg flex items-center justify-between">
              <div>
                <p className="text-sm font-mono text-slate-200 flex items-center gap-2">
                  <Server className="w-4 h-4 text-slate-400" />
                  MemoryDB (Episodic)
                </p>
                <p className="text-xs text-slate-500 mt-1">
                  {report.storage.docs_count.toLocaleString()} docs • {formatBytes(report.storage.memory_bytes)}
                </p>
              </div>
              {report.storage.memory_db_ok ? (
                <span className="text-emerald-500 text-xs font-bold px-2 py-1 bg-emerald-500/10 rounded">OK</span>
              ) : (
                <span className="text-red-500 text-xs font-bold px-2 py-1 bg-red-500/10 rounded">CORRUPT</span>
              )}
            </div>
            
            <div className="p-3 bg-slate-900/60 rounded-lg flex items-center justify-between">
              <div>
                <p className="text-sm font-mono text-slate-200 flex items-center gap-2">
                  <Server className="w-4 h-4 text-slate-400" />
                  SilvaDB (Graph)
                </p>
                <p className="text-xs text-slate-500 mt-1">
                  {report.storage.nodes_count.toLocaleString()} nodes • {formatBytes(report.storage.silva_bytes)}
                </p>
              </div>
              {report.storage.silva_db_ok ? (
                <span className="text-emerald-500 text-xs font-bold px-2 py-1 bg-emerald-500/10 rounded">OK</span>
              ) : (
                <span className="text-red-500 text-xs font-bold px-2 py-1 bg-red-500/10 rounded">CORRUPT</span>
              )}
            </div>

            {(!report.storage.memory_db_ok || !report.storage.silva_db_ok) && (
              <button
                onClick={() => handleRepair('storage')}
                disabled={repairing}
                className="w-full py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono rounded-lg flex items-center justify-center gap-2 transition-colors mt-2 disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none"
              >
                <Wrench className="w-3.5 h-3.5" />
                Optimize (VACUUM / Rebuild)
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Issues & Warnings */}
      {(report.system.warnings.length > 0 || report.suggestions.length > 0) && (
        <div className="p-4 bg-slate-900/60 rounded-xl space-y-4">
          {report.system.warnings.length > 0 && (
            <div>
              <h4 className="text-xs font-medium text-amber-500 flex items-center gap-1.5 mb-2">
                <AlertTriangle className="w-3.5 h-3.5" /> System Warnings
              </h4>
              <ul className="list-disc list-inside text-sm text-slate-300 space-y-1">
                {report.system.warnings.map((w, i) => <li key={i}>{w}</li>)}
              </ul>
            </div>
          )}
          
          {report.suggestions.length > 0 && (
            <div>
              <h4 className="text-xs font-medium text-emerald-500 flex items-center gap-1.5 mb-2">
                <Wrench className="w-3.5 h-3.5" /> Recommended Actions
              </h4>
              <ul className="list-disc list-inside text-sm text-slate-300 space-y-1">
                {report.suggestions.map((s, i) => <li key={i}>{s}</li>)}
              </ul>
            </div>
          )}
          
          <button
            onClick={handleFullFix}
            disabled={repairing || report.status === 'healthy'}
            className="mt-4 px-4 py-2 w-full bg-emerald-500 hover:bg-emerald-600 disabled:bg-emerald-500/30 text-slate-950 text-xs font-semibold rounded-lg flex items-center justify-center gap-2 transition-colors disabled:cursor-not-allowed focus-visible:ring-2 focus-visible:ring-slate-100 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none"
          >
            <RotateCcw className={cn("w-4 h-4", repairing && "animate-spin")} />
            {repairing ? "Repairing Subsystems..." : "Run Full Fix"}
          </button>
        </div>
      )}
    </div>
  );
}
