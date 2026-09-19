import { useState, useEffect, useCallback } from 'react';
import { ShieldCheck, Check, X, Clock, Terminal, Hash, User, AlertTriangle, RefreshCw, Loader2, Copy } from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import { agentStyle } from '../lib/agent-meta';

export interface PendingDispatch {
  id: string;
  agent_id: string;
  author_id: string;
  channel: string;
  turn: number;
  content_snapshot: string;
  content_hash: string;
  command: string[];
  state: 'Pending' | 'Approved' | 'Rejected' | 'Expired';
  queued_at: number;
}

interface DispatchesPanelProps {
  bridge: any;
}

export function DispatchesPanel({ bridge }: DispatchesPanelProps) {
  const [dispatches, setDispatches] = useState<PendingDispatch[]>([]);
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState<Record<string, boolean>>({});
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<{ id: string; message: string; type: 'success' | 'error' | 'conflict' } | null>(null);

  const fetchDispatches = useCallback(async () => {
    if (!bridge) return;
    try {
      setLoading(true);
      const res = await bridge.fetchRaw('/api/v1/dispatches');
      if (Array.isArray(res)) {
        setDispatches(res);
      }
    } catch (e) {
      console.error('Failed to fetch dispatches:', e);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    fetchDispatches();
  }, [fetchDispatches]);

  // Polling for new pending dispatches
  usePolling('dispatches-list', fetchDispatches, { interval: 'medium', enabled: !!bridge });

  const handleApprove = async (d: PendingDispatch) => {
    if (!bridge) return;
    setActionLoading(p => ({ ...p, [d.id]: true }));
    setFeedback(null);
    try {
      const res = await bridge.fetchRaw(`/api/v1/dispatches/${d.id}/approve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ expected_hash: d.content_hash }),
      });
      if (res.status === 'approved') {
        setFeedback({ id: d.id, message: `Approved dispatch for @${d.agent_id}`, type: 'success' });
        await fetchDispatches();
      } else if (res.error === 'already_resolved') {
        setFeedback({ id: d.id, message: `Already resolved as ${res.state}`, type: 'conflict' });
        await fetchDispatches();
      } else {
        setFeedback({ id: d.id, message: res.error || 'Approval failed', type: 'error' });
      }
    } catch (e: any) {
      setFeedback({ id: d.id, message: e.message || String(e), type: 'error' });
    } finally {
      setActionLoading(p => ({ ...p, [d.id]: false }));
    }
  };

  const handleReject = async (d: PendingDispatch) => {
    if (!bridge) return;
    setActionLoading(p => ({ ...p, [d.id]: true }));
    setFeedback(null);
    try {
      const res = await bridge.fetchRaw(`/api/v1/dispatches/${d.id}/reject`, {
        method: 'POST',
      });
      if (res.status === 'rejected') {
        setFeedback({ id: d.id, message: `Rejected dispatch for @${d.agent_id}`, type: 'success' });
        await fetchDispatches();
      } else if (res.error === 'already_resolved') {
        setFeedback({ id: d.id, message: `Already resolved as ${res.state}`, type: 'conflict' });
        await fetchDispatches();
      } else {
        setFeedback({ id: d.id, message: res.error || 'Rejection failed', type: 'error' });
      }
    } catch (e: any) {
      setFeedback({ id: d.id, message: e.message || String(e), type: 'error' });
    } finally {
      setActionLoading(p => ({ ...p, [d.id]: false }));
    }
  };

  const handleCopyHash = (id: string, hash: string) => {
    navigator.clipboard.writeText(hash);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const formatTime = (ts: number) => {
    return new Date(ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  };

  return (
    <div className="flex-1 flex flex-col space-y-4 p-4 min-h-0 overflow-y-auto">
      {/* Header banner */}
      <div className="flex items-center justify-between bg-slate-900/60 border border-slate-800 rounded-xl p-4 shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400">
            <ShieldCheck className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-sm font-semibold text-slate-100 flex items-center gap-2">
              HITL Push Dispatch Queue
              <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">
                BWC-2 Active
              </span>
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              Cryptographically bound pending agent triggers awaiting explicit operator confirmation.
            </p>
          </div>
        </div>

        <button
          onClick={fetchDispatches}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-800/80 hover:bg-slate-700/80 text-slate-300 text-xs font-mono rounded-lg border border-slate-700 transition-all cursor-pointer disabled:opacity-50"
        >
          <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin')} />
          Refresh
        </button>
      </div>

      {/* Global feedback message */}
      {feedback && (
        <div
          className={cn(
            'p-3 rounded-lg border text-xs font-mono flex items-center justify-between shrink-0',
            feedback.type === 'success' && 'bg-emerald-950/40 border-emerald-500/30 text-emerald-300',
            feedback.type === 'conflict' && 'bg-amber-950/40 border-amber-500/30 text-amber-300',
            feedback.type === 'error' && 'bg-red-950/40 border-red-500/30 text-red-300'
          )}
        >
          <span>{feedback.message}</span>
          <button onClick={() => setFeedback(null)} className="text-slate-400 hover:text-slate-200">
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* Dispatches List */}
      <div className="space-y-3">
        {dispatches.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center bg-slate-900/30 border border-slate-800/60 rounded-xl">
            <ShieldCheck className="w-10 h-10 text-slate-600 mb-3" />
            <p className="text-sm font-semibold text-slate-300">No Pending Dispatches</p>
            <p className="text-xs text-slate-500 max-w-sm mt-1">
              When a Coloquio mention triggers a wake policy from a trusted author, it will appear here for 1-click verification.
            </p>
          </div>
        ) : (
          dispatches.map((d) => {
            const targetStyle = agentStyle(d.agent_id);
            const authorStyle = agentStyle(d.author_id);
            const isProcessing = actionLoading[d.id];

            return (
              <div
                key={d.id}
                className="bg-slate-900/70 border border-slate-800 hover:border-slate-700/80 rounded-xl p-4 transition-all shadow-sm flex flex-col gap-3"
              >
                {/* Meta row */}
                <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-800/80 pb-2.5">
                  <div className="flex items-center gap-2">
                    <span
                      className={cn(
                        'px-2.5 py-0.5 rounded-full text-[11px] font-bold font-mono border flex items-center gap-1',
                        targetStyle.bg,
                        targetStyle.color,
                        targetStyle.border
                      )}
                    >
                      <User className="w-3 h-3" />
                      @{d.agent_id}
                    </span>
                    <span className="text-slate-600 text-xs">←</span>
                    <span
                      className={cn(
                        'px-2 py-0.5 rounded text-[10px] font-mono border',
                        authorStyle.bg,
                        authorStyle.color,
                        authorStyle.border
                      )}
                    >
                      @{d.author_id}
                    </span>
                    <span className="text-slate-500 text-[11px] font-mono">
                      in #{d.channel} (T{d.turn})
                    </span>
                  </div>

                  <div className="flex items-center gap-2 text-[10px] font-mono text-slate-400">
                    <Clock className="w-3 h-3 text-slate-500" />
                    <span>{formatTime(d.queued_at)}</span>
                  </div>
                </div>

                {/* Content Snapshot */}
                <div className="bg-slate-950/60 border border-slate-800/80 rounded-lg p-3">
                  <div className="text-[10px] font-mono uppercase text-slate-500 tracking-wider mb-1 font-bold">
                    Message Snapshot (Exact Reviewer Context)
                  </div>
                  <p className="text-xs text-slate-200 whitespace-pre-wrap font-sans leading-relaxed">
                    {d.content_snapshot}
                  </p>
                </div>

                {/* Command and SHA-256 Hash */}
                <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-2 bg-slate-950/40 p-2.5 rounded-lg border border-slate-800/50 text-[11px] font-mono">
                  <div className="flex items-center gap-1.5 text-slate-400 truncate max-w-full">
                    <Terminal className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
                    <span className="text-slate-300 font-bold truncate">
                      {d.command.join(' ')}
                    </span>
                  </div>

                  <div className="flex items-center gap-1.5 shrink-0">
                    <Hash className="w-3 h-3 text-slate-500" />
                    <span className="text-slate-400 text-[10px]" title={d.content_hash}>
                      SHA-256: {d.content_hash.substring(0, 12)}…
                    </span>
                    <button
                      onClick={() => handleCopyHash(d.id, d.content_hash)}
                      className="p-1 hover:bg-slate-800 rounded text-slate-400 hover:text-slate-200 transition-colors"
                      title="Copy full SHA-256 hash"
                    >
                      {copiedId === d.id ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                    </button>
                  </div>
                </div>

                {/* Actions */}
                <div className="flex items-center justify-end gap-2 pt-1">
                  <button
                    onClick={() => handleReject(d)}
                    disabled={isProcessing}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-red-950/30 hover:bg-red-900/40 border border-red-500/30 text-red-300 text-xs font-mono font-bold transition-all cursor-pointer disabled:opacity-50"
                  >
                    <X className="w-3.5 h-3.5" />
                    Reject
                  </button>

                  <button
                    onClick={() => handleApprove(d)}
                    disabled={isProcessing}
                    className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-slate-950 text-xs font-mono font-bold transition-all shadow-lg hover:shadow-emerald-500/20 cursor-pointer disabled:opacity-50"
                  >
                    {isProcessing ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : (
                      <Check className="w-3.5 h-3.5 stroke-[3]" />
                    )}
                    Approve (CAS Hash-Bound)
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
export default DispatchesPanel;
