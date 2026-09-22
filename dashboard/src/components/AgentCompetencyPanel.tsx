import React, { useState, useEffect, useCallback, useMemo } from 'react';
import {
  Brain,
  Zap,
  ShieldCheck,
  Award,
  TrendingUp,
  Activity,
  CheckCircle2,
  XCircle,
  Search,
  RefreshCw,
  Layers,
  Copy,
  Check,
  X,
  ChevronRight,
  Terminal,
  HardDrive,
  MessageSquare,
  Database,
  Code2,
  Network,
  Compass,
  Eye,
  BarChart3,
  Users,
} from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import { agentStyle } from '../lib/agent-meta';
import { MetricCard } from './ui/MetricPrimitives';

export interface DomainScore {
  domain: string;
  agent_id?: string;
  successes: number;
  failures: number;
  total: number;
  rate: number;
}

export interface AgentCompetencyProfile {
  agent_id: string;
  role: string;
  total_calls: number;
  first_seen: string | number;
  last_intent?: string | null;
  competencies?: Record<string, number> | string | null;
  identity_node?: string;
  domains: DomainScore[];
}

export interface AgentCompetencyPanelProps {
  bridge: {
    fetchRaw: (url: string, init?: RequestInit) => Promise<unknown>;
  } | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}

export function parseCompetencies(raw: unknown): Record<string, number> {
  if (!raw) return {};
  if (typeof raw === 'object' && !Array.isArray(raw)) {
    const res: Record<string, number> = {};
    for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
      if (typeof v === 'number') {
        res[k] = v;
      } else if (typeof v === 'string') {
        const num = parseFloat(v);
        if (!isNaN(num)) res[k] = num;
      }
    }
    return res;
  }
  if (typeof raw === 'string') {
    try {
      const parsed = JSON.parse(raw);
      return parseCompetencies(parsed);
    } catch {
      return {};
    }
  }
  return {};
}

function getDomainIcon(domain: string) {
  const d = domain.toLowerCase();
  if (d.includes('bash') || d.includes('term') || d.includes('cli') || d.includes('cmd')) {
    return <Terminal className="w-3.5 h-3.5 text-amber-400" />;
  }
  if (d.includes('file') || d.includes('fs') || d.includes('io') || d.includes('disk')) {
    return <HardDrive className="w-3.5 h-3.5 text-blue-400" />;
  }
  if (d.includes('coloquio') || d.includes('chat') || d.includes('social') || d.includes('msg')) {
    return <MessageSquare className="w-3.5 h-3.5 text-emerald-400" />;
  }
  if (d.includes('mem') || d.includes('silva') || d.includes('fsrs') || d.includes('graph')) {
    return <Database className="w-3.5 h-3.5 text-violet-400" />;
  }
  if (d.includes('code') || d.includes('kernel') || d.includes('rust') || d.includes('build')) {
    return <Code2 className="w-3.5 h-3.5 text-cyan-400" />;
  }
  if (d.includes('federation') || d.includes('mesh') || d.includes('p2p') || d.includes('net')) {
    return <Network className="w-3.5 h-3.5 text-indigo-400" />;
  }
  if (d.includes('research') || d.includes('web') || d.includes('search') || d.includes('doc')) {
    return <Compass className="w-3.5 h-3.5 text-rose-400" />;
  }
  if (d.includes('vision') || d.includes('image') || d.includes('screen') || d.includes('ocr')) {
    return <Eye className="w-3.5 h-3.5 text-fuchsia-400" />;
  }
  return <Layers className="w-3.5 h-3.5 text-slate-400" />;
}

function getDomainColor(domain: string) {
  const d = domain.toLowerCase();
  if (d.includes('bash') || d.includes('term') || d.includes('cli')) {
    return 'bg-amber-950/40 text-amber-300 border-amber-800/40';
  }
  if (d.includes('file') || d.includes('fs') || d.includes('io')) {
    return 'bg-blue-950/40 text-blue-300 border-blue-800/40';
  }
  if (d.includes('coloquio') || d.includes('chat') || d.includes('social')) {
    return 'bg-emerald-950/40 text-emerald-300 border-emerald-800/40';
  }
  if (d.includes('mem') || d.includes('silva') || d.includes('fsrs')) {
    return 'bg-violet-950/40 text-violet-300 border-violet-800/40';
  }
  if (d.includes('code') || d.includes('kernel') || d.includes('rust')) {
    return 'bg-cyan-950/40 text-cyan-300 border-cyan-800/40';
  }
  if (d.includes('federation') || d.includes('mesh') || d.includes('p2p')) {
    return 'bg-indigo-950/40 text-indigo-300 border-indigo-800/40';
  }
  if (d.includes('research') || d.includes('web') || d.includes('search')) {
    return 'bg-rose-950/40 text-rose-300 border-rose-800/40';
  }
  if (d.includes('vision') || d.includes('image')) {
    return 'bg-fuchsia-950/40 text-fuchsia-300 border-fuchsia-800/40';
  }
  return 'bg-slate-800/40 text-slate-300 border-slate-700/40';
}

function getCompetencyScoreColor(score: number) {
  if (score >= 0.8) return 'text-emerald-400 bg-emerald-500';
  if (score >= 0.6) return 'text-cyan-400 bg-cyan-500';
  if (score >= 0.4) return 'text-amber-400 bg-amber-500';
  return 'text-rose-400 bg-rose-500';
}

export function AgentCompetencyPanel({ bridge, notify }: AgentCompetencyPanelProps) {
  const [agents, setAgents] = useState<AgentCompetencyProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastRefreshed, setLastRefreshed] = useState<Date | null>(null);

  // Filter & Search states
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedDomain, setSelectedDomain] = useState<string>('all');
  const [selectedRole, setSelectedRole] = useState<string>('all');
  const [sortBy, setSortBy] = useState<'calls' | 'competency' | 'rate' | 'name'>('calls');
  const [viewMode, setViewMode] = useState<'agents' | 'leaderboard'>('agents');

  // Selected agent for inspector modal
  const [selectedAgent, setSelectedAgent] = useState<AgentCompetencyProfile | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const fetchAgents = useCallback(async () => {
    if (!bridge) {
      setLoading(false);
      return;
    }
    try {
      setError(null);
      const data = (await bridge.fetchRaw('/api/v1/agents')) as {
        agents?: AgentCompetencyProfile[];
        count?: number;
      };
      if (data && Array.isArray(data.agents)) {
        setAgents(data.agents);
      } else {
        setAgents([]);
      }
      setLastRefreshed(new Date());
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  usePolling('agent-competency-panel', fetchAgents, { interval: 'standard' });

  useEffect(() => {
    fetchAgents();
  }, [fetchAgents]);

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard?.writeText(text);
    setCopiedId(id);
    if (notify) notify(`Copied ${id} to clipboard`, 'info');
    setTimeout(() => setCopiedId(null), 2000);
  };

  // Extract all unique cognitive domains across agents and domain scores
  const allDomains = useMemo(() => {
    const domainSet = new Set<string>();
    agents.forEach((agent) => {
      const comp = parseCompetencies(agent.competencies);
      Object.keys(comp).forEach((d) => domainSet.add(d));
      (agent.domains || []).forEach((d) => {
        if (d.domain) domainSet.add(d.domain);
      });
    });
    return Array.from(domainSet).sort();
  }, [agents]);

  // Extract all unique roles
  const allRoles = useMemo(() => {
    const roleSet = new Set<string>();
    agents.forEach((a) => {
      if (a.role) roleSet.add(a.role);
    });
    return Array.from(roleSet).sort();
  }, [agents]);

  // Overall KPIs
  const stats = useMemo(() => {
    const totalAgents = agents.length;
    let totalCalls = 0;
    let highCompetencyCount = 0;

    agents.forEach((a) => {
      totalCalls += a.total_calls || 0;
      const comp = parseCompetencies(a.competencies);
      const maxComp = Object.values(comp).reduce((max, val) => Math.max(max, val), 0);
      const maxDomainRate = (a.domains || []).reduce(
        (max, d) => Math.max(max, d.rate <= 1 ? d.rate : d.rate / 100),
        0
      );
      if (maxComp >= 0.8 || maxDomainRate >= 0.8) {
        highCompetencyCount++;
      }
    });

    return {
      totalAgents,
      totalCalls,
      highCompetencyCount,
      uniqueDomainsCount: allDomains.length,
    };
  }, [agents, allDomains]);

  // Filtered and Sorted Agents
  const filteredAgents = useMemo(() => {
    return agents
      .filter((agent) => {
        const matchesSearch =
          !searchQuery.trim() ||
          agent.agent_id.toLowerCase().includes(searchQuery.toLowerCase()) ||
          (agent.role && agent.role.toLowerCase().includes(searchQuery.toLowerCase())) ||
          (agent.last_intent && agent.last_intent.toLowerCase().includes(searchQuery.toLowerCase()));

        const matchesRole = selectedRole === 'all' || agent.role === selectedRole;

        const comp = parseCompetencies(agent.competencies);
        const hasDomainScore =
          selectedDomain === 'all' ||
          comp[selectedDomain] !== undefined ||
          (agent.domains || []).some((d) => d.domain === selectedDomain);

        return matchesSearch && matchesRole && hasDomainScore;
      })
      .sort((a, b) => {
        if (sortBy === 'calls') {
          return (b.total_calls || 0) - (a.total_calls || 0);
        }
        if (sortBy === 'name') {
          return a.agent_id.localeCompare(b.agent_id);
        }
        if (sortBy === 'competency') {
          const compA = parseCompetencies(a.competencies);
          const compB = parseCompetencies(b.competencies);
          const avgA =
            Object.values(compA).length > 0
              ? Object.values(compA).reduce((s, v) => s + v, 0) / Object.values(compA).length
              : 0;
          const avgB =
            Object.values(compB).length > 0
              ? Object.values(compB).reduce((s, v) => s + v, 0) / Object.values(compB).length
              : 0;
          return avgB - avgA;
        }
        if (sortBy === 'rate') {
          const rateA =
            (a.domains || []).length > 0
              ? a.domains.reduce((s, d) => s + (d.rate <= 1 ? d.rate : d.rate / 100), 0) /
                a.domains.length
              : 0;
          const rateB =
            (b.domains || []).length > 0
              ? b.domains.reduce((s, d) => s + (d.rate <= 1 ? d.rate : d.rate / 100), 0) /
                b.domains.length
              : 0;
          return rateB - rateA;
        }
        return 0;
      });
  }, [agents, searchQuery, selectedDomain, selectedRole, sortBy]);

  // Domain Leaderboard breakdown (ADR-014 Scheduler routing view)
  const domainLeaderboards = useMemo(() => {
    const map: Record<
      string,
      Array<{
        agent_id: string;
        role: string;
        score: number;
        successes: number;
        failures: number;
        total: number;
        rate: number;
      }>
    > = {};

    allDomains.forEach((domain) => {
      map[domain] = [];
    });

    agents.forEach((agent) => {
      const comp = parseCompetencies(agent.competencies);
      const domainMap: Record<string, DomainScore> = {};
      (agent.domains || []).forEach((d) => {
        domainMap[d.domain] = d;
      });

      allDomains.forEach((domain) => {
        const compScore = comp[domain] !== undefined ? comp[domain] : 0;
        const dScore = domainMap[domain];
        if (compScore > 0 || dScore) {
          const successes = dScore?.successes || 0;
          const failures = dScore?.failures || 0;
          const total = dScore?.total || successes + failures;
          const rawRate = dScore?.rate !== undefined ? dScore.rate : total > 0 ? successes / total : 0;
          const rate = rawRate <= 1 ? rawRate : rawRate / 100;

          map[domain].push({
            agent_id: agent.agent_id,
            role: agent.role,
            score: compScore,
            successes,
            failures,
            total,
            rate,
          });
        }
      });
    });

    // Sort each domain by rate DESC, score DESC, successes DESC
    Object.keys(map).forEach((d) => {
      map[d].sort((a, b) => {
        if (b.rate !== a.rate) return b.rate - a.rate;
        if (b.score !== a.score) return b.score - a.score;
        return b.successes - a.successes;
      });
    });

    return map;
  }, [agents, allDomains]);

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 font-sans text-slate-100 overflow-y-auto pr-1">
      {/* Header Banner */}
      <div className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 backdrop-blur-md flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="p-2.5 rounded-lg bg-emerald-500/10 border border-emerald-500/30 text-emerald-400">
            <Brain className="w-6 h-6" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-base font-bold text-slate-100 tracking-tight font-mono">
                Agent Competency & Cognitive Domains
              </h2>
              <span className="px-2 py-0.5 text-[10px] font-mono uppercase rounded-full bg-emerald-950/60 border border-emerald-800/50 text-emerald-300">
                ADR-014 Scheduler
              </span>
            </div>
            <p className="text-xs text-slate-400 mt-0.5">
              Live domain-specific task reputation, success rates, and autonomous routing competencies
            </p>
          </div>
        </div>

        {/* View Mode & Actions */}
        <div className="flex items-center gap-2 self-start md:self-auto">
          <div className="flex rounded-lg border border-slate-800 bg-slate-950/60 p-0.5">
            <button
              onClick={() => setViewMode('agents')}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono rounded-md transition-all',
                viewMode === 'agents'
                  ? 'bg-emerald-500/20 text-emerald-300 font-bold border border-emerald-500/30'
                  : 'text-slate-400 hover:text-slate-200'
              )}
            >
              <Users className="w-3.5 h-3.5" />
              By Agent
            </button>
            <button
              onClick={() => setViewMode('leaderboard')}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono rounded-md transition-all',
                viewMode === 'leaderboard'
                  ? 'bg-emerald-500/20 text-emerald-300 font-bold border border-emerald-500/30'
                  : 'text-slate-400 hover:text-slate-200'
              )}
            >
              <BarChart3 className="w-3.5 h-3.5" />
              Domain Matrix
            </button>
          </div>

          <button
            onClick={() => fetchAgents()}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-800 bg-slate-900/60 hover:bg-slate-800/80 text-slate-300 text-xs font-mono transition-all disabled:opacity-50"
            title="Refresh Competency Data"
          >
            <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin text-emerald-400')} />
            <span className="hidden sm:inline">Refresh</span>
          </button>
        </div>
      </div>

      {/* Metric Cards Banner */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard
          icon={Users}
          label="Registered Agents"
          value={stats.totalAgents}
          sub={lastRefreshed ? `Updated ${lastRefreshed.toLocaleTimeString()}` : 'Live Scheduler Data'}
        />
        <MetricCard
          icon={Award}
          label="Specialist Agents"
          value={stats.highCompetencyCount}
          unit={`/ ${stats.totalAgents}`}
          sub=">= 80% Domain Score"
          valueClass="text-emerald-400"
        />
        <MetricCard
          icon={Layers}
          label="Cognitive Domains"
          value={stats.uniqueDomainsCount}
          sub="Tracked in Scheduler"
          valueClass="text-cyan-400"
        />
        <MetricCard
          icon={Activity}
          label="Total Task Calls"
          value={stats.totalCalls.toLocaleString()}
          sub="Across fleet lifetime"
          valueClass="text-indigo-400"
        />
      </div>

      {/* Search & Filter Controls */}
      <div className="p-3 rounded-xl border border-slate-800 bg-slate-900/30 flex flex-col md:flex-row items-stretch md:items-center justify-between gap-3">
        <div className="flex-1 relative">
          <Search className="w-4 h-4 text-slate-500 absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            placeholder="Filter by agent id, role, or intent..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-9 pr-3 py-1.5 bg-slate-950/60 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 placeholder-slate-500 focus:outline-none focus:border-emerald-500/50"
          />
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Domain Filter */}
          <select
            value={selectedDomain}
            onChange={(e) => setSelectedDomain(e.target.value)}
            className="px-2.5 py-1.5 bg-slate-950/60 border border-slate-800 rounded-lg text-xs font-mono text-slate-300 focus:outline-none focus:border-emerald-500/50"
            aria-label="Filter by Domain"
          >
            <option value="all">All Domains ({allDomains.length})</option>
            {allDomains.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>

          {/* Role Filter */}
          <select
            value={selectedRole}
            onChange={(e) => setSelectedRole(e.target.value)}
            className="px-2.5 py-1.5 bg-slate-950/60 border border-slate-800 rounded-lg text-xs font-mono text-slate-300 focus:outline-none focus:border-emerald-500/50"
            aria-label="Filter by Role"
          >
            <option value="all">All Roles ({allRoles.length})</option>
            {allRoles.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>

          {/* Sort By */}
          <select
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as typeof sortBy)}
            className="px-2.5 py-1.5 bg-slate-950/60 border border-slate-800 rounded-lg text-xs font-mono text-slate-300 focus:outline-none focus:border-emerald-500/50"
            aria-label="Sort by"
          >
            <option value="calls">Sort by Calls (Desc)</option>
            <option value="competency">Sort by Competency Avg</option>
            <option value="rate">Sort by Success Rate</option>
            <option value="name">Sort by Name (A-Z)</option>
          </select>
        </div>
      </div>

      {/* Error state */}
      {error && (
        <div className="p-3 rounded-lg border border-rose-800/50 bg-rose-950/40 text-rose-300 text-xs font-mono flex items-center gap-2">
          <XCircle className="w-4 h-4 text-rose-400 flex-shrink-0" />
          <span>Error loading agent competencies: {error}</span>
        </div>
      )}

      {/* Main Content Area */}
      {viewMode === 'agents' ? (
        <div className="space-y-3">
          {filteredAgents.length === 0 ? (
            <div className="p-8 rounded-xl border border-slate-800 bg-slate-900/20 text-center text-slate-400 text-xs font-mono">
              <Brain className="w-8 h-8 text-slate-600 mx-auto mb-2" />
              <p>No agents matched the selected criteria.</p>
            </div>
          ) : (
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
              {filteredAgents.map((agent) => {
                const style = agentStyle(agent.agent_id);
                const comp = parseCompetencies(agent.competencies);
                const compEntries = Object.entries(comp);
                const domainScores = agent.domains || [];

                return (
                  <div
                    key={agent.agent_id}
                    className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 backdrop-blur-md hover:border-slate-700 transition-all flex flex-col justify-between space-y-4 group"
                  >
                    {/* Agent Header */}
                    <div>
                      <div className="flex items-start justify-between gap-3">
                        <div className="flex items-center gap-3">
                          <div
                            className={cn(
                              'w-10 h-10 rounded-xl flex items-center justify-center font-bold text-sm border ring-1',
                              style.bg,
                              style.color,
                              style.border,
                              style.ring
                            )}
                          >
                            {style.initial}
                          </div>
                          <div>
                            <div className="flex items-center gap-2">
                              <span className="font-mono font-bold text-sm text-slate-100">
                                @{agent.agent_id}
                              </span>
                              <span className="px-2 py-0.5 text-[10px] font-mono uppercase rounded-md bg-slate-800 text-slate-300 border border-slate-700">
                                {agent.role || 'generalist'}
                              </span>
                            </div>
                            <div className="text-[11px] text-slate-400 font-mono mt-0.5 flex items-center gap-3">
                              <span>Calls: <strong className="text-slate-200">{agent.total_calls}</strong></span>
                              {agent.identity_node && (
                                <span className="text-slate-500 truncate max-w-[140px]" title={agent.identity_node}>
                                  Node: {agent.identity_node}
                                </span>
                              )}
                            </div>
                          </div>
                        </div>

                        <button
                          onClick={() => setSelectedAgent(agent)}
                          className="px-2.5 py-1 text-[11px] font-mono text-slate-400 hover:text-emerald-300 border border-slate-800 hover:border-emerald-500/40 rounded-lg bg-slate-950/40 transition-all flex items-center gap-1"
                        >
                          <span>Inspect</span>
                          <ChevronRight className="w-3 h-3" />
                        </button>
                      </div>

                      {/* Last Intent (if available) */}
                      {agent.last_intent && (
                        <div className="mt-3 p-2 rounded-lg bg-slate-950/60 border border-slate-800/80 text-[11px] font-mono text-slate-300 flex items-start gap-2">
                          <Zap className="w-3.5 h-3.5 text-amber-400 flex-shrink-0 mt-0.5" />
                          <span className="truncate" title={agent.last_intent}>
                            Intent: {agent.last_intent}
                          </span>
                        </div>
                      )}
                    </div>

                    {/* Competency Scores Meters */}
                    <div className="space-y-3 pt-2 border-t border-slate-800/80">
                      <div>
                        <div className="flex items-center justify-between text-[11px] font-mono text-slate-400 mb-1.5">
                          <span className="flex items-center gap-1 font-bold text-slate-300 uppercase tracking-wider">
                            <Brain className="w-3 h-3 text-emerald-400" />
                            Competency Scores
                          </span>
                          <span>{compEntries.length} evaluated</span>
                        </div>

                        {compEntries.length === 0 ? (
                          <div className="text-[11px] font-mono text-slate-500 italic py-1">
                            No evaluated competencies yet (defaults applied)
                          </div>
                        ) : (
                          <div className="space-y-2">
                            {compEntries.map(([domain, score]) => {
                              const pct = Math.round((score <= 1 ? score : score / 100) * 100);
                              const colorClass = getCompetencyScoreColor(score <= 1 ? score : score / 100);

                              return (
                                <div key={domain} className="space-y-1">
                                  <div className="flex items-center justify-between text-[11px] font-mono">
                                    <div className="flex items-center gap-1.5">
                                      {getDomainIcon(domain)}
                                      <span className="text-slate-300">{domain}</span>
                                    </div>
                                    <span className={cn('font-bold', colorClass.split(' ')[0])}>
                                      {pct}%
                                    </span>
                                  </div>
                                  <div className="w-full bg-slate-800/60 rounded-full h-1.5 overflow-hidden">
                                    <div
                                      className={cn('h-1.5 rounded-full transition-all duration-500', colorClass.split(' ')[1])}
                                      style={{ width: `${Math.min(100, Math.max(0, pct))}%` }}
                                    />
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>

                      {/* Domain Outcomes & Success Rates */}
                      {domainScores.length > 0 && (
                        <div className="pt-2 border-t border-slate-800/60">
                          <div className="flex items-center justify-between text-[11px] font-mono text-slate-400 mb-1.5">
                            <span className="flex items-center gap-1 font-bold text-slate-300 uppercase tracking-wider">
                              <ShieldCheck className="w-3 h-3 text-cyan-400" />
                              Domain Outcomes (ADR-014)
                            </span>
                            <span>{domainScores.length} domains</span>
                          </div>

                          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                            {domainScores.map((ds) => {
                              const rawRate = ds.rate !== undefined ? ds.rate : ds.total > 0 ? ds.successes / ds.total : 0;
                              const ratePct = Math.round((rawRate <= 1 ? rawRate : rawRate / 100) * 100);

                              return (
                                <div
                                  key={ds.domain}
                                  className="p-2 rounded-lg bg-slate-950/40 border border-slate-800/60 flex flex-col justify-between space-y-1 text-[11px] font-mono"
                                >
                                  <div className="flex items-center justify-between">
                                    <span
                                      className={cn(
                                        'px-1.5 py-0.5 rounded text-[10px] border flex items-center gap-1',
                                        getDomainColor(ds.domain)
                                      )}
                                    >
                                      {getDomainIcon(ds.domain)}
                                      {ds.domain}
                                    </span>
                                    <span className="text-slate-300 font-bold">{ratePct}%</span>
                                  </div>
                                  <div className="flex items-center justify-between text-slate-400 text-[10px] pt-1">
                                    <span className="text-emerald-400 flex items-center gap-0.5">
                                      <Check className="w-3 h-3" /> {ds.successes}
                                    </span>
                                    <span className="text-rose-400 flex items-center gap-0.5">
                                      <X className="w-3 h-3" /> {ds.failures}
                                    </span>
                                    <span className="text-slate-500">Tot: {ds.total}</span>
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ) : (
        /* Domain Matrix / Leaderboard Mode */
        <div className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {allDomains
              .filter((d) => selectedDomain === 'all' || d === selectedDomain)
              .map((domain) => {
                const leaderboard = domainLeaderboards[domain] || [];
                const bestAgent = leaderboard[0];

                return (
                  <div
                    key={domain}
                    className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 backdrop-blur-md flex flex-col justify-between space-y-3"
                  >
                    <div>
                      <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center gap-2">
                          <div className={cn('p-1.5 rounded-lg border flex items-center justify-center', getDomainColor(domain))}>
                            {getDomainIcon(domain)}
                          </div>
                          <div>
                            <h3 className="font-mono font-bold text-sm text-slate-100 capitalize">
                              {domain}
                            </h3>
                            <span className="text-[10px] font-mono text-slate-400">
                              {leaderboard.length} eligible agent{leaderboard.length === 1 ? '' : 's'}
                            </span>
                          </div>
                        </div>

                        {bestAgent && (
                          <div className="flex items-center gap-1 px-2 py-0.5 rounded-full bg-amber-950/50 border border-amber-800/40 text-amber-300 text-[10px] font-mono">
                            <Award className="w-3 h-3 text-amber-400" />
                            <span>Top: @{bestAgent.agent_id}</span>
                          </div>
                        )}
                      </div>

                      {/* Ranked Agents List */}
                      <div className="space-y-2 mt-3">
                        {leaderboard.length === 0 ? (
                          <p className="text-[11px] font-mono text-slate-500 italic py-2">
                            No performance records for this domain yet.
                          </p>
                        ) : (
                          leaderboard.slice(0, 4).map((entry, idx) => {
                            const style = agentStyle(entry.agent_id);
                            const ratePct = Math.round(entry.rate * 100);

                            return (
                              <div
                                key={entry.agent_id}
                                className="p-2 rounded-lg bg-slate-950/60 border border-slate-800/80 flex items-center justify-between text-[11px] font-mono"
                              >
                                <div className="flex items-center gap-2 min-w-0">
                                  <span className="text-slate-500 font-bold w-3">#{idx + 1}</span>
                                  <div
                                    className={cn(
                                      'w-5 h-5 rounded flex items-center justify-center font-bold text-[10px] border flex-shrink-0',
                                      style.bg,
                                      style.color,
                                      style.border
                                    )}
                                  >
                                    {style.initial}
                                  </div>
                                  <span className="text-slate-200 font-bold truncate">
                                    @{entry.agent_id}
                                  </span>
                                </div>

                                <div className="flex items-center gap-3 flex-shrink-0">
                                  <div className="text-right">
                                    <span
                                      className={cn(
                                        'font-bold',
                                        ratePct >= 80
                                          ? 'text-emerald-400'
                                          : ratePct >= 50
                                          ? 'text-cyan-400'
                                          : 'text-amber-400'
                                      )}
                                    >
                                      {ratePct}%
                                    </span>
                                    <span className="text-[10px] text-slate-500 ml-1">
                                      ({entry.successes}/{entry.total})
                                    </span>
                                  </div>
                                </div>
                              </div>
                            );
                          })
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
          </div>
        </div>
      )}

      {/* Agent Detail Modal */}
      {selectedAgent && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4 animate-in fade-in duration-150">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-2xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden font-sans">
            {/* Modal Header */}
            <div className="p-4 border-b border-slate-800 flex items-center justify-between bg-slate-950/60">
              <div className="flex items-center gap-3">
                <div
                  className={cn(
                    'w-10 h-10 rounded-xl flex items-center justify-center font-bold text-sm border ring-1',
                    agentStyle(selectedAgent.agent_id).bg,
                    agentStyle(selectedAgent.agent_id).color,
                    agentStyle(selectedAgent.agent_id).border,
                    agentStyle(selectedAgent.agent_id).ring
                  )}
                >
                  {agentStyle(selectedAgent.agent_id).initial}
                </div>
                <div>
                  <div className="flex items-center gap-2">
                    <h3 className="font-mono font-bold text-slate-100 text-sm">
                      @{selectedAgent.agent_id}
                    </h3>
                    <span className="px-2 py-0.5 text-[10px] font-mono uppercase rounded bg-slate-800 text-slate-300 border border-slate-700">
                      {selectedAgent.role || 'generalist'}
                    </span>
                  </div>
                  <p className="text-xs text-slate-400 font-mono mt-0.5">
                    Identity Node: {selectedAgent.identity_node || 'N/A'}
                  </p>
                </div>
              </div>

              <button
                onClick={() => setSelectedAgent(null)}
                className="p-1.5 text-slate-400 hover:text-slate-200 rounded-lg hover:bg-slate-800 transition-colors"
                aria-label="Close modal"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {/* Modal Body */}
            <div className="p-4 overflow-y-auto space-y-4 text-xs font-mono">
              {/* Basic Stats Grid */}
              <div className="grid grid-cols-3 gap-2">
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-[10px] text-slate-500 uppercase block">Total Calls</span>
                  <span className="text-base font-bold text-slate-200">{selectedAgent.total_calls}</span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-[10px] text-slate-500 uppercase block">First Seen</span>
                  <span className="text-base font-bold text-slate-200 truncate block" title={String(selectedAgent.first_seen)}>
                    {String(selectedAgent.first_seen)}
                  </span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-[10px] text-slate-500 uppercase block">Domains</span>
                  <span className="text-base font-bold text-slate-200">
                    {selectedAgent.domains?.length || 0}
                  </span>
                </div>
              </div>

              {/* Last Intent */}
              {selectedAgent.last_intent && (
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800 space-y-1">
                  <span className="text-[10px] text-slate-500 uppercase block">Last Intent</span>
                  <p className="text-slate-300 font-mono text-[11px]">{selectedAgent.last_intent}</p>
                </div>
              )}

              {/* Granular Domains Outcome */}
              <div>
                <h4 className="text-xs font-bold text-slate-300 mb-2 uppercase tracking-wider flex items-center gap-1.5">
                  <ShieldCheck className="w-4 h-4 text-emerald-400" />
                  Granular Domain Outcomes (ADR-014)
                </h4>
                {(!selectedAgent.domains || selectedAgent.domains.length === 0) ? (
                  <p className="text-slate-500 italic py-2">No domain outcome history recorded.</p>
                ) : (
                  <div className="border border-slate-800 rounded-lg overflow-hidden">
                    <table className="w-full text-left border-collapse">
                      <thead>
                        <tr className="bg-slate-950/80 text-[10px] uppercase text-slate-400 border-b border-slate-800">
                          <th className="py-2 px-3">Domain</th>
                          <th className="py-2 px-3 text-right">Successes</th>
                          <th className="py-2 px-3 text-right">Failures</th>
                          <th className="py-2 px-3 text-right">Total</th>
                          <th className="py-2 px-3 text-right">Success Rate</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-slate-800/60">
                        {selectedAgent.domains.map((d) => {
                          const rawRate = d.rate !== undefined ? d.rate : d.total > 0 ? d.successes / d.total : 0;
                          const ratePct = Math.round((rawRate <= 1 ? rawRate : rawRate / 100) * 100);

                          return (
                            <tr key={d.domain} className="hover:bg-slate-800/30">
                              <td className="py-2 px-3">
                                <span
                                  className={cn(
                                    'inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] border',
                                    getDomainColor(d.domain)
                                  )}
                                >
                                  {getDomainIcon(d.domain)}
                                  {d.domain}
                                </span>
                              </td>
                              <td className="py-2 px-3 text-right text-emerald-400">{d.successes}</td>
                              <td className="py-2 px-3 text-right text-rose-400">{d.failures}</td>
                              <td className="py-2 px-3 text-right text-slate-300">{d.total}</td>
                              <td className="py-2 px-3 text-right font-bold text-slate-200">{ratePct}%</td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>

              {/* Raw JSON Dump */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <h4 className="text-xs font-bold text-slate-300 uppercase tracking-wider">
                    Raw Profile Data
                  </h4>
                  <button
                    onClick={() =>
                      copyToClipboard(JSON.stringify(selectedAgent, null, 2), selectedAgent.agent_id)
                    }
                    className="flex items-center gap-1 text-[11px] text-slate-400 hover:text-slate-200"
                  >
                    {copiedId === selectedAgent.agent_id ? (
                      <>
                        <Check className="w-3 h-3 text-emerald-400" />
                        <span className="text-emerald-400">Copied</span>
                      </>
                    ) : (
                      <>
                        <Copy className="w-3 h-3" />
                        <span>Copy JSON</span>
                      </>
                    )}
                  </button>
                </div>
                <pre className="p-3 rounded-lg bg-slate-950/80 border border-slate-800 text-[10px] text-slate-400 overflow-x-auto max-h-40 scrollbar-thin">
                  {JSON.stringify(selectedAgent, null, 2)}
                </pre>
              </div>
            </div>

            {/* Modal Footer */}
            <div className="p-3 border-t border-slate-800 bg-slate-950/60 flex justify-end">
              <button
                onClick={() => setSelectedAgent(null)}
                className="px-4 py-1.5 rounded-lg border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono transition-colors"
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

export default AgentCompetencyPanel;
