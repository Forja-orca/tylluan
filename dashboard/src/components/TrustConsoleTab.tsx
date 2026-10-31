import React, { useState, useEffect, useCallback } from 'react';
import { Shield, Activity, CheckCircle, AlertTriangle, RefreshCw, Terminal, Layers, FileCode, Wrench } from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import { usePolling } from '../hooks/usePolling';
import { cn } from '../lib/utils';
import { RecallHealthCard, RecallTelemetryData } from './RecallHealthCard';
import { QuarantinePanel } from './QuarantinePanel';

// Mirrors the real `/health?verbose=true` body built in
// crates/tylluan-kernel/src/transport/http/mod.rs:1381-1402 (components block).
interface HealthData {
  status: string;
  version: string;
  commit: string;
  components?: {
    sovereign_tools?: {
      ok: boolean;
      count: number;
    };
    kernel?: { ok: boolean };
    embeddings?: { ok: boolean; model?: string };
    silva?: { ok: boolean; nodes?: number; edges?: number };
    guilds?: { ok: boolean; active?: number; total?: number };
    recall?: RecallTelemetryData;
  };
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
  capabilities?: Record<string, unknown>;
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
        const resHealth = await bridge.fetchRaw('/health?verbose=true') as HealthData;
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

  const sovereignTools = health?.components?.sovereign_tools;
  const sovereignCount = sovereignTools?.count ?? null;
  const sovereignOk = sovereignTools ? (sovereignTools.ok && sovereignTools.count === 6) : null;

  // WCAG-A pre-flight 2026-08-12 (dark theme, exact token vars):
  //  emerald-400 on emerald-700/40: 6.31:1 (RUNNING cell/card) · amber-400 on amber-600/40: 5.42:1
  //  rose-400 on rose-700/40: 4.97:1 (contradicted card) · emerald-400 on slate-900 9.59:1
  return (
    <div className="p-6 space-y-6 bg-slate-950 text-slate-100 rounded border border-slate-800/80">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-slate-800 pb-4">
        <div>
          <h1 className="text-2xl font-bold font-sans flex items-center gap-3 text-amber-400">
            <Shield className="w-7 h-7" /> Trust Console — Trust &amp; Continuity Layer
          </h1>
          <p className="text-sm text-slate-300 mt-1">
            Runtime monitor, CONTRACT-01 sovereign tool invariant, version drift detection (M40-P6) and guild self-documented contracts (M40-P1).
          </p>
        </div>
        <button
          type="button"
          onClick={fetchData}
          disabled={loading}
          aria-label="Refresh Trust Console telemetry"
          className="flex items-center gap-2 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 rounded text-xs text-slate-200 transition cursor-pointer self-start sm:self-auto"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          {lastRefreshed ? `Updated ${lastRefreshed}` : 'Refresh'}
        </button>
      </div>

      {/* Overview Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Kernel Health */}
        <div className="bg-slate-900 border border-slate-800 rounded p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-300">Kernel Status</span>
            <CheckCircle className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-xl font-mono text-emerald-400 font-bold">{health?.status?.toUpperCase() || '—'}</div>
          <div className="text-xs text-slate-300 mt-2">Version: <span className="font-mono text-slate-100">{health?.version || '—'}</span></div>
        </div>

        {/* Commit Hash */}
        <div className="bg-slate-900 border border-slate-800 rounded p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-300">Loaded Commit (Runtime)</span>
            <Terminal className="w-4 h-4 text-amber-400" />
          </div>
          <div className="text-xl font-mono text-amber-400 font-bold">{health?.commit?.slice(0, 12) || '—'}</div>
          <div className="text-xs text-slate-300 mt-2">Commit loaded by the runtime kernel</div>
        </div>

        {/* Sovereign Tools Invariant (CONTRACT-01 / ADR-021 / D5) */}
        <div
          aria-label="Sovereign tools CONTRACT-01 status"
          className={`rounded p-4 border ${
            sovereignOk === false
              ? 'bg-rose-950/50 border-rose-500/60'
              : 'bg-slate-900 border-slate-800'
          }`}
        >
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-300">Sovereign Tools (CONTRACT-01)</span>
            {sovereignOk === false ? (
              <AlertTriangle className="w-4 h-4 text-rose-400" aria-hidden="true" />
            ) : (
              <Wrench className="w-4 h-4 text-emerald-400" aria-hidden="true" />
            )}
          </div>
          <div
            className={`text-xl font-mono font-bold ${
              sovereignOk === false ? 'text-rose-400' : 'text-emerald-400'
            }`}
          >
            {sovereignCount !== null ? `${sovereignCount} / 6` : '— / 6'}
            <span
              className={cn(
                'ml-2 px-2 py-0.5 text-xs font-semibold rounded border',
                sovereignOk === false
                  ? 'bg-rose-500/15 text-rose-300 border-rose-500/30'
                  : sovereignOk === true
                  ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30'
                  : 'bg-slate-800 text-slate-400 border-slate-700'
              )}
            >
              {sovereignOk === false ? 'DRIFT ALERT' : sovereignOk === true ? 'VERIFIED' : 'PENDING'}
            </span>
          </div>
          <div className={`text-xs mt-2 ${sovereignOk === false ? 'text-rose-300 font-medium' : 'text-slate-300'}`}>
            {sovereignOk === false
              ? `Invariant violation: expected 6 tools (ok=true), got count=${sovereignCount} (ok=${String(sovereignTools?.ok)})`
              : 'do · remember · recall · think · graph · decide'}
          </div>
        </div>

        {/* MCP Spec & Extensiones */}
        <div className="bg-slate-900 border border-slate-800 rounded p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-300">MCP Spec &amp; Extensions</span>
            <Layers className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-xl font-mono text-emerald-400 font-bold">SSE</div>
          <div className="text-xs text-slate-300 mt-2">Endpoint: <span className="font-mono text-emerald-400">/sse</span> · Extensions: <span className="text-emerald-400">tasks: &#123;&#125;, apps: &#123;&#125;</span></div>
        </div>
      </div>

      {/* Salud del recall (LOTE AD — D-5 telemetry) */}
      <RecallHealthCard
        recall={health?.components?.recall}
        isAbsent={health?.components !== undefined && !('recall' in health.components)}
      />

      {/* Cuarentena de Memoria (LOTE AE — ASI06 Judge & D-12) */}
      <QuarantinePanel bridge={bridge} />

      {/* Guild Self-Documented Contracts (M40-P1) */}
      <div className="bg-slate-900 border border-slate-800 rounded p-5">
        <h2 className="text-lg font-semibold font-sans text-slate-200 flex items-center gap-2 mb-4">
          <FileCode className="w-5 h-5 text-amber-400" /> Guild Self-Documented Contracts (M40-P1)
        </h2>
        {contracts.length === 0 ? (
          <div className="text-sm text-slate-500 py-4">{loading ? 'Loading guild contracts...' : 'No guilds registered'}</div>
        ) : (
          <div
            tabIndex={0}
            role="region"
            aria-label="Tabla de contratos de guilds"
            className="overflow-x-auto focus-visible:ring-1 focus-visible:ring-amber-400 focus-visible:outline-none"
          >
            <table className="w-full text-left text-xs border-collapse">
              <thead>
                <tr className="border-b border-slate-800 text-slate-400">
                  <th className="py-2 px-3">Guild</th>
                  <th className="py-2 px-3">Status</th>
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
                      <span className={`px-2 py-0.5 rounded text-[11px] ${g.running ? 'bg-emerald-700/40 text-emerald-400 border border-emerald-700/40' : 'bg-slate-800 text-slate-400'}`}>
                        {g.running ? 'RUNNING' : 'STOPPED'}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {g.launcher_type || <span className="text-slate-600 dark:text-slate-400">—</span>}
                    </td>
                    <td className="py-2.5 px-3 text-emerald-400">
                      {g.tools_count}
                      {g.always_on && <span className="text-[11px] text-slate-500 ml-1">always_on</span>}
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {g.agent_roles && g.agent_roles.length > 0 ? g.agent_roles.join(', ') : <span className="text-slate-600 dark:text-slate-400">all</span>}
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
      <div className="bg-slate-900 border border-slate-800 rounded p-5">
        <h2 className="text-lg font-semibold font-sans text-slate-200 flex items-center gap-2 mb-3">
          <Activity className="w-5 h-5 text-emerald-400" /> Memory Status & Evidence Matrix (M40-P4)
        </h2>
        <p className="text-xs text-slate-400 mb-4">
          Explicit status dynamically derived on SilvaDB on each call to <span className="font-mono text-amber-400">tylluan_recall</span>:
        </p>
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3 text-xs font-mono">
          <div className="p-3 bg-emerald-700/40 border border-emerald-700/60 rounded">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-emerald-400">status="confirmed"</span>
              <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">Verified fact, high confidence, no active conflict.</p>
          </div>

          <div className="p-3 bg-amber-600/40 border border-amber-700/60 rounded">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-amber-400">status="provisional"</span>
              <span className="w-2 h-2 rounded-full bg-amber-400"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">Recent entry or initial low confidence level.</p>
          </div>

          <div className="p-3 bg-rose-700/40 border border-rose-700/60 rounded">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-rose-400">status="contradicted"</span>
              <span className="w-2 h-2 rounded-full bg-rose-400"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">Under explicit conflict registered in SilvaDB.</p>
          </div>

          <div className="p-3 bg-slate-800/40 border border-slate-700/60 rounded">
            <div className="flex items-center justify-between mb-1">
              <span className="font-bold text-slate-400">status="superseded"</span>
              <span className="w-2 h-2 rounded-full bg-slate-500"></span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">Historical fact superseded by a more recent version.</p>
          </div>
        </div>
      </div>
    </div>
  );
};
