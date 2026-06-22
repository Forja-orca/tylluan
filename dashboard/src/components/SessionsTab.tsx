import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { 
  Users, 
  RefreshCw, 
  Trash2, 
  Clock, 
  Shield, 
  Activity,
  User,
  ExternalLink,
  Search
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import type { McpSession, NexusBridge } from '../lib/nexus-bridge';
import { cn, relativeTime } from '../lib/utils';

interface SessionWithStatus extends McpSession {
  status: 'active' | 'revoking';
}

export function SessionsTab({ bridge, notify }: { bridge: NexusBridge | null; notify: (msg: string, type?: 'info' | 'error') => void }) {
  const { sessions: globalSessions, loading: globalLoading } = useNexus();
  const [revokingIds, setRevokingIds] = useState<Set<string>>(new Set());
  const [searchTerm, setSearchTerm] = useState('');

  // Note: currentSessionId is not currently exposed by the bridge/SSE.
  // We'll use a placeholder or check for 'dashboard' in client_name.
  const currentSessionId = null; 

  // Session list is now managed by useNexus hook globally

  const revokeSession = async (id: string) => {
    if (!bridge) return;
    setRevokingIds(prev => new Set(prev).add(id));

    try {
      await bridge.revokeSession(id);
      notify(`Session ${id.slice(0, 8)} revoked`, 'info');
    } catch (e) {
      notify(`Failed to revoke session: ${e instanceof Error ? e.message : 'Unknown error'}`, 'error');
    } finally {
      setRevokingIds(prev => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const filteredSessions = useMemo(() => {
    const list = globalSessions || [];
    if (!searchTerm) return list;
    const low = searchTerm.toLowerCase();
    return list.filter(s => 
      (s.id || '').toLowerCase().includes(low) || 
      (s.client_name || '').toLowerCase().includes(low) || 
      (s.agent_id?.toLowerCase().includes(low) ?? false)
    );
  }, [globalSessions, searchTerm]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2 mb-1">
            <Users className="w-5 h-5 text-emerald-400" />
            <h2 className="text-xl font-bold text-white tracking-tight">Access Management</h2>
          </div>
          <p className="text-xs text-slate-500 font-mono uppercase tracking-widest">Active Sovereign Sessions</p>
        </div>

        <div className="flex items-center gap-3">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-600" />
            <input 
              type="text" 
              placeholder="Filter sessions..." 
              value={searchTerm}
              onChange={e => setSearchTerm(e.target.value)}
              className="bg-slate-900 border border-slate-800 rounded-xl pl-9 pr-4 py-2 text-xs focus:outline-none focus:border-emerald-500/50 transition-colors w-64"
            />
          </div>
          <button 
            onClick={() => bridge?.fetchRaw('/api/v1/sessions', {})}
            disabled={globalLoading}
            className="p-2 bg-slate-900 border border-slate-800 rounded-xl hover:bg-slate-800 transition-colors disabled:opacity-50"
          >
            <RefreshCw className={cn("w-4 h-4 text-slate-400", globalLoading && "animate-spin")} />
          </button>
        </div>
      </div>

      {/* Sessions Table */}
      <div className="bg-slate-900/50 rounded-2xl border border-slate-800 overflow-hidden shadow-xl">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="bg-slate-800/30 border-b border-slate-800 text-[10px] font-bold text-slate-500 uppercase tracking-widest">
              <th className="px-6 py-4">Client Name</th>
              <th className="px-6 py-4">Agent Identity</th>
              <th className="px-6 py-4 text-center">Tools</th>
              <th className="px-6 py-4">Last Activity</th>
              <th className="px-6 py-4">Active Guild</th>
              <th className="px-6 py-4 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800">
            {filteredSessions.map((session) => (
              <tr key={session.id} className="group hover:bg-slate-800/20 transition-colors">
                <td className="px-6 py-4">
                  <div className="flex items-center gap-3">
                    <div className="w-8 h-8 rounded-lg bg-slate-800 flex items-center justify-center border border-slate-700 text-slate-400">
                      <User className="w-4 h-4" />
                    </div>
                    <div>
                      <div className="text-sm font-bold text-slate-200">{session.client_name}</div>
                      <div className="text-[10px] font-mono text-slate-600 uppercase tracking-tighter">ID: {session.id.slice(0, 13)}...</div>
                    </div>
                  </div>
                </td>
                <td className="px-6 py-4">
                  {session.agent_id ? (
                    <div className="flex items-center gap-2">
                      <Shield className="w-3.5 h-3.5 text-emerald-500/70" />
                      <span className="text-xs font-mono text-emerald-400/80 truncate max-w-[150px]">{session.agent_id}</span>
                    </div>
                  ) : (
                    <span className="text-xs text-slate-600 italic">No identity bound</span>
                  )}
                </td>
                <td className="px-6 py-4 text-center">
                  <span className="px-2 py-1 rounded bg-slate-800 border border-slate-700 text-xs font-mono text-slate-300">
                    {session.tool_count}
                  </span>
                </td>
                <td className="px-6 py-4">
                  <div className="flex items-center gap-2 text-xs text-slate-400 font-mono">
                    <Clock className="w-3.5 h-3.5 text-slate-600" />
                    {relativeTime(session.last_active_unix)}
                  </div>
                </td>
                <td className="px-6 py-4">
                  {session.last_guild ? (
                    <div className="flex items-center gap-2">
                      <Activity className="w-3.5 h-3.5 text-blue-500/70" />
                      <span className="text-xs font-mono text-blue-400/80">{session.last_guild}</span>
                    </div>
                  ) : (
                    <span className="text-xs text-slate-700">—</span>
                  )}
                </td>
                <td className="px-6 py-4 text-right">
                  {session.id === currentSessionId ? (
                    <span className="text-[10px] font-bold text-emerald-500/40 uppercase tracking-widest px-3 py-1.5 border border-emerald-500/10 rounded-lg">Current Session</span>
                  ) : (
                    <button
                      onClick={() => revokeSession(session.id)}
                      disabled={revokingIds.has(session.id)}
                      className={cn(
                        "p-2 rounded-xl transition-all",
                        revokingIds.has(session.id) 
                          ? "bg-slate-800 cursor-not-allowed" 
                          : "bg-red-500/10 hover:bg-red-500/20 text-red-400 border border-red-500/20 hover:border-red-500/40"
                      )}
                      title="Revoke Session"
                    >
                      {revokingIds.has(session.id) ? (
                        <RefreshCw className="w-4 h-4 animate-spin opacity-50" />
                      ) : (
                        <Trash2 className="w-4 h-4" />
                      )}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>

        {(filteredSessions.length === 0 && !globalLoading) && (
          <div className="py-24 flex flex-col items-center justify-center text-slate-600 bg-slate-900/20">
            <Users className="w-12 h-12 mb-4 opacity-20" />
            <p className="text-sm font-medium italic">{searchTerm ? 'No sessions match your filter' : 'No active MCP sessions'}</p>
            {!searchTerm && <p className="text-[10px] mt-2 text-slate-500 max-w-xs">Sessions appear here when an MCP client connects to the kernel. Connect a client to start a session.</p>}
            {searchTerm && <p className="text-[10px] mt-1 uppercase tracking-widest opacity-50">Adjust your filter or search term</p>}
          </div>
        )}
      </div>

      {/* Footer Info */}
      <div className="flex items-center gap-4 p-4 rounded-2xl bg-amber-500/5 border border-amber-500/10">
        <Shield className="w-5 h-5 text-amber-500/60 shrink-0" />
        <div className="text-[11px] text-slate-500 leading-relaxed">
          <span className="font-bold text-amber-500/80 uppercase mr-2">Security Note:</span>
          Revoking a session will immediately disconnect the associated client. All pending tool calls from that session will be aborted.
          Sessions automatically expire after 1 hour of inactivity.
        </div>
      </div>
    </div>
  );
}
