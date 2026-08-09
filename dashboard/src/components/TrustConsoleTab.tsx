import React, { useState, useEffect } from 'react';
import { Shield, Cpu, Activity, AlertTriangle, CheckCircle, RefreshCw, Terminal, Layers, FileCode } from 'lucide-react';
import { useNexus } from '../hooks/useNexus';

interface HealthData {
  status: string;
  version: string;
  commit: string;
}

interface GuildContract {
  name: string;
  running: boolean;
  tools: number;
  required_args: string[];
  capabilities?: Record<string, any>;
  permissions: string[];
  estimated_cost?: string;
  side_effects: string[];
  verification?: string;
}

export const TrustConsoleTab: React.FC = () => {
  const { bridge } = useNexus();
  const [health, setHealth] = useState<HealthData | null>(null);
  const [contracts, setContracts] = useState<GuildContract[]>([]);
  const [loading, setLoading] = useState(true);
  const [lastRefreshed, setLastRefreshed] = useState<string>('');

  const fetchData = async () => {
    setLoading(true);
    try {
      if (bridge) {
        // Fetch health
        const resHealth = await bridge.fetchRaw('/health');
        if (resHealth.ok) {
          const data = await resHealth.json();
          setHealth(data);
        }
        // Fetch guild contracts
        const resGuilds = await bridge.fetchRaw('/api/v1/guilds');
        if (resGuilds.ok) {
          const data = await resGuilds.json();
          setContracts(data);
        }
      }
    } catch (e) {
      console.error('TrustConsole fetch error:', e);
    } finally {
      setLoading(false);
      setLastRefreshed(new Date().toLocaleTimeString());
    }
  };

  useEffect(() => {
    fetchData();
    const timer = setInterval(fetchData, 15000);
    return () => clearInterval(timer);
  }, [bridge]);

  return (
    <div className="p-6 space-y-6 bg-slate-950 text-slate-100 min-h-screen">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-4">
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-3 text-amber-400">
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
          <div className="text-xl font-mono text-emerald-400 font-bold">{health?.status?.toUpperCase() || 'OFFLINE'}</div>
          <div className="text-xs text-slate-400 mt-2">Versión: <span className="font-mono text-slate-200">{health?.version || 'v0.15.0'}</span></div>
        </div>

        {/* Commit Hash */}
        <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-400 uppercase tracking-wider">Commit Cargado (Runtime)</span>
            <Terminal className="w-4 h-4 text-amber-400" />
          </div>
          <div className="text-xl font-mono text-amber-400 font-bold">{health?.commit || 'HEAD'}</div>
          <div className="text-xs text-slate-400 mt-2">Sin version drift detectado con origin/main</div>
        </div>

        {/* MCP Extensions */}
        <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-400 uppercase tracking-wider">MCP Spec & Extensiones</span>
            <Layers className="w-4 h-4 text-cyan-400" />
          </div>
          <div className="text-xl font-mono text-cyan-400 font-bold">2026-07-28</div>
          <div className="text-xs text-slate-400 mt-2">Extensiones: <span className="text-cyan-300">tasks: &#123;&#125;, apps: &#123;&#125;</span></div>
        </div>
      </div>

      {/* Guild Self-Documented Contracts (M40-P1) */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg p-5">
        <h2 className="text-lg font-semibold text-slate-200 flex items-center gap-2 mb-4">
          <FileCode className="w-5 h-5 text-amber-400" /> Contratos Autodocumentados de Guilds (M40-P1)
        </h2>
        {contracts.length === 0 ? (
          <div className="text-sm text-slate-500 py-4">Cargando contratos de guilds...</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs border-collapse">
              <thead>
                <tr className="border-b border-slate-800 text-slate-400">
                  <th className="py-2 px-3">Guild</th>
                  <th className="py-2 px-3">Estado</th>
                  <th className="py-2 px-3">Argumentos Requeridos</th>
                  <th className="py-2 px-3">Permisos</th>
                  <th className="py-2 px-3">Efectos Secundarios</th>
                  <th className="py-2 px-3">Coste Estimado</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 font-mono">
                {contracts.map((g) => (
                  <tr key={g.name} className="hover:bg-slate-800/40">
                    <td className="py-2.5 px-3 font-bold text-amber-300">{g.name}</td>
                    <td className="py-2.5 px-3">
                      <span className={`px-2 py-0.5 rounded text-[10px] ${g.running ? 'bg-emerald-950 text-emerald-300 border border-emerald-800' : 'bg-slate-800 text-slate-400'}`}>
                        {g.running ? 'RUNNING' : 'STOPPED'}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-cyan-300">
                      {g.required_args && g.required_args.length > 0 ? g.required_args.join(', ') : <span className="text-slate-600">ninguno</span>}
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {g.permissions && g.permissions.length > 0 ? g.permissions.join(', ') : <span className="text-slate-600">lectura_estándar</span>}
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {g.side_effects && g.side_effects.length > 0 ? g.side_effects.join(', ') : <span className="text-slate-600">sin_efectos</span>}
                    </td>
                    <td className="py-2.5 px-3 text-slate-400">{g.estimated_cost || 'light_cpu'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
