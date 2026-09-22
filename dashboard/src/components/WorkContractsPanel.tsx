import React, { useState, useEffect, useCallback, useMemo } from 'react';
import {
  Briefcase,
  CheckCircle2,
  Clock,
  RefreshCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Zap,
  Users,
  FileText,
  Copy,
  Check,
  X,
  ChevronRight,
  TrendingDown,
  Layers,
  Award,
} from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import { agentStyle } from '../lib/agent-meta';
import { MetricCard } from './ui/MetricPrimitives';

export interface WorkContractDelivery {
  agent_id: string;
  artifact?: string;
  note?: string;
  ts?: number;
}

export interface WorkContractVote {
  agent_id: string;
  vote: string;
  cycles?: number;
}

export interface WorkContract {
  id: string;
  task: string;
  budget: number;
  budget_remaining: number;
  team: string[];
  consolidator: string;
  channel_id: string;
  status: string; // 'open' | 'in_progress' | 'review' | 'done' | 'blocked' | 'extended'
  created_at: number;
  deliveries?: WorkContractDelivery[];
  votes?: WorkContractVote[];
  extensions?: number;
}

export interface WorkContractsPanelProps {
  bridge: {
    fetchRaw: (url: string, init?: RequestInit) => Promise<unknown>;
  } | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
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

function getStatusBadge(status: string) {
  switch (status?.toLowerCase()) {
    case 'open':
      return {
        label: 'Open',
        bg: 'bg-indigo-950/60 border-indigo-800/50 text-indigo-300',
        icon: <Zap className="w-3.5 h-3.5 text-indigo-400" />,
      };
    case 'in_progress':
      return {
        label: 'In Progress',
        bg: 'bg-cyan-950/60 border-cyan-800/50 text-cyan-300',
        icon: <Clock className="w-3.5 h-3.5 text-cyan-400 animate-spin" />,
      };
    case 'done':
      return {
        label: 'Done',
        bg: 'bg-emerald-950/60 border-emerald-800/50 text-emerald-300',
        icon: <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />,
      };
    case 'blocked':
      return {
        label: 'Blocked',
        bg: 'bg-rose-950/60 border-rose-800/50 text-rose-300',
        icon: <ShieldAlert className="w-3.5 h-3.5 text-rose-400 animate-pulse" />,
      };
    case 'extended':
      return {
        label: 'Extended',
        bg: 'bg-amber-950/60 border-amber-800/50 text-amber-300',
        icon: <Layers className="w-3.5 h-3.5 text-amber-400" />,
      };
    default:
      return {
        label: status || 'Unknown',
        bg: 'bg-slate-800/80 border-slate-700 text-slate-300',
        icon: <Briefcase className="w-3.5 h-3.5" />,
      };
  }
}

export function WorkContractsPanel({ bridge, notify }: WorkContractsPanelProps) {
  const [activeContract, setActiveContract] = useState<WorkContract | null>(null);
  const [allContracts, setAllContracts] = useState<WorkContract[]>([]);
  const [loading, setLoading] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedContract, setSelectedContract] = useState<WorkContract | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [manualLookupId, setManualLookupId] = useState('');

  const fetchContracts = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      // 1. Fetch active contract on channel 'general'
      let activeDetail: WorkContract | null = null;
      try {
        const activeRes = (await bridge.fetchRaw(
          '/api/v1/work-contracts/active?channel_id=general'
        )) as { contract_id?: string; budget_remaining?: number; status?: string };

        if (activeRes && activeRes.contract_id) {
          const detail = (await bridge.fetchRaw(
            `/api/v1/work-contracts/${activeRes.contract_id}`
          )) as WorkContract;
          if (detail && detail.id) {
            activeDetail = detail;
          }
        }
      } catch {
        // No active contract or 404 is allowed
      }

      setActiveContract(activeDetail);

      // 2. Discover known contract IDs by scanning recent Coloquio messages
      const discoveredIds = new Set<string>();
      if (activeDetail?.id) discoveredIds.add(activeDetail.id);

      try {
        const msgRaw = (await bridge.fetchRaw(
          '/api/v1/coloquio/channels/general?limit=200&offset=0'
        )) as { messages?: { content?: string }[] };
        const msgs = msgRaw?.messages || [];
        for (const m of msgs) {
          if (!m.content) continue;
          const matches = m.content.match(/\bbwc-[a-zA-Z0-9_-]+\b/g);
          if (matches) {
            for (const match of matches) {
              discoveredIds.add(match);
            }
          }
        }
      } catch {
        // Fallback silently if Coloquio scan is unavailable
      }

      // 3. Fetch details for all discovered contracts
      const loaded: WorkContract[] = [];
      if (activeDetail) loaded.push(activeDetail);

      for (const cid of discoveredIds) {
        if (activeDetail && activeDetail.id === cid) continue;
        try {
          const c = (await bridge.fetchRaw(`/api/v1/work-contracts/${cid}`)) as WorkContract;
          if (c && c.id) {
            loaded.push(c);
          }
        } catch {
          // Ignore nonexistent or deleted contract IDs
        }
      }

      // Deduplicate by id and sort newest first
      const unique = Array.from(new Map(loaded.map(item => [item.id, item])).values());
      unique.sort((a, b) => (b.created_at || 0) - (a.created_at || 0));

      setAllContracts(unique);
      setLastUpdated(new Date());
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      notify?.(`Failed to fetch work contracts: ${errorMsg}`, 'error');
    } finally {
      setLoading(false);
    }
  }, [bridge, notify]);

  useEffect(() => {
    fetchContracts();
  }, [fetchContracts]);

  // Centralized polling every 10s
  usePolling('work-contracts-panel-refresh', fetchContracts, { interval: 'standard', enabled: Boolean(bridge) });

  const handleLookup = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!bridge || !manualLookupId.trim()) return;
    setLoading(true);
    try {
      const cleanId = manualLookupId.trim();
      const contract = (await bridge.fetchRaw(`/api/v1/work-contracts/${cleanId}`)) as WorkContract;
      if (contract && contract.id) {
        setSelectedContract(contract);
        // Add to list if not present
        setAllContracts(prev => {
          if (prev.some(c => c.id === contract.id)) return prev;
          return [contract, ...prev];
        });
        setManualLookupId('');
      } else {
        notify?.(`Contract '${cleanId}' not found.`, 'error');
      }
    } catch {
      notify?.(`Contract '${manualLookupId.trim()}' not found.`, 'error');
    } finally {
      setLoading(false);
    }
  };

  const filteredContracts = useMemo(() => {
    return allContracts.filter(c => {
      if (!searchQuery.trim()) return true;
      const q = searchQuery.toLowerCase();
      const matchesId = c.id?.toLowerCase().includes(q);
      const matchesTask = c.task?.toLowerCase().includes(q);
      const matchesConsolidator = c.consolidator?.toLowerCase().includes(q);
      const matchesTeam = c.team?.some(t => t.toLowerCase().includes(q));
      return matchesId || matchesTask || matchesConsolidator || matchesTeam;
    });
  }, [allContracts, searchQuery]);

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard?.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  return (
    <div className="flex-1 flex flex-col space-y-4 p-4 min-h-0 overflow-y-auto">
      {/* Top Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 p-4 rounded-xl border border-slate-800 bg-slate-900/40 backdrop-blur-md">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center text-indigo-400">
            <Briefcase className="w-5 h-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-base font-bold text-slate-100 font-sans tracking-tight">
                Bounded Work Contracts (BWC)
              </h2>
              <span className="px-2 py-0.5 text-[9px] font-mono font-bold uppercase rounded bg-indigo-950/60 border border-indigo-700/50 text-indigo-300">
                Finite Protocol · BWC-1..4
              </span>
            </div>
            <p className="text-xs text-slate-400 font-mono">
              Deterministic task delegation, turn budgets, deliverables ledger & multi-agent consensus
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
            onClick={fetchContracts}
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
          icon={Zap}
          label="Active Contract"
          value={activeContract ? `#${activeContract.id.slice(0, 8)}` : 'None'}
          valueClass={activeContract ? 'text-indigo-300' : 'text-slate-400'}
          sub={activeContract ? `Status: ${activeContract.status}` : 'No open contract in #general'}
        />
        <MetricCard
          icon={TrendingDown}
          label="Budget Remaining"
          value={
            activeContract
              ? `${activeContract.budget_remaining} / ${activeContract.budget}`
              : 'N/A'
          }
          valueClass={
            activeContract && activeContract.budget_remaining <= 3
              ? 'text-rose-400'
              : 'text-emerald-400'
          }
          unit={activeContract ? 'turns' : undefined}
          sub={
            activeContract
              ? `${Math.round((activeContract.budget_remaining / (activeContract.budget || 1)) * 100)}% budget left`
              : 'Finite multi-agent quota'
          }
        />
        <MetricCard
          icon={Users}
          label="Team Members"
          value={activeContract ? activeContract.team?.length || 0 : 0}
          sub={activeContract ? `Consolidator: @${activeContract.consolidator}` : 'Assigned fleet members'}
        />
        <MetricCard
          icon={Award}
          label="Deliveries Tracked"
          value={
            allContracts.reduce((acc, c) => acc + (c.deliveries?.length || 0), 0)
          }
          valueClass="text-emerald-400"
          sub="Artifacts submitted across contracts"
        />
      </div>

      {/* Hero Card: Active Work Contract */}
      {activeContract ? (
        <div className="p-5 rounded-xl border border-indigo-800/40 bg-indigo-950/20 backdrop-blur-md relative overflow-hidden">
          <div className="flex flex-col md:flex-row md:items-start justify-between gap-4">
            <div className="space-y-2 flex-1">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-xs font-mono text-indigo-300 font-bold">
                  ACTIVE CONTRACT · {activeContract.id}
                </span>
                <div
                  className={cn(
                    'flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold uppercase border',
                    getStatusBadge(activeContract.status).bg
                  )}
                >
                  {getStatusBadge(activeContract.status).icon}
                  <span>{getStatusBadge(activeContract.status).label}</span>
                </div>
                <span className="text-[11px] font-mono text-slate-400">
                  Channel: #{activeContract.channel_id} · Created {formatRelativeTime(activeContract.created_at)}
                </span>
              </div>

              {/* Task Statement */}
              <div className="p-3.5 rounded-lg bg-slate-950/80 border border-slate-800/80 text-xs font-mono text-slate-200 leading-relaxed">
                <span className="text-[10px] uppercase font-bold text-indigo-400 block mb-1">
                  Task Specification & Deliverable:
                </span>
                {activeContract.task}
              </div>

              {/* Team Members & Consolidator */}
              <div className="flex items-center gap-2 flex-wrap pt-1">
                <span className="text-[11px] font-mono text-slate-400">Team:</span>
                {activeContract.team?.map(member => {
                  const mStyle = agentStyle(member);
                  const isConsolidator = member.toLowerCase() === activeContract.consolidator?.toLowerCase();
                  return (
                    <div
                      key={member}
                      className={cn(
                        'flex items-center gap-1.5 px-2 py-1 rounded-md text-[11px] font-mono border',
                        mStyle.bg,
                        mStyle.color,
                        mStyle.border
                      )}
                    >
                      <span className="font-bold">{mStyle.initial}</span>
                      <span>@{member}</span>
                      {isConsolidator && (
                        <span className="ml-1 px-1 py-0.2 rounded bg-amber-950/80 border border-amber-700/60 text-amber-300 text-[9px] font-bold">
                          CONSOLIDATOR
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Right Budget Meter */}
            <div className="w-full md:w-56 p-3 rounded-lg bg-slate-950/60 border border-slate-800/80 font-mono space-y-2.5">
              <div className="flex justify-between items-center text-xs">
                <span className="text-slate-400 text-[10px] uppercase">Turn Budget</span>
                <span className="font-bold text-slate-100">
                  {activeContract.budget_remaining} / {activeContract.budget}
                </span>
              </div>
              {/* Progress Bar */}
              <div className="w-full h-2 bg-slate-800 rounded-full overflow-hidden">
                <div
                  className={cn(
                    'h-full transition-all duration-300',
                    activeContract.budget_remaining <= 3 ? 'bg-rose-500' : 'bg-emerald-500'
                  )}
                  style={{
                    width: `${Math.min(
                      100,
                      Math.max(0, (activeContract.budget_remaining / (activeContract.budget || 1)) * 100)
                    )}%`,
                  }}
                />
              </div>
              <div className="flex justify-between items-center text-[10px] text-slate-400">
                <span>Deliveries: {activeContract.deliveries?.length || 0}</span>
                <span>Votes: {activeContract.votes?.length || 0}</span>
              </div>
              <button
                onClick={() => setSelectedContract(activeContract)}
                className="w-full mt-1 py-1.5 text-center text-xs text-indigo-300 hover:text-indigo-200 bg-indigo-950/60 hover:bg-indigo-900/60 border border-indigo-800/50 rounded-lg transition-all"
              >
                Inspect Details & Deliverables
              </button>
            </div>
          </div>
        </div>
      ) : (
        <div className="p-4 rounded-xl border border-slate-800 bg-slate-900/30 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <ShieldCheck className="w-5 h-5 text-emerald-400" />
            <div>
              <h4 className="text-xs font-bold text-slate-200 font-sans">No Active Work Contract in #general</h4>
              <p className="text-[11px] font-mono text-slate-400">
                All previous contracts have reached terminal status or no contract is currently active.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Contract Search & Manual Lookup */}
      <div className="flex flex-col sm:flex-row items-center justify-between gap-3 bg-slate-900/30 p-2.5 rounded-xl border border-slate-800">
        <div className="relative w-full sm:w-72">
          <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-slate-400" />
          <input
            type="text"
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            placeholder="Filter contracts by task, id or agent..."
            className="w-full bg-slate-950/80 border border-slate-800 rounded-lg pl-9 pr-3 py-1.5 text-xs text-slate-200 placeholder-slate-400 focus:outline-none focus:border-indigo-500/50 font-mono"
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

        {/* Lookup ID Form */}
        <form onSubmit={handleLookup} className="flex items-center gap-2 w-full sm:w-auto">
          <input
            type="text"
            value={manualLookupId}
            onChange={e => setManualLookupId(e.target.value)}
            placeholder="Lookup contract ID (bwc-...)"
            className="bg-slate-950/80 border border-slate-800 rounded-lg px-3 py-1.5 text-xs text-slate-200 placeholder-slate-400 focus:outline-none focus:border-indigo-500/50 font-mono w-full sm:w-64"
          />
          <button
            type="submit"
            disabled={!manualLookupId.trim() || loading}
            className="px-3 py-1.5 rounded-lg bg-indigo-950/80 border border-indigo-800/60 hover:bg-indigo-900 text-indigo-200 text-xs font-mono transition-all disabled:opacity-50 whitespace-nowrap"
          >
            Lookup
          </button>
        </form>
      </div>

      {/* Contracts Ledger Table / Grid */}
      <div className="space-y-3">
        <h3 className="text-xs font-bold text-slate-300 font-sans uppercase tracking-wider">
          Tracked Work Contracts ({filteredContracts.length})
        </h3>

        {filteredContracts.length === 0 ? (
          <div className="flex flex-col items-center justify-center p-8 border border-slate-800 border-dashed rounded-xl text-center bg-slate-900/20">
            <FileText className="w-8 h-8 text-slate-500 mb-2" />
            <p className="text-xs text-slate-400 font-mono">No work contracts matching query.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {filteredContracts.map(contract => {
              const statusBadge = getStatusBadge(contract.status);
              const consolidatorStyle = agentStyle(contract.consolidator || 'unknown');

              return (
                <div
                  key={contract.id}
                  className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 hover:border-slate-700 transition-all flex flex-col justify-between space-y-3"
                >
                  <div className="space-y-2">
                    <div className="flex items-start justify-between gap-2">
                      <div className="space-y-0.5">
                        <div className="flex items-center gap-1.5">
                          <span className="font-mono text-xs font-bold text-indigo-300">
                            {contract.id}
                          </span>
                          <button
                            onClick={() => copyToClipboard(contract.id, `copy-${contract.id}`)}
                            className="text-slate-500 hover:text-slate-300"
                          >
                            {copiedId === `copy-${contract.id}` ? (
                              <Check className="w-3 h-3 text-emerald-400" />
                            ) : (
                              <Copy className="w-3 h-3" />
                            )}
                          </button>
                        </div>
                        <span className="text-[10px] font-mono text-slate-500 block">
                          Created {formatRelativeTime(contract.created_at)}
                        </span>
                      </div>

                      <div
                        className={cn(
                          'flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold uppercase border',
                          statusBadge.bg
                        )}
                      >
                        {statusBadge.icon}
                        <span>{statusBadge.label}</span>
                      </div>
                    </div>

                    {/* Task Excerpt */}
                    <p className="text-xs font-mono text-slate-300 line-clamp-2 leading-relaxed">
                      {contract.task}
                    </p>
                  </div>

                  {/* Footer Info */}
                  <div className="flex items-center justify-between pt-2 border-t border-slate-800/60 text-[11px] font-mono">
                    <div className="flex items-center gap-2">
                      <span className="text-slate-500">Consolidator:</span>
                      <span className={cn('font-semibold', consolidatorStyle.color)}>
                        @{contract.consolidator}
                      </span>
                    </div>

                    <button
                      onClick={() => setSelectedContract(contract)}
                      className="text-indigo-400 hover:text-indigo-300 flex items-center gap-1 font-semibold"
                    >
                      <span>Inspect</span>
                      <ChevronRight className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Contract Detail Modal */}
      {selectedContract && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-150">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-2xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden font-mono">
            {/* Modal Header */}
            <div className="flex items-center justify-between p-4 border-b border-slate-800 bg-slate-950/40">
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-sm font-bold text-slate-100 font-sans">
                    Contract Specification
                  </h3>
                  <div
                    className={cn(
                      'flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-bold uppercase border',
                      getStatusBadge(selectedContract.status).bg
                    )}
                  >
                    {getStatusBadge(selectedContract.status).icon}
                    <span>{getStatusBadge(selectedContract.status).label}</span>
                  </div>
                </div>
                <p className="text-[11px] text-indigo-300 font-mono mt-0.5">
                  ID: {selectedContract.id}
                </p>
              </div>
              <button
                onClick={() => setSelectedContract(null)}
                className="p-1 rounded-lg text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {/* Modal Body */}
            <div className="flex-1 overflow-y-auto p-4 space-y-4 text-xs">
              {/* Task Details */}
              <div className="space-y-1.5">
                <span className="text-[10px] uppercase text-slate-500 font-bold block">
                  Task Statement
                </span>
                <div className="p-3 rounded-lg bg-slate-950/80 border border-slate-800 text-slate-200 whitespace-pre-wrap leading-relaxed">
                  {selectedContract.task}
                </div>
              </div>

              {/* Meta Grid */}
              <div className="grid grid-cols-2 gap-2 text-[11px]">
                <div className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-slate-500 text-[9px] uppercase block">Turn Budget</span>
                  <span className="text-slate-200 font-bold">
                    {selectedContract.budget_remaining} remaining / {selectedContract.budget} total
                  </span>
                </div>
                <div className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-slate-500 text-[9px] uppercase block">Consolidator</span>
                  <span className="text-slate-200 font-bold">@{selectedContract.consolidator}</span>
                </div>
              </div>

              {/* Team Members */}
              <div className="space-y-1.5">
                <span className="text-[10px] uppercase text-slate-500 font-bold block">
                  Assigned Team ({selectedContract.team?.length || 0})
                </span>
                <div className="flex items-center gap-1.5 flex-wrap">
                  {selectedContract.team?.map(m => {
                    const st = agentStyle(m);
                    return (
                      <span
                        key={m}
                        className={cn(
                          'px-2 py-1 rounded text-[11px] border flex items-center gap-1',
                          st.bg,
                          st.color,
                          st.border
                        )}
                      >
                        <span className="font-bold">{st.initial}</span>
                        <span>@{m}</span>
                      </span>
                    );
                  })}
                </div>
              </div>

              {/* Deliverables Ledger */}
              <div className="space-y-1.5">
                <span className="text-[10px] uppercase text-slate-500 font-bold block">
                  Registered Deliverables ({selectedContract.deliveries?.length || 0})
                </span>
                {!selectedContract.deliveries || selectedContract.deliveries.length === 0 ? (
                  <div className="p-3 rounded-lg bg-slate-950/40 border border-slate-800/40 text-slate-500 italic">
                    No deliverables recorded yet for this contract.
                  </div>
                ) : (
                  <div className="space-y-2">
                    {selectedContract.deliveries.map((del, idx) => (
                      <div
                        key={idx}
                        className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800 flex items-center justify-between"
                      >
                        <div className="space-y-0.5">
                          <span className="text-indigo-300 font-bold">@{del.agent_id}</span>
                          <p className="text-slate-300 text-[11px]">{del.artifact || del.note || 'Delivered'}</p>
                        </div>
                        {del.ts && (
                          <span className="text-[10px] text-slate-500">{formatRelativeTime(del.ts)}</span>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Consensus & Votes */}
              {selectedContract.votes && selectedContract.votes.length > 0 && (
                <div className="space-y-1.5">
                  <span className="text-[10px] uppercase text-slate-500 font-bold block">
                    Consensus Votes ({selectedContract.votes.length})
                  </span>
                  <div className="flex items-center gap-2 flex-wrap">
                    {selectedContract.votes.map((v, idx) => (
                      <div
                        key={idx}
                        className="px-2 py-1 rounded-md bg-slate-950/60 border border-slate-800 text-[11px] flex items-center gap-1.5"
                      >
                        <span className="font-bold text-slate-300">@{v.agent_id}:</span>
                        <span
                          className={cn(
                            'font-bold uppercase text-[10px]',
                            v.vote === 'approve' ? 'text-emerald-400' : 'text-rose-400'
                          )}
                        >
                          {v.vote}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Modal Footer */}
            <div className="p-3 border-t border-slate-800 bg-slate-950/40 flex justify-end">
              <button
                onClick={() => setSelectedContract(null)}
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

export default WorkContractsPanel;
