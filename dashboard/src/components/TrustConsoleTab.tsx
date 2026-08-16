import React, { useState, useEffect, useCallback } from 'react';
import { Shield, Cpu, Activity, AlertTriangle, CheckCircle, RefreshCw, Terminal, Layers, FileCode } from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import { usePolling } from '../hooks/usePolling';

interface HealthData {
  status: string;
  version: string;
  commit: string;
}

// Mirrors crates/tylluan-kernel/src/registry/guild_process.rs GuildStatus (real fields).
interface GuildContract {
  name: string;
  running: boolean;
  always_on: boolean;
  tools_count: number;
  idle_seconds: number;
  restarts_5m: number;
  total_calls: number;
  last_latency_ms?: number | null;
  launcher_type: string;
  capabilities?: Record<string, any>;
  agent_roles: string[];
}

export const TrustConsoleTab: React.FC = () => {
  const { bridge } = useNexus();
  const [health, setHealth] = useState<HealthData | null>(null);
  const [contracts, setContracts] = useState<GuildContract[]>([]);
  const [loading, setLoading] = useState(true);
  const [lastRefreshed, setLastRefreshed] = useState<string>('');

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      if (bridge) {
        // fetchRaw returns the parsed JSON body (not a Response) — no .ok / .json()
        const resHealth = await bridge.fetchRaw('/health') as HealthData;
        if (resHealth?.status) setHealth(resHealth);
        const resGuilds = await bridge.fetchRaw('/api/v1/guilds');
        if (Array.isArray(resGuilds)) setContracts(resGuilds as GuildContract[]);
      }
    } catch (e) {
      console.error('TrustConsole fetch error:', e);
    } finally {
      setLoading(false);
      setLastRefreshed(new Date().toLocaleTimeString());
    }
  }, [bridge]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('trust-console', fetchData, { interval: 'medium', enabled: !!bridge });

  // WCAG-A pre-flight 2026-08-12 (dark theme, exact token vars):
  //  emerald-400 on emerald-700/40: 6.31:1 (RUNNING cell/card) · amber-400 on amber-600/40: 5.42:1
  //  rose-400 on rose-700/40: 4.97:1 (contradicted card) · emerald-400 on slate-900 9.59:1
  return (
    <div className="p-6 space-y-6 bg-slate-950 text-slate-100 min-h-screen">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-4">
        <div>
          <h1 className="text-2xl font-bold font-sans flex items-center gap-3 text-amber-400">
            <Shield className="w-7 h-7" /> Trust Console — Capa de Confianza & Continuidad
          </h1>
          <p className="text-sm text-slate-400 mt-1">
            Monitor de runtime, detección de version drift (M40-P6) y contratos autodocumentados de guilds (M40-P1).
          </p>
        </div>
        <button
          onClick={fetchData}
          disabled={loading}
          className="flex items-center gap-2 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 rounded text-xs text-slate-300 transition"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          {lastRefreshed ? `Actualizado ${lastRefreshed}` : 'Actualizar'}
        </button>
      </div>

      {/* Overview Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {/* Kernel Health */}
        <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-400 uppercase tracking-wider">Estado del Kernel</span>
            <CheckCircle className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-xl font-mono text-emerald-400 font-bold">{health?.status?.toUpperCase() || '—'}</div>
          <div className="text-xs text-slate-400 mt-2">Versión: <span className="font-mono text-slate-200">{health?.version || '—'}</span></div>
        </div>

        {/* Commit Hash */}
        <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-400 uppercase tracking-wider">Commit Cargado (Runtime)</span>
            <Terminal className="w-4 h-4 text-amber-400" />
          </div>
          <div className="text-xl font-mono text-amber-400 font-bold">{health?.commit?.slice(0, 12) || '—'}</div>
          <div className="text-xs text-slate-400 mt-2">Commit cargado por el kernel en runtime</div>
        </div>

        {/* MCP Spec & Extensiones */}
        <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-400 uppercase tracking-wider">MCP Spec & Extensiones</span>
            <Layers className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-xl font-mono text-emerald-400 font-bold">SSE</div>
          <div className="text-xs text-slate-400 mt-2">Endpoint: <span className="font-mono text-emerald-400">/sse</span> · Extensiones: <span className="text-emerald-400">tasks: &#123;&#125;, apps: &#123;&#125;</span></div>
        </div>
      </div>

      {/* Guild Self-Documented Contracts (M40-P1) */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg p-5">
        <h2 className="text-lg font-semibold font-sans text-slate-200 flex items-center gap-2 mb-4">
          <FileCode className="w-5 h-5 text-amber-400" /> Contratos Autodocumentados de Guilds (M40-P1)
        </h2>
        {contracts.length === 0 ? (
          <div className="text-sm text-slate-500 py-4">{loading ? 'Cargando contratos de guilds...' : 'Sin guilds registradas'}</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs border-collapse">
              <thead>
                <tr className="border-b border-slate-800 text-slate-400">
                  <th className="py-2 px-3">Guild</th>
                  <th className="py-2 px-3">Estado</th>
                  <th className="py-2 px-3">Launcher</th>
                  <th className="py-2 px-3">Tools</th>
                  <th className="py-2 px-3">Agent Roles</th>
                  <th className="py-2 px-3">Restarts 5m</th>
                  <th className="py-2 px-3">Total Calls</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 font-mono">
                {contracts.map((g) => (
                  <tr key={g.name} className="hover:bg-slate-800/40">
                    <td className="py-2.5 px-3 font-bold text-amber-400">{g.name}</td>
                    <td className="py-2.5 px-3">
                      <span className={`px-2 py-0.5 rounded text-[10px] ${g.running ? 'bg-emerald-700/40 text-emerald-400 border border-emerald-700/40' : 'bg-slate-800 text-slate-400'}`}>
                        {g.running ? 'RUNNING' : 'STOPPED'}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {g.launcher_type || <span className="text-slate-600">—</span>}
                    </td>
                    <td className="py-2.5 px-3 text-emerald-400">
                      {g.tools_count}
                      {g.always_on && <span className="text-[9px] text-slate-500 ml-1">always_on</span>}
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {g.agent_roles && g.agent_roles.length > 0 ? g.agent_roles.join(', ') : <span className="text-slate-600">todos</span>}
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {g.restarts_5m}
                    </td>
                    <td className="py-2.5 px-3 text-slate-400">{g.total_calls}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Memory Status & Evidence Matrix (M40-P4) */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg p-5">
        <h2 className="text-lg font-semibold font-sans text-slate-200 flex items-center gap-2 mb-3">
          <Activity className="w-5 h-5 text-emerald-400" /> Matriz de Estados de Memoria & Procedencia (M40-P4)
        </h2>
        <p className="text-xs text-slate-400 mb-4">
          Estado explícito derivado dinámicamente sobre SilvaDB en cada llamada a <span className="font-mono text-amber-400">tylluan_recall</span>:
        </p>
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3 text-xs font-mono">
          <div className="p-3 bg-emerald-700/40 border border-emerald-700/60 rounded-lg">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-emerald-400">status="confirmed"</span>
              <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">Hecho verificado, alta confianza, sin conflicto activo.</p>
          </div>

          <div className="p-3 bg-amber-600/40 border border-amber-700/60 rounded-lg">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-amber-400">status="provisional"</span>
              <span className="w-2 h-2 rounded-full bg-amber-400"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">Registro reciente o bajo nivel de confianza inicial.</p>
          </div>

          <div className="p-3 bg-rose-700/40 border border-rose-700/60 rounded-lg">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-rose-400">status="contradicted"</span>
              <span className="w-2 h-2 rounded-full bg-rose-400"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">En conflicto explícito registrado en SilvaDB.</p>
          </div>

          <div className="p-3 bg-slate-800/40 border border-slate-700/60 rounded-lg">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-slate-400">status="superseded"</span>
              <span className="w-2 h-2 rounded-full bg-slate-500"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">Hecho histórico superado por una versión más reciente.</p>
          </div>
        </div>
      </div>
    </div>
  );
};
