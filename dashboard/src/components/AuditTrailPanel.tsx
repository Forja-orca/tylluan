import React, { useState, useEffect, useCallback } from 'react';
import { 
  ShieldCheck, 
  Search, 
  RefreshCw, 
  CheckCircle2, 
  XCircle, 
  Clock, 
  SlidersHorizontal,
  ServerOff
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

export interface AuditEntry {
  agent_id: string;
  guild: string;
  intent_preview: string;
  allowed: boolean;
  timestamp: string;
}

export interface AuditTrailPanelProps {
  bridge: NexusBridge | null;
}

function formatRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSecs = Math.floor(diffMs / 1000);
    const diffMins = Math.floor(diffSecs / 60);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffSecs < 60) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  } catch {
    return dateStr;
  }
}

export default function AuditTrailPanel({ bridge }: AuditTrailPanelProps) {
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [agentIdFilter, setAgentIdFilter] = useState('');
  const [limit, setLimit] = useState(25);
  const [error, setError] = useState<string | null>(null);

  const fetchAuditTrail = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const data = await bridge.getAuditTrail(agentIdFilter || undefined, limit);
      setEntries(data.entries || []);
      setTotal(data.total || (data.entries ? data.entries.length : 0));
    } catch (err: any) {
      console.error("Audit Trail fetch error:", err.message);
      setEntries([]);
      setTotal(0);
      setError(`Error querying audit trail records: ${err.message}`);
    } finally {
      setLoading(false);
    }
  }, [bridge, agentIdFilter, limit]);

  useEffect(() => {
    fetchAuditTrail();
  }, [fetchAuditTrail]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-50 flex items-center gap-2">
            <ShieldCheck className="w-5 h-5 text-emerald-500" />
            Audit Trail Logs
          </h2>
          <p className="text-xs text-slate-400">
            Real-time tracking of security evaluations, execution authorization records, and capability grants.
          </p>
        </div>
        <button
          onClick={fetchAuditTrail}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-900 hover:bg-slate-800 text-xs text-slate-300 font-medium rounded-xl transition-all disabled:opacity-50"
        >
          <RefreshCw className={cn("w-3.5 h-3.5", loading && "animate-spin")} />
          Refresh Logs
        </button>
      </div>

      {/* Control Bar */}
      <div className="flex flex-col sm:flex-row items-center gap-3 p-4 bg-slate-900/60 rounded-2xl">
        <div className="relative w-full sm:max-w-xs">
          <Search className="absolute left-3 top-2.5 w-3.5 h-3.5 text-slate-500" />
          <input
            type="text"
            value={agentIdFilter}
            onChange={(e) => setAgentIdFilter(e.target.value)}
            placeholder="Filter by Agent ID..."
            className="w-full pl-9 pr-4 py-2 bg-slate-950 focus:border-emerald-500 focus:outline-none rounded-xl text-xs text-slate-200 placeholder-slate-500 font-mono transition-colors"
          />
        </div>
        
        <div className="flex items-center gap-2 ml-auto w-full sm:w-auto justify-end">
          <SlidersHorizontal className="w-3.5 h-3.5 text-slate-500" />
          <select
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="bg-slate-950 rounded-xl px-3 py-2 text-xs text-slate-300 font-mono focus:border-emerald-500 focus:outline-none"
          >
            <option value={10}>10 records</option>
            <option value={25}>25 records</option>
            <option value={50}>50 records</option>
            <option value={100}>100 records</option>
          </select>
        </div>
      </div>

      {/* Table Container */}
      <div className="bg-slate-900/40 rounded-2xl overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="border-b border-slate-800 bg-slate-900/60">
                <th className="px-6 py-3.5 text-[11px] font-medium text-slate-400">Agent ID</th>
                <th className="px-6 py-3.5 text-[11px] font-medium text-slate-400">Guild</th>
                <th className="px-6 py-3.5 text-[11px] font-medium text-slate-400">Intent (Action Description)</th>
                <th className="px-6 py-3.5 text-[11px] font-medium text-slate-400 text-center">Status</th>
                <th className="px-6 py-3.5 text-[11px] font-medium text-slate-400 text-right">Time</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-850">
              {entries.length > 0 ? (
                entries.map((entry, idx) => (
                  <tr 
                    key={idx} 
                    className="hover:bg-slate-900/50 odd:bg-slate-900/10 transition-colors"
                  >
                    <td className="px-4 py-3.5">
                      <span className={cn(
                        "inline-flex items-center px-2 py-0.5 rounded font-mono text-[10px] font-medium leading-none",
                        entry.agent_id === 'unverified'
                          ? "bg-slate-800/60 text-slate-400 border-slate-750"
                          : "bg-emerald-500/10 text-emerald-300 border-emerald-500/20"
                      )}>
                        {entry.agent_id === 'unverified' ? '@internal' : `@${entry.agent_id}`}
                      </span>
                    </td>
                    <td className="px-4 py-3.5">
                      <span className="inline-flex items-center text-[11px] text-emerald-400 font-mono font-bold tracking-tight">
                        {entry.guild}
                      </span>
                    </td>
                    <td className="px-4 py-3.5">
                      <div className="max-w-xs md:max-w-sm lg:max-w-md truncate text-[11px] text-slate-300 font-mono font-medium leading-normal" title={entry.intent_preview}>
                        {entry.intent_preview}
                      </div>
                    </td>
                    <td className="px-4 py-3.5 text-center">
                      <span className={cn(
                        "inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium font-mono leading-none whitespace-nowrap",
                        entry.allowed
                          ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                          : "bg-red-500/10 text-red-400 border-red-500/20"
                      )}>
                        {entry.allowed ? (
                          <>
                            <CheckCircle2 className="w-2.5 h-2.5" />
                            Allowed
                          </>
                        ) : (
                          <>
                            <XCircle className="w-2.5 h-2.5" />
                            Blocked
                          </>
                        )}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-right">
                      <div className="flex items-center justify-end gap-1.5 text-slate-400 font-mono text-[10px]">
                        <Clock className="w-3.5 h-3.5 text-slate-500" />
                        <span>{formatRelativeTime(entry.timestamp)}</span>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={5} className="px-6 py-12 text-center text-slate-500 font-mono text-xs leading-normal">
                    No audit records matching filters found.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        
        {/* Footer info */}
        <div className="px-6 py-3 border-t border-slate-800 bg-slate-900/20 flex justify-between items-center text-[10px] text-slate-500 font-mono">
          <span>Showing {entries.length} of {total} records</span>
          <span>Security Sandbox Logs Layer</span>
        </div>
      </div>
    </div>
  );
}
