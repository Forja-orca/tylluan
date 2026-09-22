import React, { useState, useEffect, useCallback, useMemo } from 'react';
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Clock,
  RefreshCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Zap,
  ChevronDown,
  ChevronUp,
  X,
  MessageSquare,
  Radio,
  Copy,
  Check,
} from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import { agentStyle, KNOWN_AGENTS } from '../lib/agent-meta';
import { MetricCard } from './ui/MetricPrimitives';

export interface ColoquioMsg {
  msg_id?: string;
  channel_id: string;
  author_id: string;
  role?: string;
  content: string;
  turn: number;
  created_at?: number | string;
  metadata?: Record<string, unknown> | string;
}

export type FleetHealthStatus = 'healthy' | 'incident' | 'active' | 'idle' | 'nominal';

export interface AgentHealthSummary {
  authorId: string;
  lastTurn: number;
  lastTs: number;
  lastContent: string;
  status: FleetHealthStatus;
  statusReason: string;
  matchedKeywords: string[];
  totalTurns: number;
  history: ColoquioMsg[];
}

export interface FleetHealthPanelProps {
  bridge: {
    fetchRaw: (url: string, init?: RequestInit) => Promise<unknown>;
  } | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}

const INCIDENT_KEYWORDS = [
  'error',
  'fallo',
  'colgado',
  'failed',
  'timeout',
  'zombie',
  'sharing_violation',
  'excedio',
  'exit=1',
  'exit 1',
  'falso timeout',
  'huerfano',
  'bloqueo',
  'roto',
  'rechazado',
  'panic',
  'crash',
];

const HEALTHY_KEYWORDS = [
  'ok',
  'ack',
  'completado',
  'verde',
  'resuelto',
  'sincronizado',
  'exit 0',
  'exit=0',
  'exito',
  'clean',
  'passing',
  'aprobado',
  'aprobada',
  'cerrado',
  'entregado',
];

const ACTIVE_KEYWORDS = [
  'iniciando',
  'corriendo',
  'benchmark',
  'ejecutando',
  'midiendo',
  'wip',
  'procesando',
  'progreso',
  'investigando',
  'revisando',
];

function normalizeTimestamp(val?: number | string): number {
  if (!val) return 0;
  if (typeof val === 'number') {
    return val > 1e11 ? Math.floor(val / 1000) : val;
  }
  const parsed = Date.parse(val);
  return Number.isNaN(parsed) ? 0 : Math.floor(parsed / 1000);
}

function formatRelativeTime(unixSecs: number): string {
  if (!unixSecs) return 'unknown';
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unixSecs;
  if (diff < 0) return 'just now';
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function analyzeAgentHealth(authorId: string, msgs: ColoquioMsg[]): AgentHealthSummary {
  const sorted = [...msgs].sort((a, b) => b.turn - a.turn);
  const latest = sorted[0];
  if (!latest) {
    return {
      authorId,
      lastTurn: 0,
      lastTs: 0,
      lastContent: '',
      status: 'idle',
      statusReason: 'No activity recorded in Coloquio',
      matchedKeywords: [],
      totalTurns: 0,
      history: [],
    };
  }

  const contentLower = latest.content.toLowerCase();
  const matchedIncidents = INCIDENT_KEYWORDS.filter(k => contentLower.includes(k));
  const matchedHealthy = HEALTHY_KEYWORDS.filter(k => contentLower.includes(k));
  const matchedActive = ACTIVE_KEYWORDS.filter(k => contentLower.includes(k));

  const ts = normalizeTimestamp(latest.created_at);
  const nowSecs = Math.floor(Date.now() / 1000);
  const ageSecs = nowSecs - ts;

  let status: FleetHealthStatus;
  let reason: string;
  const allMatched: string[] = [];

  const isResolvedOrAcked =
    matchedHealthy.length > 0 ||
    contentLower.includes('resuelto') ||
    contentLower.includes('cerrado') ||
    contentLower.includes('ack') ||
    contentLower.includes('fix aplicado');

  if (matchedIncidents.length > 0 && !isResolvedOrAcked) {
    status = 'incident';
    reason = `Warning or error detected (${matchedIncidents.slice(0, 3).join(', ')})`;
    allMatched.push(...matchedIncidents);
  } else if (matchedHealthy.length > 0) {
    status = 'healthy';
    reason = `Explicit success signal (${matchedHealthy.slice(0, 3).join(', ')})`;
    allMatched.push(...matchedHealthy);
  } else if (matchedActive.length > 0) {
    status = 'active';
    reason = `Active execution in progress (${matchedActive.slice(0, 3).join(', ')})`;
    allMatched.push(...matchedActive);
  } else if (ageSecs > 7200) {
    status = 'idle';
    reason = `Idle (${formatRelativeTime(ts)})`;
  } else {
    status = 'nominal';
    reason = 'Recent normal transmission';
  }

  return {
    authorId,
    lastTurn: latest.turn,
    lastTs: ts,
    lastContent: latest.content,
    status,
    statusReason: reason,
    matchedKeywords: Array.from(new Set(allMatched)),
    totalTurns: sorted.length,
    history: sorted.slice(0, 10),
  };
}

export function FleetHealthPanel({ bridge, notify }: FleetHealthPanelProps) {
  const [messages, setMessages] = useState<ColoquioMsg[]>([]);
  const [loading, setLoading] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [expandedAgents, setExpandedAgents] = useState<Record<string, boolean>>({});
  const [inspectAgent, setInspectAgent] = useState<AgentHealthSummary | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const fetchChannelData = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      // Primary fetch against /api/v1/coloquio/channels/general with offset=0 fallback
      let raw = await bridge.fetchRaw('/api/v1/coloquio/channels/general?limit=500&offset=0');
      let msgs = (raw as { messages?: ColoquioMsg[] })?.messages;

      if (!msgs || msgs.length === 0) {
        raw = await bridge.fetchRaw('/api/v1/coloquio/channels/general');
        msgs = (raw as { messages?: ColoquioMsg[] })?.messages;
      }

      setMessages(msgs || []);
      setLastUpdated(new Date());
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      notify?.(`Failed to fetch fleet Coloquio health: ${errorMsg}`, 'error');
    } finally {
      setLoading(false);
    }
  }, [bridge, notify]);

  useEffect(() => {
    fetchChannelData();
  }, [fetchChannelData]);

  // Centralized polling every 10s
  usePolling('fleet-health-panel-refresh', fetchChannelData, { interval: 'standard', enabled: Boolean(bridge) });

  const agentSummaries = useMemo(() => {
    const byAuthor: Record<string, ColoquioMsg[]> = {};
    for (const msg of messages) {
      const aid = (msg.author_id || 'unknown').toLowerCase();
      if (!byAuthor[aid]) byAuthor[aid] = [];
      byAuthor[aid].push(msg);
    }

    // Ensure all KNOWN_AGENTS are represented even if silent
    for (const ka of KNOWN_AGENTS) {
      if (!byAuthor[ka]) byAuthor[ka] = [];
    }

    const list: AgentHealthSummary[] = Object.keys(byAuthor).map(authorId =>
      analyzeAgentHealth(authorId, byAuthor[authorId])
    );

    // Sorting: incidents first, then active, then healthy/nominal, then idle
    const priority: Record<FleetHealthStatus, number> = {
      incident: 1,
      active: 2,
      healthy: 3,
      nominal: 4,
      idle: 5,
    };

    return list.sort((a, b) => {
      const pDiff = priority[a.status] - priority[b.status];
      if (pDiff !== 0) return pDiff;
      return b.lastTs - a.lastTs;
    });
  }, [messages]);

  const filteredSummaries = useMemo(() => {
    return agentSummaries.filter(summary => {
      if (statusFilter !== 'all' && summary.status !== statusFilter) {
        return false;
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchesAuthor = summary.authorId.toLowerCase().includes(q);
        const matchesContent = summary.lastContent.toLowerCase().includes(q);
        const matchesReason = summary.statusReason.toLowerCase().includes(q);
        if (!matchesAuthor && !matchesContent && !matchesReason) {
          return false;
        }
      }
      return true;
    });
  }, [agentSummaries, statusFilter, searchQuery]);

  const stats = useMemo(() => {
    const total = agentSummaries.length;
    const healthy = agentSummaries.filter(s => s.status === 'healthy' || s.status === 'nominal').length;
    const incidents = agentSummaries.filter(s => s.status === 'incident').length;
    const active = agentSummaries.filter(s => s.status === 'active').length;
    const maxTurn = messages.reduce((max, m) => (m.turn > max ? m.turn : max), 0);
    return { total, healthy, incidents, active, maxTurn };
  }, [agentSummaries, messages]);

  const toggleExpand = (aid: string) => {
    setExpandedAgents(prev => ({ ...prev, [aid]: !prev[aid] }));
  };

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard?.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  return (
    <div className="flex-1 flex flex-col space-y-4 p-4 min-h-0 overflow-y-auto">
      {/* Top Header Card */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 p-4 rounded-xl border border-slate-800 bg-slate-900/40 backdrop-blur-md">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-emerald-400">
            <Activity className="w-5 h-5 animate-pulse" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-base font-bold text-slate-100 font-sans tracking-tight">
                Fleet Health & Loop Supervisor
              </h2>
              <span className="px-2 py-0.5 text-[9px] font-mono font-bold uppercase rounded bg-indigo-950/60 border border-indigo-700/50 text-indigo-300">
                WORK_PROTOCOL §7
              </span>
            </div>
            <p className="text-xs text-slate-400 font-mono">
              Live loop observability · Heuristic status extraction from #general Coloquio channel
            </p>
          </div>
        </div>

        <div className="flex items-center gap-3 flex-wrap">
          {lastUpdated && (
            <div className="text-[11px] text-slate-400 font-mono flex items-center gap-1.5">
              <Clock className="w-3.5 h-3.5 text-slate-400" />
              <span>Synced {formatRelativeTime(Math.floor(lastUpdated.getTime() / 1000))}</span>
            </div>
          )}
          <button
            onClick={fetchChannelData}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-700 bg-slate-800/80 hover:bg-slate-700 text-slate-200 text-xs font-mono transition-all disabled:opacity-50"
          >
            <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin text-emerald-400')} />
            <span>{loading ? 'Refreshing...' : 'Refresh'}</span>
          </button>
        </div>
      </div>

      {/* Metrics Row */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard
          icon={Radio}
          label="Monitored Agents"
          value={stats.total}
          sub="Fleet members in registry"
        />
        <MetricCard
          icon={CheckCircle2}
          label="Healthy / Nominal"
          value={stats.healthy}
          valueClass="text-emerald-400"
          sub="Clear loop iterations"
        />
        <MetricCard
          icon={ShieldAlert}
          label="Incidents / Warnings"
          value={stats.incidents}
          valueClass={stats.incidents > 0 ? 'text-rose-400' : 'text-slate-400'}
          sub="Requiring attention"
        />
        <MetricCard
          icon={Zap}
          label="Latest Turn"
          value={`#${stats.maxTurn}`}
          valueClass="text-indigo-300"
          sub="Active Coloquio turn"
        />
      </div>

      {/* Filter and Search Bar */}
      <div className="flex flex-col sm:flex-row items-center justify-between gap-3 bg-slate-900/30 p-2.5 rounded-xl border border-slate-800">
        <div className="relative w-full sm:w-72">
          <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-slate-400" />
          <input
            type="text"
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            placeholder="Filter by agent, keywords or content..."
            className="w-full bg-slate-950/80 border border-slate-800 rounded-lg pl-9 pr-3 py-1.5 text-xs text-slate-200 placeholder-slate-400 focus:outline-none focus:border-emerald-500/50 font-mono"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-200"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>

        {/* Status Filter Tabs */}
        <div className="flex items-center gap-1.5 overflow-x-auto w-full sm:w-auto">
          {[
            { id: 'all', label: 'All' },
            { id: 'healthy', label: 'Healthy' },
            { id: 'incident', label: 'Incidents' },
            { id: 'active', label: 'Active' },
            { id: 'idle', label: 'Idle' },
          ].map(f => (
            <button
              key={f.id}
              onClick={() => setStatusFilter(f.id)}
              className={cn(
                'px-2.5 py-1 text-[11px] font-mono font-medium rounded-lg border transition-all whitespace-nowrap',
                statusFilter === f.id
                  ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
                  : 'bg-slate-900/40 border-slate-800 text-slate-400 hover:text-slate-200'
              )}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

      {/* Agents Loop Health Grid */}
      {filteredSummaries.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center p-12 border border-slate-800 border-dashed rounded-xl text-center bg-slate-900/20">
          <ShieldCheck className="w-10 h-10 text-slate-400 mb-3" />
          <h3 className="text-sm font-bold text-slate-300 font-sans">No matching agent loops</h3>
          <p className="text-xs text-slate-400 font-mono mt-1 max-w-sm">
            No agents found matching the current status filter or search parameters.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {filteredSummaries.map(summary => {
            const style = agentStyle(summary.authorId);
            const isExpanded = Boolean(expandedAgents[summary.authorId]);

            let badgeBg = 'bg-slate-800/80 border-slate-700 text-slate-300';
            let badgeIcon = <Clock className="w-3.5 h-3.5" />;
            if (summary.status === 'healthy' || summary.status === 'nominal') {
              badgeBg = 'bg-emerald-950/60 border-emerald-800/50 text-emerald-300';
              badgeIcon = <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />;
            } else if (summary.status === 'incident') {
              badgeBg = 'bg-rose-950/60 border-rose-800/50 text-rose-300';
              badgeIcon = <AlertTriangle className="w-3.5 h-3.5 text-rose-400 animate-pulse" />;
            } else if (summary.status === 'active') {
              badgeBg = 'bg-cyan-950/60 border-cyan-800/50 text-cyan-300';
              badgeIcon = <Activity className="w-3.5 h-3.5 text-cyan-400 animate-spin" />;
            }

            return (
              <div
                key={summary.authorId}
                className={cn(
                  'flex flex-col p-4 rounded-xl border bg-slate-900/40 backdrop-blur-md transition-all',
                  summary.status === 'incident'
                    ? 'border-rose-800/40 hover:border-rose-600/50'
                    : 'border-slate-800 hover:border-slate-700'
                )}
              >
                {/* Agent Header */}
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-center gap-3">
                    <div
                      className={cn(
                        'w-10 h-10 rounded-xl flex items-center justify-center font-bold text-sm border shadow-sm font-sans',
                        style.bg,
                        style.color,
                        style.border
                      )}
                    >
                      {style.initial}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <h4 className="font-bold text-sm text-slate-100 font-sans">{style.label}</h4>
                        <span className="text-[10px] font-mono text-slate-400">@{summary.authorId}</span>
                      </div>
                      <p className="text-[11px] font-mono text-slate-400">{summary.statusReason}</p>
                    </div>
                  </div>

                  {/* Status Badge */}
                  <div
                    className={cn(
                      'flex items-center gap-1.5 px-2.5 py-1 rounded-lg border text-[11px] font-mono font-bold uppercase',
                      badgeBg
                    )}
                  >
                    {badgeIcon}
                    <span>{summary.status}</span>
                  </div>
                </div>

                {/* Telemetry Summary Bar */}
                <div className="grid grid-cols-3 gap-2 my-3 p-2.5 rounded-lg bg-slate-950/60 border border-slate-800/80 text-[11px] font-mono">
                  <div>
                    <span className="text-slate-400 block text-[9px] uppercase">Last Turn</span>
                    <span className="text-slate-200 font-bold">
                      {summary.lastTurn > 0 ? `#${summary.lastTurn}` : 'None'}
                    </span>
                  </div>
                  <div>
                    <span className="text-slate-400 block text-[9px] uppercase">Timestamp</span>
                    <span className="text-slate-200">
                      {summary.lastTs > 0 ? formatRelativeTime(summary.lastTs) : 'N/A'}
                    </span>
                  </div>
                  <div>
                    <span className="text-slate-400 block text-[9px] uppercase">Total Logs</span>
                    <span className="text-slate-200">{summary.totalTurns} msgs</span>
                  </div>
                </div>

                {/* Keywords Matched Pill */}
                {summary.matchedKeywords.length > 0 && (
                  <div className="flex items-center gap-1.5 mb-2 flex-wrap">
                    <span className="text-[10px] font-mono text-slate-400">Keywords:</span>
                    {summary.matchedKeywords.map(kw => (
                      <span
                        key={kw}
                        className={cn(
                          'px-1.5 py-0.5 rounded text-[10px] font-mono border',
                          summary.status === 'incident'
                            ? 'bg-rose-950/40 border-rose-800/40 text-rose-300'
                            : 'bg-emerald-950/40 border-emerald-800/40 text-emerald-300'
                        )}
                      >
                        {kw}
                      </span>
                    ))}
                  </div>
                )}

                {/* Message Content Preview */}
                {summary.lastContent ? (
                  <div className="mt-1 flex-1 flex flex-col justify-between">
                    <div className="p-3 rounded-lg bg-slate-950/80 border border-slate-800/80 text-xs font-mono text-slate-300 overflow-hidden relative">
                      <div className={cn('whitespace-pre-wrap break-words', !isExpanded && 'line-clamp-3')}>
                        {summary.lastContent}
                      </div>
                      {summary.lastContent.length > 150 && (
                        <button
                          onClick={() => toggleExpand(summary.authorId)}
                          className="mt-2 text-[10px] text-indigo-400 hover:text-indigo-300 flex items-center gap-1 font-semibold"
                        >
                          {isExpanded ? (
                            <>
                              <ChevronUp className="w-3 h-3" /> Show Less
                            </>
                          ) : (
                            <>
                              <ChevronDown className="w-3 h-3" /> Read Full Output
                            </>
                          )}
                        </button>
                      )}
                    </div>

                    <div className="flex items-center justify-between mt-3 pt-2 border-t border-slate-800/60">
                      <button
                        onClick={() => setInspectAgent(summary)}
                        className="text-[11px] text-emerald-400 hover:text-emerald-300 font-mono flex items-center gap-1"
                      >
                        <MessageSquare className="w-3 h-3" /> View Turn History ({summary.history.length})
                      </button>

                      <button
                        onClick={() => copyToClipboard(summary.lastContent, `card-${summary.authorId}`)}
                        className="text-[11px] text-slate-400 hover:text-slate-200 font-mono flex items-center gap-1"
                      >
                        {copiedId === `card-${summary.authorId}` ? (
                          <>
                            <Check className="w-3 h-3 text-emerald-400" /> Copied
                          </>
                        ) : (
                          <>
                            <Copy className="w-3 h-3" /> Copy Output
                          </>
                        )}
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="mt-2 p-3 rounded-lg bg-slate-950/40 border border-slate-800/40 text-xs font-mono text-slate-400 italic">
                    No recent transmission in #general
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* History Inspector Modal */}
      {inspectAgent && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-150">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-3xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden">
            {/* Modal Header */}
            <div className="flex items-center justify-between p-4 border-b border-slate-800 bg-slate-950/40">
              <div className="flex items-center gap-3">
                <div
                  className={cn(
                    'w-8 h-8 rounded-lg flex items-center justify-center font-bold text-xs border font-sans',
                    agentStyle(inspectAgent.authorId).bg,
                    agentStyle(inspectAgent.authorId).color,
                    agentStyle(inspectAgent.authorId).border
                  )}
                >
                  {agentStyle(inspectAgent.authorId).initial}
                </div>
                <div>
                  <h3 className="text-sm font-bold text-slate-100 font-sans">
                    Turn History — {agentStyle(inspectAgent.authorId).label} (@{inspectAgent.authorId})
                  </h3>
                  <p className="text-[11px] text-slate-400 font-mono">
                    Showing latest {inspectAgent.history.length} messages in #general
                  </p>
                </div>
              </div>
              <button
                onClick={() => setInspectAgent(null)}
                className="p-1 rounded-lg text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {/* Modal Content / Timeline */}
            <div className="flex-1 overflow-y-auto p-4 space-y-3 font-mono">
              {inspectAgent.history.map(msg => (
                <div key={msg.msg_id || msg.turn} className="p-3 rounded-xl border border-slate-800 bg-slate-950/60">
                  <div className="flex items-center justify-between text-[11px] text-slate-400 mb-2 border-b border-slate-800/60 pb-1.5">
                    <div className="flex items-center gap-2">
                      <span className="font-bold text-indigo-300">Turn #{msg.turn}</span>
                      <span className="text-slate-400">·</span>
                      <span>{formatRelativeTime(normalizeTimestamp(msg.created_at))}</span>
                    </div>
                    <button
                      onClick={() => copyToClipboard(msg.content, `modal-${msg.turn}`)}
                      className="text-slate-400 hover:text-slate-200 flex items-center gap-1"
                    >
                      {copiedId === `modal-${msg.turn}` ? (
                        <>
                          <Check className="w-3 h-3 text-emerald-400" /> Copied
                        </>
                      ) : (
                        <>
                          <Copy className="w-3 h-3" /> Copy
                        </>
                      )}
                    </button>
                  </div>
                  <pre className="whitespace-pre-wrap break-words text-xs text-slate-200 font-mono leading-relaxed">
                    {msg.content}
                  </pre>
                </div>
              ))}
            </div>

            {/* Modal Footer */}
            <div className="p-3 border-t border-slate-800 bg-slate-950/40 flex justify-end">
              <button
                onClick={() => setInspectAgent(null)}
                className="px-4 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono transition-all"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default FleetHealthPanel;
