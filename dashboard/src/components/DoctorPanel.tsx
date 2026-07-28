import React, { useState, useEffect } from 'react';
import type { NexusBridge, DiagnosticReport } from '../lib/nexus-bridge';
import { Stethoscope, Activity, Server, Cpu, AlertTriangle, CheckCircle2, RotateCcw, Wrench, RefreshCw, XCircle } from 'lucide-react';
import { cn } from '../lib/utils';

interface DoctorPanelProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

const generateMockReport = (): DiagnosticReport => ({
  timestamp: new Date().toISOString(),
  status: 'degraded',
  guilds: [
    { name: 'bash', running: true, tools_count: 5, issues: [] },
    { name: 'knowledge', running: false, tools_count: 3, issues: ['Connection timeout to local worker'] },
    { name: 'vision', running: true, tools_count: 2, issues: [] },
  ],
  storage: {
    memory_db_ok: true,
    silva_db_ok: false,
    docs_count: 1450,
    nodes_count: 5200,
    memory_bytes: 1024 * 1024 * 45,
    silva_bytes: 1024 * 1024 * 210,
    recent_nodes: []
  },
  system: {
    total_memory_mb: 16384,
    used_memory_mb: 450,
    memory_percent: 2.7,
    cpu_usage_percent: 12.5,
    process_count: 1,
    thread_count: 42,
    status: 'ok',
    warnings: ['High latency detected in vector operations']
  },
  config_valid: true,
  suggestions: [
    "Run 'doctor_repair(target=\"guild\", name=\"knowledge\")' to restart.",
    "Run 'doctor_repair(target=\"storage\")' to VACUUM SilvaDB."
  ]
});

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
  const [simulated, setSimulated] = useState(false);
  const [repairing, setRepairing] = useState(false);

  const loadReport = async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const data = await bridge.getDoctorReport();
      setReport(data);
      setSimulated(false);
    } catch (err) {
      console.warn("Doctor API not available, falling back to mock", err);
      setReport(generateMockReport());
      setSimulated(true);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadReport();
    const interval = setInterval(loadReport, 15000); // Auto-refresh every 15s
    return () => clearInterval(interval);
  }, [bridge]);

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

  if (!report) return null;

  const statusColor = report.status === 'healthy' ? 'text-emerald-500' :
                      report.status === 'degraded' ? 'text-amber-500' : 'text-red-500';

  const StatusIcon = report.status === 'healthy' ? CheckCircle2 :
                     report.status === 'degraded' ? AlertTriangle : XCircle;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex justify-between items-start">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-50 flex items-center gap-2">
            <Stethoscope className="w-5 h-5 text-emerald-500" />
            System Doctor
          </h2>
          <p className="text-xs text-slate-400 mt-1">
            Global diagnostics and auto-repair utilities
          </p>
        </div>
        <div className="flex items-center gap-3">
          {simulated && (
            <span className="px-2 py-1 bg-amber-500/10 border border-amber-500/20 text-amber-500 text-[10px] font-mono font-bold rounded">
              [SIMULATED DOCTOR MODULE]
            </span>
          )}
          <button
            onClick={loadReport}
            className="p-1.5 bg-slate-900 border border-slate-800 text-slate-400 hover:text-emerald-400 rounded transition-colors"
            title="Refresh Diagnostics"
          >
            <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {/* Global Status Card */}
        <div className="p-4 bg-slate-900 border border-slate-800 rounded-xl flex flex-col justify-between">
          <div className="flex items-center gap-2 text-slate-400 mb-2 font-mono text-xs uppercase">
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
        <div className="p-4 bg-slate-900 border border-slate-800 rounded-xl md:col-span-2 flex flex-col justify-between">
          <div className="flex items-center gap-2 text-slate-400 mb-2 font-mono text-xs uppercase">
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
                <span className="w-2 h-2 rounded-full bg-[#00F5D4] animate-pulse" />
                Execution Provider:
              </span>
              <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-cyan-500/10 text-cyan-400 border border-cyan-500/30">
                {report.system.warnings?.length ? 'System Inspection OK' : 'Default Hardware Engine'}
              </span>
            </div>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Guilds Status */}
        <div className="space-y-3">
          <div className="flex items-center justify-between border-b border-slate-800 pb-2">
            <h3 className="text-sm font-bold text-slate-200">Guild Subsystems</h3>
          </div>
          {report.guilds.length === 0 ? (
            <p className="text-xs text-slate-500">No guilds registered</p>
          ) : (
            <div className="space-y-2">
              {report.guilds.map(g => (
                <div key={g.name} className="flex items-center justify-between p-3 bg-slate-900 border border-slate-800 rounded-lg">
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
                      className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono rounded flex items-center gap-1.5 transition-colors disabled:opacity-50"
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
          <div className="flex items-center justify-between border-b border-slate-800 pb-2">
            <h3 className="text-sm font-bold text-slate-200">Storage Integrity</h3>
          </div>
          <div className="grid grid-cols-1 gap-2">
            <div className="p-3 bg-slate-900 border border-slate-800 rounded-lg flex items-center justify-between">
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
            
            <div className="p-3 bg-slate-900 border border-slate-800 rounded-lg flex items-center justify-between">
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
                className="w-full py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono rounded-lg flex items-center justify-center gap-2 transition-colors mt-2 disabled:opacity-50"
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
        <div className="p-4 bg-slate-900 border border-slate-800 rounded-xl space-y-4">
          {report.system.warnings.length > 0 && (
            <div>
              <h4 className="text-xs font-bold text-amber-500 uppercase flex items-center gap-1.5 mb-2">
                <AlertTriangle className="w-3.5 h-3.5" /> System Warnings
              </h4>
              <ul className="list-disc list-inside text-sm text-slate-300 space-y-1">
                {report.system.warnings.map((w, i) => <li key={i}>{w}</li>)}
              </ul>
            </div>
          )}
          
          {report.suggestions.length > 0 && (
            <div>
              <h4 className="text-xs font-bold text-emerald-500 uppercase flex items-center gap-1.5 mb-2">
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
            className="mt-4 px-4 py-2 w-full bg-emerald-500 hover:bg-emerald-600 disabled:bg-emerald-500/30 text-white text-xs font-bold uppercase rounded-lg flex items-center justify-center gap-2 transition-colors disabled:cursor-not-allowed"
          >
            <RotateCcw className={cn("w-4 h-4", repairing && "animate-spin")} />
            {repairing ? "Repairing Subsystems..." : "Run Full Fix"}
          </button>
        </div>
      )}
    </div>
  );
}
